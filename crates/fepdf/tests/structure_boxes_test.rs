//! A structure element comes back with the rectangle its marked content drew in.
//!
//! No element in any of the nine samples declares a `/BBox`, which is the only place this
//! tree used to look. Every consumer of `StructureTreeNode::rect` therefore got `None`
//! for every element of every document: the GUI's reading-order overlay and its element
//! outlines are both on by default and both drew nothing, on every file ever opened.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_fixtures::assemble;

/// A stream object with `extra` merged into its dictionary.
fn stream(extra: &str, data: &str) -> String {
    format!("<< {extra} /Length {} >>\nstream\n{data}endstream", data.len())
}

/// A one-page tagged document: object 5 is the `/StructTreeRoot`, object 6 the element
/// `element` describes, and object 7 is free for a test that needs one more.
fn tagged(element: &str, content: &str, extra: &[&str]) -> PdfDocument {
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        stream("", content),
        "<< /Type /StructTreeRoot /K [6 0 R] >>".to_string(),
        element.to_string(),
    ];
    bodies.extend(extra.iter().map(|body| (*body).to_string()));
    PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// The tree of `doc`, placed.
fn placed(doc: &PdfDocument) -> fepdf::StructureTreeNode {
    let mut root = doc.extract_struct_tree().expect("the document is tagged");
    doc.fill_structure_boxes(&mut root);
    root
}

#[test]
fn an_element_takes_the_rectangle_its_marks_drew_in() {
    let doc = tagged(
        "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /K [0] >>",
        "/P << /MCID 0 >> BDC\n10 20 30 40 re f\nEMC\n",
        &[],
    );
    let root = placed(&doc);
    let element = &root.children[0];
    assert_eq!(element.tag, "P");
    assert_eq!(element.mcids, vec![0]);
    assert_eq!(element.rect, Some([10.0, 20.0, 40.0, 60.0]));
}

#[test]
fn an_element_that_declares_a_bbox_keeps_the_one_it_declared() {
    // 14.8.5.4.5 lets an element state its own rectangle. A measurement does not get to
    // overrule a declaration — the file is the authority on what it says about itself.
    let doc = tagged(
        "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R /BBox [1 2 3 4] /K [0] >>",
        "/P << /MCID 0 >> BDC\n10 20 30 40 re f\nEMC\n",
        &[],
    );
    assert_eq!(placed(&doc).children[0].rect, Some([1.0, 2.0, 3.0, 4.0]));
}

#[test]
fn a_parent_takes_its_extent_from_the_children_it_holds() {
    // A `/Sect` claims no `/MCID` of its own. Its rectangle is the union of what it
    // holds, which is why the walk runs bottom-up.
    let doc = tagged(
        "<< /Type /StructElem /S /Sect /P 5 0 R /Pg 3 0 R /K [7 0 R 8 0 R] >>",
        "/P << /MCID 0 >> BDC\n0 0 10 10 re f\nEMC\n\
         /P << /MCID 1 >> BDC\n90 90 10 10 re f\nEMC\n",
        &[
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K [0] >>",
            "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K [1] >>",
        ],
    );
    let section = &placed(&doc).children[0];
    assert!(section.mcids.is_empty(), "the section claims {:?}", section.mcids);
    assert_eq!(section.rect, Some([0.0, 0.0, 100.0, 100.0]));
}

#[test]
fn a_marked_content_reference_is_a_mark_and_not_a_child_element() {
    // 14.7.4.2 gives `/MCR` the same shape as an element: a dictionary. Read as one it
    // becomes a phantom paragraph, because an element with no `/S` falls back to `P`.
    // `volvo_xc90.pdf` writes 13,558 of these.
    let doc = tagged(
        "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R \
         /K [<< /Type /MCR /Pg 3 0 R /MCID 0 >>] >>",
        "/P << /MCID 0 >> BDC\n10 20 30 40 re f\nEMC\n",
        &[],
    );
    let element = &placed(&doc).children[0];
    assert!(element.children.is_empty(), "the reference became {:?}", element.children);
    assert_eq!(element.mcids, vec![0]);
    assert_eq!(element.rect, Some([10.0, 20.0, 40.0, 60.0]));
}

#[test]
fn a_reference_carries_the_page_for_an_element_that_names_none() {
    // `volvo_xc90.pdf` puts `/Pg` on the reference and nowhere else. An element that read
    // only its own would know its marks and not which page they are on, which is the same
    // as not knowing them — 23,414 of its 23,416 elements are placed by this line.
    let doc = tagged(
        "<< /Type /StructElem /S /P /P 5 0 R /K [<< /Type /MCR /Pg 3 0 R /MCID 0 >>] >>",
        "/P << /MCID 0 >> BDC\n10 20 30 40 re f\nEMC\n",
        &[],
    );
    let element = &placed(&doc).children[0];
    assert_eq!(element.page_index, Some(0));
    assert_eq!(element.rect, Some([10.0, 20.0, 40.0, 60.0]));
}

#[test]
fn an_object_reference_is_neither_a_mark_nor_a_child() {
    // `/OBJR` names an annotation, which is content with no `/MCID` to place it by.
    // `print_sample.pdf` writes 20 of them and every one arrived here as a `P`.
    let doc = tagged(
        "<< /Type /StructElem /S /P /P 5 0 R /Pg 3 0 R \
         /K [<< /Type /OBJR /Pg 3 0 R /Obj 7 0 R >>] >>",
        "10 20 30 40 re f\n",
        &["<< /Type /Annot /Subtype /Square /Rect [0 0 10 10] >>"],
    );
    let element = &placed(&doc).children[0];
    assert!(element.children.is_empty(), "the reference became {:?}", element.children);
    assert!(element.mcids.is_empty(), "the reference became a mark: {:?}", element.mcids);
    assert_eq!(element.rect, None);
}

#[test]
fn a_tagged_sample_comes_back_almost_entirely_placed() {
    // The corpus, which is the only thing that says whether the shapes above are the
    // shapes that occur. `print_sample.pdf` leaves 30 elements unplaced: they hold
    // `/OBJR`s and nothing else, and an annotation has no mark to be measured by.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/print_sample.pdf");
    let bytes = std::fs::read(path).expect("the sample is in the tree");
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens");
    let root = placed(&doc);
    let (mut with, mut total) = (0, 0);
    count(&root, &mut with, &mut total);
    assert_eq!((with, total), (1198, 1228), "print_sample placed {with} of {total}");
}

/// How many of the tree's elements have a rectangle.
fn count(node: &fepdf::StructureTreeNode, with: &mut usize, total: &mut usize) {
    *total += 1;
    if node.rect.is_some() {
        *with += 1;
    }
    for child in &node.children {
        count(child, with, total);
    }
}
