//! Off-thread document worker and request/response dispatch loop.

use bytes::Bytes;
use fepdf::{FallbackFontType, VelloBackend};
use fepdf::{Operation, PageSelection, PdfDocument};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use vello::Scene;

#[allow(dead_code)]
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
        key_path: Option<std::path::PathBuf>,
        signature_position: Option<(usize, [f32; 4])>,
    },
    Audit,
    /// 6.3.2.3: a person turning a layer on or off. Not a document edit — the worker
    /// re-renders and the saved bytes are unchanged.
    SetLayerVisible {
        layer: fepdf::LayerId,
        on: bool,
    },
    ReorderPagesBatch {
        source_indices: Vec<usize>,
        target_insert_pos: usize,
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
    ReplaceDocument {
        data: Bytes,
        at_index: usize,
        count: usize,
    },
    RotatePages {
        indices: Vec<usize>,
        delta: fepdf::Quarter,
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
    pub metadata: fepdf::MetadataInfo,
    pub security_method: String,
    pub permissions: Option<i32>,
    pub fonts: Vec<fepdf::FontSummary>,
    pub viewer_direction: Option<String>,
    /// What to present for optional content, per `/Order` (8.11.4.3). Empty when the
    /// document has no layers *or* when its configuration lists none — the clause makes
    /// those the same answer.
    pub layers: Vec<fepdf::LayerRow>,
    /// Reading decisions recorded by the engine while opening or repairing the document (6.3.2.3).
    pub decisions: Vec<fepdf::Decision>,
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
    /// A layer was toggled: the panel's states have moved and the page needs redrawing.
    LayersChanged {
        layers: Vec<fepdf::LayerRow>,
    },
    DocumentSaved {
        path: std::path::PathBuf,
        /// What the write cost, in the document's own terms. Empty for most files;
        /// non-empty when the source declared restrictions the output cannot carry,
        /// which the user is about to hand to someone else (7.6.4.2).
        notices: Vec<String>,
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
                key_path,
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
                    key_path,
                    signature_position,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::Audit => {
                handle_audit(current_doc.as_ref(), &tx);
                ctx.request_repaint();
            }
            WorkerRequest::SetLayerVisible { layer, on } => {
                if let Some(ref doc) = current_doc {
                    // The panel is re-read rather than cached: it carries `/Locked` and
                    // `/RBGroups`, and it is what refuses a locked group rather than the
                    // UI being trusted to have disabled the row.
                    let panel = doc.layers();
                    if doc.set_layer_visible(&panel, layer, on) {
                        // What is drawn changed, so every cached page is stale.
                        text_cache.clear();
                        spans_cache.clear();
                        let _ =
                            tx.send(WorkerResponse::LayersChanged { layers: doc.layers().rows });
                    }
                }
                ctx.request_repaint();
            }
            WorkerRequest::ReorderPagesBatch { source_indices, target_insert_pos } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc
                    && let Err(e) = doc.apply(Operation::ReorderBatch {
                        sources: source_indices,
                        target: target_insert_pos,
                    })
                {
                    log::error!("Failed to batch reorder pages in worker: {e:?}");
                }
                ctx.request_repaint();
            }
            WorkerRequest::RemovePages { mut indices } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc {
                    // One operation, not a descending loop. Sorting the indices so that
                    // removing one did not move the next was the frontend doing the
                    // engine's arithmetic; `RemovePages` takes the set and owns the order.
                    indices.sort_unstable();
                    indices.dedup();
                    if let Err(e) =
                        doc.apply(Operation::RemovePages(PageSelection::Indices(indices)))
                    {
                        log::error!("Failed to remove pages in worker: {e:?}");
                    }
                }
                ctx.request_repaint();
            }
            WorkerRequest::DuplicatePage { index } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc
                    && let Err(e) =
                        doc.apply(Operation::DuplicatePages(PageSelection::Single(index)))
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
            WorkerRequest::ReplaceDocument { data, at_index, count } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc {
                    handle_replace_document(doc, data, at_index, count, &tx);
                }
                ctx.request_repaint();
            }
            WorkerRequest::RotatePages { indices, delta } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc
                    && let Err(e) = doc.apply(fepdf::Operation::Rotate {
                        pages: fepdf::PageSelection::Indices(indices),
                        mode: fepdf::RotateMode::Relative(delta),
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

/// Re-reads everything the UI shows about a document, after its page set changed.
///
/// Both handlers below had their own copy of this — forty lines each, identical but for
/// the error string. Two copies of "what the UI needs to know" is how one of them comes to
/// answer a question the other does not.
fn reload_after_page_change(doc: &PdfDocument, tx: &Sender<WorkerResponse>) {
    let num_pages = doc.page_count().unwrap_or(0);
    let mut page_sizes = Vec::with_capacity(num_pages);
    for i in 0..num_pages {
        page_sizes.push(doc.get_page_size(i).unwrap_or((595.0, 842.0)));
    }

    let mut next_id = 0;
    let ust_root = resolve_struct_tree_root(doc, &mut next_id).or_else(|| {
        Some(crate::sidebar::USTNode {
            id: 0,
            tag: "Document".to_string(),
            title: "PDF Document Catalog (Untagged)".to_string(),
            alt_text: None,
            rect: None,
            page_index: None,
            handle_index: None,
            children: Vec::new(),
        })
    });

    let version = doc.get_summary().ok().map_or_else(|| "1.7".to_string(), |s| s.version);
    let _ = tx.send(WorkerResponse::DocumentLoaded(Box::new(LoadedDocument {
        name: None,
        num_pages,
        page_sizes,
        ust_root,
        file_size: 0,
        version,
        metadata: doc.metadata(),
        security_method: doc.security_method(),
        permissions: doc.permissions(),
        fonts: doc.fonts(),
        viewer_direction: doc.viewer_direction(),
        layers: doc.layers().rows,
        decisions: doc.decisions(),
    })));
}

/// Inserts every page of a dropped document at `at_index`.
///
/// The source is handed over as bytes and opened inside `apply`. This used to open it
/// here and call `PdfDocument::insert_pages_from`, which is a mutation outside the
/// `Operation` vocabulary — Rule D, and one of the eight sites that had left it.
fn handle_insert_document(
    doc: &mut PdfDocument,
    data: Bytes,
    at_index: usize,
    tx: &Sender<WorkerResponse>,
) {
    if let Err(e) = doc.apply(Operation::InsertFrom { source: data.to_vec(), at: at_index }) {
        log::error!("Failed to insert pages in worker: {e:?}");
        let _ = tx.send(WorkerResponse::Error(format!("Failed to insert pages: {e:?}")));
        return;
    }
    reload_after_page_change(doc, tx);
}

/// Replaces `count` pages at `at_index` with every page of a dropped document.
///
/// Two operations, in the order the name says: the removal first, so the insertion lands
/// where the removed run was.
fn handle_replace_document(
    doc: &mut PdfDocument,
    data: Bytes,
    at_index: usize,
    count: usize,
    tx: &Sender<WorkerResponse>,
) {
    let page_count = doc.page_count().unwrap_or(0);
    let doomed: Vec<usize> = (at_index..at_index + count).filter(|i| *i < page_count).collect();
    if !doomed.is_empty()
        && let Err(e) = doc.apply(Operation::RemovePages(PageSelection::Indices(doomed)))
    {
        log::error!("Failed to remove pages being replaced: {e:?}");
        let _ = tx.send(WorkerResponse::Error(format!("Failed to replace pages: {e:?}")));
        return;
    }
    if let Err(e) = doc.apply(Operation::InsertFrom { source: data.to_vec(), at: at_index }) {
        log::error!("Failed to replace pages in worker: {e:?}");
        let _ = tx.send(WorkerResponse::Error(format!("Failed to replace pages: {e:?}")));
        return;
    }
    reload_after_page_change(doc, tx);
}

/// Whether a document with no declared reading direction is a vertically set CJK one.
///
/// **Table 30 gives `/Direction` a default of L2R, and almost nothing declares it** — 2
/// files of 524, `samples/bokutokitan.pdf` saying `R2L` and one external file `L2R`. So a
/// document that is set vertically and says nothing gets bound the wrong way round unless
/// something looks at it, and `samples/fy05.pdf` is exactly that: six fonts in a vertical
/// writing mode and no declaration.
///
/// It is a guess and is recorded as one. [ADR-0041] settled the shape: where a file
/// declares, obey it; where it declares nothing, a heuristic is what there is to go on,
/// and it says so. `-V` is the writing-mode suffix Adobe's CMap names carry for vertical
/// forms (9.7.5.2), which is why a font name is evidence at all.
///
/// [ADR-0041]: ../../docs/adr/0041-a-character-collection-is-declared-not-guessed.md
fn infer_binding(fonts: &[fepdf::FontSummary], lang: Option<&str>) -> Option<String> {
    let _ = lang;
    let vertical = fonts.iter().filter(|f| f.is_vertical).count();
    let share = vertical as f32 / fonts.len().max(1) as f32;
    (share >= VERTICAL_SHARE).then(|| "R2L".to_string())
}

/// What proportion of a document's fonts must be set vertically before it is taken to be a
/// vertically set book.
///
/// **Presence was not enough.** The test was whether *any* font carried the writing-mode
/// suffix, and `samples/fy05.pdf` — a horizontally set government report — has 6 of them
/// among **316**, in a table or on its cover. 1.9% was enough to bind it right to left.
/// `samples/bokutokitan.pdf`, which is a vertically set book, is 4 of 18: **22%**.
///
/// **The threshold is fitted to those two documents and one of them is unreachable.**
/// `bokutokitan.pdf` declares `R2L` itself, so the guess is never asked about it; the only
/// document in either corpus that this function is consulted for is `fy05.pdf`, where the
/// answer is that it is not vertical. There is no case here where the heuristic is known to
/// be right, only one where it is now known not to be wrong.
const VERTICAL_SHARE: f32 = 0.10;

fn handle_open(
    // RR-15 Limit: Dispatcher - handles open document worker requests and packages file properties
    data: Bytes,
    name: Option<String>,
    tx: &Sender<WorkerResponse>,
) -> Option<PdfDocument> {
    let file_size = data.len();
    let tx_clone = tx.clone();
    let options = fepdf::IngestionOptions {
        progress_callback: Some(Arc::new(move |msg| {
            let _ = tx_clone.send(WorkerResponse::LoadingProgress { message: msg });
        })),
        ..fepdf::IngestionOptions::default()
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
            let mut decisions = doc.decisions();
            let mut viewer_direction = doc.viewer_direction();
            if viewer_direction.is_none()
                && let Some(inferred) = infer_binding(&fonts, doc.language().as_deref())
            {
                decisions.push(fepdf::Decision::ambiguity(
                    "12.2",
                    "the document declares no /ViewerPreferences /Direction and is set in \
                     vertical CJK: its fonts name a `-V` writing mode, or its /Lang is \
                     Japanese",
                    "bound it right to left rather than taking Table 30's default of L2R, \
                     which opens a vertically set book at the wrong end",
                ));
                viewer_direction = Some(inferred);
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
                layers: doc.layers().rows,
                decisions,
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
    let r = doc.get_page_box(index).unwrap_or_else(|_| fepdf::Rect::new(0.0, 0.0, 595.0, 842.0));
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

    match doc.render_page(index, &mut backend, initial_transform) {
        Ok(()) => {
            let scene = Arc::new(backend.scene().clone());
            let _ =
                tx.send(WorkerResponse::PageRendered { index, _scale: scale, scene, text, spans });
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Error(format!("Failed to render page {index}: {e}")));
        }
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
    let _ = doc.apply(fepdf::Operation::UpdateStructElem(fepdf::StructElemUpdate {
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
    key_path: Option<std::path::PathBuf>,
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
    let options = fepdf::SaveOptions {
        compress,
        compression_level: 6,
        vacuum,
        ..fepdf::SaveOptions::default()
    };

    let res = if let (Some(certificate), Some(key)) = (cert_path, key_path) {
        // Read as-is and let the engine judge them. Reporting "not a PKCS#8 key" from
        // the layer that knows what a key is beats guessing here, and `unwrap_or_default`
        // used to turn an unreadable file into empty bytes and carry on.
        let read =
            |p: &std::path::Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
        match (read(&certificate), read(&key)) {
            (Ok(certificate), Ok(key)) => {
                let sign_opts = fepdf::SignOptions {
                    // Rule D: a frontend translates. The reason and location used to be
                    // invented here — "Tokyo, Japan", a support address nobody gave —
                    // and were signed into the document as if the user had said them.
                    certificate: Some(certificate),
                    private_key: Some(key),
                    page_index: signature_position.map_or(0, |(idx, _)| idx),
                    ..fepdf::SignOptions::default()
                };
                doc.save_signed(&path, version, &options, &sign_opts)
            }
            (Err(e), _) | (_, Err(e)) => {
                let _ = tx.send(WorkerResponse::Error(format!("Failed to save PDF: {e}")));
                return;
            }
        }
    } else if linearize {
        doc.save_linearized(&path, version, &options)
    } else {
        doc.save_with_options(&path, version, &options)
    };

    match res {
        Ok(decisions) => {
            let notices = decisions.iter().map(ToString::to_string).collect();
            let _ = tx.send(WorkerResponse::DocumentSaved { path, notices });
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Error(format!("Failed to save PDF: {e}")));
        }
    }
}

#[cfg(test)]
mod binding_direction {
    //! **Two files of 524 declare `/ViewerPreferences /Direction`**, so for almost every
    //! document Table 30's default of L2R is what applies and a vertically set book opens
    //! at the wrong end unless something looks at it.
    //!
    //! **The rule is vertical setting, not the Japanese language.** Japanese is set
    //! horizontally far more often than not — reports, papers, manuals — and binding
    //! follows the setting. Testing `/Lang` bound every Japanese document right to left,
    //! which is how `samples/fy05.pdf`, a horizontally set government report, came to open
    //! from the wrong end and lay its tiles out mirrored.
    //!
    //! **Presence of a vertical font was not enough either.** `fy05.pdf` has 6 of them
    //! among 316 — 1.9%, a table or a cover — against `samples/bokutokitan.pdf`'s 4 of 18,
    //! or 22%.
    //!
    //! **And the evidence is the declaration, not the name.** This used to look for `-V` in
    //! the font's name, which is a naming convention; `FontSummary::is_vertical` carries
    //! what `detect_wmode` reads from the encoding's CMap — the same reading the
    //! interpreter uses to place a glyph (9.7.5.2). It is a guess and is recorded as one
    //! where the reader can see it.
    use super::infer_binding;

    /// `vertical` fonts set vertically, out of `total`. The names are deliberately
    /// uninformative: what decides is the declared writing mode, and a test that fed the
    /// answer in through the name would be testing the old rule.
    fn fonts(vertical: usize, total: usize) -> Vec<fepdf::FontSummary> {
        (0..total)
            .map(|i| fepdf::FontSummary {
                name: format!("Font{i}"),
                font_type: "Type0".to_string(),
                is_embedded: true,
                is_type3: false,
                is_subset: false,
                encoding: if i < vertical { "Identity-V" } else { "Identity-H" }.to_string(),
                has_to_unicode: false,
                is_vertical: i < vertical,
                object_id: 0,
            })
            .collect()
    }

    /// A document set vertically opens from the right.
    #[test]
    fn a_document_set_vertically_binds_right_to_left() {
        assert_eq!(infer_binding(&fonts(4, 18), None).as_deref(), Some("R2L"));
        assert_eq!(infer_binding(&fonts(18, 18), None).as_deref(), Some("R2L"));
    }

    /// **The measured case.** `samples/bokutokitan.pdf` is 4 vertical fonts of 18 and is a
    /// vertically set book; `samples/fy05.pdf` is 6 of 316 and is a horizontally set report
    /// with a vertical table in it. A test for presence answered R2L for both.
    #[test]
    fn a_few_vertical_fonts_in_a_horizontal_document_are_not_a_vertical_document() {
        assert_eq!(infer_binding(&fonts(6, 316), Some("ja-JP")), None, "6 of 316 is not");
        assert_eq!(infer_binding(&fonts(4, 18), None).as_deref(), Some("R2L"), "4 of 18 is");
    }

    /// **The language is not the evidence.** Japanese is set horizontally more often than
    /// not, and binding follows the setting. Testing `/Lang` bound every Japanese document
    /// right to left.
    #[test]
    fn the_language_alone_decides_nothing() {
        assert_eq!(infer_binding(&fonts(0, 12), Some("ja-JP")), None);
        assert_eq!(infer_binding(&fonts(4, 18), Some("en-US")).as_deref(), Some("R2L"));
    }

    /// **The evidence is the declaration, not the name.** The rule used to look for `-V` in
    /// the font's name, so a font *named* for a vertical CMap decided it and a font that
    /// merely declared one did not. `NotoSerif-Vietnamese` was the control for the first
    /// half; this is the control for the second.
    #[test]
    fn a_name_decides_nothing_either_way() {
        let mut named_vertical = fonts(0, 4);
        for f in &mut named_vertical {
            f.name = "KozMinPr6N-Regular-V".to_string();
        }
        assert_eq!(infer_binding(&named_vertical, None), None, "a name is not a declaration");

        let mut declared_vertical = fonts(4, 4);
        for f in &mut declared_vertical {
            f.name = "NotoSerif-Vietnamese".to_string();
        }
        assert_eq!(
            infer_binding(&declared_vertical, None).as_deref(),
            Some("R2L"),
            "and a declaration stands whatever the font is called"
        );
    }

    /// The control. Without it, "vertical CJK binds right to left" and "everything binds
    /// right to left" are the same green test.
    #[test]
    fn anything_else_is_left_alone_for_the_standard_default() {
        assert_eq!(infer_binding(&fonts(0, 1), None), None);
        assert_eq!(infer_binding(&fonts(0, 40), Some("en-US")), None);
        assert_eq!(infer_binding(&[], None), None, "and a document with no fonts at all");
    }
}
