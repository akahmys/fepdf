//! A page selection that names a page the document does not have.
//!
//! **Refused, by number, whichever operation is asked.** Five of the operations taking a
//! selection — removing, rotating, resizing, cropping and combining pages, and the header
//! and Bates stamps with them — dropped such a page and returned `Ok`, so removing page 99
//! of a two-page document succeeded having done nothing and an MCP caller off by one was
//! told it had worked. Duplicating and splitting refused. Found while trying to make a
//! page replacement's removal fail (ROADMAP).

use fepdf::operation::Operation;
use fepdf::{IngestionOptions, PageSelection, PdfDocument};

fn two_pages() -> PdfDocument {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] >>".to_string(),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// Every selection-taking operation, naming page 0 and a page 9 that is not there.
fn asking_for_a_missing_page() -> Vec<(&'static str, Operation)> {
    let pages = || PageSelection::Indices(vec![0, 9]);
    vec![
        ("remove", Operation::RemovePages(pages())),
        (
            "rotate",
            Operation::Rotate {
                pages: pages(),
                mode: fepdf::operation::RotateMode::Relative(fepdf::Quarter::Q90),
            },
        ),
        ("duplicate", Operation::DuplicatePages(pages())),
        (
            "resize",
            Operation::ResizePages(
                pages(),
                fepdf::PageResize {
                    sheet: Some((400.0, 400.0)),
                    scale: fepdf::ContentScale::Keep,
                    offset: (0.0, 0.0),
                },
            ),
        ),
        (
            "crop",
            Operation::CropPages(
                pages(),
                fepdf::operation::CropRegion {
                    keep: (0.0, 0.0, 100.0, 100.0),
                    outside: fepdf::operation::WhatFallsOutside::Stays,
                },
            ),
        ),
        (
            "combine",
            Operation::CombinePages(
                pages(),
                fepdf::operation::PageArrangement { sheet: None, columns: 2, rows: 1 },
            ),
        ),
        (
            "decorate",
            Operation::AddPageDecoration {
                pages: pages(),
                text: "DRAFT".to_string(),
                position: fepdf::operation::DecorationPosition::TopRight,
                layer: None,
            },
        ),
        (
            "bates",
            Operation::ApplyBatesNumbering {
                pages: pages(),
                prefix: "DOC-".to_string(),
                start_number: 1,
                digits: 4,
                position: fepdf::operation::DecorationPosition::BottomRight,
            },
        ),
        ("reorder", Operation::Reorder { from: 0, to: 9 }),
    ]
}

#[test]
fn a_page_that_is_not_there_is_refused_by_every_operation() {
    for (name, operation) in asking_for_a_missing_page() {
        let mut doc = two_pages();
        let error = doc.apply(operation).expect_err(name);
        assert!(
            error.to_string().contains("no page 9"),
            "{name}: the refusal does not name the page: {error}"
        );
    }
}

/// What an operation on pages could change: how many there are, each one's box and turn,
/// and how many objects the document holds — a stamp adds some before it draws.
fn state(doc: &PdfDocument) -> (usize, Vec<String>, u32) {
    let count = doc.page_count().expect("it counts");
    let pages = (0..count)
        .map(|page| {
            let area = doc.get_page_box(page).expect("a box");
            let turn = doc.get_page_rotation(page).expect("a rotation");
            format!("{:?} {turn}", (area.x1, area.y1, area.x2, area.y2))
        })
        .collect();
    (count, pages, doc.inner().arena().object_count())
}

/// **And refused before anything changed.** Page 0 is named too, so an operation that
/// acted on the pages it had before looking at the rest would leave page 0 changed.
#[test]
fn a_refused_selection_changes_nothing() {
    for (name, operation) in asking_for_a_missing_page() {
        let mut doc = two_pages();
        let before = state(&doc);
        let _refused = doc.apply(operation);
        assert_eq!(state(&doc), before, "{name}: a refused selection changed the document");
    }
}
