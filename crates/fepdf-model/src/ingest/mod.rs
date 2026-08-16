//! Ingestion: turning the objects the reader placed in an arena into a document.

use crate::Document;
use crate::arena::PdfArena;
use crate::font::FontResource;
use crate::handle::Handle;
use crate::object::Object;
use crate::reader::{DictHandle, RawDocument};
use crate::refine::{ParallelRefinery, RefineContext};
use std::collections::BTreeMap;
use std::sync::Arc;

mod discovery;
pub use discovery::*;

/// Policy for color validation.
///
/// **Not consulted by any ingestion path.** The type and the field exist; the
/// colour validation they were meant to govern does not. Its CLI flag is hidden
/// rather than removed (ADR-0007). Implementing the policy means making this the
/// input to a real check, not finding the code that already reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPolicy {
    /// Reject colour definitions that violate the specification.
    Strict,
    /// Accept malformed colour definitions, substituting a usable value.
    Relaxed,
}

/// Options for document ingestion.
#[derive(Clone)]
pub struct IngestionOptions {
    /// Run the refinement passes (Pass 2) during ingestion.
    pub active_refinement: bool,
    /// Recover legacy `/Info` metadata into XMP.
    ///
    /// **Not read.** Recovery runs unconditionally; setting this to `false` does not
    /// stop it. Kept because the option is the right shape for the behaviour that
    /// should exist — see [`ColorPolicy`] for the same situation and ADR-0007.
    pub sublime_metadata: bool,
    /// How strictly colour definitions are validated.
    ///
    /// **Not read.** See [`ColorPolicy`].
    pub color_policy: ColorPolicy,
    /// Substitute bundled fonts when an embedded program fails to parse.
    pub force_fallback: bool,
    /// Password used to decrypt an encrypted document.
    pub password: Option<String>,
    /// Invoked with progress messages during a long ingestion.
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

/// Fonts resolved during ingestion, keyed by object number.
type FontCache = BTreeMap<u32, Arc<FontResource>>;
/// For each stream, the fonts its resource dictionary names.
type StreamContexts = BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>;

/// Brings a source document into a [`PdfArena`].
pub struct Ingestor;

/// Result of a document ingestion.
pub struct IngestedDocument {
    /// The populated arena.
    pub arena: PdfArena,
    /// Handle of the document catalogue (`/Root`).
    pub root: Handle<Object>,
    /// Handle of the document information dictionary (`/Info`).
    pub info: Option<Handle<Object>>,
    /// Non-fatal problems encountered while ingesting.
    pub issues: Vec<crate::interpretation::Decision>,
    /// Fonts already resolved, keyed by object number.
    pub font_cache: BTreeMap<u32, Arc<FontResource>>,
    /// Description of the encryption that was in force, if any.
    pub security_method: String,
    /// Permission flags recovered from the encryption dictionary.
    pub permissions: Option<i32>,
    /// Which password authenticated, when the document was encrypted.
    pub access: Option<fepdf_syntax::security::Access>,
    /// What the source document was, for the output to record as its origin.
    pub provenance: crate::document::Provenance,
}

impl Ingestor {
    /// Ingests a document that the reader has already placed into an arena.
    ///
    /// The remaining passes are:
    /// 1. **Pass 0 (Normalization)**: decrypts every object and drops `/Encrypt`.
    /// 2. **Pass 1.5 (Context Discovery)**: discovers fonts and maps them to streams.
    /// 3. **Pass 2 (Refinement)**: parallel normalisation of content and metadata.
    ///
    /// Pass 1 no longer exists. The reader writes each object into the slot matching
    /// its number, so there is nothing left to inhale or remap.
    pub fn ingest(
        raw: RawDocument,
        options: &IngestionOptions,
    ) -> crate::PdfResult<IngestedDocument> {
        let report = |msg: &str| {
            if let Some(c) = &options.progress_callback {
                c(msg.to_string());
            }
        };
        report("1/4: Decrypting and normalizing document...");
        let mut decisions = raw.decisions;
        let security =
            crate::decrypt::unlock(&raw.arena, raw.trailer, password(options), &mut decisions)?;

        // Before refinement, which replaces the metadata stream and normalises the
        // objects: this is the last moment the source's own identity is visible.
        let provenance = capture_provenance(&raw.arena, raw.trailer);

        report("2/4: Mapping objects and loading structure...");
        let arena = raw.arena;
        let (root_handle, info_handle) =
            Self::resolve_root_info(&arena, raw.trailer, &mut decisions)?;
        let temp_doc = Document::new(arena.clone(), root_handle, info_handle);

        report("3/4: Discovering font resources and stream contexts...");
        let (handle_font_cache, stream_contexts) =
            Self::discover_contexts(&arena, &temp_doc, &mut decisions);

        report("4/4: Performing active refinement and layout optimization...");
        let mut issues = decisions.into_entries();
        if options.active_refinement {
            issues.append(&mut Self::perform_active_refinement(
                &arena,
                &handle_font_cache,
                &stream_contexts,
            ));
        }

        Ok(IngestedDocument {
            arena,
            root: root_handle,
            info: info_handle,
            issues,
            font_cache: handle_font_cache,
            security_method: security.method,
            permissions: security.permissions,
            access: security.access,
            provenance,
        })
    }

