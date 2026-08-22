//! What a document does when opened (12.6), and what it takes to fire each action.
//!
//! The cases here are the ones a screen gets wrong by omission rather than by error: a
//! script filed in the `/Names /JavaScript` tree that nothing points at, a `/Launch`
//! whose target is only in the deprecated `/Win` dictionary, a `/JS` written as a stream
//! because the script is long, and an `/S` the standard does not define. Each is a shape
//! the external corpus actually carries, rebuilt small enough to assert on.

use fepdf::{ActionReport, Capability, IngestionOptions, PdfDocument, Says, Trigger};

/// Assembles a file from object bodies numbered from 1.
fn assemble(bodies: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let size = bodies.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

/// A one-page document with `catalogue` merged into the catalogue, `page` into the page,
/// and `extra` as objects 4 onward.
fn document(catalogue: &str, page: &str, extra: &[String]) -> ActionReport {
    let mut bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalogue} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 99 99] {page} >>"),
    ];
    bodies.extend_from_slice(extra);
    let doc =
        PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
            .expect("the fixture opens");
    ActionReport::of(doc.inner()).expect("the report reads")
}

/// The script the report found for the first action that carries one.
fn first_script(report: &ActionReport) -> Option<String> {
    report.actions.iter().find_map(|a| match &a.says {
        Some(Says::Script(script)) => Some(script.clone()),
        _ => None,
    })
}

/// **The case nothing points at.** A script in the `/Names /JavaScript` tree runs when the
/// document opens, and no annotation, page or catalogue entry refers to it — so a walk
/// that only follows `/OpenAction`, `/A` and `/AA` reports the document as doing nothing.
/// Two files of the external corpus are exactly this, and they are the only two in 524
/// that run code without the reader touching anything.
#[test]
fn a_script_in_the_name_tree_runs_when_the_document_opens() {
    let report = document(
        "/Names << /JavaScript 4 0 R >>",
        "",
        &[
            "<< /Names [(Compatibility) 5 0 R] >>".to_string(),
            "<< /Type /Action /S /JavaScript /JS (app.alert\\(\"hi\"\\);) >>".to_string(),
        ],
    );
    let unprompted = report.without_interaction();
    assert_eq!(unprompted.len(), 1, "the name tree was not walked: {:?}", report.actions);
    assert_eq!(unprompted[0].capability, Capability::RunsCode);
    assert_eq!(
        unprompted[0].trigger,
        Trigger::DocumentScript("Compatibility".to_string()),
        "the name it is filed under is part of the answer"
    );
    assert_eq!(first_script(&report).as_deref(), Some("app.alert(\"hi\");"));
}

/// A name tree that nests through `/Kids` is walked to the leaves.
#[test]
fn a_nested_name_tree_is_walked_to_its_leaves() {
    let report = document(
        "/Names << /JavaScript 4 0 R >>",
        "",
        &[
            "<< /Kids [5 0 R] >>".to_string(),
            "<< /Names [(deep) 6 0 R] >>".to_string(),
            "<< /Type /Action /S /JavaScript /JS (deep\\(\\);) >>".to_string(),
        ],
    );
    assert_eq!(report.without_interaction().len(), 1, "{:?}", report.actions);
}

/// **The target only the deprecated entry carries.** 12.6.4.6 deprecates `/Win`, `/Mac`
/// and `/Unix`, and `isartor-6-6-1-t01-fail-a.pdf` writes `/Win << /F (TextPad.exe) … >>`
/// with no `/F` of its own — so reading only the undeprecated entry reports a document
/// that launches an executable as launching nothing.
#[test]
fn a_launch_target_is_read_out_of_the_deprecated_platform_dictionary() {
    let report = document(
        "",
        "/Annots [4 0 R]",
        &[
            "<< /Type /Annot /Subtype /Link /Rect [0 0 9 9] /A 5 0 R >>".to_string(),
            "<< /Type /Action /S /Launch /Win << /F (TextPad.exe) /P (status.txt) >> >>"
                .to_string(),
        ],
    );
    assert_eq!(report.actions.len(), 1);
    assert_eq!(report.actions[0].capability, Capability::LaunchesAnother);
    assert_eq!(report.actions[0].says, Some(Says::File("TextPad.exe".to_string())));
    assert_eq!(
        report.actions[0].trigger,
        Trigger::AnnotationActivated { page: 0, subtype: "Link".to_string() },
        "which page and which kind of annotation is the difference between a warning \
         and a shrug"
    );
    assert!(report.without_interaction().is_empty(), "a link waits for the reader");
}

