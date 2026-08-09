//! Refinery 2.1 Refined PDF 2.0 Object Model.
//!
//! This model adheres strictly to ISO 32000-2:2020 and utilizes the Handle system
//! for all internal references, ensuring maximum memory efficiency.

use crate::handle::Handle;
use crate::{PdfArena, PdfResult};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
pub mod sublimation;

/// A trait for types that can be initialized from a PDF Object within an Arena.
pub trait FromPdfObject: Sized {
    /// Attempts to create an instance of this type from the given Object and Arena.
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self>;
}

/// A trait for types that map to a specific part of the ISO 32000-2 specification.
pub trait PdfSchema {
    /// Returns the ISO 32000-2:2020 clause associated with this type.
    fn iso_clause() -> &'static str;
}

impl FromPdfObject for Handle<BTreeMap<Handle<PdfName>, Object>> {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let resolved = obj.resolve(arena);
        match resolved {
            Object::Reference(h) => Ok(h.cast()),
            Object::Dictionary(h) => Ok(h),
            _ => Err(crate::PdfError::Parse {
                pos: 0,
                message: format!("Expected dictionary handle, got {:?}", resolved).into(),
            }),
        }
    }
}

impl FromPdfObject for Handle<Vec<Object>> {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let resolved = obj.resolve(arena);
        match resolved {
            Object::Reference(h) => Ok(h.cast()),
            Object::Array(h) => Ok(h),
            _ => Err(crate::PdfError::Parse {
                pos: 0,
                message: format!("Expected array handle, got {:?}", resolved).into(),
            }),
        }
    }
}

impl FromPdfObject for Handle<Object> {
    fn from_pdf_object(obj: Object, _arena: &PdfArena) -> PdfResult<Self> {
        match obj {
            Object::Reference(h) => Ok(h),
            _ => {
                // If it's not a reference, we can't really have a "handle to the object"
                // in the sense of an indirect reference.
                Err(crate::PdfError::Parse {
                    pos: 0,
                    message: format!("Expected reference handle, got {:?}", obj).into(),
                })
            }
        }
    }
}

impl FromPdfObject for Handle<PdfName> {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let resolved = obj.resolve(arena);
        resolved.as_name().ok_or_else(|| crate::PdfError::Parse {
            pos: 0,
            message: format!("Expected name handle, got {:?}", resolved).into(),
        })
    }
}

impl FromPdfObject for bool {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        obj.resolve(arena)
            .as_bool()
            .ok_or_else(|| crate::PdfError::Parse { pos: 0, message: "Expected boolean".into() })
    }
}

impl FromPdfObject for i64 {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        obj.resolve(arena)
            .as_integer()
            .ok_or_else(|| crate::PdfError::Parse { pos: 0, message: "Expected integer".into() })
    }
}

impl FromPdfObject for f64 {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        obj.resolve(arena)
            .as_f64()
            .ok_or_else(|| crate::PdfError::Parse { pos: 0, message: "Expected real".into() })
    }
}

impl FromPdfObject for String {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let resolved = obj.resolve(arena);
        if let Some(text) = resolved.as_text() {
            Ok(text.to_string())
        } else if let Some(bytes) = resolved.as_string() {
            Ok(crate::refine::text::recover_string(bytes))
        } else {
            Err(crate::PdfError::Parse {
                pos: 0,
                message: format!("Expected string or text, got {:?}", resolved).into(),
            })
        }
    }
}

impl FromPdfObject for Object {
    fn from_pdf_object(obj: Object, _arena: &PdfArena) -> PdfResult<Self> {
        Ok(obj)
    }
}

impl<T: FromPdfObject> FromPdfObject for Option<T> {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        if matches!(obj.resolve(arena), Object::Null) {
            Ok(None)
        } else {
            T::from_pdf_object(obj, arena).map(Some)
        }
    }
}

/// ISO 32000-2:2020 Clause 7.3.5 - Name Objects
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PdfName(pub String);

