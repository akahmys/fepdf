//! Whether the faces this machine offers can be read at all.
//!
//! **Every platform face on macOS is a TrueType Collection**, and a collection puts its
//! fonts behind a header: read a table directory at offset 0 of one and the collection
//! header is parsed as if it were a font, so no table is found. A face then reads as
//! stating no permission and carrying no outlines, which is exactly how a face that
//! genuinely states neither reads — the failure has no shape of its own.
//!
//! Measured here on 2026-09-19: four faces, all `ttcf`, holding four to six fonts each.
//! Three carry TrueType outlines and state `fsType` 0; the Japanese one is CFF-based, with
//! `CFF `, `VORG`, `vhea` and `vmtx` and no `glyf`, and states an editable embedding.
//! **So Latin can be embedded here today and Japanese cannot**, for want of a CFF
//! subsetter rather than for want of a permission.
//!
//! What this asserts holds wherever it runs: a face this engine offers is a face it can
//! read the tables of. What it prints is what this machine happens to have.

use fepdf_font::embedding::embedding_permission;
use fepdf_font::metrics::read_metrics;
use fepdf_font::subset::glyph_outline;

/// **A face the engine hands out is one it can read.**
///
/// `read_metrics` needs `head` and `hhea`, which every SFNT has and no collection header
/// does, so this fails against a reader that does not resolve a `ttcf` — as it did for all
/// four of this machine's faces until one did.
#[test]
fn every_face_this_machine_offers_can_be_read() {
    let faces = fepdf_model::document::fallback_fonts();
    assert!(!faces.is_empty(), "this engine seeds a default face, so the map is never empty");

    for (kind, data) in &faces {
        let metrics = read_metrics(data);
        assert!(
            metrics.is_some(),
            "{kind:?} is handed out as a usable face and its tables cannot be found: \
             {} bytes, starting {:02X?}",
            data.len(),
            data.get(..4).unwrap_or_default()
        );
        assert!(
            metrics.is_some_and(|m| m.units_per_em > 0),
            "{kind:?} states a grid of nothing, which a real face does not"
        );
    }
}

/// What this machine has, for the record rather than as an assertion.
///
/// `cargo test -p fepdf-model --test platform_face_test -- --nocapture`
#[test]
fn what_this_machine_offers_is_printable() {
    for (kind, data) in &fepdf_model::document::fallback_fonts() {
        let outlines = (1..40u16).filter(|g| glyph_outline(data, *g).is_some()).count();
        println!(
            "{kind:?}: {} bytes, {}, permission {:?}, {outlines} of 39 glyphs with a TrueType outline",
            data.len(),
            if data.get(..4) == Some(b"ttcf") { "a collection" } else { "one face" },
            embedding_permission(data).map(|p| p.usage),
        );
    }
}