    /// Resolves every font and maps each stream to the resources it draws with.
    fn discover_contexts(
        arena: &PdfArena,
        temp_doc: &Document,
        decisions: &mut crate::interpretation::DecisionLog,
    ) -> (FontCache, StreamContexts) {
        let (font_indices, page_and_form_indices) = scan_ingested_objects(arena);
        let handle_font_cache = discover_fonts(arena, temp_doc, Some(&font_indices));

        for font in handle_font_cache.values() {
            for decision in &font.decisions {
                decisions.push(decision.clone());
            }
        }

        let global_font_registry = Self::build_global_font_registry(arena, &handle_font_cache);
        let mut stream_contexts =
            map_stream_contexts(arena, &handle_font_cache, Some(&page_and_form_indices));
        merge_global_fonts_into_contexts(&mut stream_contexts, &global_font_registry);
        (handle_font_cache, stream_contexts)
    }

    /// Finds the catalogue and the information dictionary.
    ///
    /// A file recovered by scanning has no trailer to ask, so the catalogue is found by
    /// looking for the object that declares itself one (7.7.2).
    fn resolve_root_info(
        arena: &PdfArena,
        trailer: Option<DictHandle>,
        decisions: &mut crate::interpretation::DecisionLog,
    ) -> crate::PdfResult<(Handle<Object>, Option<Handle<Object>>)> {
        let info = trailer.and_then(|t| trailer_reference(arena, t, "Info"));
        if let Some(root) = trailer.and_then(|t| trailer_reference(arena, t, "Root")) {
            return Ok((root, info));
        }
        let Some(root) = find_catalog(arena) else {
            decisions.push(crate::interpretation::Decision::violation(
                "7.7.2",
                "no /Root in the trailer and no /Type /Catalog object anywhere",
                "cannot open the file as a document; its objects are still readable",
            ));
            return Err(crate::PdfError::Ingestion {
                context: "Catalogue".into(),
                message: "the file has no document catalogue (ISO 7.7.2); \
                          it may be truncated before the trailer"
                    .into(),
            });
        };
        decisions.push(crate::interpretation::Decision::repaired(
            "7.7.2",
            "the trailer named no /Root",
            format!("used object {} , which declares /Type /Catalog", root.index()),
        ));
        Ok((root, info))
    }

