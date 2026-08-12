//! Off-thread document worker and request/response dispatch loop.

use bytes::Bytes;
use fepdf_render::{FallbackFontType, VelloBackend};
use fepdf_sdk::PdfDocument;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use vello::Scene;

pub enum WorkerRequest {
    Open {
        data: Bytes,
        name: Option<String>,
    },
    RenderPage {
        index: usize,
        scale: f64,
    },
    UpdateNode {
        handle_id: u32,
        tag: String,
        alt_text: Option<String>,
    },
    Save {
        path: std::path::PathBuf,
        compress: bool,
        linearize: bool,
        vacuum: bool,
        upgrade_pdf20: bool,
        redaction_zones: Vec<crate::redaction::RedactionZone>,
        cert_path: Option<std::path::PathBuf>,
        cert_password: String,
        signature_position: Option<(usize, [f32; 4])>,
    },
    Audit,
    ReorderPages {
        from: usize,
        to: usize,
    },
    RemovePages {
        indices: Vec<usize>,
    },
    DuplicatePage {
        index: usize,
    },
    InsertDocument {
        data: Bytes,
        at_index: usize,
    },
    RotatePages {
        indices: Vec<usize>,
        delta: fepdf_sdk::Quarter,
    },
    ExtractPages {
        indices: Vec<usize>,
    },
}

/// Everything the UI needs after a document finishes loading.
///
/// Kept behind a `Box` in [`WorkerResponse`] so that the far more frequent
/// `PageRendered` messages are not padded out to this variant's size.
pub struct LoadedDocument {
    pub name: Option<String>,
    pub num_pages: usize,
    pub page_sizes: Vec<(f64, f64)>, // (width, height)
    pub ust_root: Option<crate::sidebar::USTNode>,
    pub file_size: usize,
    pub version: String,
    pub metadata: fepdf_sdk::MetadataInfo,
    pub security_method: String,
    pub permissions: Option<i32>,
    pub fonts: Vec<fepdf_sdk::FontSummary>,
    pub viewer_direction: Option<String>,
}

pub enum WorkerResponse {
    DocumentLoaded(Box<LoadedDocument>),
    LoadingProgress {
        message: String,
    },
    PageRendered {
        index: usize,
        _scale: f64,
        scene: Arc<Scene>,
        text: Option<String>,
        spans: Option<Vec<crate::interaction::TextSpan>>,
    },
    AuditFindings {
        findings: Vec<(String, String, String, Option<u32>)>,
    },
    DocumentSaved {
        path: std::path::PathBuf,
    },
    Error(String),
}

pub fn run_worker(rx: Receiver<WorkerRequest>, tx: Sender<WorkerResponse>, ctx: egui::Context) {
    // RR-15 Limit: GUI - main routing message loop dispatcher for background worker thread
    let mut current_doc: Option<PdfDocument> = None;
    let system_fonts = VelloBackend::load_system_fonts();
    let mut text_cache = std::collections::BTreeMap::new();
    let mut spans_cache = std::collections::BTreeMap::new();

    for request in rx {
        match request {
            WorkerRequest::Open { data, name } => {
                text_cache.clear();
                spans_cache.clear();
                current_doc = handle_open(data, name, &tx);
                ctx.request_repaint();
            }
            WorkerRequest::RenderPage { index, scale } => {
                handle_render(
                    current_doc.as_ref(),
                    index,
                    scale,
                    &tx,
                    Arc::clone(&system_fonts),
                    &mut text_cache,
                    &mut spans_cache,
                );
                ctx.request_repaint();
            }
            WorkerRequest::UpdateNode { handle_id, tag, alt_text } => {
                text_cache.clear();
                spans_cache.clear();
                handle_update_node(&mut current_doc, handle_id, tag, alt_text, &tx);
                ctx.request_repaint();
            }
            WorkerRequest::Save {
                path,
                compress,
                linearize,
                vacuum,
                upgrade_pdf20,
                redaction_zones,
                cert_path,
                cert_password,
                signature_position,
            } => {
                text_cache.clear();
                spans_cache.clear();
                handle_save(
                    current_doc.as_ref(),
                    path,
                    compress,
                    linearize,
                    vacuum,
                    upgrade_pdf20,
                    redaction_zones,
                    cert_path,
                    cert_password,
                    signature_position,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::Audit => {
                handle_audit(current_doc.as_ref(), &tx);
                ctx.request_repaint();
            }
            WorkerRequest::ReorderPages { from, to } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc
                    && let Err(e) = doc.reorder_page(from, to)
                {
                    log::error!("Failed to reorder page in worker: {e:?}");
                }
                ctx.request_repaint();
            }
            WorkerRequest::RemovePages { mut indices } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc {
                    indices.sort_unstable_by(|a, b| b.cmp(a));
                    for idx in indices {
                        if let Err(e) = doc.remove_page(idx) {
                            log::error!("Failed to remove page {idx} in worker: {e:?}");
                        }
                    }
                }
                ctx.request_repaint();
            }
            WorkerRequest::DuplicatePage { index } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc
                    && let Err(e) = doc.duplicate_page(index)
                {
                    log::error!("Failed to duplicate page {index} in worker: {e:?}");
                }
                ctx.request_repaint();
            }
            WorkerRequest::InsertDocument { data, at_index } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc {
                    handle_insert_document(doc, data, at_index, &tx);
                } else {
                    current_doc = handle_open(data, None, &tx);
                }
                ctx.request_repaint();
            }
            WorkerRequest::RotatePages { indices, delta } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc
                    && let Err(e) = doc.apply(fepdf_sdk::Operation::Rotate {
                        pages: fepdf_sdk::PageSelection::Indices(indices),
                        mode: fepdf_sdk::RotateMode::Relative(delta),
                    })
                {
                    log::error!("Failed to rotate pages in worker: {e:?}");
                }
                ctx.request_repaint();
            }
            WorkerRequest::ExtractPages { indices } => {
                if let Some(ref doc) = current_doc
                    && let Ok(extracted_doc) = doc.extract_pages(indices)
                {
                    let mut temp_path = std::env::temp_dir();
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis());
                    temp_path.push(format!("fepdf_extracted_{timestamp}.pdf"));

                    if extracted_doc.save_as_version(&temp_path, "1.7").is_ok()
                        && let Ok(exe) = std::env::current_exe()
                    {
                        let _ = std::process::Command::new(exe).arg(&temp_path).spawn();
                    }
                }
                ctx.request_repaint();
            }
        }
    }
}

