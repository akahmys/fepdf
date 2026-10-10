//! **Every subtype but the deprecated, the 3D and the RichMedia is made** (ROADMAP AA-4f):
//! the ten `AddAnnotation` could not make, each with its table's entries and, where Table
//! 166 asks for one, an appearance that draws.

use fepdf::{
    AnnotationKind, AnnotationSpec, Authorship, MediaClip, Operation, PdfDocument, PdfError,
    PrinterMarkKind, ShapeForm,
};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

fn blank() -> PdfDocument {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> >>",
    ];
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

fn add(doc: &mut PdfDocument, rect: [f32; 4], kind: AnnotationKind) -> Result<(), PdfError> {
    let by = Authorship { author: Some("Ann".into()), when: None };
    doc.apply(Operation::AddAnnotation(AnnotationSpec { page: 0, rect, kind, by }))
}

/// Each kind, and the subtype it is made as.
fn kinds() -> Vec<(&'static str, AnnotationKind)> {
    let points = vec![[20.0, 20.0], [180.0, 20.0], [100.0, 180.0]];
    vec![
        (
            "Polygon",
            AnnotationKind::Shape {
                form: ShapeForm::Polygon { vertices: points.clone() },
                color_rgb: [1.0, 0.0, 0.0],
                width: 2.0,
            },
        ),
        (
            "PolyLine",
            AnnotationKind::Shape {
                form: ShapeForm::PolyLine { vertices: points },
                color_rgb: [0.0, 0.0, 1.0],
                width: 1.0,
            },
        ),
        (
            "Caret",
            AnnotationKind::Caret {
                contents: "insert".into(),
                color_rgb: [0.0, 0.0, 1.0],
                paragraph: true,
            },
        ),
        (
            "FileAttachment",
            AnnotationKind::FileAttachment {
                filename: "note.txt".into(),
                mime_type: Some("text/plain".into()),
                data: b"Hello".to_vec(),
                description: None,
            },
        ),
        (
            "Screen",
            AnnotationKind::Screen {
                title: Some("clip".into()),
                clip: Some(MediaClip {
                    filename: "clip.mp4".into(),
                    mime_type: "video/mp4".into(),
                    data: vec![0; 16],
                }),
            },
        ),
        ("PrinterMark", AnnotationKind::PrinterMark { mark: PrinterMarkKind::ColorBar }),
        (
            "Watermark",
            AnnotationKind::Watermark { text: "DRAFT".into(), font_size: 48.0, opacity: 0.3 },
        ),
        (
            "Redact",
            AnnotationKind::Redact {
                overlay_text: Some("removed".into()),
                interior_rgb: Some([0.0, 0.0, 0.0]),
            },
        ),
        ("Projection", AnnotationKind::Projection { contents: "a measurement".into() }),
    ]
}

/// The entry `key` of annotation `index` on page 0, resolved.
fn entry(doc: &PdfDocument, index: usize, key: &str) -> Option<fepdf::Object> {
    let inner = doc.inner();
    let arena = inner.arena();
    let page = inner.resolve_to_dict(inner.page_handle(0).ok()?).ok()?;
    let annots = arena
        .dict_entry(page, arena.name("Annots"))
        .and_then(|a| a.resolve(arena).as_array())
        .and_then(|a| arena.get_array(a))?;
    let dict = annots.get(index)?.resolve(arena).as_dict_handle()?;
    arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena))
}

fn name(doc: &PdfDocument, index: usize, key: &str) -> Option<String> {
    let found = entry(doc, index, key)?.as_name()?;
    doc.inner().arena().get_name_str(found)
}

fn painted(doc: &PdfDocument) -> usize {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder.count("fill") + recorder.count("stroke") + recorder.text().len()
}

