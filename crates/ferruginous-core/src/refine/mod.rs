//! Refinery 2.1 Concurrent Normalization Pipeline.
//!
//! This module implements the parallel refinement strategy,
//! where objects are refined using `rayon` before being sequentially
//! integrated into the `PdfArena`.

use crate::arena::{PdfArena, RemappingTable};
use crate::font::FontResource;
use crate::handle::Handle;
use crate::object::{Object, PdfName};

use bytes::Bytes;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod color;
pub mod font;
pub mod metadata;
pub mod text;

/// A thread-safe intermediate representation of a refined PDF object.
#[derive(Debug, Clone)]
pub enum RefinedObject {
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(Bytes),
    Hex(Bytes),
    Text(String),
    Name(PdfName),
    Array(Vec<RefinedObject>),
    Dictionary(BTreeMap<PdfName, RefinedObject>),
    Stream(BTreeMap<PdfName, RefinedObject>, Bytes),
    Sublimated(BTreeMap<PdfName, RefinedObject>, crate::object::SublimatedData),
    Null,
    Reference(Handle<Object>),
}

impl RefinedObject {
    pub fn as_name(&self) -> Option<&PdfName> {
        match self {
            Self::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        self.as_name().map(|n| n.as_str())
    }

    pub fn from_lopdf(obj: &lopdf::Object, table: &RemappingTable) -> Self {
        match obj {
            lopdf::Object::Boolean(b) => Self::Boolean(*b),
            lopdf::Object::Integer(i) => Self::Integer(*i),
            lopdf::Object::Real(f) => Self::Real(*f as f64),
            lopdf::Object::String(s, fmt) => {
                if matches!(fmt, lopdf::StringFormat::Hexadecimal) {
                    Self::Hex(Bytes::copy_from_slice(s))
                } else {
                    Self::String(Bytes::copy_from_slice(s))
                }
            }
            lopdf::Object::Name(n) => Self::Name(PdfName::from_bytes(n)),
            lopdf::Object::Reference(id) => {
                Self::Reference(table.get(&(id.0, id.1)).cloned().unwrap_or(Handle::new(0)))
            }
            lopdf::Object::Array(arr) => {
                Self::Array(arr.iter().map(|item| Self::from_lopdf(item, table)).collect())
            }
            lopdf::Object::Dictionary(dict) => {
                let mut refined = BTreeMap::new();
                for (k, v) in dict {
                    refined.insert(PdfName::from_bytes(k), Self::from_lopdf(v, table));
                }
                Self::Dictionary(refined)
            }
            lopdf::Object::Stream(s) => {
                let mut refined = BTreeMap::new();
                for (k, v) in &s.dict {
                    refined.insert(PdfName::from_bytes(k), Self::from_lopdf(v, table));
                }
                Self::Stream(refined, Bytes::copy_from_slice(&s.content))
            }
            lopdf::Object::Null => Self::Null,
        }
    }
}

pub struct ParallelRefinery;

const UI_TEXT_FIELDS: &[&str] = &["Title", "Author", "Subject", "Keywords", "Creator", "Producer"];

impl ParallelRefinery {
    pub fn refine_all(
        doc: &lopdf::Document,
        table: &RemappingTable,
        handle_fonts: &BTreeMap<u32, Arc<FontResource>>,
        stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
        distilled_fonts: &BTreeMap<Handle<Object>, Arc<Vec<u8>>>,
    ) -> Vec<((u32, u16), RefinedObject, Vec<String>)> {
        doc.objects
            .par_iter()
            .map(|(&id, obj)| {
                let mut issues = Vec::new();
                let refined = Self::refine_recursive(
                    doc,
                    id,
                    obj,
                    table,
                    handle_fonts,
                    stream_contexts,
                    distilled_fonts,
                    0,
                    &mut issues,
                );
                (id, refined, issues)
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn refine_recursive(
        doc: &lopdf::Document,
        id: (u32, u16),
        obj: &lopdf::Object,
        table: &RemappingTable,
        handle_fonts: &BTreeMap<u32, Arc<FontResource>>,
        _stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
        distilled_fonts: &std::collections::BTreeMap<Handle<Object>, Arc<Vec<u8>>>,
        depth: usize,
        issues: &mut Vec<String>,
    ) -> RefinedObject {
        // Hardening: Recursion depth limit (ISO 32000-2 Clause 7.1)
        if depth > 128 {
            issues.push(format!("Recursion depth limit exceeded for object {:?}", id));
            return RefinedObject::from_lopdf(obj, table);
        }
        match obj {
            lopdf::Object::Dictionary(dict) => Self::refine_dict(
                doc,
                id,
                dict,
                table,
                handle_fonts,
                _stream_contexts,
                distilled_fonts,
                depth,
                issues,
            ),
            lopdf::Object::Stream(s) => Self::refine_stream(
                doc,
                id,
                s,
                table,
                handle_fonts,
                _stream_contexts,
                distilled_fonts,
                depth,
                issues,
            ),
            _ => RefinedObject::from_lopdf(obj, table),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn refine_dict_entry(
        doc: &lopdf::Document,
        id: (u32, u16),
        key_name: &PdfName,
        value: &lopdf::Object,
        table: &RemappingTable,
        handle_fonts: &BTreeMap<u32, Arc<FontResource>>,
        _stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
        distilled_fonts: &std::collections::BTreeMap<Handle<Object>, Arc<Vec<u8>>>,
        depth: usize,
        issues: &mut Vec<String>,
    ) -> RefinedObject {
        if matches!(
            key_name.as_str(),
            "Title" | "Author" | "Subject" | "Creator" | "Producer" | "Keywords"
        ) {
            match value {
                lopdf::Object::String(s, _) => {
                    RefinedObject::Text(crate::refine::text::recover_string(s))
                }
                _ => Self::refine_recursive(
                    doc,
                    id,
                    value,
                    table,
                    handle_fonts,
                    _stream_contexts,
                    distilled_fonts,
                    depth + 1,
                    issues,
                ),
            }
        } else {
            Self::refine_recursive(
                doc,
                id,
                value,
                table,
                handle_fonts,
                _stream_contexts,
                distilled_fonts,
                depth + 1,
                issues,
            )
        }
    }

    fn normalize_ui_text_fields(refined_dict: &mut BTreeMap<PdfName, RefinedObject>) {
        for field in UI_TEXT_FIELDS {
            let field_key = PdfName::new(field);
            let recovered = match refined_dict.get(&field_key) {
                Some(RefinedObject::String(s)) => Some(text::recover_string(s)),
                Some(RefinedObject::Hex(s)) => Some(text::recover_string(s)),
                _ => None,
            };
            if let Some(val) = recovered {
                refined_dict.insert(field_key, RefinedObject::Text(val));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn refine_dict(
        doc: &lopdf::Document,
        id: (u32, u16),
        dict: &lopdf::Dictionary,
        table: &RemappingTable,
        handle_fonts: &BTreeMap<u32, Arc<FontResource>>,
        _stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
        distilled_fonts: &std::collections::BTreeMap<Handle<Object>, Arc<Vec<u8>>>,
        depth: usize,
        issues: &mut Vec<String>,
    ) -> RefinedObject {
        let mut refined_dict = BTreeMap::new();
        for (k, v) in dict {
            let key_name = PdfName::from_bytes(k);
            let refined_v = Self::refine_dict_entry(
                doc,
                id,
                &key_name,
                v,
                table,
                handle_fonts,
                _stream_contexts,
                distilled_fonts,
                depth,
                issues,
            );
            refined_dict.insert(key_name, refined_v);
        }

        // Font Normalization
        if let Some("Font") = refined_dict.get(&PdfName::new("Type")).and_then(|o| o.as_str())
            && let Some(handle) = table.get(&id)
        {
            let resource = handle_fonts.get(&handle.index()).map(|arc| arc.as_ref());
            refined_dict = match font::normalize_font(refined_dict, resource) {
                RefinedObject::Dictionary(d) => d,
                _ => return RefinedObject::Null,
            };
        }

        Self::normalize_ui_text_fields(&mut refined_dict);
        RefinedObject::Dictionary(refined_dict)
    }

    fn sublimate_stream_content_static(
        id: (u32, u16),
        refined_dict: &BTreeMap<PdfName, RefinedObject>,
        content: &Bytes,
        table: &RemappingTable,
        _stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
    ) -> Option<RefinedObject> {
        let subtype = refined_dict.get(&PdfName::new("Subtype")).and_then(|o| o.as_str());
        let is_image = subtype == Some("Image");
        let is_form = subtype == Some("Form");
        let is_likely_content = (subtype.is_none() || is_form) && !is_image;

        if is_likely_content
            && let Some(handle) = table.get(&id)
            && let Some(context) = _stream_contexts.get(&handle.index())
        {
            let mut sublimator = crate::object::sublimation::parser::Sublimator::new(context);
            let commands = sublimator.sublimate(content);
            return Some(RefinedObject::Sublimated(
                refined_dict.clone(),
                crate::object::SublimatedData::Commands { items: commands },
            ));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn refine_stream_dict(
        doc: &lopdf::Document,
        id: (u32, u16),
        dict: &lopdf::Dictionary,
        table: &RemappingTable,
        handle_fonts: &BTreeMap<u32, Arc<FontResource>>,
        _stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
        distilled_fonts: &std::collections::BTreeMap<Handle<Object>, Arc<Vec<u8>>>,
        depth: usize,
        issues: &mut Vec<String>,
    ) -> BTreeMap<PdfName, RefinedObject> {
        let mut refined_dict = BTreeMap::new();
        for (k, v) in dict {
            refined_dict.insert(
                PdfName::from_bytes(k),
                Self::refine_recursive(
                    doc,
                    id,
                    v,
                    table,
                    handle_fonts,
                    _stream_contexts,
                    distilled_fonts,
                    depth,
                    issues,
                ),
            );
        }
        refined_dict
    }

    #[allow(clippy::too_many_arguments)]
    fn refine_stream(
        // RR-15 Limit: Dispatcher - refines streams by decompressing, sublimating contents, and mapping fonts
        doc: &lopdf::Document,
        id: (u32, u16),
        s: &lopdf::Stream,
        table: &RemappingTable,
        handle_fonts: &BTreeMap<u32, Arc<FontResource>>,
        _stream_contexts: &BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
        distilled_fonts: &std::collections::BTreeMap<Handle<Object>, Arc<Vec<u8>>>,
        depth: usize,
        issues: &mut Vec<String>,
    ) -> RefinedObject {
        log::debug!("Refinery: refining stream {:?}", id);
        let mut refined_dict = Self::refine_stream_dict(
            doc,
            id,
            &s.dict,
            table,
            handle_fonts,
            _stream_contexts,
            distilled_fonts,
            depth + 1,
            issues,
        );

        if let Some(handle) = table.get(&id)
            && let Some(distilled_data) = distilled_fonts.get(handle)
        {
            refined_dict.remove(&PdfName::new("Filter"));
            refined_dict.remove(&PdfName::new("DecodeParms"));
            return RefinedObject::Stream(refined_dict, Bytes::copy_from_slice(distilled_data));
        }

        let subtype = refined_dict.get(&PdfName::new("Subtype")).and_then(|o| o.as_str());
        let is_image = subtype == Some("Image");
        let is_font = refined_dict.contains_key(&PdfName::new("Length1"))
            || refined_dict.contains_key(&PdfName::new("Length2"))
            || refined_dict.contains_key(&PdfName::new("Length3"));

        let (content, was_decompressed) = if is_image || is_font {
            (Bytes::copy_from_slice(&s.content), false)
        } else {
            match s.decompressed_content() {
                Ok(c) => (Bytes::from(c), true),
                Err(_) => (Bytes::copy_from_slice(&s.content), false),
            }
        };

        if was_decompressed {
            refined_dict.remove(&PdfName::new("Filter"));
            refined_dict.remove(&PdfName::new("DecodeParms"));
        }

        if let Some(sub) = Self::sublimate_stream_content_static(
            id,
            &refined_dict,
            &content,
            table,
            _stream_contexts,
        ) {
            return sub;
        }

        RefinedObject::Stream(refined_dict, content)
    }
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
        .map(|n: PdfName| n.as_str() == "Image")
        .unwrap_or(false);

    let is_font = committed_dict.contains_key(&arena.name("Length1"))
        || committed_dict.contains_key(&arena.name("Length2"))
        || committed_dict.contains_key(&arena.name("Length3"));

    let sublimated = if is_image || is_font {
        // High-Fidelity Preservation (Phase 2 & 3):
        // Do NOT decode or re-compress images/fonts to internal format during refine.
        crate::object::SublimatedData::Raw(bytes)
    } else if bytes.len() > 4096 {
        let compressed = zstd::encode_all(&*bytes, 3).unwrap_or_else(|_| bytes.to_vec());
        crate::object::SublimatedData::Compressed { original_len: bytes.len(), data: compressed }
    } else {
        crate::object::SublimatedData::Raw(bytes)
    };

    let dh = arena.alloc_dict(committed_dict);
    Object::Stream(dh, std::sync::Arc::new(sublimated))
}

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