    fn build_global_font_registry(
        arena: &PdfArena,
        handle_font_cache: &BTreeMap<u32, Arc<FontResource>>,
    ) -> BTreeMap<String, Arc<FontResource>> {
        let mut global_font_registry = BTreeMap::new();
        for (h_idx, font_res) in handle_font_cache {
            if let Some(_dict) = arena.get_dict(Handle::new(*h_idx)) {
                global_font_registry.insert(format!("obj_{h_idx}"), font_res.clone());
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
        arena: &PdfArena,
        handle_font_cache: &BTreeMap<u32, Arc<FontResource>>,
        stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
    ) -> Vec<crate::interpretation::Decision> {
        let distilled_fonts = BTreeMap::new();
        let context = RefineContext {
            arena,
            fonts: handle_font_cache,
            contexts: stream_contexts,
            distilled: &distilled_fonts,
        };
        let refined_results = ParallelRefinery::refine_all(&context);

        let mut all_issues = Vec::new();
        for (number, refined, mut issues) in refined_results {
            let committed = crate::refine::commit_to_arena(arena, refined, 0);
            arena.set_object(Handle::new(number), committed);
            all_issues.append(&mut issues);
        }
        all_issues
    }
}

/// The password to try, defaulting to the empty one every reader starts with.
fn password(options: &IngestionOptions) -> &str {
    options.password.as_deref().unwrap_or("")
}

/// Reads the source's identity and how many signatures it carried.
///
/// `xmpMM:DocumentID` first, since it survives a producer's own edits; the trailer's
/// `/ID[0]` otherwise, which is what a file without XMP has to offer.
fn capture_provenance(
    arena: &PdfArena,
    trailer: Option<DictHandle>,
) -> crate::document::Provenance {
    let mut signatures = 0;
    for handle in arena.all_dict_handles() {
        if let Some(dict) = arena.get_dict(handle)
            && dict.get(&arena.name("Type")).and_then(|o| o.resolve(arena).as_name())
                == arena.get_name_by_str("Sig")
            && arena.get_name_by_str("Sig").is_some()
        {
            signatures += 1;
        }
    }

    crate::document::Provenance { source_id: source_document_id(arena, trailer), signatures }
}

/// `xmpMM:DocumentID` from the catalogue's metadata stream, else the trailer `/ID[0]`.
fn source_document_id(arena: &PdfArena, trailer: Option<DictHandle>) -> Option<String> {
    let trailer = trailer?;
    if let Some(root) = trailer_reference(arena, trailer, "Root")
        && let Some(Object::Dictionary(d)) = arena.get_object(root)
        && let Some(catalog) = arena.get_dict(d)
        && let Some(Object::Stream(_, payload)) =
            catalog.get(&arena.name("Metadata")).map(|o| o.resolve(arena))
        && let Ok(bytes) = arena.get_stream_bytes(&payload)
    {
        let xmp = String::from_utf8_lossy(&bytes);
        if let Some(id) = between(&xmp, "<xmpMM:DocumentID>", "</xmpMM:DocumentID>") {
            return Some(id);
        }
    }

    let Some(Object::Array(h)) = arena.get_dict(trailer)?.get(&arena.name("ID")).cloned() else {
        return None;
    };
    match arena.get_array(h)?.first()? {
        Object::String(b) | Object::Hex(b) => Some(b.iter().map(|x| format!("{x:02x}")).collect()),
        _ => None,
    }
}

fn between(haystack: &str, open: &str, close: &str) -> Option<String> {
    let start = haystack.find(open)? + open.len();
    let end = haystack[start..].find(close)? + start;
    Some(haystack[start..end].to_string())
}

/// An indirect reference held under `key` in the trailer.
fn trailer_reference(arena: &PdfArena, trailer: DictHandle, key: &str) -> Option<Handle<Object>> {
    match arena.get_dict(trailer)?.get(&arena.name(key))? {
        Object::Reference(h) => Some(*h),
        _ => None,
    }
}

/// The first object declaring `/Type /Catalog`, for files with no usable trailer.
fn find_catalog(arena: &PdfArena) -> Option<Handle<Object>> {
    let type_key = arena.name("Type");
    let catalog = arena.name("Catalog");
    for number in 0..arena.object_count() {
        let handle = Handle::new(number);
        let Some(Object::Dictionary(d)) = arena.get_object(handle) else { continue };
        if arena.get_dict(d)?.get(&type_key) == Some(&Object::Name(catalog)) {
            return Some(handle);
        }
    }
    None
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
