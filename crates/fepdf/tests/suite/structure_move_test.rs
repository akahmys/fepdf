//! Moving a structure element, which is how reading order gets fixed (14.7.4).
//!
//! The vocabulary could retag an element and delete one, so the only way to put a
//! paragraph in the right place was to delete it and lose its content. The GUI's tree has
//! had drag-and-drop the whole time and it rearranged the window's own copy — an edit that
//! looked like it worked, and reached neither the file, the undo history, nor the dot that
//! says the document is unsaved.

use fepdf::{IngestionOptions, Operation, PdfDocument, Placement, StructElemMove};
use fepdf_fixtures::assemble;

/// A document whose `/StructTreeRoot` holds a `/Sect` (object 6) over two paragraphs
/// (objects 7 and 8), each claiming one mark.
fn tagged() -> PdfDocument {
    let content = "/P << /MCID 0 >> BDC\n0 0 10 10 re f\nEMC\n\
                   /P << /MCID 1 >> BDC\n90 90 10 10 re f\nEMC\n";
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /StructTreeRoot /K [6 0 R] >>".to_string(),
        "<< /Type /StructElem /S /Sect /P 5 0 R /Pg 3 0 R /K [7 0 R 8 0 R] >>".to_string(),
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /Alt (first) /K [0] >>".to_string(),
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /Alt (second) /K [1] >>".to_string(),
    ];
    PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// The `/Alt` of each paragraph under the section, in the order the tree holds them.
fn order(doc: &PdfDocument) -> Vec<String> {
    let root = doc.extract_struct_tree().expect("the document is tagged");
    root.children[0]
        .children
        .iter()
        .map(|child| child.alt_text.clone().unwrap_or_default())
        .collect()
}

/// The two paragraphs' handle indices, first then second.
fn paragraphs(doc: &PdfDocument) -> (u32, u32) {
    let root = doc.extract_struct_tree().expect("the document is tagged");
    let kids = &root.children[0].children;
    (kids[0].handle_index.expect("a handle"), kids[1].handle_index.expect("a handle"))
}

#[test]
fn a_paragraph_moved_before_another_comes_first() {
    let mut doc = tagged();
    assert_eq!(order(&doc), vec!["first", "second"]);
    let (first, second) = paragraphs(&doc);
    doc.apply(Operation::MoveStructElem(StructElemMove {
        handle_index: second,
        target_index: first,
        placement: Placement::Before,
    }))
    .expect("the move is legal");
    assert_eq!(order(&doc), vec!["second", "first"]);
}

#[test]
fn a_paragraph_moved_after_another_comes_second() {
    let mut doc = tagged();
    let (first, second) = paragraphs(&doc);
    doc.apply(Operation::MoveStructElem(StructElemMove {
        handle_index: first,
        target_index: second,
        placement: Placement::After,
    }))
    .expect("the move is legal");
    assert_eq!(order(&doc), vec!["second", "first"]);
}

#[test]
fn a_paragraph_moved_inside_another_becomes_its_child() {
    let mut doc = tagged();
    let (first, second) = paragraphs(&doc);
    doc.apply(Operation::MoveStructElem(StructElemMove {
        handle_index: second,
        target_index: first,
        placement: Placement::Inside,
    }))
    .expect("the move is legal");
    assert_eq!(order(&doc), vec!["first"], "the section holds one paragraph now");
    let root = doc.extract_struct_tree().expect("tagged");
    let inner = &root.children[0].children[0].children;
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].alt_text.as_deref(), Some("second"));
}

#[test]
fn a_move_that_would_make_a_cycle_is_refused_and_changes_nothing() {
    // Dropping the section inside its own paragraph would make `/K` a ring, and every
    // walk over this structure is bounded by a depth precisely because one exists in the
    // corpus already.
    let mut doc = tagged();
    let root = doc.extract_struct_tree().expect("tagged");
    let section = root.children[0].handle_index.expect("a handle");
    let (first, _) = paragraphs(&doc);
    let refused = doc.apply(Operation::MoveStructElem(StructElemMove {
        handle_index: section,
        target_index: first,
        placement: Placement::Inside,
    }));
    assert!(refused.is_err(), "a cycle was allowed");
    assert_eq!(order(&doc), vec!["first", "second"], "the refusal moved something");
}

