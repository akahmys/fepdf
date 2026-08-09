use crate::Document;
use crate::Handle;
use crate::object::{Object, PdfName};
use std::collections::BTreeMap;

/// Container for extracted font binary data and its metadata.
pub struct FontData {
    pub data: Vec<u8>,
    pub length1: Option<u32>,
    pub length2: Option<u32>,
    pub length3: Option<u32>,
}

pub struct FontLoader;

impl FontLoader {
    /// Extracts font data from a FontDescriptor dictionary, with fallback search logic (Hardening).
    pub fn extract_data(
        fd_obj: &Object,
        doc: &Document,
        parent_dict: Option<&BTreeMap<Handle<PdfName>, Object>>,
    ) -> Option<FontData> {
        let arena = doc.arena();
        let fd_resolved = Object::resolve(fd_obj, arena);

        let fd_dict =
            if let Object::Dictionary(fdh) = fd_resolved { arena.get_dict(fdh) } else { None };

        // Try extracting from FontDescriptor first, then fallback to Parent Dictionary if available.
        if let Some(dict) = fd_dict
            && let Some(fd) = Self::extract_from_dict(&dict, doc)
        {
            return Some(fd);
        }

        if let Some(dict) = parent_dict
            && let Some(fd) = Self::extract_from_dict(dict, doc)
        {
            log::info!("[HARDENING] Found font data in main font dictionary (non-standard)");
            return Some(fd);
        }

        None
    }

    fn extract_from_dict(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
    ) -> Option<FontData> {
        let arena = doc.arena();

        // Priority: FontFile3 (CFF/OpenType) -> FontFile2 (TrueType) -> FontFile (Type 1)
        let keys = [arena.name("FontFile3"), arena.name("FontFile2"), arena.name("FontFile")];

        for key in keys {
            if let Some(ff) = dict.get(&key) {
                let resolved = Object::resolve(ff, arena);
                if let Ok(data) = doc.decode_stream(&resolved) {
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