/// `/JS` may be a stream rather than a string, which is what a script longer than a line
/// is written as. Reading only the string form reports a long script as absent.
#[test]
fn a_script_written_as_a_stream_is_read() {
    let script = "var v = app.viewerVersion;";
    let report = document(
        "/OpenAction 4 0 R",
        "",
        &[
            "<< /Type /Action /S /JavaScript /JS 5 0 R >>".to_string(),
            format!("<< /Length {} >>\nstream\n{script}\nendstream", script.len()),
        ],
    );
    assert_eq!(first_script(&report).as_deref(), Some(script));
    assert_eq!(report.without_interaction().len(), 1);
}

/// An `/S` clause 12.6.4 does not define is **not** harmless-by-default. The corpus
/// carries `/SetState` and `/NOP`, which no edition of this standard defines, and folding
/// them into "stays inside the document" would report an unknown as safe.
#[test]
fn an_action_type_the_standard_does_not_define_is_not_called_harmless() {
    let report = document("/OpenAction << /Type /Action /S /SetState >>", "", &[]);
    assert_eq!(report.actions.len(), 1);
    assert_eq!(report.actions[0].capability, Capability::Undefined);
    assert_ne!(report.actions[0].capability, Capability::StaysInside);
}

/// A `/Next` chain is followed, and what it reaches is attributed to the chain rather
/// than to the trigger — an action that runs *because another one did* is a different
/// fact from one the reader fired.
#[test]
fn a_next_chain_is_followed_and_marked_as_chained() {
    let report = document(
        "/OpenAction 4 0 R",
        "",
        &[
            "<< /Type /Action /S /GoTo /Next 5 0 R >>".to_string(),
            "<< /Type /Action /S /JavaScript /JS (chained\\(\\);) >>".to_string(),
        ],
    );
    assert_eq!(report.actions.len(), 2, "the chain was not followed: {:?}", report.actions);
    assert_eq!(report.actions[1].trigger, Trigger::Chained);
    assert_eq!(report.actions[1].capability, Capability::RunsCode);
    assert!(
        report.without_interaction().len() == 1,
        "a chained action is fired by the one before it, which is already reported"
    );
}

/// A page's `/AA` and an annotation's `/AA` are different triggers on different objects,
/// and both carry which page they are on.
#[test]
fn page_and_annotation_events_are_told_apart() {
    let report = document(
        "",
        "/AA << /O 4 0 R >> /Annots [5 0 R]",
        &[
            "<< /Type /Action /S /URI /URI (https://example.invalid/page) >>".to_string(),
            "<< /Type /Annot /Subtype /Widget /Rect [0 0 9 9] \
              /AA << /K 6 0 R >> >>"
                .to_string(),
            "<< /Type /Action /S /URI /URI (https://example.invalid/keystroke) >>".to_string(),
        ],
    );
    let triggers: Vec<&Trigger> = report.actions.iter().map(|a| &a.trigger).collect();
    assert!(
        triggers.contains(&&Trigger::PageEvent { page: 0, event: "O".to_string() }),
        "{triggers:?}"
    );
    assert!(
        triggers.contains(&&Trigger::AnnotationEvent { page: 0, event: "K".to_string() }),
        "{triggers:?}"
    );
    assert_eq!(
        report.capabilities(),
        vec![(Capability::ReachesOutside, 2)],
        "a URI reaches outside the document whichever event fires it"
    );
}

/// A document that carries no action at all says so, rather than reporting nothing
/// because the walk failed.
#[test]
fn a_document_with_no_actions_reports_none() {
    let report = document("", "", &[]);
    assert!(report.actions.is_empty());
    assert!(report.capabilities().is_empty());
    assert_eq!(report.unreadable, 0);
}

/// `/OpenAction` may be a destination array rather than an action (12.3.2). That moves
/// the view and does nothing else, so it is not an action and not a defect — counting it
/// as unreadable would make a conforming file look suspicious.
#[test]
fn an_open_action_that_is_a_destination_is_not_counted_as_unreadable() {
    let report = document("/OpenAction [3 0 R /Fit]", "", &[]);
    assert!(report.actions.is_empty());
    assert_eq!(report.unreadable, 0);
}
