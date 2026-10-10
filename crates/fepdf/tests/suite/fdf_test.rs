//! **FDF annotations** (12.7.8, ROADMAP AA-3): a document's comments exported to a file of
//! their own and imported again, matched by `/NM` (ADR-0117).

use fepdf::comments::Comment;
use fepdf::{AnnotationAt, AnnotationState, Authorship, Operation, PdfDocument, PdfError};

/// A 400-point page with `annots` as its `/Annots`; `objects` are objects 4 on.
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

/// What a reviewer reads of a comment, without where it sits.
fn read(
    doc: &PdfDocument,
) -> Vec<(String, Option<String>, Option<String>, Option<usize>, Vec<String>)> {
    doc.comments(0)
        .expect("the page is there")
        .iter()
        .map(|c: &Comment| {
            let states = c.states.iter().map(|m| format!("{}:{:?}", m.author, m.state)).collect();
            (c.subtype.clone(), c.author.clone(), c.contents.clone(), c.reply_to, states)
        })
        .collect()
}

/// A note by Ann with no `/NM`, a link, a reply by Bo and Bo's state.
fn reviewed() -> PdfDocument {
    let mut doc = page_with(
        "4 0 R 5 0 R",
        &[
            "<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (Fix this) /T (Ann) >>",
            "<< /Type /Annot /Subtype /Link /Rect [50 50 90 60] /A << /S /URI /URI (https://example.com) >> >>",
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

/// **An export imported into a document without the comments gives the same comments**:
/// the reply answers the note, the state is Bo's, and the link is not exported (Table 246).
#[test]
fn an_export_imported_elsewhere_reproduces_the_comments() {
    let fdf = reviewed().export_fdf().expect("it exports");
    assert!(fdf.starts_with(b"%FDF-1.2"), "12.7.8.2.2's header");

    let mut other = blank();
    other.apply(Operation::ImportFdf { fdf }).expect("it imports");
    let mut expected = read(&reviewed());
    expected.retain(|c| c.0 != "Link");
    // The link was at index 1, so on the other page the reply and the state each sit one
    // place earlier; what answers what is the same.
    for c in &mut expected {
        c.3 = c.3.map(|to| if to > 1 { to - 1 } else { to });
    }
    assert_eq!(read(&other), expected);
}

/// **A second import of the same file changes nothing**: every annotation it carries is
/// named, so each one meets itself and replaces itself.
#[test]
fn importing_the_same_file_twice_is_importing_it_once() {
    let fdf = reviewed().export_fdf().expect("it exports");
    let mut other = blank();
    other.apply(Operation::ImportFdf { fdf: fdf.clone() }).expect("first");
    let once = read(&other);
    other.apply(Operation::ImportFdf { fdf }).expect("second");
    assert_eq!(read(&other), once);
}

/// **A matching `/NM` replaces in place**: the words are the file's, and a reply in the
/// document still answers the annotation, because it is the same object.
#[test]
fn a_matching_name_replaces_in_place_and_keeps_its_replies() {
    let mut doc = page_with(
        "4 0 R",
        &["<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (old) /NM (x) >>"],
    );
    doc.apply(Operation::ReplyToAnnotation {
        at: AnnotationAt { page: 0, index: 0 },
        contents: "kept".into(),
        by: by("Cy"),
    })
    .expect("it replies");
    let fdf = b"%FDF-1.2\n1 0 obj\n<< /FDF << /Annots [2 0 R] >> >>\nendobj\n2 0 obj\n<< /Type /Annot /Subtype /Text /Page 0 /Rect [10 10 30 30] /Contents (new) /NM (x) >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n";
    doc.apply(Operation::ImportFdf { fdf: fdf.to_vec() }).expect("it imports");

    let list = doc.comments(0).expect("the page");
    assert_eq!(list.len(), 2, "replaced, not added: {list:?}");
    assert_eq!(list[0].contents.as_deref(), Some("new"));
    assert_eq!(list[1].reply_to, Some(0), "the reply still answers it");
}

/// **What the document cannot take is left out and said**, and a file that is not FDF
/// changes nothing.
#[test]
fn a_page_the_document_lacks_is_left_out_and_a_non_fdf_is_refused() {
    let mut doc = blank();
    let fdf = b"%FDF-1.2\n1 0 obj\n<< /FDF << /Annots [2 0 R 3 0 R] >> >>\nendobj\n2 0 obj\n<< /Type /Annot /Subtype /Text /Page 7 /Rect [1 1 2 2] /Contents (far) >>\nendobj\n3 0 obj\n<< /Type /Annot /Subtype /Text /Page 0 /Rect [1 1 2 2] /Contents (near) >>\nendobj\n%%EOF\n";
    doc.apply(Operation::ImportFdf { fdf: fdf.to_vec() }).expect("it imports what it can");
    let list = doc.comments(0).expect("the page");
    assert_eq!(
        list.iter().map(|c| c.contents.clone()).collect::<Vec<_>>(),
        vec![Some("near".into())]
    );
    assert!(
        doc.decisions().iter().any(|d| d.clause == "12.7.8.3.4"),
        "the one left out is said: {:?}",
        doc.decisions()
    );

    let pdf = blank().export_fdf().expect("an empty export");
    let not_fdf = b"%PDF-2.0\n1 0 obj\n<< /Type /Catalog >>\nendobj\n%%EOF\n".to_vec();
    let refused = doc.apply(Operation::ImportFdf { fdf: not_fdf });
    assert!(matches!(refused, Err(PdfError::Refused { .. })), "{refused:?}");
    assert_eq!(doc.comments(0).expect("the page").len(), 1, "nothing changed");
    assert!(pdf.starts_with(b"%FDF-1.2"));
}

/// **The file is FDF by 12.7.8.1's rules, read from its bytes and not by this engine's
/// reader**: every cross-reference entry points at its own `n 0 obj`, every object is of
/// generation 0 and numbered once, and the trailer's `/Root` is the dictionary that
/// holds `/FDF`. qpdf cannot check it: it refuses a file with no page tree.
#[test]
fn the_export_is_well_formed_fdf_by_its_bytes() {
    let fdf = reviewed().export_fdf().expect("it exports");
    // Read as Latin-1, one character a byte, so that positions in `text` are positions in
    // the file for everything after the header.
    let text: String = fdf.iter().map(|b| char::from(*b)).collect();
    let xref_at = text.rfind("\nxref").expect("a cross-reference table") + 1;
    let mut lines = text[xref_at..].lines().skip(1);
    let header: Vec<usize> = lines
        .next()
        .expect("a subsection")
        .split_whitespace()
        .map(|n| n.parse().expect("a number"))
        .collect();
    assert_eq!(header[0], 0, "one subsection, from object 0");
    let mut seen = std::collections::BTreeSet::new();
    for number in 0..header[1] {
        let entry = lines.next().expect("an entry");
        let (offset, generation, kind) = (&entry[0..10], &entry[11..16], &entry[17..18]);
        if kind == "f" {
            continue;
        }
        assert_eq!(generation, "00000", "12.7.8.1: generation 0 only");
        let offset: usize = offset.parse().expect("an offset");
        let declared = format!("{number} 0 obj");
        // On the bytes, not on `text`: the header's binary comment is four bytes there and
        // three characters each here.
        assert!(
            fdf.get(offset..).is_some_and(|at| at.starts_with(declared.as_bytes())),
            "entry {number} points at {:?}",
            fdf.get(offset..offset + 12)
        );
        assert!(seen.insert(number), "{number} numbered twice");
    }
    let trailer = &text[text.rfind("trailer").expect("a trailer")..];
    let root: usize = trailer
        .split("/Root ")
        .nth(1)
        .expect("/Root")
        .split_whitespace()
        .next()
        .expect("n")
        .parse()
        .expect("n");
    let root_body = text
        .split(&format!("\n{root} 0 obj"))
        .nth(1)
        .expect("the catalogue")
        .split("endobj")
        .next()
        .expect("body");
    assert!(root_body.contains("/FDF"), "the root is the FDF catalogue: {root_body}");
    assert!(text.ends_with("%%EOF\r\n") || text.ends_with("%%EOF\n") || text.ends_with("%%EOF"));
}