fn resolve_struct_tree_root(
    doc: &PdfDocument,
    _next_id: &mut usize,
) -> Option<crate::sidebar::USTNode> {
    doc.extract_struct_tree()
}

fn handle_insert_document(
    // RR-15 Limit: Dispatcher - handles page insertion from external document in worker thread
    doc: &mut PdfDocument,
    data: Bytes,
    at_index: usize,
    tx: &Sender<WorkerResponse>,
) {
    let options = fepdf_sdk::IngestionOptions::default();
    match PdfDocument::open_with_options(data, &options) {
        Ok(source_doc) => {
            if let Err(e) = doc.insert_pages_from(&source_doc, at_index) {
                log::error!("Failed to insert pages in worker: {e:?}");
                let _ = tx.send(WorkerResponse::Error(format!("Failed to insert pages: {e:?}")));
                return;
            }
            let num_pages = doc.page_count().unwrap_or(0);
            let mut page_sizes = Vec::with_capacity(num_pages);
            for i in 0..num_pages {
                page_sizes.push(doc.get_page_size(i).unwrap_or((595.0, 842.0)));
            }

            let mut next_id = 0;
            let mut ust_root = resolve_struct_tree_root(doc, &mut next_id);

            if ust_root.is_none() {
                ust_root = Some(crate::sidebar::USTNode {
                    id: 0,
                    tag: "Document".to_string(),
                    title: "PDF Document Catalog (Untagged)".to_string(),
                    alt_text: None,
                    rect: None,
                    page_index: None,
                    handle_index: None,
                    children: Vec::new(),
                });
            }

            let version = doc.get_summary().ok().map_or_else(|| "1.7".to_string(), |s| s.version);
            let metadata = doc.metadata();
            let security_method = doc.security_method();
            let permissions = doc.permissions();
            let fonts = doc.fonts();
            let viewer_direction = doc.viewer_direction();

            let _ = tx.send(WorkerResponse::DocumentLoaded(Box::new(LoadedDocument {
                name: None,
                num_pages,
                page_sizes,
                ust_root,
                file_size: 0,
                version,
                metadata,
                security_method,
                permissions,
                fonts,
                viewer_direction,
            })));
        }
        Err(e) => {
            log::error!("Failed to open dropped document for insertion: {e:?}");
            let _ =
                tx.send(WorkerResponse::Error(format!("Failed to open dropped document: {e:?}")));
        }
    }
}

