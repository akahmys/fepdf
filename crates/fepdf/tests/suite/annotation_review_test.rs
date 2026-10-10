//! **What a reviewer does to an annotation**, through the facade (ROADMAP AA-2a): one is
//! removed with what answers it, its words edited, answered, and given a state, each
//! named by its place on the page (ADR-0115); and a new one is named and signed only as
//! far as the caller said (ADR-0116).

use fepdf::comments::{Comment, StateMark};
use fepdf::{
    AnnotationAt, AnnotationKind, AnnotationSpec, AnnotationState, Authorship, Operation,
    PdfDocument, PdfError, SaveOptions,
};

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

/// A page with one note by Ann, written by another program: no `/NM`.
fn one_note() -> PdfDocument {
    page_with(
        "4 0 R",
        &["<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (Fix this) /T (Ann) >>"],
    )
}

const fn at(index: usize) -> AnnotationAt {
    AnnotationAt { page: 0, index }
}

fn by(author: &str) -> Authorship {
    Authorship { author: Some(author.to_owned()), when: Some("D:20261010120000+09'00".to_owned()) }
}

fn comments(doc: &PdfDocument) -> Vec<Comment> {
    doc.comments(0).expect("the page is there")
}

fn note(doc: &mut PdfDocument, by: Authorship) {
    let kind = AnnotationKind::TextComment { contents: "new".to_owned() };
    doc.apply(Operation::AddAnnotation(AnnotationSpec {
        page: 0,
        rect: [50.0, 50.0, 70.0, 70.0],
        kind,
        by,
    }))
    .expect("it adds");
}

/// **A new annotation is named on its page, and carries an author only when one is
/// given.** No login name, no clock: what the caller said, and nothing else.
#[test]
fn a_new_annotation_is_named_and_signed_as_far_as_the_caller_said() {
    let mut doc = one_note();
    note(&mut doc, Authorship::default());
    note(&mut doc, by("Bo"));
    let list = comments(&doc);
    let (silent, signed) = (&list[1], &list[2]);

    assert_eq!(silent.name.as_deref(), Some("fepdf-1"));
    assert_eq!(silent.author, None, "no author was given, so none is written");
    assert_eq!(silent.created, None, "no time was given, so none is written");
    assert_eq!(signed.name.as_deref(), Some("fepdf-2"), "names are unique on the page");
    assert_eq!(signed.author.as_deref(), Some("Bo"));
    assert_eq!(signed.created.as_deref(), Some("D:20261010120000+09'00"));
}

/// **A reply answers by `/IRT`, and is listed as answering what it answers.**
#[test]
fn a_reply_is_listed_against_what_it_answers() {
    let mut doc = one_note();
    doc.apply(Operation::ReplyToAnnotation {
        at: at(0),
        contents: "Done".to_owned(),
        by: by("Bo"),
    })
    .expect("it replies");
    let list = comments(&doc);

    let reply = &list[1];
    assert_eq!(reply.reply_to, Some(0));
    assert!(reply.is_reply());
    assert_eq!(reply.contents.as_deref(), Some("Done"));
    assert_eq!(reply.author.as_deref(), Some("Bo"));
    // The same numbers, copied rather than computed: compared exactly.
    assert_eq!(
        reply.rect.map(f64::to_bits),
        list[0].rect.map(f64::to_bits),
        "a reply is placed where what it answers is"
    );
    assert!(!list[0].is_reply());
}

/// **A state needs a person**: 12.5.6.3 says the reply's `/T` "shall specify the user".
#[test]
fn a_state_without_an_author_is_refused() {
    let mut doc = one_note();
    let refused = doc.apply(Operation::SetAnnotationState {
        at: at(0),
        state: AnnotationState::Accepted,
        by: Authorship::default(),
    });
    assert!(matches!(refused, Err(PdfError::Refused { .. })), "{refused:?}");
    assert_eq!(comments(&doc).len(), 1, "nothing was added");
}

