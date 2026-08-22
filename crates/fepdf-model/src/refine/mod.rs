//! Refinery 2.1 Concurrent Normalization Pipeline.
//!
//! This module implements the parallel refinement strategy,
//! where objects are refined using `rayon` before being sequentially
//! integrated into the `PdfArena`.

use crate::arena::PdfArena;
use crate::font::FontResource;
use crate::handle::Handle;
use crate::object::{Object, PdfName};

use bytes::Bytes;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod color;
/// Font normalisation performed during refinement.
pub mod font;
pub mod metadata;
/// Text and string normalisation performed during refinement.
pub mod text;

/// A thread-safe intermediate representation of a refined PDF object.
#[derive(Debug, Clone)]
pub enum RefinedObject {
    /// A boolean.
    Boolean(bool),
    /// An integer.
    Integer(i64),
    /// A real number.
    Real(f64),
    /// A literal string, still as bytes.
    String(Bytes),
    /// A hexadecimal string, still as bytes.
    Hex(Bytes),
    /// A string already decoded to UTF-8.
    Text(String),
    /// A name.
    Name(PdfName),
    /// An array, refined element-wise.
    Array(Vec<RefinedObject>),
    /// A dictionary, refined value-wise.
    Dictionary(BTreeMap<PdfName, RefinedObject>),
    /// A stream: its dictionary and its raw bytes.
    Stream(BTreeMap<PdfName, RefinedObject>, Bytes),
    /// A stream whose payload has already been sublimated.
    Sublimated(BTreeMap<PdfName, RefinedObject>, crate::object::SublimatedData),
    /// The null object.
    Null,
    /// An indirect reference into the arena.
    Reference(Handle<Object>),
}

impl RefinedObject {
    /// The name, if this is a name object.
    pub fn as_name(&self) -> Option<&PdfName> {
        match self {
            Self::Name(n) => Some(n),
            _ => None,
        }
    }

    /// The text, if this object holds a decoded string.
    pub fn as_str(&self) -> Option<&str> {
        self.as_name().map(|n| n.as_str())
    }

    /// Converts an arena object to the refined representation, unchanged.
    ///
    /// References need no remapping: the reader places every object at the slot
    /// matching its number, so a `Handle` built from `n 0 R` already points at it.
    pub fn from_arena(arena: &PdfArena, object: &Object, depth: usize) -> Self {
        if depth > 128 {
            return Self::Null;
        }
        match object {
            Object::Boolean(b) => Self::Boolean(*b),
            Object::Integer(i) => Self::Integer(*i),
            Object::Real(f) => Self::Real(*f),
            Object::String(s) => Self::String(s.clone()),
            Object::Hex(s) => Self::Hex(s.clone()),
            Object::Text(s) => Self::Text(s.clone()),
            Object::Name(n) => arena.get_name(*n).map_or(Self::Null, Self::Name),
            Object::Reference(h) => Self::Reference(*h),
            Object::Null => Self::Null,
            Object::Array(h) => Self::Array(
                arena
                    .get_array(*h)
                    .unwrap_or_default()
                    .iter()
                    .map(|item| Self::from_arena(arena, item, depth + 1))
                    .collect(),
            ),
            Object::Dictionary(h) => Self::Dictionary(named_dict(arena, *h, depth)),
            Object::Stream(h, data) => match data.as_ref() {
                crate::object::SublimatedData::Raw(bytes) => {
                    Self::Stream(named_dict(arena, *h, depth), bytes.clone())
                }
                other @ (crate::object::SublimatedData::Commands { .. }
                | crate::object::SublimatedData::Image { .. }
                | crate::object::SublimatedData::Compressed { .. }) => {
                    Self::Sublimated(named_dict(arena, *h, depth), other.clone())
                }
            },
        }
    }
}

/// Converts a dictionary held in the arena into one keyed by spelt-out names.
fn named_dict(
    arena: &PdfArena,
    handle: Handle<BTreeMap<Handle<PdfName>, Object>>,
    depth: usize,
) -> BTreeMap<PdfName, RefinedObject> {
    let mut out = BTreeMap::new();
    for (key, value) in arena.get_dict(handle).unwrap_or_default() {
        let Some(name) = arena.get_name(key) else { continue };
        out.insert(name, RefinedObject::from_arena(arena, &value, depth + 1));
    }
    out
}

