//! **A document opens with every annotation given the appearance it can be given**
//! (ADR-0120, ROADMAP AA-4d): widgets from their fields, media a stand-in, and what cannot
//! be drawn said to be so.

use fepdf::PdfDocument;
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// A 200-point page whose `/Annots` is objects 4 on, given as `annots`, under a form whose
/// fields are those same objects.
fn opened(annots: &[&str]) -> PdfDocument {
    let refs: Vec<String> = (0..annots.len()).map(|i| format!("{} 0 R", i + 4)).collect();
    let mut bodies = vec![
        format!(
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [{}] /DA (/Helv 0 Tf 0 g) >> >>",
            refs.join(" ")
        ),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Annots [{}] >>",
            refs.join(" ")
        ),
    ];
    bodies.extend(annots.iter().map(ToString::to_string));
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

fn drawn(doc: &PdfDocument) -> Recorder {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

fn said(doc: &PdfDocument, what: &str) -> bool {
    doc.decisions().iter().any(|d| d.clause == "12.5.5" && d.found.contains(what))
}

/// **A text field's value is set where it had no appearance**, and an empty one gets an
/// appearance with nothing in it and no word about a face.
#[test]
fn a_text_fields_value_is_drawn() {
    let doc = opened(&[
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Hello) /Rect [20 20 180 40] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (empty) /Rect [20 60 180 80] >>",
    ]);
    let text = drawn(&doc).text();
    assert!(text.contains("Hello"), "the value was not drawn: {text:?}");
    assert!(said(&doc, "/Widget"));
    assert!(
        !doc.decisions().iter().any(|d| d.found.contains("cannot show \"\"")),
        "an empty field looked for a face: {:?}",
        doc.decisions()
    );
}

/// **A check box gets its two states**, the on one named by `/AS`, and draws a mark when on.
#[test]
fn a_check_box_gets_an_on_and_an_off_state() {
    let doc = opened(&[
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (box) /V /Ja /AS /Ja /Rect [20 20 40 40] >>",
    ]);
    let marks = drawn(&doc);
    assert!(marks.count("stroke") >= 2, "the frame and the check: {}", marks.count("stroke"));
    let inner = doc.inner();
    let arena = inner.arena();
    let page = inner.resolve_to_dict(inner.page_handle(0).expect("page")).expect("dict");
    let annots = arena
        .dict_entry(page, arena.name("Annots"))
        .and_then(|a| a.resolve(arena).as_array())
        .and_then(|a| arena.get_array(a))
        .expect("annotations");
    let widget = annots[0].resolve(arena).as_dict_handle().expect("a dictionary");
    let ap = arena
        .dict_entry(widget, arena.name("AP"))
        .and_then(|a| a.resolve(arena).as_dict_handle())
        .expect("an /AP");
    let normal = arena
        .dict_entry(ap, arena.name("N"))
        .and_then(|n| n.resolve(arena).as_dict_handle())
        .expect("/N");
    let states: Vec<String> = arena
        .get_dict(normal)
        .unwrap_or_default()
        .keys()
        .filter_map(|k| arena.get_name_str(*k))
        .collect();
    assert!(states.contains(&"Ja".to_owned()) && states.contains(&"Off".to_owned()), "{states:?}");
}

/// **Media this engine does not play gets a stand-in**, and what is its own appearance gets
/// none and is said to be in breach of Table 166. A screen gets none and nothing is said.
#[test]
fn media_get_a_stand_in_and_a_printers_mark_is_said() {
    let doc = opened(&[
        "<< /Type /Annot /Subtype /3D /Rect [20 20 100 100] >>",
        "<< /Type /Annot /Subtype /Movie /Rect [110 20 190 100] /Movie << /F (clip.mov) >> >>",
        "<< /Type /Annot /Subtype /PrinterMark /Rect [20 120 60 160] /MN /ColorBar >>",
        "<< /Type /Annot /Subtype /Screen /Rect [110 120 190 190] >>",
    ]);
    let marks = drawn(&doc);
    // Two frames filled, and the movie's play mark; the 3D cube is stroked, with the two
    // frames' borders.
    assert!(marks.count("fill") >= 3, "two frames and a play mark: {}", marks.count("fill"));
    assert!(marks.count("stroke") >= 3, "two borders and a cube: {}", marks.count("stroke"));
    assert!(said(&doc, "/3D") && said(&doc, "/Movie"));
    assert!(
        !doc.decisions().iter().any(|d| d.found.contains("/Screen")),
        "a screen without one shows nothing, as 12.5.6.18 says: {:?}",
        doc.decisions()
    );
    assert!(
        doc.decisions()
            .iter()
            .any(|d| d.found.contains("/PrinterMark") && d.action.contains("left it without one")),
        "{:?}",
        doc.decisions()
    );
}

/// **A choice value that holds itself is not followed** (Rule 6): the field opens, given an
/// appearance with nothing in it.
#[test]
fn a_choice_value_that_holds_itself_opens() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] /DA (/Helv 0 Tf 0 g) >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Annots [4 0 R] >>",
        "<< /Type /Annot /Subtype /Widget /FT /Ch /T (pick) /V 5 0 R /Rect [20 20 180 40] >>",
        "[5 0 R]",
    ];
    let doc = PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("it opens");
    assert!(said(&doc, "/Widget"), "{:?}", doc.decisions());
}
