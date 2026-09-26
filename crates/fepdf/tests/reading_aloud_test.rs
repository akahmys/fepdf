//! What a synthesiser is handed: a tagged document's words in the structure's order,
//! each in its language, with the lexicons that say how to say them (ROADMAP W-19a).
//!
//! **Testable without sound**, which is why it is separate from binding a synthesiser
//! (W-19b). Every fixture draws its words in an order other than the one its structure
//! gives, so a reading that followed the page would fail.

use fepdf::reading::{Passage, Spoken};
use fepdf::{IngestionOptions, Operation, PdfDocument};

/// A one-page tagged document.
///
/// Object 5 is Helvetica as `/F1`, 6 the structure tree root holding `kids`, and the
/// elements from 7 on are `elements` in order; `catalog` and `page` are added to those
/// dictionaries, and `more` follows the elements.
fn tagged(content: &str, kids: &str, elements: &[&str], catalog: &str, page: &str) -> PdfDocument {
    tagged_with(content, kids, elements, catalog, page, &[])
}

fn tagged_with(
    content: &str,
    kids: &str,
    elements: &[&str],
    catalog: &str,
    page: &str,
    more: &[&str],
) -> PdfDocument {
    let mut bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R {catalog} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> {page} >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
        format!("<< /Type /StructTreeRoot /K [{kids}] >>"),
    ];
    bodies.extend(elements.iter().map(|e| (*e).to_string()));
    bodies.extend(more.iter().map(|e| (*e).to_string()));
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// Words shown at `(x, y)` under `/MCID mcid`.
fn marked(mcid: u32, x: u32, y: u32, words: &str) -> String {
    format!("/P << /MCID {mcid} >> BDC BT /F1 12 Tf {x} {y} Td ({words}) Tj ET EMC\n")
}

fn said(passages: &[Passage]) -> Vec<&str> {
    passages.iter().map(|p| p.text.as_str()).collect()
}

/// **Two columns drawn right first are read left first**, because the structure says so.
#[test]
fn the_order_is_the_structures_not_the_pages() {
    let content = marked(0, 220, 300, "right column") + &marked(1, 20, 300, "left column");
    let doc = tagged(
        &content,
        "7 0 R 8 0 R",
        &[
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 1 >>",
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>",
        ],
        "",
        "",
    );
    let reading = doc.reading();
    assert_eq!(said(&reading.passages), ["left column", "right column"]);
    assert!(reading.passages.iter().all(|p| p.spoken == Spoken::Content && p.page == Some(0)));
}

/// **A span inside a paragraph is read where it stands**, between the paragraph's two
/// marks, not after both.
#[test]
fn a_child_is_read_between_the_marks_either_side_of_it() {
    let content =
        marked(2, 20, 100, "three") + &marked(1, 20, 200, "two") + &marked(0, 20, 300, "one");
    let doc = tagged(
        &content,
        "7 0 R",
        &[
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K [0 8 0 R 2] >>",
            "<< /Type /StructElem /S /Span /P 7 0 R /Pg 3 0 R /K 1 >>",
        ],
        "",
        "",
    );
    assert_eq!(said(&doc.reading().passages), ["one", "two", "three"]);
}

/// **Each passage carries the language in force** (14.9.2): the catalogue's until an
/// element says otherwise, and that element's for everything below it.
#[test]
fn the_language_is_inherited_and_overridden() {
    let content =
        marked(0, 20, 300, "Bonjour") + &marked(1, 20, 200, "Hello") + &marked(2, 20, 100, "World");
    let doc = tagged(
        &content,
        "7 0 R 8 0 R",
        &[
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>",
            "<< /Type /StructElem /S /Div /P 6 0 R /Pg 3 0 R /Lang (en-GB) /K [9 0 R] >>",
            "<< /Type /StructElem /S /P /P 8 0 R /Pg 3 0 R /K [1 2] >>",
        ],
        "/Lang (fr-FR)",
        "",
    );
    let passages = doc.reading().passages;
    let langs: Vec<Option<&str>> = passages.iter().map(|p| p.lang.as_deref()).collect();
    // Two lines of one paragraph, broken where extraction breaks them — composed together,
    // not run into one word as they were when each mark was composed apart.
    assert_eq!(said(&passages), ["Bonjour", "Hello\nWorld"]);
    assert_eq!(langs, [Some("fr-FR"), Some("en-GB")]);
}

/// **What an element says in place of its content is read in place of it**: `/Alt` for a
/// figure, `/E` for an abbreviation, `/ActualText` for a replacement — and nothing of the
/// content below is read as well.
#[test]
fn replacements_are_read_instead_of_the_content() {
    let content =
        marked(0, 20, 300, "Dr.") + &marked(1, 20, 200, "ff") + &marked(2, 20, 100, "drawn");
    let doc = tagged(
        &content,
        "7 0 R 8 0 R 9 0 R",
        &[
            "<< /Type /StructElem /S /Span /P 6 0 R /Pg 3 0 R /E (Doctor) /K 0 >>",
            "<< /Type /StructElem /S /Span /P 6 0 R /Pg 3 0 R /ActualText (off) /K 1 >>",
            "<< /Type /StructElem /S /Figure /P 6 0 R /Pg 3 0 R /Alt (A drawing) /K 2 >>",
        ],
        "",
        "",
    );
    let passages = doc.reading().passages;
    assert_eq!(said(&passages), ["Doctor", "off", "A drawing"]);
    let spoken: Vec<Spoken> = passages.iter().map(|p| p.spoken).collect();
    assert_eq!(spoken, [Spoken::Expansion, Spoken::ActualText, Spoken::Alt]);
}

