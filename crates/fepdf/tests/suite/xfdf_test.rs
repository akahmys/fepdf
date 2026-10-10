//! **XFDF annotations** (ISO 19444-1, ROADMAP AA-4c): the same comments as FDF, as XML,
//! read and written by Tables 33 and 34, matched by name (ADR-0117) and drawn from their
//! entries where they carry no appearance (ADR-0119).

use fepdf::comments::Comment;
use fepdf::{AnnotationAt, AnnotationState, Authorship, Operation, PdfDocument, PdfError};

fn page_with(annots: &str, objects: &[&str]) -> PdfDocument {
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << >> /Annots [{annots}] >>"
        ),
    ];
    bodies.extend(objects.iter().map(ToString::to_string));
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

fn blank() -> PdfDocument {
    page_with("", &[])
}

fn by(author: &str) -> Authorship {
    Authorship { author: Some(author.to_owned()), when: None }
}

/// A note by Ann, Bo's reply and Bo's state.
fn reviewed() -> PdfDocument {
    let mut doc = page_with(
        "4 0 R",
        &[
            "<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (Fix <this> & \"that\") /T (Ann) /C [1 0.5 0] /F 4 >>",
        ],
    );
    let note = AnnotationAt { page: 0, index: 0 };
    doc.apply(Operation::ReplyToAnnotation { at: note, contents: "Done".into(), by: by("Bo") })
        .expect("it replies");
    doc.apply(Operation::SetAnnotationState {
        at: note,
        state: AnnotationState::Completed,
        by: by("Bo"),
    })
    .expect("it sets");
    doc
}

fn read(
    doc: &PdfDocument,
) -> Vec<(String, Option<String>, Option<String>, Option<usize>, Vec<String>)> {
    doc.comments(0)
        .expect("the page")
        .iter()
        .map(|c: &Comment| {
            let states = c.states.iter().map(|m| format!("{}:{:?}", m.author, m.state)).collect();
            (c.subtype.clone(), c.author.clone(), c.contents.clone(), c.reply_to, states)
        })
        .collect()
}

/// **An XFDF export imported into a document without the comments gives the same
/// comments**, and the file starts as 5.5.2 says it shall.
#[test]
fn an_xfdf_export_imported_elsewhere_reproduces_the_comments() {
    let xfdf = reviewed().export_xfdf().expect("it exports");
    assert!(xfdf.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">"));
    assert!(xfdf.contains("&lt;this&gt; &amp; &quot;that&quot;"), "5.8.2's escapes: {xfdf}");
    assert!(xfdf.contains("color=\"#FF8000\""), "Table 5's colour: {xfdf}");
    assert!(xfdf.contains("flags=\"print\""), "Table 5's flags: {xfdf}");

    let mut other = blank();
    other.apply(Operation::ImportXfdf { xfdf: xfdf.clone().into_bytes() }).expect("it imports");
    assert_eq!(read(&other), read(&reviewed()));
    other.apply(Operation::ImportXfdf { xfdf: xfdf.into_bytes() }).expect("again");
    assert_eq!(read(&other), read(&reviewed()), "a second import changes nothing");
}

