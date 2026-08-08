#![allow(
    clippy::collapsible_if,
    clippy::match_result_ok,
    clippy::too_many_arguments,
    clippy::large_enum_variant
)]

use bytes::Bytes;
use ferruginous_render::{FallbackFontType, VelloBackend};
use ferruginous_sdk::PdfDocument;
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
    InsertDocument {
        data: Bytes,
        at_index: usize,
    },
}

pub enum WorkerResponse {
    DocumentLoaded {
        name: Option<String>,
        num_pages: usize,
        page_sizes: Vec<(f64, f64)>, // (width, height)
        ust_root: Option<crate::sidebar::USTNode>,
        file_size: usize,
        version: String,
        metadata: ferruginous_core::metadata::MetadataInfo,
        security_method: String,
        permissions: Option<i32>,
        fonts: Vec<ferruginous_core::font::FontSummary>,
        viewer_direction: Option<String>,
    },
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

pub fn run_worker(rx: Receiver<WorkerRequest>, tx: Sender<WorkerResponse>, ctx: egui::Context) { // RR-15 Limit: GUI - main routing message loop dispatcher for background worker thread
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
                    &current_doc,
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
                    &current_doc,
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
                handle_audit(&current_doc, &tx);
                ctx.request_repaint();
            }
            WorkerRequest::ReorderPages { from, to } => {
                text_cache.clear();
                spans_cache.clear();
                if let Some(ref mut doc) = current_doc {
                    if let Err(e) = doc.reorder_page(from, to) {
                        log::error!("Failed to reorder page in worker: {:?}", e);
                    }
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
                            log::error!("Failed to remove page {} in worker: {:?}", idx, e);
                        }
                    }
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
        }
    }
}

use ferruginous_core::{Handle, Object, PdfArena, PdfName};

fn load_page_info(doc: &PdfDocument) -> (Vec<(f64, f64)>, Vec<String>) {
    let num_pages = doc.page_count().unwrap_or(0);
    let mut page_sizes = Vec::with_capacity(num_pages);
    let mut page_texts = Vec::with_capacity(num_pages);
    for i in 0..num_pages {
        page_sizes.push(doc.get_page_size(i).unwrap_or((595.0, 842.0)));
        page_texts.push(doc.extract_text(i).unwrap_or_default());
    }
    (page_sizes, page_texts)
}

fn resolve_to_node_handle(arena: &PdfArena, obj: &Object) -> Option<Handle<Object>> {
    match obj {
        Object::Reference(h) => Some(*h),
        Object::Dictionary(dh) => Some(Handle::new(dh.index())),
        _ => {
            let resolved = obj.resolve(arena);
            match resolved {
                Object::Reference(h) => Some(h),
                Object::Dictionary(dh) => Some(Handle::new(dh.index())),
                _ => None,
            }
        }
    }
}