/// Runs the refinement passes across objects in parallel.
pub struct ParallelRefinery;

const UI_TEXT_FIELDS: &[&str] = &["Title", "Author", "Subject", "Keywords", "Creator", "Producer"];

/// Everything refinement needs besides the object itself, kept together so the
/// recursive walk stays within the argument limits.
pub struct RefineContext<'a> {
    /// The arena holding the objects being refined.
    pub arena: &'a PdfArena,
    /// Fonts already discovered, keyed by the handle of their dictionary.
    pub fonts: &'a BTreeMap<u32, Arc<FontResource>>,
    /// Per-stream font context, keyed by the stream's handle.
    pub contexts: &'a BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
    /// Subsetted font programs to substitute, keyed by the stream's handle.
    pub distilled: &'a BTreeMap<Handle<Object>, Arc<Vec<u8>>>,
    /// How strictly color spaces and palettes are validated.
    pub color_policy: crate::ingest::ColorPolicy,
}

impl ParallelRefinery {
    /// Refines every object in the arena, returning the refined tree.
    pub fn refine_all(
        context: &RefineContext<'_>,
    ) -> Vec<(u32, RefinedObject, Vec<crate::interpretation::Decision>)> {
        (0..context.arena.object_count())
            .into_par_iter()
            .filter_map(|number| {
                let object = context.arena.get_object(Handle::new(number))?;
                if matches!(object, Object::Null) {
                    return None;
                }
                let mut issues = Vec::new();
                let refined = Self::refine_recursive(context, number, &object, 0, &mut issues);
                Some((number, refined, issues))
            })
            .collect()
    }

    fn refine_recursive(
        context: &RefineContext<'_>,
        number: u32,
        object: &Object,
        depth: usize,
        issues: &mut Vec<crate::interpretation::Decision>,
    ) -> RefinedObject {
        // Hardening: Recursion depth limit (ISO 32000-2 Clause 7.1)
        if depth > 128 {
            issues.push(crate::interpretation::Decision::violation(
                "7.3.10",
                format!("object {number} nests past the resolution limit"),
                "stopped resolving; the object is left unrefined",
            ));
            return RefinedObject::from_arena(context.arena, object, depth);
        }
        match object {
            Object::Dictionary(h) => Self::refine_dict(context, number, *h, depth, issues),
            Object::Stream(h, data) => {
                Self::refine_stream(context, number, *h, data, depth, issues)
            }
            other => RefinedObject::from_arena(context.arena, other, depth),
        }
    }

    fn refine_dict_entry(
        context: &RefineContext<'_>,
        number: u32,
        key: &PdfName,
        value: &Object,
        depth: usize,
        issues: &mut Vec<crate::interpretation::Decision>,
    ) -> RefinedObject {
        let is_text_field = matches!(
            key.as_str(),
            "Title" | "Author" | "Subject" | "Creator" | "Producer" | "Keywords"
        );
        match value {
            Object::String(s) if is_text_field => {
                RefinedObject::Text(crate::refine::text::recover_string(s))
            }
            other => Self::refine_recursive(context, number, other, depth + 1, issues),
        }
    }

    fn normalize_ui_text_fields(refined_dict: &mut BTreeMap<PdfName, RefinedObject>) {
        for field in UI_TEXT_FIELDS {
            let field_key = PdfName::new(field);
            let recovered = match refined_dict.get(&field_key) {
                Some(RefinedObject::String(s) | RefinedObject::Hex(s)) => {
                    Some(text::recover_string(s))
                }
                _ => None,
            };
            if let Some(val) = recovered {
                refined_dict.insert(field_key, RefinedObject::Text(val));
            }
        }
    }

