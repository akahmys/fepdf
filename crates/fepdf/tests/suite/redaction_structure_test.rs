//! **What a redaction leaves of the replacement text that read what it removed**
//! (14.9.3, 14.9.4, ROADMAP Y-10): a marked-content sequence's `/ActualText`, an
//! element's `/Alt` and `/ActualText`, and its ancestors'.
//!
//! Decided by the owner 2026-10-03: an element whose content goes whole is pruned; one
//! whose content goes in part, and its ancestors, have their replacement text replaced by
//! a marker, as iText pdfSweep 5.0.8 does.

use fepdf::{Operation, PdfDocument, Redaction, SaveOptions};

/// A tagged page drawing `content` with Helvetica as `/F1` and `resources` beside it,
/// its structure tree `tree` from object 6 on: object 6 is the tree's root, and the page
/// is `/StructParents 0`.
fn tagged(content: &str, resources: &str, tree: &[&str]) -> PdfDocument {
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /StructParents 0 \
               /Resources << /Font << /F1 5 0 R >> {resources} >> >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    bodies.extend(tree.iter().map(ToString::to_string));
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

fn redact(doc: &mut PdfDocument, region: (f64, f64, f64, f64)) {
    let redaction = Redaction { page: 0, regions: vec![region], fill: Some(vec![]) };
    doc.apply(Operation::Redact(redaction)).expect("it redacts");
}

/// Whether the saved file holds `marker`, literally or as hexadecimal digits.
fn saved_holds(doc: &PdfDocument, name: &str, marker: &str) -> bool {
    let path =
        std::env::temp_dir().join(format!("fepdf-redact-struct-{name}-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    doc.save_with_options(&path, "2.0", &options).expect("it writes");
    let file = std::fs::read(&path).expect("it is there");
    let _ = std::fs::remove_file(&path);
    let hex: String = marker.bytes().map(|b| format!("{b:02X}")).collect::<Vec<_>>().concat();
    let text = String::from_utf8_lossy(&file);
    text.contains(marker) || text.to_uppercase().contains(&hex)
}

/// Two paragraphs, one per line, each its own marked content and element.
fn two_paragraphs() -> PdfDocument {
    tagged(
        "/P <</MCID 0 /ActualText (MARKERACTUAL)>> BDC BT /F1 24 Tf 72 700 Td (AAA) Tj ET EMC \
         /P <</MCID 1>> BDC BT /F1 24 Tf 72 600 Td (BBB) Tj ET EMC",
        "",
        &[
            "<< /Type /StructTreeRoot /K [7 0 R] /ParentTree << /Nums [0 [8 0 R 9 0 R]] >> >>",
            "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R] /Alt (MARKERDOCUMENT) >>",
            "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K 0 /Alt (MARKERGONE) >>",
            "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K 1 /Alt (KEPTALT) >>",
        ],
    )
}

/// **A paragraph whose content goes whole is pruned**, its `/Alt` and its marked
/// content's `/ActualText` with it; the other paragraph keeps its own.
#[test]
fn an_element_whose_content_goes_whole_is_pruned() {
    let mut doc = two_paragraphs();
    redact(&mut doc, (60.0, 690.0, 300.0, 730.0));
    assert!(
        !saved_holds(&doc, "whole-alt", "MARKERGONE"),
        "the pruned element's /Alt is in the file"
    );
    assert!(
        !saved_holds(&doc, "whole-actual", "MARKERACTUAL"),
        "the marked content's /ActualText is in the file"
    );
    assert!(saved_holds(&doc, "whole-kept", "KEPTALT"), "the other paragraph's /Alt went");
    assert!(
        !saved_holds(&doc, "whole-doc", "MARKERDOCUMENT"),
        "the ancestor's /Alt still reads what went"
    );
    let arena = doc.inner().arena();
    let document = arena
        .get_object(arena.handle(7))
        .and_then(|o| o.as_dict_handle())
        .expect("the Document element");
    let kids = arena
        .dict_entry(document, arena.name("K"))
        .and_then(|k| k.as_array())
        .and_then(|k| arena.get_array(k));
    assert_eq!(kids.map(|k| k.len()), Some(1), "the emptied paragraph was not pruned");
}

/// **A paragraph whose content goes in part keeps its place, and its replacement text,
/// and its ancestors', become the marker.**
#[test]
fn an_element_touched_in_part_has_its_text_marked() {
    let mut doc = two_paragraphs();
    // Over the first A only.
    redact(&mut doc, (70.0, 690.0, 80.0, 730.0));
    assert!(
        !saved_holds(&doc, "part-alt", "MARKERGONE"),
        "the touched element's /Alt is unchanged"
    );
    assert!(
        !saved_holds(&doc, "part-actual", "MARKERACTUAL"),
        "the marked content's /ActualText is unchanged"
    );
    assert!(!saved_holds(&doc, "part-doc", "MARKERDOCUMENT"), "the ancestor's /Alt is unchanged");
    assert!(saved_holds(&doc, "part-marker", "[REDACTED]"), "no marker was written");
    assert!(saved_holds(&doc, "part-kept", "KEPTALT"), "the untouched paragraph's /Alt went");
    // The marked content reads as its /ActualText, which now says it was redacted.
    let text = doc.extract_text(0).expect("it reads");
    assert!(text.contains("[REDACTED]") && text.contains("BBB"), "{text:?}");
}

/// **A property list named from the resources is marked too**, in place, so the original
/// is not written beside a copy.
#[test]
fn a_named_property_list_is_marked() {
    let mut doc = tagged(
        "/Span /PL0 BDC BT /F1 24 Tf 72 700 Td (AAA) Tj ET EMC",
        "/Properties << /PL0 << /ActualText (MARKERNAMED) >> >>",
        &["<< /Type /StructTreeRoot /K [] >>"],
    );
    redact(&mut doc, (60.0, 690.0, 300.0, 730.0));
    assert!(
        !saved_holds(&doc, "named", "MARKERNAMED"),
        "the named list's /ActualText is in the file"
    );
}
