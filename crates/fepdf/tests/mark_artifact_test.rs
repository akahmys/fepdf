//! Marking tagged content as an artifact (ISO 32000-2 14.8.2.2; WTPDF 8.3).
//!
//! **Nothing could do it.** A running header tagged as a paragraph is content a reader
//! reads on every page; the vocabulary could retag it and move it and delete its element,
//! and not make it the artefact it is.

use fepdf::{IngestionOptions, Operation, Outcome, PdfDocument};
use fepdf_model::Handle;
use fepdf_model::object::Object;

/// A tagged page: a header, MCID 0, and a paragraph, MCID 1, each a `<P>`, objects 6 and 7.
fn page(content: &str) -> PdfDocument {
    let content = format!("{content}\n");
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /Lang (en) \
           /MarkInfo << /Marked true >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /StructParents 0 >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /StructTreeRoot /K [6 0 R 7 0 R] /ParentTree << /Nums [0 [6 0 R 7 0 R]] >> >>"
            .to_string(),
        "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /K 0 >>".to_string(),
        "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /K [1] >>".to_string(),
    ]);
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default()).expect("it opens")
}

const HEADER_AND_BODY: &str =
    "/P <</MCID 0>> BDC 0 190 50 5 re f EMC /P <</MCID 1>> BDC 0 0 50 50 re f EMC";

/// The page's content, as text.
fn content(doc: &PdfDocument) -> String {
    let arena = doc.inner().arena();
    let page = doc.inner().get_page_handle(0).expect("a page");
    let dict = arena.get_object(page).and_then(|p| p.as_dict_handle()).expect("a dictionary");
    let contents = arena.dict_entry(dict, arena.name("Contents")).expect("contents");
    let bytes = doc.inner().decode_stream(&contents.resolve(arena)).expect("it decodes");
    String::from_utf8_lossy(&bytes).into_owned()
}

/// **The header becomes a `Pagination` / `Header` artifact, and the tree lets it go**: the
/// parent tree's entry for MCID 0 is `null`, element 6 claims nothing, and element 7 still
/// claims MCID 1. Checkpoint 01 stays sound, and 18-002 now has an artefact to ask about.
#[test]
fn a_tagged_header_becomes_an_artifact() {
    let mut doc = page(HEADER_AND_BODY);
    doc.apply(Operation::MarkArtifact {
        page: 0,
        mcid: 0,
        kind: Some("Pagination".into()),
        subtype: Some("Header".into()),
    })
    .expect("it is marked");

    let text = content(&doc);
    assert!(text.contains("/Artifact"), "{text}");
    assert!(text.contains("/Pagination") && text.contains("/Header"), "{text}");
    assert!(!text.contains("/MCID 0"), "{text}");
    assert!(text.contains("/MCID 1"), "{text}");

    let arena = doc.inner().arena();
    let kids = |element: u32| {
        let dict = arena.get_object(Handle::new(element)).and_then(|o| o.as_dict_handle());
        match dict.and_then(|d| arena.dict_entry(d, arena.name("K"))).map(|k| k.resolve(arena)) {
            Some(Object::Array(a)) => arena.get_array(a).unwrap_or_default().len(),
            Some(_) => 1,
            None => 0,
        }
    };
    assert_eq!(kids(6), 0, "the header's element still claims its MCID");
    assert_eq!(kids(7), 1, "the paragraph lost its MCID");

    let report = doc.audit_ua2_report().expect("it audits");
    for condition in ["01-003", "01-004", "01-005"] {
        let outcome: Vec<Outcome> = report
            .findings
            .iter()
            .filter(|f| f.checkpoint == condition)
            .map(|f| f.outcome)
            .collect();
        assert_eq!(outcome, vec![Outcome::Sound], "{condition}");
    }
}

/// **A sequence that is not there, or that holds tagged content, is refused** — the second
/// would put content an element claims inside an artefact (01-004).
#[test]
fn what_cannot_become_an_artifact_is_refused() {
    let mut doc = page(HEADER_AND_BODY);
    let mark = |mcid| Operation::MarkArtifact { page: 0, mcid, kind: None, subtype: None };
    assert!(doc.apply(mark(5)).is_err(), "a missing MCID was marked");
    let mut nested = page("/Sect <</MCID 0>> BDC /P <</MCID 1>> BDC 0 0 5 5 re f EMC EMC");
    assert!(nested.apply(mark(0)).is_err(), "a sequence holding tagged content was marked");
    assert!(content(&nested).contains("/MCID 0"), "a refused mark changed the page");
}

/// **A mark on another page with the same number is not the one let go.** An integer in
/// `/K` is on the element's own page (14.7.4.2), and it was dropped on its number alone:
/// marking MCID 0 of the second page an artifact took the first page's MCID 0 out of the
/// element too, and left that page's content claimed by nothing.
#[test]
fn a_mark_of_the_same_number_on_another_page_stays() {
    let content = "/P <</MCID 0>> BDC 0 0 50 50 re f EMC\n";
    let stream = format!("<< /Length {} >>\nstream\n{content}endstream", content.len());
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 7 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /StructParents 0 >>"
            .to_string(),
        stream.clone(),
        "<< /Type /StructTreeRoot /K [6 0 R] /ParentTree << /Nums [0 [6 0 R] 1 [6 0 R]] >> >>"
            .to_string(),
        "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R \
           /K [0 << /Type /MCR /Pg 7 0 R /MCID 0 >>] >>"
            .to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 8 0 R \
           /StructParents 1 >>"
            .to_string(),
        stream,
    ]);
    let mut doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("it opens");
    doc.apply(Operation::MarkArtifact { page: 1, mcid: 0, kind: None, subtype: None })
        .expect("it is marked");
    let tree = doc.extract_struct_tree().expect("the document is tagged");
    assert_eq!(tree.children[0].mcids, vec![0], "the first page's mark went with the second's");
    assert_eq!(tree.children[0].mark_pages, vec![Some(0)], "the mark kept is not the first page's");
}
