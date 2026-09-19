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

/// **A face is inspected as a CFF exactly when it carries one.**
///
/// `extract_cff_stream` tested for an SFNT by looking for `OTTO` or the version tag at
/// offset 0 and did not list `ttcf`, although `detect` three hundred lines above it does.
/// So every face on this machine took the other branch and was read as a bare CFF: the
/// three TrueType collections reported a glyph count out of a failed parse instead of
/// saying they have none, and the Japanese one reported 1,024 glyphs where it has 20,327.
///
/// This asks the question that separates the two: does the engine agree with itself about
/// which faces have a charstring index at all?
#[test]
fn a_face_is_inspected_as_a_cff_exactly_when_it_carries_one() {
    for (kind, data) in &fepdf_model::document::fallback_fonts() {
        let carries = fepdf_font::subset::cff_table(data).is_some();
        let inspected = fepdf_font::reconstruction::FontReconstructor::inspect_cff(data).is_ok();
        assert_eq!(
            carries, inspected,
            "{kind:?} carries a CFF table: {carries}, and is inspected as one: {inspected}"
        );
    }
}

/// **A face with a CFF subsets, and what comes out draws the same glyphs.**
///
/// The corpus's CFF programs are the general case and are checked elsewhere; this is the
/// one that matters on this platform, where the Japanese face is a CID-keyed CFF of 20,327
/// glyphs with an `FDArray` — a shape the samples do not carry.
#[test]
fn a_face_with_a_cff_subsets_and_keeps_its_charstrings() {
    let mut exercised = 0;
    for (kind, data) in &fepdf_model::document::fallback_fonts() {
        let Ok(count) = fepdf_font::cff::glyph_count(data) else { continue };
        if count < 12 {
            continue;
        }
        exercised += 1;
        let wanted: std::collections::BTreeSet<u16> = (1..12).collect();
        let subsetted = fepdf_font::cff::subset_cff(data, &wanted)
            .unwrap_or_else(|e| panic!("{kind:?} carries a CFF and will not subset: {e}"));

        assert_eq!(
            fepdf_font::cff::glyph_count(&subsetted).ok(),
            Some(count),
            "{kind:?}: a subset that changes the glyph count moves every id after it"
        );
        for gid in &wanted {
            assert_eq!(
                fepdf_font::cff::charstring(&subsetted, *gid),
                fepdf_font::cff::charstring(data, *gid),
                "{kind:?}: glyph {gid} did not survive"
            );
        }
        assert!(
            subsetted.len() < data.len() / 2,
            "{kind:?}: a subset of eleven glyphs is not half the program"
        );
        let charset = |program: &[u8]| {
            fepdf_font::reconstruction::FontReconstructor::inspect_cff(fepdf_font::cff::body(
                program,
            ))
            .ok()
            .and_then(|info| info.sid_to_gid)
        };
        assert_eq!(
            charset(&subsetted),
            charset(data),
            "{kind:?}: the charset moved out from under the offset that names it"
        );
        // The block after the charstrings moves by a different amount again, and holds
        // the `FDArray` a CID-keyed face picks a font dictionary from.
        assert_eq!(
            fepdf_font::cff::private_dicts(&subsetted),
            fepdf_font::cff::private_dicts(data),
            "{kind:?}: a font dictionary points where its Private DICT used to be"
        );
        assert!(
            !fepdf_font::cff::private_dicts(data).is_empty(),
            "{kind:?}: no font dictionary was read, so the comparison above asks nothing"
        );
    }
    println!("{exercised} of this machine's faces carry a CFF and were subsetted");
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
        if let Ok(cff) = fepdf_font::reconstruction::FontReconstructor::inspect_cff(data) {
            println!("    a CFF of {} glyphs, CID-keyed: {}", cff.num_glyphs, cff.is_cid);
            let wanted: std::collections::BTreeSet<u16> = (1..12).collect();
            match fepdf_font::cff::subset_cff(data, &wanted) {
                Ok(subsetted) => println!(
                    "    subsetted to 11 glyphs: {} bytes, from {} ({}%)",
                    subsetted.len(),
                    data.len(),
                    subsetted.len() * 100 / data.len().max(1)
                ),
                Err(e) => println!("    will not subset: {e}"),
            }
        }
    }
}

/// **The face chosen out of a collection is the regular weight, upright.**
///
/// Face 0 is that face in all four of this machine's collections — Helvetica of six,
/// Times of four, Hiragino Mincho of four — so taking the first was right here by
/// convention. This asks the question the convention answers by luck: a collection that
/// listed a bold or an italic first would be set in it without a word.
#[test]
fn the_face_taken_from_a_collection_is_the_regular_one() {
    for (kind, data) in &fepdf_model::document::fallback_fonts() {
        let chosen = fepdf_font::metrics::regular_face(data);
        let weight = weight_of(data, chosen);
        assert!(
            weight.is_none_or(|w| (300..=500).contains(&w)),
            "{kind:?}: face {chosen} was chosen and states weight {weight:?}"
        );
        assert!(!italic(data, chosen), "{kind:?}: face {chosen} was chosen and is italic");
    }
}

/// `OS/2.usWeightClass` of face `index`, through the engine's own reader.
fn weight_of(program: &[u8], index: u32) -> Option<u16> {
    let bold = fepdf_font::metrics::regular_face(program);
    let _ = bold;
    face_u16(program, index, b"OS/2", 4)
}

fn italic(program: &[u8], index: u32) -> bool {
    face_u16(program, index, b"head", 44).is_some_and(|style| style & 0x0002 != 0)
}

/// A `uint16` read out of a face of a collection, the long way, so that the test does not
/// ask the code under test where to look.
fn face_u16(program: &[u8], index: u32, tag: &[u8; 4], offset: usize) -> Option<u16> {
    let base = if program.get(..4) == Some(b"ttcf") {
        let at = 12 + (index as usize) * 4;
        u32::from_be_bytes(program.get(at..at + 4)?.try_into().ok()?) as usize
    } else {
        0
    };
    let count = u16::from_be_bytes([*program.get(base + 4)?, *program.get(base + 5)?]) as usize;
    (0..count).find_map(|i| {
        let e = base + 12 + i * 16;
        if program.get(e..e + 4)? != tag {
            return None;
        }
        let start = u32::from_be_bytes(program.get(e + 8..e + 12)?.try_into().ok()?) as usize;
        Some(u16::from_be_bytes([*program.get(start + offset)?, *program.get(start + offset + 1)?]))
    })
}
