//! Ingestor module for transitioning lopdf::Document to PdfArena.

use crate::Document;
use crate::arena::{PdfArena, RemappingTable};
use crate::error::PdfError;
use crate::font::FontResource;
use crate::handle::Handle;
use crate::object::Object;
use crate::refine::ParallelRefinery;
use crate::security::SecurityHandler;
use std::collections::BTreeMap;
use std::sync::Arc;

mod discovery;
pub use discovery::*;

/// Policy for color validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPolicy {
    Strict,
    Relaxed,
}

/// Options for document ingestion.
#[derive(Clone)]
pub struct IngestionOptions {
    pub active_refinement: bool,
    pub sublime_metadata: bool,
    pub color_policy: ColorPolicy,
    pub force_fallback: bool,
    pub password: Option<String>,
    pub progress_callback: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
}

impl std::fmt::Debug for IngestionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestionOptions")
            .field("active_refinement", &self.active_refinement)
            .field("sublime_metadata", &self.sublime_metadata)
            .field("color_policy", &self.color_policy)
            .field("force_fallback", &self.force_fallback)
            .field("password", &self.password)
            .field("progress_callback", &self.progress_callback.is_some())
            .finish()
    }
}

impl Default for IngestionOptions {
    fn default() -> Self {
        Self {
            active_refinement: true,
            sublime_metadata: true,
            color_policy: ColorPolicy::Strict,
            force_fallback: false,
            password: None,
            progress_callback: None,
        }
    }
}

pub struct Ingestor;

/// Result of a document ingestion.
pub struct IngestedDocument {
    pub arena: PdfArena,
    pub root: Handle<Object>,
    pub info: Option<Handle<Object>>,
    pub issues: Vec<String>,
    pub font_cache: BTreeMap<u32, Arc<FontResource>>,
    pub security_method: String,
    pub permissions: Option<i32>,
}

impl Ingestor {
    /// Ingests a lopdf::Document into a new PdfArena and returns a refined Document.
    ///
    /// The ingestion process is a three-pass system:
    /// 1. **Pass 0 (Normalization)**: Decrypts all objects and cleans the physical structure.
    /// 2. **Pass 1 (Inhalation)**: Maps lopdf objects to PdfArena Handles, decoupling from physical offsets.
    /// 3. **Pass 1.5 (Context Discovery)**: Discovers font resources and maps them to content streams.
    /// 4. **Pass 2 (Refinement)**: Parallel processing of page content and metadata.
    fn resolve_root_info(
        doc: &lopdf::Document,
        table: &RemappingTable,
    ) -> (Handle<Object>, Option<Handle<Object>>) {
        let root_id = doc.trailer.get(b"Root").ok().and_then(|o| o.as_reference().ok());
        let info_id = doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok());

