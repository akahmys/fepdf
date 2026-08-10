//! Font loader and binary program extractor.

use fepdf_core::arena::PdfArena;
use fepdf_core::handle::Handle;
use fepdf_core::object::{Object, PdfName};
use std::collections::BTreeMap;

/// Container for extracted font binary data and its metadata.
pub struct FontData {
    /// The font program's bytes.
    pub data: Vec<u8>,
    /// `/Length1`: length of the clear-text portion.
    pub length1: Option<u32>,
    /// `/Length2`: length of the encrypted portion.
    pub length2: Option<u32>,
    /// `/Length3`: length of the trailing zeros section.
    pub length3: Option<u32>,
}

/// Extracts embedded font programs from font descriptors.
pub struct FontLoader;

impl FontLoader {
    /// Extracts font data from a FontDescriptor dictionary, with fallback search logic (Hardening).
    pub fn extract_data<F>(
        fd_obj: &Object,
        arena: &PdfArena,
        decode_stream: F,
        parent_dict: Option<&BTreeMap<Handle<PdfName>, Object>>,
    ) -> Option<FontData>
    where
        F: Fn(&Object) -> Option<Vec<u8>>,
    {
        let fd_resolved = Object::resolve(fd_obj, arena);

        let fd_dict =
            if let Object::Dictionary(fdh) = fd_resolved { arena.get_dict(fdh) } else { None };

        // Try extracting from FontDescriptor first, then fallback to Parent Dictionary if available.
        if let Some(dict) = fd_dict
            && let Some(fd) = Self::extract_from_dict(&dict, arena, &decode_stream)
        {
            return Some(fd);
        }

        if let Some(dict) = parent_dict
            && let Some(fd) = Self::extract_from_dict(dict, arena, &decode_stream)
        {
            log::info!("[HARDENING] Found font data in main font dictionary (non-standard)");
            return Some(fd);
        }

        None
    }

    fn extract_from_dict<F>(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        decode_stream: &F,
    ) -> Option<FontData>
    where
        F: Fn(&Object) -> Option<Vec<u8>>,
    {
        // Priority: FontFile3 (CFF/OpenType) -> FontFile2 (TrueType) -> FontFile (Type 1)
        let keys = [arena.name("FontFile3"), arena.name("FontFile2"), arena.name("FontFile")];

        for key in keys {
            if let Some(ff) = dict.get(&key) {
                let resolved = Object::resolve(ff, arena);
                if let Some(data) = decode_stream(&resolved) {
                    let mut length1 = None;
                    let mut length2 = None;
                    let mut length3 = None;

                    // If it's a stream, check its dictionary for lengths
                    if let Object::Stream(dh, _) = resolved
                        && let Some(sd) = arena.get_dict(dh)
                    {
                        length1 = sd
                            .get(&arena.name("Length1"))
                            .and_then(|o| Object::resolve(o, arena).as_integer())
                            .map(|i| i as u32);
                        length2 = sd
                            .get(&arena.name("Length2"))
                            .and_then(|o| Object::resolve(o, arena).as_integer())
                            .map(|i| i as u32);
                        length3 = sd
                            .get(&arena.name("Length3"))
                            .and_then(|o| Object::resolve(o, arena).as_integer())
                            .map(|i| i as u32);
                    }

                    return Some(FontData { data: data.to_vec(), length1, length2, length3 });
                }
            }
        }
        None
    }
}