#[test]
fn a_move_naming_an_element_the_tree_does_not_hold_is_refused() {
    // An error rather than a silent `Ok(())`: reporting success for a move that was not
    // made is how the window came to have a drag that rearranged nothing.
    let mut doc = tagged();
    let (first, _) = paragraphs(&doc);
    let refused = doc.apply(Operation::MoveStructElem(StructElemMove {
        handle_index: 9999,
        target_index: first,
        placement: Placement::After,
    }));
    assert!(refused.is_err(), "an element that is not there was moved");
    assert_eq!(order(&doc), vec!["first", "second"]);
}

#[test]
fn a_moved_element_says_who_holds_it_now() {
    // 14.7.2 requires `/P`, and a tree whose child disagrees with its parent is a tree
    // every other reader walks differently from this one.
    let mut doc = tagged();
    let (first, second) = paragraphs(&doc);
    doc.apply(Operation::MoveStructElem(StructElemMove {
        handle_index: second,
        target_index: first,
        placement: Placement::Inside,
    }))
    .expect("the move is legal");
    assert_eq!(parent_of(&doc, second), Some(first), "/P still names the old parent");
}

/// What `/P` on the element at `handle` points at.
fn parent_of(doc: &PdfDocument, handle: u32) -> Option<u32> {
    let arena = doc.inner().arena();
    let object = arena.get_object(fepdf_model::Handle::new(handle))?;
    let dict = arena.get_dict(object.as_dict_handle()?)?;
    match dict.get(&arena.name("P"))? {
        fepdf_model::Object::Reference(h) => Some(h.index()),
        _ => None,
    }
}

/// **A deleted element leaves the file** (14.7.5.4): its marks belong to no element, and
/// the `/IDTree` names it no more. It was taken out of its parent's `/K` alone, so the
/// parent tree went on saying its marks were its, and the writer wrote it — `/Alt` and
/// all — for anyone who opened the file. An element the tree does not hold is refused, as
/// an update of one is.
#[test]
fn a_deleted_element_is_not_written_and_its_marks_belong_to_nothing() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
         /StructParents 0 >>"
            .to_string(),
        "<< /Length 0 >>\nstream\nendstream".to_string(),
        "<< /Type /StructTreeRoot /K [6 0 R] /ParentTree << /Nums [0 [7 0 R 8 0 R]] >> \
         /IDTree << /Names [(one) 7 0 R] >> >>"
            .to_string(),
        "<< /Type /StructElem /S /Sect /P 5 0 R /Pg 3 0 R /K [7 0 R 8 0 R] >>".to_string(),
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /ID (one) /Alt (SECRETFIRST) /K [0] >>"
            .to_string(),
        "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /Alt (second) /K [1] >>".to_string(),
    ];
    let mut doc =
        PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
            .expect("the fixture opens");
    doc.apply(Operation::DeleteStructElem { handle_index: 99 }).expect_err("99 is not in the tree");
    doc.apply(Operation::DeleteStructElem { handle_index: 7 }).expect("the paragraph is deleted");

    let path = std::env::temp_dir().join(format!("fepdf_deleted_elem_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it writes");
    let back = PdfDocument::open(std::fs::read(&path).expect("it is there").into())
        .expect("it reads back");
    let _ = std::fs::remove_file(&path);
    let arena = back.inner().arena();
    let secret = arena.all_dict_handles().into_iter().any(|d| {
        arena.get_dict(d).unwrap_or_default().values().any(|v| match v {
            fepdf_model::Object::Text(t) => t.contains("SECRETFIRST"),
            fepdf_model::Object::String(b) | fepdf_model::Object::Hex(b) => {
                String::from_utf8_lossy(b).contains("SECRETFIRST")
            }
            _ => false,
        })
    });
    assert!(!secret, "the deleted element is still in the file");
    let tree = back.extract_struct_tree().expect("the document is tagged");
    assert_eq!(order(&back), ["second"]);
    assert_eq!(tree.children[0].children.len(), 1);
}
