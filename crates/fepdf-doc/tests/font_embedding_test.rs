//! What comes back out of a font this engine embedded.
//!
//! 9.7.4.3 requires a CIDFont's `/W` to be consistent with the widths in the program
//! beside it, which is a property a round trip can check on its own: write the font, read
//! the file back, and ask both places what a glyph advances by. The engine that wrote the
//! `/W` and the reader that reads `hmtx` are different code, so an agreement here is
//! evidence rather than a tautology.

use fepdf_doc::apply::font::{EmbeddedFace, embed_truetype};
use fepdf_model::document::Document;
use fepdf_model::ingest::IngestionOptions;
use fepdf_model::{Handle, Object, PdfArena};
use std::collections::BTreeMap;

use fepdf_fixtures::truetype_program as program;

/// An empty one-page document to embed into.
fn document() -> Document {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ];
    Document::open(fepdf_fixtures::assemble(&bodies).into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// The value of `key` in the dictionary `object` is or points at, resolved.
fn entry_of(arena: &PdfArena, object: &Object, key: &str) -> Option<Object> {
    let dh = match object.resolve(arena) {
        Object::Dictionary(dh) | Object::Stream(dh, _) => dh,
        _ => return None,
    };
    let dict = arena.get_dict(dh)?;
    dict.iter()
        .find(|(k, _)| arena.get_name(**k).is_some_and(|n| n.as_str() == key))
        .map(|(_, v)| v.resolve(arena))
}

/// The same, for the handle an embedding answers with.
fn entry(arena: &PdfArena, handle: Handle<Object>, key: &str) -> Option<Object> {
    entry_of(arena, &Object::Reference(handle), key)
}

fn name_of(arena: &PdfArena, object: &Object) -> Option<String> {
    arena.get_name(object.as_name()?).map(|n| n.as_str().to_string())
}

/// The glyph, and the text it stands for.
fn glyphs() -> BTreeMap<u16, String> {
    BTreeMap::from([(1u16, "図".to_string()), (2, "面".to_string()), (3, "A".to_string())])
}

/// **The widths in `/W` are the widths in the program** (9.7.4.3), through a round trip.
#[test]
fn the_width_array_agrees_with_the_program_beside_it() {
    let doc = document();
    let raw = program(&[1024, 2048, 1024, 512]);
    let handle = embed_truetype(
        &doc,
        &EmbeddedFace { program: &raw, base_font: "TestFace", glyphs: &glyphs() },
    )
    .expect("it embeds");

    let arena = doc.arena();
    let descendants = entry(arena, handle, "DescendantFonts").expect("it has descendants");
    let Object::Array(ah) = descendants else { panic!("DescendantFonts is not an array") };
    let cid_font =
        arena.get_array(ah).and_then(|items| items.first().cloned()).expect("one of them");
    let widths = entry_of(arena, &cid_font, "W").expect("it has a W");
    let Object::Array(wh) = widths else { panic!("W is not an array") };
    let items = arena.get_array(wh).expect("the W array is there");

    // `1 [w1 w2 w3]`: one run, because the glyphs asked for are consecutive.
    assert_eq!(items.len(), 2, "one group was expected: {items:?}");
    assert_eq!(items[0].as_f64(), Some(1.0), "the run starts at the first glyph");
    let Object::Array(run) = items[1].clone() else { panic!("the group holds no array") };
    let written: Vec<f64> =
        arena.get_array(run).expect("the run").iter().filter_map(Object::as_f64).collect();

    let expected: Vec<f64> = glyphs()
        .keys()
        .map(|gid| {
            let advance =
                fepdf_font::metrics::advance_width(&raw, *gid).expect("the program states one");
            f64::from(advance) * 1000.0 / 2048.0
        })
        .collect();
    assert_eq!(written, expected, "the W array and hmtx disagree");
    // The first glyph asked for advances a full em on a 2048 grid, which is 1000 in
    // glyph space: a writer that passed the program's units straight through would
    // answer 2048 here and agree with `hmtx` while being wrong.
    assert!(
        (written[0] - 1000.0).abs() < f64::EPSILON,
        "a full-em glyph on a 2048 grid is 1000 in glyph space, not {}",
        written[0]
    );
    assert!((written[1] - 500.0).abs() < f64::EPSILON);
}

/// The shape 9.7.4.3 and Table 124 ask for, and the encoding this engine chooses.
#[test]
fn the_font_is_a_type_zero_keyed_by_glyph() {
    let doc = document();
    let raw = program(&[1024, 2048, 1024, 512]);
    let handle = embed_truetype(
        &doc,
        &EmbeddedFace { program: &raw, base_font: "TestFace", glyphs: &glyphs() },
    )
    .expect("it embeds");
    let arena = doc.arena();

    assert_eq!(
        name_of(arena, &entry(arena, handle, "Subtype").expect("subtype")).as_deref(),
        Some("Type0")
    );
    assert_eq!(
        name_of(arena, &entry(arena, handle, "Encoding").expect("encoding")).as_deref(),
        Some("Identity-H")
    );
    let base =
        name_of(arena, &entry(arena, handle, "BaseFont").expect("base font")).expect("a name");
    assert!(base.ends_with("+TestFace"), "no subset tag: {base}");
    assert_eq!(
        fepdf_font::subset::subset_tag(&base).map(str::len),
        Some(6),
        "the tag is not six letters: {base}"
    );
}

/// **A tag derived from the subset**, so that the same glyphs of the same face twice are
/// one face and not two claiming to differ.
#[test]
fn the_subset_tag_is_the_same_for_the_same_subset() {
    let doc = document();
    let raw = program(&[1024, 2048, 1024, 512]);
    let face = EmbeddedFace { program: &raw, base_font: "TestFace", glyphs: &glyphs() };
    let first = embed_truetype(&doc, &face).expect("it embeds");
    let second = embed_truetype(&doc, &face).expect("it embeds again");
    let arena = doc.arena();
    assert_eq!(
        name_of(arena, &entry(arena, first, "BaseFont").expect("base font")),
        name_of(arena, &entry(arena, second, "BaseFont").expect("base font"))
    );
}

/// Without `/ToUnicode` the text this engine writes cannot be read back, which is the
/// defect this whole phase started from.
#[test]
fn every_glyph_says_what_it_stands_for() {
    let doc = document();
    let raw = program(&[1024, 2048, 1024, 512]);
    let handle = embed_truetype(
        &doc,
        &EmbeddedFace { program: &raw, base_font: "TestFace", glyphs: &glyphs() },
    )
    .expect("it embeds");
    let arena = doc.arena();
    let to_unicode = entry(arena, handle, "ToUnicode").expect("it has a ToUnicode");
    let Object::Stream(_, data) = to_unicode else { panic!("ToUnicode is not a stream") };
    let bytes = arena.get_stream_bytes(&data).expect("the stream reads");
    let cmap = String::from_utf8_lossy(&bytes);

    assert!(cmap.contains("<0001> <56F3>"), "図 is not mapped: {cmap}");
    assert!(cmap.contains("<0002> <9762>"), "面 is not mapped: {cmap}");
    assert!(cmap.contains("<0003> <0041>"), "A is not mapped: {cmap}");
    assert!(cmap.contains("3 beginbfchar"), "the section does not count its entries: {cmap}");
}

/// A stem this engine has not measured is stated as unknown, which 9.8.1 defines as 0.
#[test]
fn an_unmeasured_stem_is_stated_as_unknown() {
    let doc = document();
    let raw = program(&[1024, 2048, 1024, 512]);
    let handle = embed_truetype(
        &doc,
        &EmbeddedFace { program: &raw, base_font: "TestFace", glyphs: &glyphs() },
    )
    .expect("it embeds");
    let arena = doc.arena();
    let descendants = entry(arena, handle, "DescendantFonts").expect("descendants");
    let Object::Array(ah) = descendants else { panic!("not an array") };
    let cid_font = arena.get_array(ah).and_then(|i| i.first().cloned()).expect("one");
    let descriptor = entry_of(arena, &cid_font, "FontDescriptor").expect("a descriptor");
    assert_eq!(
        entry_of(arena, &descriptor, "StemV").and_then(|v| v.as_f64()),
        Some(0.0),
        "a stem nobody measured has to be stated as unknown, not estimated"
    );
    // Required entries, and a program with no `post` states no angle, so 0 is written and
    // an `Ambiguity` is recorded rather than the claim being made silently.
    assert!(entry_of(arena, &descriptor, "Flags").is_some(), "Flags is required");
    assert!(entry_of(arena, &descriptor, "FontBBox").is_some(), "FontBBox is required");
    assert_eq!(entry_of(arena, &descriptor, "ItalicAngle").and_then(|v| v.as_f64()), Some(0.0));
    assert!(
        doc.decisions.entries().iter().any(|d| d.clause == "9.8.1"),
        "writing an angle the program did not state was not recorded"
    );
    assert!(entry_of(arena, &descriptor, "FontFile2").is_some(), "the program is not there");
}