fn handle_open(
    // RR-15 Limit: Dispatcher - handles open document worker requests and packages file properties
    data: Bytes,
    name: Option<String>,
    tx: &Sender<WorkerResponse>,
) -> Option<PdfDocument> {
    let file_size = data.len();
    let tx_clone = tx.clone();
    let options = fepdf_sdk::IngestionOptions {
        progress_callback: Some(Arc::new(move |msg| {
            let _ = tx_clone.send(WorkerResponse::LoadingProgress { message: msg });
        })),
        ..fepdf_sdk::IngestionOptions::default()
    };
    match PdfDocument::open_with_options(data, &options) {
        Ok(doc) => {
            let num_pages = doc.page_count().unwrap_or(0);
            let mut page_sizes = Vec::with_capacity(num_pages);
            for i in 0..num_pages {
                page_sizes.push(doc.get_page_size(i).unwrap_or((595.0, 842.0)));
            }

            let mut next_id = 0;
            let mut ust_root = resolve_struct_tree_root(&doc, &mut next_id);

            if ust_root.is_none() {
                ust_root = Some(crate::sidebar::USTNode {
                    id: 0,
                    tag: "Document".to_string(),
                    title: "PDF Document Catalog (Untagged)".to_string(),
                    alt_text: None,
                    rect: None,
                    page_index: None,
                    handle_index: None,
                    children: Vec::new(),
                });
            }

            let version = doc.get_summary().ok().map_or_else(|| "1.7".to_string(), |s| s.version);
            let metadata = doc.metadata();
            let security_method = doc.security_method();
            let permissions = doc.permissions();
            let fonts = doc.fonts();
            let mut viewer_direction = doc.viewer_direction();

            if viewer_direction.is_none() {
                // Heuristic 1: Check fonts for CJK vertical layout
                let has_vertical_font = fonts.iter().any(|f| {
                    f.name.ends_with("-V") || f.name.contains("-V-") || f.name.contains("-V_")
                });

                let is_japanese_lang =
                    doc.language().is_some_and(|lang| lang.to_lowercase().starts_with("ja"));

                if has_vertical_font || is_japanese_lang {
                    viewer_direction = Some("R2L".to_string());
                }
            }

            let _ = tx.send(WorkerResponse::DocumentLoaded(Box::new(LoadedDocument {
                name,
                num_pages,
                page_sizes,
                ust_root,
                file_size,
                version,
                metadata,
                security_method,
                permissions,
                fonts,
                viewer_direction,
            })));
            Some(doc)
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Error(format!("Failed to load PDF: {e}")));
            None
        }
    }
}

fn get_or_extract_text(
    doc: &PdfDocument,
    index: usize,
    cache: &mut std::collections::BTreeMap<usize, String>,
) -> Option<String> {
    if let Some(cached) = cache.get(&index) {
        return Some(cached.clone());
    }
    let text = doc.extract_text(index).ok();
    if let Some(ref t) = text {
        cache.insert(index, t.clone());
    }
    text
}

fn get_or_extract_spans(
    doc: &PdfDocument,
    index: usize,
    cache: &mut std::collections::BTreeMap<usize, Vec<crate::interaction::TextSpan>>,
) -> Option<Vec<crate::interaction::TextSpan>> {
    if let Some(cached) = cache.get(&index) {
        return Some(cached.clone());
    }
    let spans: Option<Vec<crate::interaction::TextSpan>> =
        doc.extract_spans(index).ok().map(|sdk_spans| {
            sdk_spans
                .into_iter()
                .map(|s| crate::interaction::TextSpan {
                    text: s.text,
                    rect: egui::Rect::from_two_pos(
                        egui::pos2(s.x as f32, s.y as f32),
                        egui::pos2((s.x + s.width) as f32, (s.y + s.font_size) as f32),
                    ),
                })
                .collect()
        });
    if let Some(ref s) = spans {
        cache.insert(index, s.clone());
    }
    spans
}

fn handle_render(
    doc_opt: Option<&PdfDocument>,
    index: usize,
    scale: f64,
    tx: &Sender<WorkerResponse>,
    system_fonts: Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    text_cache: &mut std::collections::BTreeMap<usize, String>,
    spans_cache: &mut std::collections::BTreeMap<usize, Vec<crate::interaction::TextSpan>>,
) {
    let Some(doc) = doc_opt else { return };
    let r =
        doc.get_page_box(index).unwrap_or_else(|_| fepdf_sdk::Rect::new(0.0, 0.0, 595.0, 842.0));
    let w = (r.x2 - r.x1).abs();
    let h = (r.y2 - r.y1).abs();
    let rot = doc.get_page_rotation(index).unwrap_or(0);

    let initial_transform = match rot {
        90 => kurbo::Affine::new([0.0, scale, -scale, 0.0, h * scale, 0.0]),
        180 => kurbo::Affine::new([-scale, 0.0, 0.0, scale, w * scale, 0.0]),
        270 => kurbo::Affine::new([0.0, -scale, scale, 0.0, 0.0, w * scale]),
        _ => kurbo::Affine::new([scale, 0.0, 0.0, -scale, 0.0, h * scale]),
    };
    let mut backend = VelloBackend::new(system_fonts);

    let text = get_or_extract_text(doc, index, text_cache);
    let spans = get_or_extract_spans(doc, index, spans_cache);

    if matches!(doc.render_page(index, &mut backend, initial_transform), Ok(())) {
        let scene = Arc::new(backend.scene().clone());
        let _ = tx.send(WorkerResponse::PageRendered { index, _scale: scale, scene, text, spans });
    } else {
        let _ = tx.send(WorkerResponse::Error(format!("Failed to render page {index}")));
    }
}

