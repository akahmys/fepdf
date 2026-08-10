//! Mapping integration tests for fepdf-core.

use fepdf_core::arena::PdfArena;
use fepdf_core::font::{FontMetrics, FontResource, cmap::CMap};
use fepdf_core::object::PdfName;
use std::collections::BTreeMap;
use std::sync::Arc;

#[test]
fn test_code_to_cid_mismatch_reproduction() {
    let arena = PdfArena::new();

    let mut enc_cmap = CMap::default();
    let mut mappings_cid = BTreeMap::new();
    mappings_cid.insert(vec![0x65], 100);
    enc_cmap.mappings_cid = Arc::new(mappings_cid);

    let mut tu_cmap = CMap::default();
    let mut mappings = BTreeMap::new();
    mappings.insert(vec![0x65], "e".to_string());
    tu_cmap.mappings = Arc::new(mappings);

    let mut res = FontResource {
        subtype: PdfName::new("Type0"),
        base_font: PdfName::new("TestFont"),
        is_cid_keyed: true,
        encoding: Some(enc_cmap),
        to_unicode: Some(tu_cmap),
        ..FontResource::new_initial(
            PdfName::new("Type0"),
            PdfName::new("TestFont"),
            FontMetrics::default(),
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            &BTreeMap::new(),
            &arena,
            false,
            None,
            None,
        )
    };

    res.build_unified_map();

    let cid = res.unified_map.get("e").copied();
    assert_eq!(
        cid,
        Some(100),
        "Unicode 'e' should map to CID 100 (via Encoding), not character code 0x65"
    );
}