/// **A file written by hand, as 6.4's examples are, comes in as Table 34 says**, with an
/// appearance for every annotation, and a reply that names a comment already on the page.
#[test]
fn a_hand_written_file_is_read_by_table_34() {
    let mut doc = page_with(
        "4 0 R",
        &["<< /Type /Annot /Subtype /Text /Rect [300 300 320 320] /Contents (old) /NM (there) >>"],
    );
    let xfdf = r##"<?xml version="1.0" encoding="UTF-8"?>
<xfdf xmlns="http://ns.adobe.com/xfdf/" xml:space="preserve">
<annots>
<highlight page="0" rect="20,20,120,40" color="#FFFF00" coords="20,40,120,40,20,20,120,20" name="h1" title="Cy" flags="print,locked"><contents>marked</contents></highlight>
<line page="0" rect="10,100,200,160" start="20,110" end="190,150" head="OpenArrow" tail="ClosedArrow" interior-color="#FF0000" width="2" name="l1"/>
<ink page="0" rect="10,200,120,300" color="#000000" name="i1"><inklist><gesture>20,210;60,280;100,220</gesture></inklist></ink>
<polygon page="0" rect="200,10,390,190" interior-color="#00FF00" name="p1"><vertices>210,20;380,20;300,180</vertices></polygon>
<freetext page="0" rect="20,320,200,360" justification="centered" name="f1"><contents>Typed</contents><defaultappearance>/Helv 12 Tf 0 0 1 rg</defaultappearance></freetext>
<text page="0" rect="300,300,320,320" inreplyto="there" title="Cy" name="r1"><contents>about the old one</contents></text>
<link page="0" rect="0,0,10,10"/>
</annots>
</xfdf>"##;
    doc.apply(Operation::ImportXfdf { xfdf: xfdf.as_bytes().to_vec() }).expect("it imports");
    let list = doc.comments(0).expect("the page");
    let kinds: Vec<&str> = list.iter().map(|c| c.subtype.as_str()).collect();
    assert_eq!(kinds, ["Text", "Highlight", "Line", "Ink", "Polygon", "FreeText", "Text"]);
    assert_eq!(list[6].reply_to, Some(0), "inreplyto named the comment already on the page");
    assert_eq!(list[1].author.as_deref(), Some("Cy"));
    assert!(
        doc.decisions().iter().any(|d| d.clause == "ISO 19444-1 6.4.1"),
        "the link was left out and said so"
    );

    let inner = doc.inner();
    let arena = inner.arena();
    let page = inner.resolve_to_dict(inner.page_handle(0).expect("page")).expect("dict");
    let annots = arena
        .dict_entry(page, arena.name("Annots"))
        .and_then(|a| a.resolve(arena).as_array())
        .and_then(|a| arena.get_array(a))
        .expect("annotations");
    let entry = |index: usize, key: &str| {
        let dict = annots[index].resolve(arena).as_dict_handle().expect("a dictionary");
        arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena))
    };
    for index in 1..annots.len() {
        assert!(entry(index, "AP").is_some(), "annotation {index} has no appearance");
    }
    assert_eq!(entry(1, "F").and_then(|f| f.as_integer()), Some(4 + 128), "print and locked");
    let numbers = |index: usize, key: &str| -> Vec<f64> {
        entry(index, key)
            .and_then(|a| a.as_array())
            .and_then(|a| arena.get_array(a))
            .unwrap_or_default()
            .iter()
            .filter_map(fepdf::Object::as_f64)
            .collect()
    };
    assert_eq!(numbers(2, "L"), vec![20.0, 110.0, 190.0, 150.0], "start and end are /L");
    assert_eq!(numbers(1, "C"), vec![1.0, 1.0, 0.0], "#FFFF00 is yellow");
    assert_eq!(numbers(4, "Vertices"), vec![210.0, 20.0, 380.0, 20.0, 300.0, 180.0]);
    assert_eq!(entry(5, "Q").and_then(|q| q.as_integer()), Some(1), "centered is /Q 1");
}

/// **A file attachment's bytes survive the round trip**, written as hexadecimal data.
#[test]
fn an_attached_files_bytes_go_out_and_come_back() {
    let xfdf = r#"<?xml version="1.0" encoding="UTF-8"?>
<xfdf xmlns="http://ns.adobe.com/xfdf/" xml:space="preserve"><annots>
<fileattachment page="0" rect="20,20,40,40" icon="Paperclip" file="note.txt" name="a1"><data mode="raw" encoding="hex" length="5" mimetype="text/plain">48656C6C6F</data></fileattachment>
</annots></xfdf>"#;
    let mut doc = blank();
    doc.apply(Operation::ImportXfdf { xfdf: xfdf.as_bytes().to_vec() }).expect("it imports");
    let out = doc.export_xfdf().expect("it exports");
    assert!(out.contains(">48656C6C6F</data>"), "the bytes came back: {out}");
    assert!(out.contains("file=\"note.txt\""), "and the name: {out}");
}

/// What is not XFDF is refused before anything changes; what XFDF cannot say is said.
#[test]
fn a_non_xfdf_is_refused_and_what_cannot_be_written_is_recorded() {
    let mut doc = blank();
    for bad in [&b"%PDF-2.0"[..], b"<notxfdf/>", b"\xff\xfe"] {
        let refused = doc.apply(Operation::ImportXfdf { xfdf: bad.to_vec() });
        assert!(matches!(refused, Err(PdfError::Refused { .. })), "{refused:?}");
    }
    assert!(doc.comments(0).expect("the page").is_empty(), "nothing changed");

    let callout = page_with(
        "4 0 R",
        &[
            "<< /Type /Annot /Subtype /FreeText /Rect [10 10 200 60] /DA (/Helv 12 Tf 0 g) /Contents (x) /CL [5 5 20 20] /IT /FreeTextCallout >>",
        ],
    );
    callout.export_xfdf().expect("it exports");
    assert!(
        callout.decisions().iter().any(|d| d.clause == "ISO 19444-1 6.7.1"),
        "the callout line XFDF cannot carry is said: {:?}",
        callout.decisions()
    );
}