fn parse_struct_tree_findings(
    doc: &PdfDocument,
    next_id: &mut usize,
) -> (Option<crate::sidebar::USTNode>, Vec<(String, String, String, Option<u32>)>) {
    let mut ust_root = None;
    let mut audit_findings = Vec::new();
    let arena = doc.inner().arena();

    if let Some(cah) = doc.inner().catalog_handle() {
        if let Some(cadh) = doc.inner().resolve_to_dict(cah).ok() {
            if let Some(dict) = arena.get_dict(cadh) {
                let str_root_key = arena.name("StructTreeRoot");
                if let Some(str_root_obj) = dict.get(&str_root_key) {
                    if let Some(str_root_ref) = resolve_to_node_handle(arena, str_root_obj) {
                        let mut visited = std::collections::BTreeSet::new();
                        ust_root = parse_struct_node(arena, str_root_ref, next_id, &mut visited);

                        let auditor = ferruginous_sdk::structure::MatterhornAuditor::new(arena);
                        if let Ok(findings) = auditor.audit(str_root_ref) {
                            for f in findings {
                                audit_findings.push((
                                    f.checkpoint,
                                    f.severity,
                                    f.message,
                                    f.handle_id,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    (ust_root, audit_findings)
}

fn resolve_struct_tree_root(
    doc: &PdfDocument,
    next_id: &mut usize,
) -> Option<crate::sidebar::USTNode> {
    let arena = doc.inner().arena();
    let cah = doc.inner().catalog_handle()?;
    let cadh = doc.inner().resolve_to_dict(cah).ok()?;
    let dict = arena.get_dict(cadh)?;
    let str_root_key = arena.name("StructTreeRoot");
    let str_root_obj = dict.get(&str_root_key)?;
    let str_root_ref = resolve_to_node_handle(arena, str_root_obj)?;
    let mut visited = std::collections::BTreeSet::new();
    parse_struct_node(arena, str_root_ref, next_id, &mut visited)
}

fn handle_insert_document( // RR-15 Limit: Dispatcher - handles page insertion from external document in worker thread
    doc: &mut PdfDocument,
    data: Bytes,
    at_index: usize,
    tx: &Sender<WorkerResponse>,
) {
    let options = ferruginous_core::ingest::IngestionOptions::default();
    match PdfDocument::open_with_options(data, &options) {
        Ok(source_doc) => {
            if let Err(e) = doc.insert_pages_from(&source_doc, at_index) {
                log::error!("Failed to insert pages in worker: {:?}", e);
                let _ = tx.send(WorkerResponse::Error(format!("Failed to insert pages: {:?}", e)));
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
                    handle_id: None,
                    children: Vec::new(),
                });
            }

            let version =
                doc.get_summary().ok().map(|s| s.version).unwrap_or_else(|| "1.7".to_string());
            let metadata = doc.inner().metadata();
            let security_method = doc.inner().security_method.clone();
            let permissions = doc.inner().permissions;
            let fonts = doc.inner().fonts();

            let viewer_direction = if let Some(cah) = doc.inner().catalog_handle() {
                if let Some(cadh) = doc.inner().resolve_to_dict(cah).ok() {
                    if let Some(dict) = doc.inner().arena().get_dict(cadh) {
                        let vp_key = doc.inner().arena().name("ViewerPreferences");
                        if let Some(vp_obj) = dict.get(&vp_key) {
                            if let Some(vp_ref) = resolve_to_node_handle(doc.inner().arena(), vp_obj) {
                                if let Some(vp_dict) = doc.inner().resolve_to_dict(vp_ref).ok().and_then(|h| doc.inner().arena().get_dict(h)) {
                                    let dir_key = doc.inner().arena().name("Direction");
                                    vp_dict.get(&dir_key)
                                        .and_then(|d| d.resolve(doc.inner().arena()).as_name())
                                        .and_then(|nh| doc.inner().arena().get_name(nh))
                                        .map(|n| n.as_str().to_string())
                                } else { None }
                            } else { None }
                        } else { None }
                    } else { None }
                } else { None }
            } else { None };

            let _ = tx.send(WorkerResponse::DocumentLoaded {
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
            });
        }
        Err(e) => {
            log::error!("Failed to open dropped document for insertion: {:?}", e);
            let _ = tx.send(WorkerResponse::Error(format!("Failed to open dropped document: {:?}", e)));
        }
    }
}

fn handle_open( // RR-15 Limit: Dispatcher - handles open document worker requests and packages file properties
    data: Bytes,
    name: Option<String>,
    tx: &Sender<WorkerResponse>,
) -> Option<PdfDocument> {
    let file_size = data.len();
    let tx_clone = tx.clone();
    let options = ferruginous_core::ingest::IngestionOptions {
        progress_callback: Some(Arc::new(move |msg| {
            let _ = tx_clone.send(WorkerResponse::LoadingProgress { message: msg });
        })),
        ..ferruginous_core::ingest::IngestionOptions::default()
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
                    handle_id: None,
                    children: Vec::new(),
                });
            }

            let version =
                doc.get_summary().ok().map(|s| s.version).unwrap_or_else(|| "1.7".to_string());
            let metadata = doc.inner().metadata();
            let security_method = doc.inner().security_method.clone();
            let permissions = doc.inner().permissions;
            let fonts = doc.inner().fonts();

            let mut viewer_direction = if let Some(cah) = doc.inner().catalog_handle() {
                if let Some(cadh) = doc.inner().resolve_to_dict(cah).ok() {
                    if let Some(dict) = doc.inner().arena().get_dict(cadh) {
                        let vp_key = doc.inner().arena().name("ViewerPreferences");
                        if let Some(vp_obj) = dict.get(&vp_key) {
                            if let Some(vp_ref) = resolve_to_node_handle(doc.inner().arena(), vp_obj) {
                                if let Some(vp_dict) = doc.inner().resolve_to_dict(vp_ref).ok().and_then(|h| doc.inner().arena().get_dict(h)) {
                                    let dir_key = doc.inner().arena().name("Direction");
                                    vp_dict.get(&dir_key)
                                        .and_then(|d| d.resolve(doc.inner().arena()).as_name())
                                        .and_then(|nh| doc.inner().arena().get_name(nh))
                                        .map(|n| n.as_str().to_string())
                                } else { None }
                            } else { None }
                        } else { None }
                    } else { None }
                } else { None }
            } else { None };

            if viewer_direction.is_none() {
                // Heuristic 1: Check fonts for CJK vertical layout
                let has_vertical_font = doc.inner().fonts().iter().any(|f| f.name.ends_with("-V") || f.name.contains("-V-") || f.name.contains("-V_"));

                let is_japanese_lang = if let Some(cah) = doc.inner().catalog_handle() {
                    if let Some(cadh) = doc.inner().resolve_to_dict(cah).ok() {
                        if let Some(dict) = doc.inner().arena().get_dict(cadh) {
                            let lang_key = doc.inner().arena().name("Lang");
                            if let Some(lang_obj) = dict.get(&lang_key) {
                                let resolved = lang_obj.resolve(doc.inner().arena());
                                if let Some(bytes) = resolved.as_string() {
                                    let s = String::from_utf8_lossy(bytes).to_lowercase();
                                    s.starts_with("ja")
                                } else { false }
                            } else { false }
                        } else { false }
                    } else { false }
                } else { false };

                if has_vertical_font || is_japanese_lang {
                    viewer_direction = Some("R2L".to_string());
                }
            }

            let _ = tx.send(WorkerResponse::DocumentLoaded {
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
            });
            Some(doc)
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Error(format!("Failed to load PDF: {}", e)));
            None
        }
    }
}

fn parse_bbox_helper(arena: &PdfArena, bbox_obj: &Object) -> Option<[f32; 4]> {
    let array_h = bbox_obj.resolve(arena).as_array()?;
    let arr = arena.get_array(array_h)?;
    if arr.len() != 4 {
        return None;
    }
    let x1 = arr[0].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let y1 = arr[1].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let x2 = arr[2].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let y2 = arr[3].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    Some([x1, y1, x2, y2])
}

fn parse_kids_helper(
    arena: &PdfArena,
    kids_obj: &Object,
    next_id: &mut usize,
    visited: &mut std::collections::BTreeSet<Handle<Object>>,
    children: &mut Vec<crate::sidebar::USTNode>,
) {
    if let Some(kid_ref) = resolve_to_node_handle(arena, kids_obj) {
        if let Some(child_node) = parse_struct_node(arena, kid_ref, next_id, visited) {
            children.push(child_node);
        }
    } else if let Object::Array(ah) = kids_obj.resolve(arena) {
        if let Some(array) = arena.get_array(ah) {
            for kid in array {
                if let Some(kid_ref) = resolve_to_node_handle(arena, &kid) {
                    if let Some(child_node) = parse_struct_node(arena, kid_ref, next_id, visited) {
                        children.push(child_node);
                    }
                }
            }
        }
    }
}

fn parse_tag_helper(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<Handle<PdfName>, Object>,
) -> String {
    let type_key = arena.name("Type");
    let s_key = arena.name("S");

    if let Some(s_obj) = dict.get(&s_key) {
        let resolved: Object = s_obj.resolve(arena);
        if let Some(name_h) = resolved.as_name() {
            arena
                .get_name(name_h)
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "P".to_string())
        } else {
            "P".to_string()
        }
    } else {
        let type_val = dict.get(&type_key).and_then(|t: &Object| t.resolve(arena).as_name());
        if let Some(tv) = type_val {
            if arena.get_name(tv).map(|n| n.as_str() == "StructTreeRoot").unwrap_or(false) {
                "Document".to_string()
            } else {
                "P".to_string()
            }
        } else {
            "P".to_string()
        }
    }
}

fn parse_alt_text_helper(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<Handle<PdfName>, Object>,
) -> Option<String> {
    let alt_key = arena.name("Alt");
    let alt_obj = dict.get(&alt_key)?;
    let resolved = alt_obj.resolve(arena);
    let bytes = resolved.as_string()?;
    String::from_utf8(bytes.to_vec()).ok()
}

fn parse_struct_node(
    arena: &PdfArena,
    handle: Handle<Object>,
    next_id: &mut usize,
    visited: &mut std::collections::BTreeSet<Handle<Object>>,
) -> Option<crate::sidebar::USTNode> {
    if !visited.insert(handle) {
        return None;
    }
    let obj = arena.get_object(handle)?;
    let dh = obj.as_dict_handle()?;
    let dict = arena.get_dict(dh)?;

    let tag = parse_tag_helper(arena, &dict);
    let title = tag.clone();
    let alt_text = parse_alt_text_helper(arena, &dict);

    let rect = dict.get(&arena.name("BBox")).and_then(|b| parse_bbox_helper(arena, b));

    let id = *next_id;
    *next_id += 1;

    let mut children = Vec::new();
    if let Some(kids) = dict.get(&arena.name("K")) {
        parse_kids_helper(arena, kids, next_id, visited, &mut children);
    }

    visited.remove(&handle);

    Some(crate::sidebar::USTNode {
        id,
        tag,
        title,
        alt_text,
        rect,
        handle_id: Some(handle.index()),
        children,
    })
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
    doc_opt: &Option<PdfDocument>,
    index: usize,
    scale: f64,
    tx: &Sender<WorkerResponse>,
    system_fonts: Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    text_cache: &mut std::collections::BTreeMap<usize, String>,
    spans_cache: &mut std::collections::BTreeMap<usize, Vec<crate::interaction::TextSpan>>,
) {
    let Some(doc) = doc_opt else { return };
    let (_, p_h) = doc.get_page_size(index).unwrap_or((595.0, 842.0));
    let mut backend = VelloBackend::new(system_fonts);
    let initial_transform = kurbo::Affine::new([scale, 0.0, 0.0, -scale, 0.0, p_h * scale]);

    let text = get_or_extract_text(doc, index, text_cache);
    let spans = get_or_extract_spans(doc, index, spans_cache);

    if let Ok(()) = doc.render_page(index, &mut backend, initial_transform) {
        let scene = Arc::new(backend.scene().clone());
        let _ = tx.send(WorkerResponse::PageRendered { index, _scale: scale, scene, text, spans });
    } else {
        let _ = tx.send(WorkerResponse::Error(format!("Failed to render page {}", index)));
    }
}

fn handle_audit(doc_opt: &Option<PdfDocument>, tx: &Sender<WorkerResponse>) {
    let Some(doc) = doc_opt else { return };
    let mut audit_findings = Vec::new();
    let arena = doc.inner().arena();

    if let Some(cah) = doc.inner().catalog_handle() {
        if let Some(cadh) = doc.inner().resolve_to_dict(cah).ok() {
            if let Some(dict) = arena.get_dict(cadh) {
                let str_root_key = arena.name("StructTreeRoot");
                if let Some(str_root_obj) = dict.get(&str_root_key) {
                    if let Some(str_root_ref) = str_root_obj.resolve(arena).as_reference() {
                        let auditor = ferruginous_sdk::structure::MatterhornAuditor::new(arena);
                        if let Ok(findings) = auditor.audit(str_root_ref) {
                            for f in findings {
                                audit_findings.push((
                                    f.checkpoint,
                                    f.severity,
                                    f.message,
                                    f.handle_id,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
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
    let arena = doc.inner().arena();
    let handle = Handle::<Object>::new(handle_id);

    if let Some(Object::Dictionary(dh)) = arena.get_object(handle) {
        if let Some(mut dict) = arena.get_dict(dh) {
            // Update tag Subtype /S
            let s_key = arena.name("S");
            dict.insert(s_key, Object::Name(arena.name(&tag)));

            // Update Alt-text
            let alt_key = arena.name("Alt");
            if let Some(alt) = alt_text {
                dict.insert(alt_key, Object::String(bytes::Bytes::from(alt)));
            } else {
                dict.remove(&alt_key);
            }
            arena.set_dict(dh, dict);
        }
    }

    // Run Matterhorn compliance audit on updated tree
    let mut findings = Vec::new();
    if let Some(cah) = doc.inner().catalog_handle() {
        if let Some(cadh) = doc.inner().resolve_to_dict(cah).ok() {
            if let Some(dict) = arena.get_dict(cadh) {
                let str_root_key = arena.name("StructTreeRoot");
                if let Some(str_root_obj) = dict.get(&str_root_key) {
                    if let Some(str_root_ref) = str_root_obj.resolve(arena).as_reference() {
                        let auditor = ferruginous_sdk::structure::MatterhornAuditor::new(arena);
                        if let Ok(audit_res) = auditor.audit(str_root_ref) {
                            for f in audit_res {
                                findings.push((f.checkpoint, f.severity, f.message, f.handle_id));
                            }
                        }
                    }
                }
            }
        }
    }
    let _ = tx.send(WorkerResponse::AuditFindings { findings });
}

fn handle_save( // RR-15 Limit: Dispatcher - Thread pool worker saving request routing dispatcher handling signatures, redactions and compression saving options
    doc_opt: &Option<PdfDocument>,
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
        if let Err(e) =
            ferruginous_sdk::apply_physical_redaction_to_page(doc.inner(), page_idx, &rects)
        {
            let _ = tx.send(WorkerResponse::Error(format!(
                "Failed physically redacting page {}: {}",
                page_idx, e
            )));
            return;
        }
    }

    let version = if upgrade_pdf20 { "2.0" } else { "1.7" };
    let options = ferruginous_sdk::SaveOptions {
        compress,
        compression_level: 6,
        vacuum,
        ..ferruginous_sdk::SaveOptions::default()
    };

    let res = if let Some(cp) = cert_path {
        // Read certificate file bytes
        let cert_bytes = std::fs::read(&cp).unwrap_or_default();
        let sign_opts = ferruginous_sdk::SignOptions {
            reason: Some("Signed via Ferruginous Production Studio".to_string()),
            location: Some("Tokyo, Japan".to_string()),
            contact_info: Some("support@ferruginous.com".to_string()),
            name: Some("Ferruginous Digital Signer".to_string()),
            certificate: Some(cert_bytes.clone()),
            private_key: Some(cert_bytes),
            page_index: signature_position.map(|(idx, _)| idx).unwrap_or(0),
            rect: signature_position.map(|(_, rect)| rect).unwrap_or([50.0, 50.0, 200.0, 100.0]),
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
            let _ = tx.send(WorkerResponse::Error(format!("Failed to save PDF: {}", e)));
        }
    }
}