    fn refine_dict(
        context: &RefineContext<'_>,
        number: u32,
        handle: Handle<BTreeMap<Handle<PdfName>, Object>>,
        depth: usize,
        issues: &mut Vec<crate::interpretation::Decision>,
    ) -> RefinedObject {
        let mut refined_dict = Self::refine_entries(context, number, handle, depth, issues);

        // Font Normalization
        if refined_dict.get(&PdfName::new("Type")).and_then(RefinedObject::as_str) == Some("Font") {
            let resource = context.fonts.get(&number).map(std::convert::AsRef::as_ref);
            refined_dict = match font::normalize_font(refined_dict, resource) {
                RefinedObject::Dictionary(d) => d,
                _ => return RefinedObject::Null,
            };
        }

        Self::normalize_ui_text_fields(&mut refined_dict);
        RefinedObject::Dictionary(refined_dict)
    }

    /// Refines every value of a dictionary, leaving its keys as written.
    fn refine_entries(
        context: &RefineContext<'_>,
        number: u32,
        handle: Handle<BTreeMap<Handle<PdfName>, Object>>,
        depth: usize,
        issues: &mut Vec<crate::interpretation::Decision>,
    ) -> BTreeMap<PdfName, RefinedObject> {
        let mut refined = BTreeMap::new();
        for (key, value) in context.arena.get_dict(handle).unwrap_or_default() {
            let Some(name) = context.arena.get_name(key) else { continue };
            let refined_value =
                Self::refine_dict_entry(context, number, &name, &value, depth, issues);
            refined.insert(name, refined_value);
        }
        color::refine_palette(&mut refined, context.color_policy, issues);
        refined
    }

