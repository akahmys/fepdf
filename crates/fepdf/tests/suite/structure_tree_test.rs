//! `print_structure` prints the structure tree, which is what its caller is named for.
//!
//! **`fepdf inspect tree` is documented "Dump hierarchical logical structure tree"**, and
//! until 2026-09-06 it printed one line:
//!
//! ```text
//! --- [ DOCUMENT STRUCTURE ] ---
//! Structure Tree Root found: Handle<Object>(93)
//! ```
//!
//! — measured on `samples/fugaku.pdf`, which is tagged. `print_structure` asked for the
//! root handle and formatted it. No walk, no elements, and an arena type name in
//! user-facing output, which is the vocabulary Rule A keeps out of frontends.
//!
//! The engine already had the walk: `StructureTreeVisitor::extract` builds the tree the
//! GUI and the MCP resource read.

use fepdf::PdfDocument;

use fepdf_fixtures::assemble;

fn tagged_document() -> PdfDocument {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /StructTreeRoot /K [5 0 R] >>",
        "<< /Type /StructElem /S /Document /P 4 0 R /K [6 0 R 7 0 R] >>",
        "<< /Type /StructElem /S /H1 /P 5 0 R /Pg 3 0 R >>",
        "<< /Type /StructElem /S /Figure /P 5 0 R /Pg 3 0 R /Alt (a photograph) >>",
    ];
    PdfDocument::open(bytes::Bytes::from(assemble(&objects))).expect("the fixture opens")
}

/// The tree it prints is the tree the document carries.
#[test]
fn the_structure_is_printed_and_not_merely_located() {
    let printed = tagged_document().print_structure().expect("it prints");

    for tag in ["Document", "H1", "Figure"] {
        assert!(printed.contains(tag), "/{tag} is in the tree and must be in the dump:\n{printed}");
    }
    assert!(printed.contains("a photograph"), "and an element's /Alt with it:\n{printed}");
    assert!(printed.lines().count() >= 3, "a hierarchy is more than one line:\n{printed}");
}

/// Nesting is visible, so the dump is a tree rather than a list.
#[test]
fn the_hierarchy_is_visible_in_the_output() {
    let printed = tagged_document().print_structure().expect("it prints");
    let indent = |needle: &str| {
        printed
            .lines()
            .find(|l| l.contains(needle))
            .map(|l| l.len() - l.trim_start().len())
            .unwrap_or_default()
    };
    assert!(
        indent("H1") > indent("Document"),
        "H1 is a child of Document and reads as one:\n{printed}"
    );
}

/// No arena vocabulary reaches the page.
///
/// `Handle<Object>(93)` was the whole of the old output. Rule A keeps the storage model
/// out of frontends, and a `Debug` of a handle in a printed report is that model arriving
/// by another door.
#[test]
fn the_dump_carries_no_storage_vocabulary() {
    let printed = tagged_document().print_structure().expect("it prints");
    for leaked in ["Handle<", "PdfArena", "Handle {"] {
        assert!(!printed.contains(leaked), "{leaked} is storage vocabulary:\n{printed}");
    }
}

/// A document with no structure says so, rather than printing an empty tree.
#[test]
fn an_untagged_document_says_it_has_none() {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ];
    let doc = PdfDocument::open(bytes::Bytes::from(assemble(&objects))).expect("the fixture opens");

    let printed = doc.print_structure().expect("it prints");
    assert!(printed.to_lowercase().contains("no logical structure"), "{printed}");
}