/// **Each is made as its subtype, drawn where Table 166 asks for an appearance, and named
/// and signed only where it is a markup annotation** (Table 171).
#[test]
fn every_kind_is_made_and_drawn() {
    let mut wrong = Vec::new();
    for (subtype, kind) in kinds() {
        let mut doc = blank();
        if let Err(why) = add(&mut doc, [20.0, 20.0, 180.0, 180.0], kind) {
            wrong.push(format!("{subtype}: refused, {why}"));
            continue;
        }
        if name(&doc, 0, "Subtype").as_deref() != Some(subtype) {
            wrong.push(format!("{subtype}: made as {:?}", name(&doc, 0, "Subtype")));
        }
        let drawn = painted(&doc);
        if subtype == "Projection" {
            if entry(&doc, 0, "AP").is_some() || drawn > 0 {
                wrong.push("Projection: drew something, which Table 166 does not ask".into());
            }
        } else if drawn == 0 {
            wrong.push(format!("{subtype}: nothing drawn"));
        }
        let markup = !matches!(subtype, "Screen" | "PrinterMark" | "Watermark");
        // A screen's `/T` is its title (Table 190); a markup annotation's is its author.
        let signed = entry(&doc, 0, "T").and_then(|t| t.as_text().map(str::to_owned));
        if (signed.as_deref() == Some("Ann")) != markup {
            wrong.push(format!("{subtype}: signed is {}, markup is {markup}", !markup));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// **Each kind's own entries are written**, as its table names them.
#[test]
fn each_kind_writes_its_tables_entries() {
    let made = |kind: AnnotationKind| {
        let mut doc = blank();
        add(&mut doc, [20.0, 20.0, 180.0, 180.0], kind).expect("it is made");
        doc
    };
    let mut kinds = kinds().into_iter().map(|(_, k)| k);
    let mut next = || kinds.next().expect("a kind");
    assert!(entry(&made(next()), 0, "Vertices").is_some(), "a polygon's /Vertices");
    assert!(entry(&made(next()), 0, "Vertices").is_some(), "a polyline's /Vertices");
    assert_eq!(name(&made(next()), 0, "Sy").as_deref(), Some("P"), "a paragraph caret");
    let attached = made(next());
    assert!(entry(&attached, 0, "FS").is_some(), "the attached file");
    assert_eq!(name(&attached, 0, "Name").as_deref(), Some("PushPin"));
    let screen = made(next());
    let action = entry(&screen, 0, "A").and_then(|a| a.as_dict_handle()).expect("an action");
    let arena = screen.inner().arena();
    let names_itself = arena
        .dict_entry(action, arena.name("AN"))
        .is_some_and(|an| matches!(an, fepdf::Object::Reference(_)));
    assert!(names_itself, "Table 214's /AN names the screen");
    assert_eq!(name(&made(next()), 0, "MN").as_deref(), Some("ColorBar"));
    let watermark = made(next());
    assert_eq!(entry(&watermark, 0, "CA").and_then(|c| c.as_f64()), Some(f64::from(0.3f32)));
    let redact = made(next());
    assert!(entry(&redact, 0, "QuadPoints").is_some() && entry(&redact, 0, "DA").is_some());
    assert!(entry(&made(next()), 0, "Contents").is_some(), "a projection's comment");
}

/// **A popup and its parent name each other**, and a popup for what is not a markup
/// annotation, or for one with a popup already, is refused.
#[test]
fn a_popup_is_linked_to_its_parent() {
    let mut doc = blank();
    let note = AnnotationKind::TextComment { contents: "see".into() };
    add(&mut doc, [20.0, 150.0, 40.0, 170.0], note).expect("the note");
    add(&mut doc, [60.0, 60.0, 180.0, 140.0], AnnotationKind::Popup { parent: 0, open: true })
        .expect("the popup");
    assert!(matches!(entry(&doc, 0, "Popup"), Some(fepdf::Object::Dictionary(_))));
    assert!(matches!(entry(&doc, 1, "Parent"), Some(fepdf::Object::Dictionary(_))));
    assert_eq!(entry(&doc, 1, "Open").and_then(|o| o.as_bool()), Some(true));
    assert!(entry(&doc, 1, "AP").is_none(), "Table 166 exempts a popup");

    let again =
        add(&mut doc, [0.0, 0.0, 50.0, 50.0], AnnotationKind::Popup { parent: 0, open: false });
    assert!(matches!(again, Err(PdfError::Refused { .. })), "a second popup: {again:?}");
    let of_a_popup =
        add(&mut doc, [0.0, 0.0, 50.0, 50.0], AnnotationKind::Popup { parent: 1, open: false });
    assert!(matches!(of_a_popup, Err(PdfError::Refused { .. })), "{of_a_popup:?}");
    let nowhere =
        add(&mut doc, [0.0, 0.0, 50.0, 50.0], AnnotationKind::Popup { parent: 9, open: false });
    assert!(matches!(nowhere, Err(PdfError::Refused { .. })), "{nowhere:?}");
}

/// What would draw nothing is refused, as the first kinds are.
#[test]
fn what_would_draw_nothing_is_refused() {
    let two = AnnotationKind::Shape {
        form: ShapeForm::Polygon { vertices: vec![[0.0, 0.0], [10.0, 10.0]] },
        color_rgb: [0.0; 3],
        width: 1.0,
    };
    let blank_words = AnnotationKind::Watermark { text: " ".into(), font_size: 12.0, opacity: 1.0 };
    let unseen = AnnotationKind::Watermark { text: "x".into(), font_size: 12.0, opacity: 0.0 };
    for kind in [two, blank_words, unseen] {
        let refused = add(&mut blank(), [0.0, 0.0, 100.0, 100.0], kind.clone());
        assert!(matches!(refused, Err(PdfError::Refused { .. })), "{kind:?}: {refused:?}");
    }
}