/// **A section's own `/ActualText` replaces its glyphs**, though the interpreter begins
/// the replacement before it opens the section's mark.
#[test]
fn a_marked_sections_actual_text_replaces_its_glyphs() {
    let content = "/Span << /MCID 0 /ActualText (fi) >> BDC BT /F1 12 Tf 20 300 Td (X) Tj ET EMC";
    let doc = tagged(
        content,
        "7 0 R",
        &["<< /Type /StructElem /S /Span /P 6 0 R /Pg 3 0 R /K 0 >>"],
        "",
        "",
    );
    assert_eq!(said(&doc.reading().passages), ["fi"]);
}

/// **A phoneme goes with the words, in the alphabet in force** — an ancestor's
/// `/PhoneticAlphabet`, which Table 355 says applies to its children.
#[test]
fn a_phoneme_is_carried_in_the_alphabet_above_it() {
    let content = marked(0, 20, 300, "Kato");
    let doc = tagged(
        &content,
        "7 0 R",
        &[
            "<< /Type /StructElem /S /Div /P 6 0 R /Pg 3 0 R /PhoneticAlphabet /x-sampa /K [8 0 R] >>",
            "<< /Type /StructElem /S /Span /P 7 0 R /Pg 3 0 R /Phoneme (kato) /K 0 >>",
        ],
        "",
        "",
    );
    let passage = doc.reading().passages.remove(0);
    assert_eq!(passage.text, "Kato");
    assert_eq!(passage.phoneme, Some(("x-sampa".to_owned(), "kato".to_owned())));
}

/// **An annotation's `/MCID 0` is not the page's** (14.7.4.2): its appearance is a stream
/// of its own, and its words are not read as the page's.
#[test]
fn an_annotations_marks_are_not_the_pages() {
    let appearance = "/P << /MCID 0 >> BDC BT /F1 12 Tf 0 0 Td (Widget) Tj ET EMC";
    let doc = tagged_with(
        &marked(0, 20, 300, "Body"),
        "7 0 R",
        &["<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>"],
        "",
        "/Annots [8 0 R]",
        &[
            "<< /Type /Annot /Subtype /Square /Rect [100 100 200 150] /AP << /N 9 0 R >> >>",
            &format!(
                "<< /Type /XObject /Subtype /Form /BBox [0 0 100 50] \
                 /Resources << /Font << /F1 5 0 R >> >> /Length {} >>\nstream\n{appearance}\nendstream",
                appearance.len()
            ),
        ],
    );
    assert_eq!(said(&doc.reading().passages), ["Body"]);
}

/// **A lexicon is written where Table 354 names it and read back from there**, through a
/// save — it was written to the catalogue as `/PL`, which no reader looks for.
#[test]
fn a_lexicon_is_named_by_the_structure_tree_root() {
    let lexicon = b"<?xml version=\"1.0\"?><lexicon version=\"1.0\"/>".to_vec();
    let mut doc = tagged(
        &marked(0, 20, 300, "Body"),
        "7 0 R",
        &["<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>"],
        "",
        "",
    );
    doc.apply(Operation::SetPronunciationLexicon { lexicon_xml_bytes: lexicon.clone() })
        .expect("the lexicon is named");
    let path = std::env::temp_dir().join(format!("fepdf_lexicon_{}.pdf", std::process::id()));
    doc.save_as_version(&path, "2.0").expect("it saves");
    let saved = std::fs::read(&path).expect("what was written reads");
    let _ = std::fs::remove_file(&path);
    let reopened = PdfDocument::open(saved.into()).expect("it reopens");
    assert_eq!(reopened.reading().lexicons, vec![lexicon]);

    let arena = reopened.inner().arena();
    let catalog =
        reopened.inner().resolve_to_dict(reopened.inner().catalog_handle().expect("a catalogue"));
    let catalog = arena.get_dict(catalog.expect("a dictionary")).expect("entries");
    assert!(!catalog.contains_key(&arena.name("PL")), "the catalogue still carries /PL");
}

/// A document with no structure tree has nowhere to name a lexicon, and says so.
#[test]
fn a_lexicon_needs_a_structure_tree() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>",
    ];
    let mut doc = PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("it opens");
    let refused = doc
        .apply(Operation::SetPronunciationLexicon { lexicon_xml_bytes: b"<lexicon/>".to_vec() })
        .expect_err("refused");
    assert!(refused.to_string().contains("structure tree"), "{refused}");
    assert!(doc.reading().passages.is_empty(), "an untagged document was read in some order");
}
