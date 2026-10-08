//! Any bytes as an embedded font program, through the reconstruction a document's font
//! takes on loading: detected, and a Type 1 program converted to CFF (ROADMAP Z-1, Z-2a).
#![no_main]

use fepdf_font::cmap::CMap;
use fepdf_font::reconstruction::{FontInfo, FontReconstructor};
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;

struct Font;

impl FontInfo for Font {
    fn base_font(&self) -> &str {
        "Fuzz"
    }
    fn subtype(&self) -> &str {
        "Type1"
    }
    fn is_cid_keyed(&self) -> bool {
        false
    }
    fn is_cjk(&self) -> bool {
        false
    }
    fn cid_ordering(&self) -> Option<&str> {
        None
    }
    fn cid_to_gid_map(&self) -> Option<&BTreeMap<u32, u32>> {
        None
    }
    fn unified_map(&self) -> &BTreeMap<String, u32> {
        static EMPTY: std::sync::OnceLock<BTreeMap<String, u32>> = std::sync::OnceLock::new();
        EMPTY.get_or_init(BTreeMap::new)
    }
    fn encoding(&self) -> Option<&CMap> {
        None
    }
    fn glyph_width_by_gid(&self, _gid: u32) -> f32 {
        500.0
    }
    fn to_gid_hint(&self, cid: u32, _hint_name: Option<&str>) -> u32 {
        cid
    }
}

fuzz_target!(|data: &[u8]| {
    let _ = FontReconstructor::reconstruct(&Font, data);
});
