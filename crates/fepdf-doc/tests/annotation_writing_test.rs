//! What an annotation this engine writes carries.
//!
//! `AddAnnotation` has existed for phases and no frontend calls it, so nothing has ever
//! looked at what it produces. Three things, measured 2026-09-19:
//!
//! - **No `/AP` for any of its four kinds.** 12.5.5 is how an annotation looks, and this
//!   engine's own renderer skips an annotation without one — so what it writes, it cannot
//!   draw.
//! - **`Highlight` carries no `/QuadPoints`**, which 12.5.6.10 makes *required* and which
//!   is the whole of what a text markup marks: a highlight over three lines is three
//!   quadrilaterals and one `/Rect`.
//! - **`Stamp` discards the image it is given.** `stamp_image_bytes` is bound to `_` and
//!   the dictionary gets `/Name /Draft`, so a caller's picture reaches the file nowhere.
//!
//! The third is the shape this repository has a history with: an operation that reports
//! success and writes none of what it was handed.

use fepdf_model::document::Document;
use fepdf_model::ingest::IngestionOptions;
use fepdf_model::{Handle, Object, PdfArena};

use fepdf_doc::operation::{AnnotationKind, AnnotationSpec, Operation};

/// A one-page document to annotate.
fn document() -> Document {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ];
    Document::open(fepdf_fixtures::assemble(&bodies).into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// The annotation the page carries, after `kind` is added over `rect`.
fn annotate(doc: &mut Document, rect: [f32; 4], kind: AnnotationKind) -> Handle<Object> {
    fepdf_doc::apply::apply_operation(
        doc,
        Operation::AddAnnotation(AnnotationSpec { page: 0, rect, kind }),
    )
    .expect("the annotation applies");

    let arena = doc.arena();
    let page_h = doc.get_page_handle(0).expect("the page is there");
    let page_dh = doc.resolve_to_dict(page_h).expect("a page dictionary");
    let page = arena.get_dict(page_dh).expect("the dictionary");
    let annots = page
        .iter()
        .find(|(k, _)| arena.get_name(**k).is_some_and(|n| n.as_str() == "Annots"))
        .map(|(_, v)| v.resolve(arena))
        .expect("the page names its annotations");
    let Object::Array(ah) = annots else { panic!("/Annots is not an array") };
    arena
        .get_array(ah)
        .and_then(|items| items.last().and_then(|o| o.as_reference()))
        .expect("one annotation")
}

/// The value of `key` in the annotation at `handle`.
fn entry(arena: &PdfArena, handle: Handle<Object>, key: &str) -> Option<Object> {
    let dh = match arena.get_object(handle)? {
        Object::Dictionary(dh) => dh,
        _ => return None,
    };
    arena
        .get_dict(dh)?
        .iter()
        .find(|(k, _)| arena.get_name(**k).is_some_and(|n| n.as_str() == key))
        .map(|(_, v)| v.resolve(arena))
}

/// **An annotation without an appearance is one this engine cannot draw.**
///
/// 12.5.5, and `render_annotations` skips it quietly — which is right for a file somebody
/// else wrote and wrong for one this engine writes.
#[test]
fn every_annotation_written_carries_an_appearance() {
    let mut doc = document();
    let kinds = [
        ("Highlight", AnnotationKind::Highlight { color_rgb: [1.0, 1.0, 0.0] }),
        ("TextComment", AnnotationKind::TextComment { contents: "a note".to_string() }),
        ("Stamp", AnnotationKind::Stamp { stamp_image_bytes: (0..32u8).collect() }),
    ];
    for (name, kind) in kinds {
        let handle = annotate(&mut doc, [10.0, 20.0, 110.0, 40.0], kind);
        assert!(
            entry(doc.arena(), handle, "AP").is_some(),
            "{name} was written with no appearance stream"
        );
    }
}

/// **A link is the exception, and is one on purpose.**
///
/// 12.5.6.5 gives a link a `/Border` and the reader draws it; an appearance stream would
/// paint a rectangle where the file asks for nothing to be painted. So the rule above is
/// "an annotation that draws carries an appearance", and this is the annotation that does
/// not draw.
#[test]
fn a_link_carries_no_appearance_because_it_paints_nothing() {
    let mut doc = document();
    let handle = annotate(
        &mut doc,
        [10.0, 20.0, 110.0, 40.0],
        AnnotationKind::Link {
            destination_page: 0,
            url: Some("https://example.invalid".to_string()),
        },
    );
    assert!(
        entry(doc.arena(), handle, "AP").is_none(),
        "a link was given an appearance, which paints where the file asks for nothing"
    );
    assert!(entry(doc.arena(), handle, "A").is_some(), "the link names no action");
}

/// **`/QuadPoints` is required of a text markup**, and is what says which words are
/// marked rather than which rectangle they sit in (12.5.6.10, Table 179).
#[test]
fn a_highlight_says_which_quadrilateral_it_marks() {
    let mut doc = document();
    let handle = annotate(
        &mut doc,
        [10.0, 20.0, 110.0, 40.0],
        AnnotationKind::Highlight { color_rgb: [1.0, 1.0, 0.0] },
    );

    let Some(Object::Array(ah)) = entry(doc.arena(), handle, "QuadPoints") else {
        panic!("a highlight was written with no QuadPoints");
    };
    let points: Vec<f64> =
        doc.arena().get_array(ah).unwrap_or_default().iter().filter_map(Object::as_f64).collect();
    assert_eq!(points.len(), 8, "one quadrilateral is eight numbers: {points:?}");
    assert!(
        points.iter().any(|p| (*p - 10.0).abs() < 0.01)
            && points.iter().any(|p| (*p - 110.0).abs() < 0.01),
        "the quadrilateral does not cover the rectangle it was given: {points:?}"
    );
}

/// **A stamp keeps the picture it was handed.**
///
/// It was bound to `_` and the dictionary got `/Name /Draft`, so an operation that
/// reported success wrote none of what it was given.
#[test]
fn a_stamp_carries_the_image_it_was_given() {
    let mut doc = document();
    // A one-pixel grey PNG-like payload: what matters is that these bytes reach the file.
    let image: Vec<u8> = (0..64u8).collect();
    let handle = annotate(
        &mut doc,
        [10.0, 20.0, 110.0, 120.0],
        AnnotationKind::Stamp { stamp_image_bytes: image.clone() },
    );

    let appearance = entry(doc.arena(), handle, "AP").expect("a stamp has an appearance");
    assert!(
        reaches_bytes(doc.arena(), &appearance, &image, 0),
        "the image the stamp was given is nowhere in what was written"
    );
}

/// Whether `needle` is in the stream at `object`, or in any stream it names.
fn reaches_bytes(arena: &PdfArena, object: &Object, needle: &[u8], depth: usize) -> bool {
    if depth > 6 {
        return false;
    }
    match object.resolve(arena) {
        Object::Stream(dh, data) => {
            if arena
                .get_stream_bytes(&data)
                .is_ok_and(|bytes| bytes.windows(needle.len()).any(|w| w == needle))
            {
                return true;
            }
            arena
                .get_dict(dh)
                .is_some_and(|d| d.values().any(|v| reaches_bytes(arena, v, needle, depth + 1)))
        }
        Object::Dictionary(dh) => arena
            .get_dict(dh)
            .is_some_and(|d| d.values().any(|v| reaches_bytes(arena, v, needle, depth + 1))),
        Object::Array(ah) => arena
            .get_array(ah)
            .unwrap_or_default()
            .iter()
            .any(|v| reaches_bytes(arena, v, needle, depth + 1)),
        _ => false,
    }
}