    fn sublimate_stream_content_static(
        context: &RefineContext<'_>,
        number: u32,
        refined_dict: &BTreeMap<PdfName, RefinedObject>,
        content: &Bytes,
        issues: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<RefinedObject> {
        let subtype = refined_dict.get(&PdfName::new("Subtype")).and_then(RefinedObject::as_str);
        let is_image = subtype == Some("Image");
        let is_form = subtype == Some("Form");
        let is_likely_content = (subtype.is_none() || is_form) && !is_image;

        if is_likely_content && let Some(fonts) = context.contexts.get(&number) {
            let mut sublimator = crate::object::sublimation::parser::Sublimator::new(fonts);
            let commands = sublimator.sublimate(content);
            issues.append(&mut sublimator.take_decisions());
            return Some(RefinedObject::Sublimated(
                refined_dict.clone(),
                crate::object::SublimatedData::Commands { items: commands },
            ));
        }
        None
    }

    fn refine_stream(
        // RR-15 Limit: Dispatcher - refines streams by decompressing, sublimating contents, and mapping fonts
        context: &RefineContext<'_>,
        number: u32,
        handle: Handle<BTreeMap<Handle<PdfName>, Object>>,
        data: &Arc<crate::object::SublimatedData>,
        depth: usize,
        issues: &mut Vec<crate::interpretation::Decision>,
    ) -> RefinedObject {
        let mut refined_dict = Self::refine_entries(context, number, handle, depth + 1, issues);

        if let Some(distilled) = context.distilled.get(&Handle::new(number)) {
            refined_dict.remove(&PdfName::new("Filter"));
            refined_dict.remove(&PdfName::new("DecodeParms"));
            return RefinedObject::Stream(refined_dict, Bytes::copy_from_slice(distilled));
        }

        let crate::object::SublimatedData::Raw(raw) = data.as_ref() else {
            return RefinedObject::Sublimated(refined_dict, data.as_ref().clone());
        };

        let (content, was_decompressed) = decode_payload(context.arena, handle, raw, &refined_dict);
        if was_decompressed {
            refined_dict.remove(&PdfName::new("Filter"));
            refined_dict.remove(&PdfName::new("DecodeParms"));
        }

        Self::sublimate_stream_content_static(context, number, &refined_dict, &content, issues)
            .unwrap_or(RefinedObject::Stream(refined_dict, content))
    }
}

/// Decodes a stream's filters, except for images and font programs.
///
/// Those two are preserved exactly as written: re-encoding them loses fidelity that
/// nothing downstream needs, and font programs are handed to the shaper unchanged.
fn decode_payload(
    arena: &PdfArena,
    handle: Handle<BTreeMap<Handle<PdfName>, Object>>,
    raw: &Bytes,
    refined_dict: &BTreeMap<PdfName, RefinedObject>,
) -> (Bytes, bool) {
    let subtype = refined_dict.get(&PdfName::new("Subtype")).and_then(RefinedObject::as_str);
    let is_font = refined_dict.contains_key(&PdfName::new("Length1"))
        || refined_dict.contains_key(&PdfName::new("Length2"))
        || refined_dict.contains_key(&PdfName::new("Length3"));
    if subtype == Some("Image") || is_font {
        return (raw.clone(), false);
    }

    let Some(dict) = arena.get_dict(handle) else { return (raw.clone(), false) };
    arena.process_filters(raw, &dict).map_or((raw.clone(), false), |decoded| (decoded, true))
}

fn commit_stream_to_arena(
    arena: &PdfArena,
    dict: BTreeMap<PdfName, RefinedObject>,
    bytes: Bytes,
    depth: usize,
) -> Object {
    let committed_dict: BTreeMap<Handle<PdfName>, Object> = dict
        .into_iter()
        .map(|(k, v)| (arena.intern_name(k), commit_to_arena(arena, v, depth + 1)))
        .collect();
    // Sublimation Phase 1: Pre-decode Images or compress other large streams
    let is_image = committed_dict
        .get(&arena.name("Subtype"))
        .and_then(|o: &Object| o.resolve(arena).as_name())
        .and_then(|n: Handle<PdfName>| arena.get_name(n))
        .is_some_and(|n: PdfName| n.as_str() == "Image");

    let is_font = committed_dict.contains_key(&arena.name("Length1"))
        || committed_dict.contains_key(&arena.name("Length2"))
        || committed_dict.contains_key(&arena.name("Length3"));

    let sublimated = if is_image || is_font {
        // High-Fidelity Preservation (Phase 2 & 3):
        // Do NOT decode or re-compress images/fonts to internal format during refine.
        crate::object::SublimatedData::Raw(bytes)
    } else if bytes.len() > 4096 {
        let compressed = crate::filters::flate::deflate(&bytes).unwrap_or_else(|_| bytes.to_vec());
        crate::object::SublimatedData::Compressed { original_len: bytes.len(), data: compressed }
    } else {
        crate::object::SublimatedData::Raw(bytes)
    };

    let dh = arena.alloc_dict(committed_dict);
    Object::Stream(dh, std::sync::Arc::new(sublimated))
}

/// Writes a refined object back into the arena, allocating handles as needed.
pub fn commit_to_arena(arena: &PdfArena, refined: RefinedObject, depth: usize) -> Object {
    if depth > 64 {
        return Object::Null;
    } // Rule 6: Stack Safety

    match refined {
        RefinedObject::Boolean(b) => Object::Boolean(b),
        RefinedObject::Integer(i) => Object::Integer(i),
        RefinedObject::Real(f) => Object::Real(f),
        RefinedObject::String(s) => Object::String(s),
        RefinedObject::Hex(s) => Object::Hex(s),
        RefinedObject::Text(s) => Object::Text(s),
        RefinedObject::Name(n) => Object::Name(arena.intern_name(n)),
        RefinedObject::Reference(h) => Object::Reference(h),
        RefinedObject::Array(arr) => {
            let committed =
                arr.into_iter().map(|item| commit_to_arena(arena, item, depth + 1)).collect();
            Object::Array(arena.alloc_array(committed))
        }
        RefinedObject::Dictionary(dict) => {
            let committed = dict
                .into_iter()
                .map(|(k, v)| (arena.intern_name(k), commit_to_arena(arena, v, depth + 1)))
                .collect();
            Object::Dictionary(arena.alloc_dict(committed))
        }
        RefinedObject::Stream(dict, bytes) => commit_stream_to_arena(arena, dict, bytes, depth),
        RefinedObject::Sublimated(dict, data) => {
            let committed_dict: BTreeMap<Handle<PdfName>, Object> = dict
                .into_iter()
                .map(|(k, v)| (arena.intern_name(k), commit_to_arena(arena, v, depth + 1)))
                .collect();
            Object::Stream(arena.alloc_dict(committed_dict), std::sync::Arc::new(data))
        }
        RefinedObject::Null => Object::Null,
    }
}
