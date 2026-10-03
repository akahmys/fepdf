//! What 2.0 deprecates or keeps elsewhere, made 2.0's at load (Y-F28).

use fepdf::PdfDocument;
use fepdf_model::Object;
use fepdf_model::access::{entry, items, name_in};

/// One page with a Type 1 font, a TrueType one with a descriptor, a Type 3 one, a DCT
/// image with `/ColorTransform` in the wrong place, and a form whose widget brings `/DR`.
fn translated() -> PdfDocument {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Type /MarkInfo /Marked true >> \
           /ViewerPreferences << /Type /ViewerPreferences /HideToolbar true >> \
           /AcroForm << /Fields [9 0 R] /DR << /Encoding << /PDFDocEncoding 10 0 R >> >> >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [9 0 R] \
           /Resources << /Font << /A 4 0 R /B 5 0 R /C 7 0 R >> /XObject << /I 8 0 R >> >> >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Name /A >>",
        "<< /Type /Font /Subtype /TrueType /BaseFont /Arial /FontDescriptor 6 0 R >>",
        "<< /Type /FontDescriptor /FontName /Arial /Flags 32 /CharSet (/A/B) /Subtype /Type1C >>",
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 1 1] /FontMatrix [1 0 0 1 0 0] \
           /CharProcs << >> /Encoding << /Differences [] >> /CIDToGIDMap /Identity >>",
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB \
           /BitsPerComponent 8 /Filter /DCTDecode /ColorTransform 0 /Length 1 >>\nstream\n\0\nendstream",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (f) /Rect [0 0 10 10] /P 3 0 R \
           /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 4 0 R >> >> >>",
        "<< /Type /Encoding /Differences [] >>",
    ]);
    PdfDocument::open(bytes.into()).expect("the fixture opens")
}

fn said(doc: &PdfDocument, clause: &str) -> bool {
    doc.decisions().iter().any(|d| d.clause == clause)
}

/// **A font's deprecated `/Name`, a descriptor's `/CharSet` and a Type 3 font's
/// `/CIDToGIDMap` go, and each is said.**
#[test]
fn deprecated_font_keys_go() {
    let doc = translated();
    let arena = doc.inner().arena();
    let page = Object::Reference(doc.inner().get_page_handle(0).expect("a page"));
    let fonts =
        entry(arena, &entry(arena, &page, "Resources").expect("resources"), "Font").expect("fonts");
    let font = |name: &str| entry(arena, &fonts, name).expect("the font");
    assert!(entry(arena, &font("A"), "Name").is_none(), "a Type 1 font kept /Name");
    let descriptor = entry(arena, &font("B"), "FontDescriptor").expect("a descriptor");
    assert!(
        entry(arena, &descriptor, "CharSet").is_some(),
        "the open document lost /CharSet, which the audit asks 31-012 of"
    );
    assert!(entry(arena, &descriptor, "Subtype").is_none(), "a descriptor kept /Subtype");
    assert!(entry(arena, &font("C"), "CIDToGIDMap").is_none(), "a Type 3 font kept /CIDToGIDMap");
    assert!(said(&doc, "9.6.2.1") && said(&doc, "9.8.1"), "{:?}", doc.decisions());
}

/// **`/ColorTransform` moves to the DCT filter's decode parameters, where Table 13 puts
/// it**, and keeps its value: the colours it decides stay the same.
#[test]
fn color_transform_moves_to_the_decode_parameters() {
    let doc = translated();
    let arena = doc.inner().arena();
    let page = Object::Reference(doc.inner().get_page_handle(0).expect("a page"));
    let xobjects = entry(arena, &entry(arena, &page, "Resources").expect("resources"), "XObject")
        .expect("xobjects");
    let image = entry(arena, &xobjects, "I").expect("the image");
    assert!(entry(arena, &image, "ColorTransform").is_none(), "it stayed in the image dictionary");
    let params = entry(arena, &image, "DecodeParms").expect("the decode parameters");
    assert_eq!(entry(arena, &params, "ColorTransform").and_then(|v| v.as_integer()), Some(0));
    assert!(said(&doc, "7.4.8"), "{:?}", doc.decisions());
}

/// **A widget's `/DR` joins the form's, and the form's PDF 1.0 `/Encoding` goes**; the
/// catalogue's `/MarkInfo` and `/ViewerPreferences` lose a `/Type` their tables lack.
#[test]
fn form_resources_and_catalogue_entries_are_made_2_0s() {
    let doc = translated();
    let arena = doc.inner().arena();
    let catalog = Object::Reference(*doc.inner().root_handle());
    let form = entry(arena, &catalog, "AcroForm").expect("the form");
    let dr = entry(arena, &form, "DR").expect("the form's resources");
    assert!(entry(arena, &dr, "Encoding").is_none(), "the form kept a 1.0 /Encoding");
    let fonts = entry(arena, &dr, "Font").expect("the widget's font joined the form's");
    assert!(entry(arena, &fonts, "Helv").is_some());
    let widget = items(arena, &form, "Fields").into_iter().next().expect("the field");
    assert!(entry(arena, &widget, "DR").is_none(), "the widget kept its /DR");
    for key in ["MarkInfo", "ViewerPreferences"] {
        let dict = entry(arena, &catalog, key).expect("the entry");
        assert_eq!(name_in(arena, &dict, "Type"), None, "/{key} kept a /Type");
    }
    for clause in ["7.8.3", "12.7.3", "14.7.1", "12.2"] {
        assert!(said(&doc, clause), "{clause} was not said: {:?}", doc.decisions());
    }
}

/// **A descriptor's `/CharSet` is left out of what a save writes**, and the save says so;
/// the open document keeps it, since the audit asks 31-012 of it.
#[test]
fn a_subset_claim_is_left_out_of_the_save() {
    let doc = translated();
    let path = std::env::temp_dir().join(format!("fepdf-conform-{}.pdf", std::process::id()));
    let decisions =
        doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it saves");
    let written = std::fs::read(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(
        decisions.iter().any(|d| d.clause == "9.8.1"),
        "the save left it out in silence: {decisions:?}"
    );
    let back = PdfDocument::open(written.into()).expect("it reads back");
    let arena = back.inner().arena();
    let page = Object::Reference(back.inner().get_page_handle(0).expect("a page"));
    let fonts =
        entry(arena, &entry(arena, &page, "Resources").expect("resources"), "Font").expect("fonts");
    let descriptor =
        entry(arena, &entry(arena, &fonts, "B").expect("B"), "FontDescriptor").expect("descriptor");
    assert!(entry(arena, &descriptor, "CharSet").is_none(), "the save wrote /CharSet");
}
