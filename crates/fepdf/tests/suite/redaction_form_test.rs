//! **What a form XObject draws under a redaction region goes too** (ROADMAP Y-10).
//!
//! A form is content like a page's. The redaction copies each one a region meets, redacts
//! the copy with the region taken into its space, and draws the copy where the form was;
//! another drawing of the same form keeps the original.

use fepdf::{Operation, PdfDocument, Redaction, SaveOptions};

/// A page with `content` and Helvetica as `/F1`, drawing forms from object 5 on: `forms`
/// are written as objects 5, 6, … and named `/Fm0`, `/Fm1`, … in the page's resources.
fn page_with_forms(content: &str, forms: &[String]) -> PdfDocument {
    let names = (0..forms.len()).fold(String::new(), |mut out, n| {
        use std::fmt::Write as _;
        let _ = write!(out, "/Fm{n} {} 0 R ", 5 + n);
        out
    });
    let font = 5 + forms.len();
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents 4 0 R \
               /Resources << /Font << /F1 {font} 0 R >> /XObject << {names}>> >> >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    bodies.extend_from_slice(forms);
    bodies.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// A form drawing `content` over a 200-point box, with `extra` in its dictionary.
fn form(content: &str, extra: &str) -> String {
    format!(
        "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] {extra} /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )
}

/// A form saying SECRET at its (10, 100), in the font `/F1` its own resources name as
/// object `font`.
fn secret_form(font: usize) -> String {
    form(
        "BT /F1 20 Tf 10 100 Td (SECRET) Tj ET",
        &format!("/Resources << /Font << /F1 {font} 0 R >> >>"),
    )
}

fn redact(doc: &mut PdfDocument, region: (f64, f64, f64, f64)) {
    let redaction = Redaction { page: 0, regions: vec![region], fill: Some(vec![]) };
    doc.apply(Operation::Redact(redaction)).expect("it redacts");
}

/// The file the document saves to, uncompressed, as text.
fn saved(doc: &PdfDocument, name: &str) -> String {
    let path =
        std::env::temp_dir().join(format!("fepdf-redact-form-{name}-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    doc.save_with_options(&path, "2.0", &options).expect("it writes");
    let file = std::fs::read(&path).expect("it is there");
    let _ = std::fs::remove_file(&path);
    String::from_utf8_lossy(&file).to_string()
}

/// Whether `file` holds SECRET, as a literal or as the hexadecimal string a save writes
/// a parsed content stream's text as.
fn holds_secret(file: &str) -> bool {
    file.contains("SECRET") || file.contains("534543524554")
}

/// **A form's text under the region goes, from the page and from the file**: the original
/// form, drawn nowhere else, is not written.
#[test]
fn a_forms_text_under_the_region_goes() {
    let mut doc = page_with_forms("q 1 0 0 1 50 50 cm /Fm0 Do Q", &[secret_form(6)]);
    let said = doc
        .what_redaction_removes(&Redaction {
            page: 0,
            regions: vec![(0.0, 0.0, 400.0, 400.0)],
            fill: None,
        })
        .expect("it reads");
    assert_eq!(said.glyphs.len(), 6, "the six glyphs of SECRET, said to go");
    assert!(
        said.glyphs.iter().all(|g| g.0 >= 60.0 && g.1 >= 140.0),
        "not placed on the page: {:?}",
        said.glyphs
    );
    redact(&mut doc, (0.0, 0.0, 400.0, 400.0));
    assert!(!doc.extract_text(0).expect("it reads").contains("SECRET"));
    assert!(!holds_secret(&saved(&doc, "whole")), "the file still holds the form's text");
}

/// **Another drawing of the same form keeps it**: only the drawing the region meets is
/// pointed at a redacted copy.
#[test]
fn another_drawing_of_the_form_keeps_it() {
    let mut doc = page_with_forms(
        "q 1 0 0 1 0 0 cm /Fm0 Do Q q 1 0 0 1 0 200 cm /Fm0 Do Q",
        &[secret_form(6)],
    );
    redact(&mut doc, (0.0, 0.0, 400.0, 150.0));
    let text = doc.extract_text(0).expect("it reads");
    assert_eq!(text.matches("SECRET").count(), 1, "{text:?}");
}

/// **A form's own matrix places the region inside it**: drawn at twice the size, the
/// region over its left half on the page is its left quarter... of its own box, and the
/// text on the right of it stays.
#[test]
fn a_forms_matrix_places_the_region() {
    let wide = form(
        "BT /F1 20 Tf 10 100 Td (LEFT) Tj ET BT /F1 20 Tf 150 100 Td (RIGHT) Tj ET",
        "/Matrix [2 0 0 1 0 0] /Resources << /Font << /F1 6 0 R >> >>",
    );
    let mut doc = page_with_forms("/Fm0 Do", &[wide]);
    // LEFT is drawn on the page from x 20; RIGHT from x 300.
    redact(&mut doc, (0.0, 0.0, 200.0, 400.0));
    let text = doc.extract_text(0).expect("it reads");
    assert!(!text.contains("LEFT") && text.contains("RIGHT"), "{text:?}");
}

/// **A form drawn by a form is entered too.**
#[test]
fn a_nested_form_is_entered() {
    let outer = form("/Inner Do", "/Resources << /XObject << /Inner 6 0 R >> >>");
    let inner = secret_form(7);
    let mut doc = page_with_forms("/Fm0 Do", &[outer, inner]);
    redact(&mut doc, (0.0, 0.0, 400.0, 400.0));
    assert!(!doc.extract_text(0).expect("it reads").contains("SECRET"));
    assert!(!holds_secret(&saved(&doc, "nested")), "the file still holds the inner form's text");
}

/// **A form with no resources of its own reads the page's** (7.8.3), and its text goes.
#[test]
fn a_form_reading_the_pages_resources_is_entered() {
    let bare = form("BT /F1 20 Tf 10 100 Td (SECRET) Tj ET", "");
    let mut doc = page_with_forms("/Fm0 Do", &[bare]);
    redact(&mut doc, (0.0, 0.0, 400.0, 400.0));
    assert!(!doc.extract_text(0).expect("it reads").contains("SECRET"));
}

/// **A form that draws itself is refused**, and nothing changes: entering it would not end.
#[test]
fn a_form_drawing_itself_is_refused() {
    let looping = form(
        "BT /F1 20 Tf 10 100 Td (SECRET) Tj ET /Me Do",
        "/Resources << /XObject << /Me 5 0 R >> /Font << /F1 6 0 R >> >>",
    );
    let mut doc = page_with_forms("/Fm0 Do", &[looping]);
    let refused = doc.apply(Operation::Redact(Redaction {
        page: 0,
        regions: vec![(0.0, 0.0, 400.0, 400.0)],
        fill: None,
    }));
    assert!(refused.is_err(), "a form drawing itself was redacted");
}

/// A tagged page drawing one form whose text is one paragraph, named by the tree through
/// an MCR with `/Stm`; the paragraph is object 8 and its `/Alt` MARKERFORMALT.
fn tagged_form_page() -> PdfDocument {
    let tagged_form = form(
        "/P <</MCID 0>> BDC BT /F1 20 Tf 10 100 Td (SECRET) Tj ET EMC",
        "/StructParents 1 /Resources << /Font << /F1 6 0 R >> >>",
    );
    let content = "/Fm0 Do";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 7 0 R /MarkInfo << /Marked true >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents 4 0 R \
           /Resources << /Font << /F1 6 0 R >> /XObject << /Fm0 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        tagged_form,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /StructTreeRoot /K [8 0 R] /ParentTree << /Nums [1 [8 0 R]] >> >>".to_string(),
        "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /Alt (MARKERFORMALT) \
           /K << /Type /MCR /Pg 3 0 R /Stm 5 0 R /MCID 0 >> >>"
            .to_string(),
    ];
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// How many kids the structure tree's root has.
fn root_kids(doc: &PdfDocument) -> usize {
    let arena = doc.inner().arena();
    let root =
        arena.get_object(arena.handle(7)).and_then(|o| o.as_dict_handle()).expect("the root");
    arena
        .dict_entry(root, arena.name("K"))
        .and_then(|k| k.as_array())
        .and_then(|k| arena.get_array(k))
        .map_or(0, |k| k.len())
}

/// **A mark inside a form, which the structure tree names by `/Stm`, is followed into the
/// copy**: the element naming the original would have kept it, and the text it drew, in
/// the file; and the element whose content went whole is pruned with its `/Alt`.
#[test]
fn a_form_mark_the_tree_names_goes_with_its_element() {
    let mut doc = tagged_form_page();
    redact(&mut doc, (0.0, 0.0, 400.0, 400.0));
    let file = saved(&doc, "tagged");
    assert!(!holds_secret(&file), "the original form is still in the file");
    assert!(!file.contains("MARKERFORMALT"), "the element's /Alt is still in the file");
    assert_eq!(root_kids(&doc), 0, "the element whose content went was not pruned");
}

/// **One touched in part keeps its place, and its `/Alt` becomes the marker.**
#[test]
fn a_form_mark_touched_in_part_has_its_element_marked() {
    let mut doc = tagged_form_page();
    // Over the S of SECRET only: the form is drawn at the origin.
    redact(&mut doc, (10.0, 95.0, 15.0, 125.0));
    let file = saved(&doc, "tagged-part");
    assert!(!file.contains("MARKERFORMALT"), "the element's /Alt is unchanged");
    assert_eq!(root_kids(&doc), 1, "the element touched in part was pruned");
}