        let root_handle = root_id.and_then(|id| table.get(&id)).cloned().unwrap_or(Handle::new(0));
        let info_handle = info_id.and_then(|id| table.get(&id)).cloned();
        (root_handle, info_handle)
    }

    fn build_global_font_registry(
        arena: &PdfArena,
        handle_font_cache: &BTreeMap<u32, Arc<FontResource>>,
    ) -> BTreeMap<String, Arc<FontResource>> {
        let mut global_font_registry = BTreeMap::new();
        for (h_idx, font_res) in handle_font_cache {
            if let Some(_dict) = arena.get_dict(Handle::new(*h_idx)) {
                global_font_registry.insert(format!("obj_{}", h_idx), font_res.clone());
                let base_name = font_res.base_font.as_str();
                let is_subset = base_name.len() > 7 && base_name.as_bytes()[6] == b'+';
                let is_component_cid = font_res.subtype.as_str() == "CIDFontType0"
                    || font_res.subtype.as_str() == "CIDFontType2";

                if !is_subset && !is_component_cid {
                    global_font_registry.insert(base_name.to_string(), font_res.clone());
                }
            }
        }
        global_font_registry
    }

    fn perform_active_refinement(
        doc: &mut lopdf::Document,
        table: &RemappingTable,
        handle_font_cache: &BTreeMap<u32, Arc<FontResource>>,
        stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
        arena: &PdfArena,
    ) -> crate::PdfResult<Vec<String>> {
        let distilled_fonts = std::collections::BTreeMap::new();
        let refined_results = ParallelRefinery::refine_all(
            doc,
            table,
            handle_font_cache,
            stream_contexts,
            &distilled_fonts,
        );

        let mut all_issues = Vec::new();
        for (id, refined, mut issues) in refined_results {
            let handle = table.get(&id).cloned().ok_or_else(|| PdfError::Ingestion {
                context: "Pass 3 Integration".into(),
                message: format!("Missing handle for refined object {:?}", id).into(),
            })?;
            let committed = crate::refine::commit_to_arena(arena, refined, 0);
            arena.set_object(handle, committed);
            all_issues.append(&mut issues);
        }
        Ok(all_issues)
    }

    pub fn ingest(
        doc: &mut lopdf::Document,
        options: &IngestionOptions,
    ) -> crate::PdfResult<IngestedDocument> {
        let report = |msg: &str| {
            if let Some(c) = &options.progress_callback {
                c(msg.to_string());
            }
        };
        report("1/4: Decrypting and normalizing document...");
        let arena = PdfArena::new();
        let mut table = RemappingTable::new();

        let (security_method, permissions) = Self::perform_pass_0_decryption(doc, options)?;

        for &id in doc.objects.keys() {
            let handle = arena.alloc_object(Object::Null);
            table.insert(id, handle);
        }

        report("2/4: Mapping objects and loading structure...");
        inhale_objects(doc, &arena, &table)?;

        let (root_handle, info_handle) = Self::resolve_root_info(doc, &table);
        let temp_doc = Document::new(arena.clone(), root_handle, info_handle);

        report("3/4: Discovering font resources and stream contexts...");
        let (font_indices, page_and_form_indices) = scan_ingested_objects(&arena);
        let handle_font_cache = discover_fonts(&arena, &temp_doc, Some(&font_indices));

        let global_font_registry = Self::build_global_font_registry(&arena, &handle_font_cache);
        let mut stream_contexts =
            map_stream_contexts(&arena, &handle_font_cache, Some(&page_and_form_indices));

        merge_global_fonts_into_contexts(&mut stream_contexts, &global_font_registry);

        report("4/4: Performing active refinement and layout optimization...");
        let mut all_issues = Vec::new();
        if options.active_refinement {
            all_issues = Self::perform_active_refinement(
                doc,
                &table,
                &handle_font_cache,
                &stream_contexts,
                &arena,
            )?;
        }

        Ok(IngestedDocument {
            arena,
            root: root_handle,
            info: info_handle,
            issues: all_issues,
            font_cache: handle_font_cache,
            security_method,
            permissions,
        })
    }

    /// Performs "Pass 0" normalization by decrypting the raw PDF objects in-place.
    ///
    /// This is required because Acrobat (and other strict viewers) will fail with Error 135
    /// if a document is saved with decrypted objects but still contains an `/Encrypt` trailer entry.
    /// After decryption, this method explicitly removes the `/Encrypt` dictionary to satisfy
    /// Adobe fidelity requirements.
    fn parse_security_handler(
        doc: &lopdf::Document,
        options: &IngestionOptions,
    ) -> Option<SecurityHandler> {
        let password = options.password.as_deref().unwrap_or("");
        let encrypt_dict_obj = doc.trailer.get(b"Encrypt").ok()?;
        let encrypt_obj = if let Ok(id) = encrypt_dict_obj.as_reference() {
            doc.objects.get(&id)
        } else {
            Some(encrypt_dict_obj)
        };

        if let Some(lopdf::Object::Dictionary(dict)) = encrypt_obj {
            let v_val = dict.get(b"V").and_then(|o| o.as_i64()).unwrap_or(0);
            let r_val = dict.get(b"R").and_then(|o| o.as_i64()).unwrap_or(0);

            if v_val == 4 && r_val == 4 {
                let o_str = dict.get(b"O").and_then(|o| o.as_str()).unwrap_or(&[]);
                let u_str = dict.get(b"U").and_then(|o| o.as_str()).unwrap_or(&[]);
                let p_val = dict.get(b"P").and_then(|o| o.as_i64()).unwrap_or(0) as i32;
                let file_id = doc
                    .trailer
                    .get(b"ID")
                    .and_then(|o| o.as_array())
                    .map(|a| a.first().and_then(|o| o.as_str().ok()).unwrap_or(&[]))
                    .unwrap_or(&[]);
                let encrypt_metadata =
                    dict.get(b"EncryptMetadata").and_then(|o| o.as_bool()).unwrap_or(true);
                return SecurityHandler::new_v4(
                    password,
                    o_str,
                    u_str,
                    p_val,
                    file_id,
                    encrypt_metadata,
                )
                .ok();
            } else if v_val == 5 && (r_val == 5 || r_val == 6) {
                let mut file_id = &[][..];
                if let Ok(id_array) = doc.trailer.get(b"ID")
                    && let Ok(arr) = id_array.as_array()
                    && let Some(first) = arr.first()
                    && let Ok(s) = first.as_str()
                {
                    file_id = s;
                }
                return SecurityHandler::new_v5(password, "", file_id).ok();
            }
        }
        None
    }

    fn perform_pass_0_decryption(
        doc: &mut lopdf::Document,
        options: &IngestionOptions,
    ) -> crate::PdfResult<(String, Option<i32>)> {
        let mut security_method = "No Security".to_string();
        let mut permissions = None;

        let security_handler = Self::parse_security_handler(doc, options);

        if let Some(handler) = &security_handler {
            if let Ok(encrypt_dict_obj) = doc.trailer.get(b"Encrypt") {
                let encrypt_obj = if let Ok(id) = encrypt_dict_obj.as_reference() {
                    doc.objects.get(&id)
                } else {
                    Some(encrypt_dict_obj)
                };
                if let Some(lopdf::Object::Dictionary(dict)) = encrypt_obj {
                    let v_val = dict.get(b"V").and_then(|o| o.as_i64()).unwrap_or(0);
                    let _r_val = dict.get(b"R").and_then(|o| o.as_i64()).unwrap_or(0);
                    security_method = if v_val == 5 {
                        "Password Security (AES-256)".to_string()
                    } else if v_val == 4 {
                        "Password Security (AES-128)".to_string()
                    } else {
                        "Password Security (Standard)".to_string()
                    };
                    permissions = dict.get(b"P").and_then(|o| o.as_i64()).map(|p| p as i32).ok();
                }
            }

            let ids: Vec<lopdf::ObjectId> = doc.objects.keys().cloned().collect();
            for id in ids {
                if let Some(obj) = doc.objects.get_mut(&id) {
                    Self::decrypt_object_stacked(obj, id, handler)?;
                }
            }
            doc.trailer.remove(b"Encrypt");
        }
        Ok((security_method, permissions))
    }

    /// Iteratively traverses a PDF object tree and decrypts all strings and streams.
    ///
    /// This method is **RR-15 Rule 6 compliant** (no recursion). It uses an explicit stack
    /// to walk through Dictionaries and Arrays. This is critical for PDF documents which
    /// can have arbitrary nesting depths that would otherwise cause a stack overflow.
    fn decrypt_object_stacked(
        root_obj: &mut lopdf::Object,
        id: lopdf::ObjectId,
        handler: &SecurityHandler,
    ) -> crate::PdfResult<()> {
        let mut stack = vec![root_obj];

        while let Some(obj) = stack.pop() {
            match obj {
                lopdf::Object::String(s, _) => {
                    let decrypted = handler.decrypt_bytes(s, id.0, id.1)?;
                    *s = decrypted;
                }
                lopdf::Object::Stream(stream) => {
                    let decrypted = handler.decrypt_bytes(&stream.content, id.0, id.1)?;
                    stream.content = decrypted;
                }
                lopdf::Object::Array(arr) => {
                    for item in arr {
                        stack.push(item);
                    }
                }
                lopdf::Object::Dictionary(dict) => {
                    for (_, item) in dict.iter_mut() {
                        stack.push(item);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn scan_ingested_objects(arena: &PdfArena) -> (Vec<u32>, Vec<u32>) {
    let mut font_indices = Vec::new();
    let mut page_and_form_indices = Vec::new();

    let type_key = arena.name("Type");
    let font_val = arena.name("Font");
    let base_font_key = arena.name("BaseFont");
    let subtype_key = arena.name("Subtype");
    let page_val = arena.name("Page");
    let form_val = arena.name("Form");

    for i in 0..arena.object_count() {
        let obj_h = Handle::new(i);
        if let Some(Object::Dictionary(handle) | Object::Stream(handle, _)) =
            arena.get_object(obj_h)
            && let Some(dict) = arena.get_dict(handle)
        {
            let type_val_resolved = dict.get(&type_key).and_then(|o| o.resolve(arena).as_name());
            let subtype_val_resolved =
                dict.get(&subtype_key).and_then(|o| o.resolve(arena).as_name());

            if type_val_resolved == Some(page_val) || subtype_val_resolved == Some(form_val) {
                page_and_form_indices.push(i);
            }

            let is_font = if let Some(tv) = type_val_resolved {
                tv == font_val
            } else {
                dict.contains_key(&base_font_key) && dict.contains_key(&subtype_key)
            };
            if is_font {
                font_indices.push(i);
            }
        }
    }
    (font_indices, page_and_form_indices)
}

fn merge_global_fonts_into_contexts(
    stream_contexts: &mut BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
    global_font_registry: &BTreeMap<String, Arc<FontResource>>,
) {
    for context in stream_contexts.values_mut() {
        for (name, res) in global_font_registry {
            if !context.contains_key(name) {
                context.insert(name.clone(), Arc::clone(res));
            }
        }
    }
}

fn inhale_objects(
    doc: &lopdf::Document,
    arena: &PdfArena,
    table: &RemappingTable,
) -> crate::PdfResult<()> {
    for (&id, obj) in &doc.objects {
        let handle = table.get(&id).cloned().ok_or_else(|| PdfError::Ingestion {
            context: "Pass 1 Inhalation".into(),
            message: format!("Missing handle for object {:?}", id).into(),
        })?;
        let raw_obj = Object::from_lopdf(obj, arena, table);
        arena.set_object(handle, raw_obj);
    }
    Ok(())
}