fn handle_audit(doc_opt: Option<&PdfDocument>, tx: &Sender<WorkerResponse>) {
    let Some(doc) = doc_opt else { return };
    let audit_findings = doc
        .audit_ua2()
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f.checkpoint, f.severity, f.message, f.handle_id))
        .collect();
    let _ = tx.send(WorkerResponse::AuditFindings { findings: audit_findings });
}

fn handle_update_node(
    doc_opt: &mut Option<PdfDocument>,
    handle_id: u32,
    tag: String,
    alt_text: Option<String>,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc_opt else { return };
    let _ = doc.apply(fepdf_sdk::Operation::UpdateStructElem(fepdf_sdk::StructElemUpdate {
        handle_index: handle_id,
        new_tag: Some(tag),
        new_alt: alt_text,
    }));

    // Run Matterhorn compliance audit on updated tree
    let findings = doc
        .audit_ua2()
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f.checkpoint, f.severity, f.message, f.handle_id))
        .collect();
    let _ = tx.send(WorkerResponse::AuditFindings { findings });
}

fn handle_save(
    // RR-15 Limit: Dispatcher - Thread pool worker saving request routing dispatcher handling signatures, redactions and compression saving options
    doc_opt: Option<&PdfDocument>,
    path: std::path::PathBuf,
    compress: bool,
    linearize: bool,
    vacuum: bool,
    upgrade_pdf20: bool,
    redaction_zones: Vec<crate::redaction::RedactionZone>,
    cert_path: Option<std::path::PathBuf>,
    _cert_password: String,
    signature_position: Option<(usize, [f32; 4])>,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc_opt else {
        let _ = tx.send(WorkerResponse::Error("No document loaded to save".to_string()));
        return;
    };

    // 1. Group redaction zones by page index
    let mut page_redactions: std::collections::BTreeMap<usize, Vec<[f32; 4]>> =
        std::collections::BTreeMap::new();
    for zone in redaction_zones {
        let rect_arr = [zone.rect.min.x, zone.rect.min.y, zone.rect.max.x, zone.rect.max.y];
        page_redactions.entry(zone.page_index).or_default().push(rect_arr);
    }

    // 2. Apply physical stream sanitization to each page mutably
    for (page_idx, rects) in page_redactions {
        if let Err(e) = doc.apply_redaction_to_page(page_idx, &rects) {
            let _ = tx.send(WorkerResponse::Error(format!(
                "Failed physically redacting page {page_idx}: {e}"
            )));
            return;
        }
    }

    let version = if upgrade_pdf20 { "2.0" } else { "1.7" };
    let options = fepdf_sdk::SaveOptions {
        compress,
        compression_level: 6,
        vacuum,
        ..fepdf_sdk::SaveOptions::default()
    };

    let res = if let Some(cp) = cert_path {
        // Read certificate file bytes
        let cert_bytes = std::fs::read(&cp).unwrap_or_default();
        let sign_opts = fepdf_sdk::SignOptions {
            reason: Some("Signed via fepdf Production Studio".to_string()),
            location: Some("Tokyo, Japan".to_string()),
            contact_info: Some("support@fepdf.dev".to_string()),
            name: Some("fepdf Digital Signer".to_string()),
            certificate: Some(cert_bytes.clone()),
            private_key: Some(cert_bytes),
            page_index: signature_position.map_or(0, |(idx, _)| idx),
            rect: signature_position.map_or([50.0, 50.0, 200.0, 100.0], |(_, rect)| rect),
        };
        doc.save_signed(&path, version, &options, &sign_opts)
    } else if linearize {
        doc.save_linearized(&path, version, &options)
    } else {
        doc.save_with_options(&path, version, &options)
    };

    match res {
        Ok(()) => {
            let _ = tx.send(WorkerResponse::DocumentSaved { path });
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Error(format!("Failed to save PDF: {e}")));
        }
    }
}