/// **Each reviewer's latest state per model is the annotation's**, and a second change by
/// the same reviewer answers their first, as the clause says further changes are made.
#[test]
fn the_latest_state_per_reviewer_and_model_is_shown() {
    let mut doc = one_note();
    let set = |doc: &mut PdfDocument, state, who| {
        doc.apply(Operation::SetAnnotationState { at: at(0), state, by: by(who) })
            .expect("it sets");
    };
    set(&mut doc, AnnotationState::Accepted, "Bo");
    set(&mut doc, AnnotationState::Rejected, "Bo");
    set(&mut doc, AnnotationState::Completed, "Cy");
    set(&mut doc, AnnotationState::Marked, "Bo");
    let list = comments(&doc);

    assert_eq!(list[1].reply_to, Some(0), "Bo's first state answers the note");
    assert_eq!(list[2].reply_to, Some(1), "Bo's second answers Bo's first");
    assert_eq!(list[3].reply_to, Some(0), "Cy's first answers the note");
    assert_eq!(list[4].reply_to, Some(0), "a state in the other model starts its own chain");
    assert_eq!(list[2].sets, Some(AnnotationState::Rejected));
    let mut states = list[0].states.clone();
    states.sort_by(|a, b| (&a.author, a.state.model()).cmp(&(&b.author, b.state.model())));
    assert_eq!(
        states,
        vec![
            StateMark { author: "Bo".into(), state: AnnotationState::Marked },
            StateMark { author: "Bo".into(), state: AnnotationState::Rejected },
            StateMark { author: "Cy".into(), state: AnnotationState::Completed },
        ]
    );
}

/// **Words are replaced where nothing draws them**, and refused where the appearance does.
#[test]
fn words_are_edited_except_where_the_appearance_draws_them() {
    let mut doc = page_with(
        "4 0 R 5 0 R",
        &[
            "<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (old) >>",
            "<< /Type /Annot /Subtype /FreeText /Rect [40 40 90 60] /Contents (drawn) /DA (/Helv 12 Tf) >>",
        ],
    );
    doc.apply(Operation::EditAnnotation { at: at(0), contents: "new".to_owned(), when: None })
        .expect("a note is edited");
    let refused =
        doc.apply(Operation::EditAnnotation { at: at(1), contents: "x".to_owned(), when: None });

    let list = comments(&doc);
    assert_eq!(list[0].contents.as_deref(), Some("new"));
    assert!(matches!(refused, Err(PdfError::Refused { .. })), "{refused:?}");
    assert_eq!(list[1].contents.as_deref(), Some("drawn"));
}

/// **Removing an annotation takes its replies and states with it**, and a widget is
/// refused, because it is half of a form field.
#[test]
fn removing_takes_the_answers_and_refuses_a_widget() {
    let mut doc = page_with(
        "4 0 R 5 0 R",
        &[
            "<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (Fix this) /T (Ann) >>",
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [100 100 200 120] >>",
        ],
    );
    doc.apply(Operation::ReplyToAnnotation { at: at(0), contents: "ok".to_owned(), by: by("Bo") })
        .expect("it replies");
    doc.apply(Operation::SetAnnotationState {
        at: at(0),
        state: AnnotationState::Accepted,
        by: by("Bo"),
    })
    .expect("it sets");
    assert_eq!(comments(&doc).len(), 4);

    let widget = doc.apply(Operation::RemoveAnnotation(at(1)));
    assert!(matches!(widget, Err(PdfError::Refused { .. })), "{widget:?}");
    doc.apply(Operation::RemoveAnnotation(at(0))).expect("it removes");

    let left = comments(&doc);
    assert_eq!(left.len(), 1, "the note, its reply and its state went: {left:?}");
    assert_eq!(left[0].subtype, "Widget");
    let past = doc.apply(Operation::RemoveAnnotation(at(5)));
    assert!(matches!(past, Err(PdfError::NotFound(_))), "{past:?}");
}

/// **A reply chain survives a save**: what answers what is a reference, and the writer
/// renumbers objects.
#[test]
fn replies_and_states_survive_a_save() {
    let mut doc = one_note();
    doc.apply(Operation::ReplyToAnnotation {
        at: at(0),
        contents: "Done".to_owned(),
        by: by("Bo"),
    })
    .expect("it replies");
    doc.apply(Operation::SetAnnotationState {
        at: at(0),
        state: AnnotationState::Completed,
        by: by("Bo"),
    })
    .expect("it sets");
    let path = std::env::temp_dir().join(format!("fepdf-review-{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let back =
        PdfDocument::open(std::fs::read(&path).expect("it is there").into()).expect("it reopens");
    std::fs::remove_file(&path).expect("cleaned up");

    let list = comments(&back);
    assert_eq!(list[1].reply_to, Some(0));
    assert_eq!(
        list[0].states,
        vec![StateMark { author: "Bo".into(), state: AnnotationState::Completed }]
    );
}