impl PdfName {
    pub fn new(s: &str) -> Self {
        Self(s.to_string())
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(crate::refine::text::recover_string(bytes))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_string_lossy(&self) -> String {
        self.0.clone()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl FromPdfObject for PdfName {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let name_handle = obj
            .resolve(arena)
            .as_name()
            .ok_or_else(|| crate::PdfError::Parse { pos: 0, message: "Expected name".into() })?;
        arena
            .get_name(name_handle)
            .ok_or_else(|| crate::PdfError::Arena("Missing name in arena".into()))
    }
}

impl AsRef<[u8]> for PdfName {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::borrow::Borrow<str> for PdfName {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Object {
    /// Boolean objects (Clause 7.3.2)
    Boolean(bool),
    /// Numeric objects (Integer) (Clause 7.3.3)
    Integer(i64),
    /// Numeric objects (Real) (Clause 7.3.3)
    Real(f64),
    /// String objects (Clause 7.3.4)
    String(Bytes),
    /// Name objects (Clause 7.3.5)
    Name(Handle<PdfName>),
    /// Array objects (Clause 7.3.6)
    /// References an entry in the Arena that holds the actual Vec<Object>.
    Array(Handle<Vec<Object>>),
    /// Dictionary objects (Clause 7.3.7)
    /// References an entry in the Arena that holds the BTreeMap.
    Dictionary(Handle<BTreeMap<Handle<PdfName>, Object>>),
    /// Stream objects (Clause 7.3.8)
    /// References a dictionary handle and holds the sublimated or raw data.
    Stream(Handle<BTreeMap<Handle<PdfName>, Object>>, Arc<SublimatedData>),
    /// Hexadecimal string objects (Clause 7.3.4.3)
    Hex(Bytes),
    /// Text string objects (Refinery extension for UTF-8 internal management)
    Text(String),
    /// Null object (Clause 7.3.9)
    Null,
    /// Reference to an indirect object (external to this object but in the same arena).
    Reference(Handle<Object>),
}

/// A container for sublimated (normalized) data in the Arena.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SublimatedData {
    /// Pre-parsed drawing commands (Content Streams).
    Commands { items: Vec<sublimation::Command> },
    /// Pre-decoded image data (RGBA).
    Image { width: u32, height: u32, format: crate::graphics::PixelFormat, data: Vec<u8> },
    /// Zstd-compressed raw bytes (for Images/Fonts/Thumbnails).
    Compressed { original_len: usize, data: Vec<u8> },
    /// Raw uncompressed bytes.
    Raw(Bytes),
}

impl Object {
    pub fn as_name(&self) -> Option<Handle<PdfName>> {
        if let Self::Name(h) = self { Some(*h) } else { None }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(f) => Some(*f),
            Self::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<Handle<Vec<Object>>> {
        if let Self::Array(h) = self { Some(*h) } else { None }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Boolean(b) = self { Some(*b) } else { None }
    }

    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_, _))
    }

    pub fn as_dict_handle(&self) -> Option<Handle<BTreeMap<Handle<PdfName>, Object>>> {
        match self {
            Self::Dictionary(h) => Some(*h),
            Self::Stream(h, _) => Some(*h),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        if let Self::Integer(i) = self { Some(*i) } else { None }
    }

    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            Self::String(s) => Some(s),
            Self::Hex(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        if let Self::Text(t) = self { Some(t) } else { None }
    }

    pub fn as_reference(&self) -> Option<Handle<Object>> {
        if let Self::Reference(h) = self { Some(*h) } else { None }
    }

    pub fn resolve(&self, arena: &PdfArena) -> Self {
        if let Self::Reference(h) = self {
            arena.get_object(*h).unwrap_or(Self::Null)
        } else {
            self.clone()
        }
    }

    pub fn is_likely_content_stream(&self, arena: &PdfArena) -> bool {
        if let Some(dh) = self.as_dict_handle()
            && let Some(dict) = arena.get_dict(dh)
        {
            let subtype = dict
                .get(&arena.name("Subtype"))
                .and_then(|o| o.resolve(arena).as_name())
                .and_then(|n| arena.get_name(n))
                .map(|n| n.as_str().to_string());

            // Standard content streams (Pages) have no Subtype.
            // Form XObjects have /Subtype /Form.
            if subtype.is_none() || subtype.as_deref() == Some("Form") {
                // Robustness: Ensure it's not explicitly a different type like Image or Font
                let type_name = dict
                    .get(&arena.name("Type"))
                    .and_then(|o| o.resolve(arena).as_name())
                    .and_then(|n| arena.get_name(n))
                    .map(|n| n.as_str().to_string());

                if let Some(ref t) = type_name {
                    if t == "XObject" || t == "Page" {
                        return true;
                    }
                    if t == "Font" || t == "FontDescriptor" {
                        return false;
                    }
                }
                return true;
            }
        }
        false
    }

    pub fn from_lopdf(
        obj: &lopdf::Object,
        arena: &PdfArena,
        table: &crate::arena::RemappingTable,
    ) -> Self {
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
            lopdf::Object::Name(n) => Self::Name(arena.intern_name(PdfName::from_bytes(n))),
            lopdf::Object::Array(arr) => {
                let items: Vec<Object> =
                    arr.iter().map(|o| Self::from_lopdf(o, arena, table)).collect();
                Self::Array(arena.alloc_array(items))
            }
            lopdf::Object::Dictionary(dict) => {
                let mut map = BTreeMap::new();
                for (k, v) in dict {
                    let k_handle = arena.intern_name(PdfName::from_bytes(k));
                    let v_obj = Self::from_lopdf(v, arena, table);
                    map.insert(k_handle, v_obj);
                }
                Self::Dictionary(arena.alloc_dict(map))
            }
            lopdf::Object::Stream(s) => {
                let mut map = BTreeMap::new();
                for (k, v) in &s.dict {
                    let k_handle = arena.intern_name(PdfName::from_bytes(k));
                    let v_obj = Self::from_lopdf(v, arena, table);
                    map.insert(k_handle, v_obj);
                }
                Self::Stream(
                    arena.alloc_dict(map),
                    SublimatedData::Raw(Bytes::copy_from_slice(&s.content)).into(),
                )
            }
            lopdf::Object::Reference(id) => {
                let handle = table.get(&(id.0, id.1)).cloned().unwrap_or(Handle::new(0));
                Self::Reference(handle)
            }
            lopdf::Object::Null => Self::Null,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Reference {
    pub id: u32,
    pub generation: u16,
}

impl From<Handle<Object>> for Reference {
    fn from(h: Handle<Object>) -> Self {
        Self { id: h.index(), generation: 0 }
    }
}

impl From<Reference> for Handle<Object> {
    fn from(r: Reference) -> Self {
        Self::new(r.id)
    }
}

#[derive(Debug, Clone)]
pub struct ObjectEntry {
    pub object: Object,
    pub generation: u16,
}

impl Eq for Object {}

impl PartialOrd for Object {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Object {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Object::Boolean(a), Object::Boolean(b)) => a.cmp(b),
            (Object::Integer(a), Object::Integer(b)) => a.cmp(b),
            (Object::Real(a), Object::Real(b)) => a.total_cmp(b),
            (Object::String(a), Object::String(b)) => a.cmp(b),
            (Object::Hex(a), Object::Hex(b)) => a.cmp(b),
            (Object::Text(a), Object::Text(b)) => a.cmp(b),
            (Object::Name(a), Object::Name(b)) => a.cmp(b),
            (Object::Null, Object::Null) => Ordering::Equal,
            (Object::Reference(a), Object::Reference(b)) => a.cmp(b),
            (Object::Array(a), Object::Array(b)) => a.cmp(b),
            (Object::Dictionary(a), Object::Dictionary(b)) => a.cmp(b),
            (Object::Stream(a, _), Object::Stream(b, _)) => a.cmp(b),
            (a, b) => a.variant_index().cmp(&b.variant_index()),
        }
    }
}

impl Object {
    fn variant_index(&self) -> u8 {
        match self {
            Object::Null => 0,
            Object::Boolean(_) => 1,
            Object::Integer(_) => 2,
            Object::Real(_) => 3,
            Object::String(_) => 4,
            Object::Hex(_) => 5,
            Object::Text(_) => 6,
            Object::Name(_) => 7,
            Object::Reference(_) => 8,
            Object::Array(_) => 9,
            Object::Dictionary(_) => 10,
            Object::Stream(_, _) => 11,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, crate::FromPdfObject)]
    struct TestDict {
        #[pdf_key("Foo")]
        foo: i64,
        #[pdf_key("Bar")]
        bar: Option<bool>,
        #[pdf_key(name = "NewFeature", since = 2.0)]
        new_feature: Option<i64>,
        #[pdf_key(name = "Def", default = "123")]
        def: i64,
    }

    #[test]
    fn test_macro_expansion() {
        let arena = PdfArena::with_version(1.7);
        let mut map = BTreeMap::new();
        map.insert(arena.name("Foo"), Object::Integer(42));
        map.insert(arena.name("Bar"), Object::Boolean(true));
        map.insert(arena.name("NewFeature"), Object::Integer(100));

        let dict_handle = arena.alloc_dict(map.clone());
        let obj = Object::Dictionary(dict_handle);

        let test_dict = TestDict::from_pdf_object(obj, &arena).unwrap();
        assert_eq!(test_dict.foo, 42);
        assert_eq!(test_dict.bar, Some(true));
        assert_eq!(test_dict.new_feature, None);
        assert_eq!(test_dict.def, 123);

        let arena_20 = PdfArena::with_version(2.0);
        let mut map_20 = BTreeMap::new();
        map_20.insert(arena_20.name("Foo"), Object::Integer(42));
        map_20.insert(arena_20.name("Bar"), Object::Boolean(true));
        map_20.insert(arena_20.name("NewFeature"), Object::Integer(100));
        map_20.insert(arena_20.name("Def"), Object::Integer(999));

        let dict_handle_20 = arena_20.alloc_dict(map_20);
        let obj_20 = Object::Dictionary(dict_handle_20);
        let test_dict_20 = TestDict::from_pdf_object(obj_20, &arena_20).unwrap();
        assert_eq!(test_dict_20.new_feature, Some(100));
        assert_eq!(test_dict_20.def, 999);
    }
}
