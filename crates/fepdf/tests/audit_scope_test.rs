//! What the audit looked at, beside what it found.
//!
//! **A clean report from a check that was never run is the worst answer this engine can
//! give**, and it was the answer it gave. `audit_ua2` returned findings and a caller had
//! no way to tell "nothing is wrong" from "almost nothing was examined": the Matterhorn
//! Protocol 1.1 is 31 checkpoints comprised of 137 failure conditions, and this auditor
//! reports fourteen.
//!
//! `PdfStandard::UA2` writes a conformance claim into the catalogue, and that claim is a
//! statement about 137 things. Saying which fourteen were looked at is not a nicety; it is
//! the difference between a report and an assurance nobody checked.
//!
//! **And the numbers have to be the right ones.** All three were wrong until 2026-09-21,
//! cited from memory: 14-001 is "Headings are not tagged" where this checks that levels
//! are not skipped, which is 14-003, and 13-001 is graphics not tagged as a `<Figure>`
//! where this checks the missing alternative text, which is 13-004. The third — a
//! structure element naming a page that is not there — matched **no** failure condition,
//! because a broken reference is not a way to fail PDF/UA-1, and it went. A fourth
//! survived one level up, in the facade: a document with no structure tree was reported
//! under `00-001`, which is not a number the protocol has either.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_doc::{
    AuditFinding, AuditReport, AuditScope, FROM_CATALOGUE, FROM_CONTENT, FROM_FORM,
    FROM_STRUCTURE_TREE, MatterhornAuditor, NO_STRUCTURE_TREE, Outcome,
};
use std::collections::BTreeSet;

fn opened(bytes: Vec<u8>) -> PdfDocument {
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// A tagged document that breaks every failure condition this auditor looks at but one.
///
/// **One element or entry per check, so that a failure names which one is not found.**
/// 07-002 is the exception and cannot be here: it fails on `/DisplayDocTitle` being
/// *false*, and 07-001 fails on its being absent, so one document cannot break both.
/// [`display_doc_title_false`] is the other half.
///
/// | | How this document breaks it |
/// | :--- | :--- |
/// | 01-003 | an `/Artifact` sequence opened inside `/P <</MCID 0>>` |
/// | 01-004 | a `/P <</MCID 1>>` opened inside an `/Artifact` |
/// | 01-005 | a `re f` under neither |
/// | 01-007 | `/MarkInfo /Suspects true` |
/// | 07-001 | no `/ViewerPreferences`, so no `/DisplayDocTitle` |
/// | 11-002 | a `<Span>` with `/ActualText` and no `/Lang` reaching it |
/// | 13-004 | a `<Figure>` with neither `/Alt` nor `/ActualText` |
/// | 14-002 | the first numbered heading is `<H2>` |
/// | 14-003 | `<H4>` follows `<H2>` |
/// | 14-006 | a `<Sect>` holding two `<H>` children |
/// | 14-007 | those `<H>`s beside the `<H2>` and `<H4>` |
/// | 17-002 | a `<Formula>` with no `/Alt` |
/// | 28-005 | a form field with no `/TU` |
fn breaks_everything() -> Vec<u8> {
    // Three lines, one condition each. The `f` inside the nested sequences is under a
    // tag or an artefact either way, so the only mark under neither is the first.
    let content = "0 0 5 5 re f\n\
                   /P <</MCID 0>> BDC /Artifact BMC 0 0 5 5 re f EMC EMC\n\
                   /Artifact BMC /P <</MCID 1>> BDC 0 0 5 5 re f EMC EMC";
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 12 0 R \
           /MarkInfo << /Marked true /Suspects true >> \
           /AcroForm << /Fields [13 0 R] >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 14 0 R >>".to_string(),
        "<< /Type /StructElem /S /H2 /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /H4 /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Sect /P 12 0 R /K [7 0 R 8 0 R] >>".to_string(),
        "<< /Type /StructElem /S /H /P 6 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /H /P 6 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Figure /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Formula /P 12 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Span /P 12 0 R /Pg 3 0 R /ActualText (ibid.) >>".to_string(),
        "<< /Type /StructTreeRoot /K [4 0 R 5 0 R 6 0 R 9 0 R 10 0 R 11 0 R] >>".to_string(),
        "<< /FT /Tx /T (Given name) >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ])
    .into_iter()
    .collect()
}

/// The one condition [`breaks_everything`] cannot also break: 07-002 wants the entry
/// present and false, where 07-001 wants it absent.
fn display_doc_title_false() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /ViewerPreferences << /DisplayDocTitle false >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ])
    .into_iter()
    .collect()
}

/// A tagged document that breaks none of the conditions this auditor looks at.
///
/// Headings `<H1>` then `<H2>` and no `<H>` (14-002, 14-003, 14-007), a figure with its
/// alternative text and a formula with its own (13-004, 17-002), a `/Lang` on the
/// catalogue for the `<Span>`'s `/ActualText` to be read in (11-002), `/Suspects` absent
/// (01-007), `/DisplayDocTitle` true (07-001, 07-002), no form at all (28-005), one
/// `<H>` per node and none of them beside a `<Hn>` (14-006), and a page whose every mark
/// is under either an `/MCID` or an `/Artifact` (01-003, 01-004, 01-005).
fn breaks_nothing() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 9 0 R /Lang (en-GB) \
           /MarkInfo << /Marked true >> \
           /ViewerPreferences << /DisplayDocTitle true >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 10 0 R >>".to_string(),
        "<< /Type /StructElem /S /H1 /P 9 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /H2 /P 9 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Figure /P 9 0 R /Pg 3 0 R /Alt (a duck) >>".to_string(),
        "<< /Type /StructElem /S /Formula /P 9 0 R /Pg 3 0 R /Alt (E equals m c squared) >>"
            .to_string(),
        "<< /Type /StructElem /S /Span /P 9 0 R /Pg 3 0 R /ActualText (ibid.) >>".to_string(),
        "<< /Type /StructTreeRoot /K [4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] >>".to_string(),
        // **A page that is actually marked, rather than a page that draws nothing.** An
        // empty `/Contents` passes checkpoint 01 vacuously, which is not the sound case
        // worth having: one sequence carries an `/MCID` and the other is an artefact.
        {
            let content = "/P <</MCID 0>> BDC 0 0 5 5 re f EMC\n\
                           /Artifact BMC 10 10 5 5 re f EMC";
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len())
        },
    ])
    .into_iter()
    .collect()
}

/// A document with a catalogue and no structure tree at all.
fn untagged() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ])
    .into_iter()
    .collect()
}

/// Every condition the report names, whatever came of checking it.
fn reported(report: &AuditReport) -> Vec<&str> {
    report.findings.iter().map(|f| f.checkpoint.as_str()).collect()
}

/// What came of one condition, as many times as the report says it.
fn outcomes(report: &AuditReport, condition: &str) -> Vec<Outcome> {
    report.findings.iter().filter(|f| f.checkpoint == condition).map(|f| f.outcome).collect()
}

/// **The report says how much of the protocol it looked at.**
///
/// Fourteen of 137 failure conditions. A reader told "no findings" and not told that would
/// have been told the document conforms.
#[test]
fn the_report_says_how_much_of_the_protocol_it_checked() {
    let doc = opened(breaks_everything());
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        report.scope.in_protocol, 137,
        "the protocol's size is not what this was written about"
    );
    assert_eq!(
        report.scope.checked.len(),
        14,
        "the scope does not name the failure conditions this auditor looks at"
    );
    assert!(
        report.scope.checked.len() < report.scope.in_protocol,
        "the scope claims the whole protocol"
    );
}

/// **The three lists of conditions partition the one the scope is built from.**
///
/// A condition is decided by reading the catalogue, by reading the form, or by walking
/// the structure tree, and which of the three decides it is what says whether it was
/// examined at all in a document with no tree. A condition in `CHECKED` and in none of
/// the three would be promised and never looked at; one in two of them would be reported
/// twice.
#[test]
fn every_checked_condition_is_decided_by_exactly_one_reader() {
    let mut union: Vec<&str> = Vec::new();
    union.extend(FROM_CATALOGUE);
    union.extend(FROM_FORM);
    union.extend(FROM_CONTENT);
    union.extend(FROM_STRUCTURE_TREE);

    let distinct: BTreeSet<&str> = union.iter().copied().collect();
    assert_eq!(distinct.len(), union.len(), "a condition is in two of the four lists: {union:?}");
    assert_eq!(
        distinct,
        MatterhornAuditor::CHECKED.iter().copied().collect::<BTreeSet<&str>>(),
        "the four lists and the scope do not name the same conditions"
    );
}

/// **Every condition the audit reports is one the scope names.**
///
/// Adding a check and forgetting to say so is how a scope stops being true, and it is
/// invisible from the outside: the report grows and the promise does not. The one row
/// that is allowed not to be a failure condition is [`NO_STRUCTURE_TREE`], which is a
/// clause rather than an index — and the next test holds it to that.
#[test]
fn the_scope_names_every_checkpoint_reported() {
    for fixture in [breaks_everything(), breaks_nothing(), untagged(), display_doc_title_false()] {
        let doc = opened(fixture);
        let report = doc.audit_ua2_report().expect("it audits");
        assert!(!report.findings.is_empty(), "the audit reported nothing at all");

        for finding in &report.findings {
            if finding.checkpoint == NO_STRUCTURE_TREE {
                continue;
            }
            assert!(
                report.scope.checked.contains(&finding.checkpoint),
                "the audit reported {} and the scope does not name it: {:?}",
                finding.checkpoint,
                report.scope.checked
            );
        }
    }
}

/// **Nothing is reported under a number the protocol does not have.**
///
/// `00-001` was one: the facade filed "not a tagged PDF" under it, and no checkpoint 00
/// exists. A wrong number is not a gap in the audit — it is a finding against a defect
/// that is not there, and whoever looks it up is told something untrue. The row survives
/// as the *clause* PDF/UA-1 states the requirement in, which is a thing that can be
/// looked up.
#[test]
fn nothing_is_reported_under_a_number_the_protocol_does_not_have() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(
        reported(&report).contains(&NO_STRUCTURE_TREE),
        "a document with no structure tree did not say so: {:?}",
        reported(&report)
    );
    for finding in &report.findings {
        assert_ne!(finding.checkpoint, "00-001", "00-001 is not a Matterhorn failure condition");
        let indexed = finding.checkpoint.len() == 6
            && finding.checkpoint.is_char_boundary(2)
            && finding.checkpoint[2..3] == *"-";
        assert!(
            !indexed || MatterhornAuditor::CHECKED.contains(&finding.checkpoint.as_str()),
            "{} looks like an index number and is not one this auditor checks",
            finding.checkpoint
        );
    }
}

/// And every condition the scope names is one the audit can report as broken.
///
/// The other direction, and the stronger one: a scope naming a condition nothing looks at
/// is a promise of work that is not done. **Proved by breaking the thing each check
/// checks** — a fixture where the check merely *runs* would pass this while reporting
/// every condition sound.
#[test]
fn every_condition_the_scope_names_can_be_reported_broken() {
    let mut waiting: BTreeSet<&str> = MatterhornAuditor::CHECKED.iter().copied().collect();

    for fixture in [breaks_everything(), display_doc_title_false()] {
        let doc = opened(fixture);
        let report = doc.audit_ua2_report().expect("it audits");
        for finding in &report.findings {
            if finding.outcome != Outcome::Sound {
                waiting.remove(finding.checkpoint.as_str());
            }
        }
    }

    assert!(waiting.is_empty(), "the scope names these and no fixture provokes one: {waiting:?}");
}

/// **A document with no structure tree says which check it failed, not "no findings".**
///
/// It is not a tagged PDF, which is the one thing this can say without looking at a
/// condition — and the scope still says what it would have looked at rather than claiming
/// it did.
#[test]
fn an_untagged_document_is_told_apart_from_a_clean_one() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(!report.found_nothing(), "an untagged document came back with nothing said");
    assert_eq!(report.scope.in_protocol, 137, "the scope forgot the protocol");
}

/// **A catalogue is still a catalogue when there is no structure tree.**
///
/// The three conditions that are properties of the catalogue and the one that is a
/// property of the form do not need a tag in the document. Reporting only "not a tagged
/// PDF" about such a file left four checks promised by the scope and run on nothing —
/// which is the shape this whole item exists to remove, one document short of where it
/// was being removed.
#[test]
fn an_untagged_document_is_still_asked_the_catalogue_conditions() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");
    let named = reported(&report);

    for condition in FROM_CATALOGUE.iter().chain(FROM_FORM.iter()) {
        assert!(
            named.contains(condition),
            "{condition} does not need a structure tree and was not reported: {named:?}"
        );
    }
    // This one has no `/ViewerPreferences`, so one of the four is broken rather than
    // merely asked.
    assert_eq!(
        outcomes(&report, "07-001"),
        vec![Outcome::Broken],
        "07-001 is broken by this document and was not reported so"
    );
}

/// **`found_nothing` could not return true, and nothing noticed.**
///
/// It was `findings.is_empty()`. Once a checked and unbroken condition became a `Sound`
/// finding, a clean document carried one row per condition in `CHECKED`, so the list was
/// never empty and the method always answered `false` — which made the assertion above,
/// that a broken document did not come back silent, pass for every input including a
/// perfect one. This is the document that makes it answer `true`.
#[test]
fn a_document_breaking_nothing_checked_is_said_to_have_broken_nothing() {
    let doc = opened(breaks_nothing());
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(
        report.found_nothing(),
        "a document breaking no checked condition was reported as breaking one: {:?}",
        report
            .findings
            .iter()
            .filter(|f| f.outcome != Outcome::Sound)
            .map(|f| (&f.checkpoint, &f.message))
            .collect::<Vec<_>>()
    );
    // And it is not silent: the sound conditions are still reported, so "nothing to act
    // on" and "nothing was looked at" stay apart.
    assert_eq!(
        report.findings.iter().filter(|f| f.outcome == Outcome::Sound).count(),
        MatterhornAuditor::CHECKED.len(),
        "a clean document did not report the conditions it was checked against"
    );
}

/// **A condition checked and not broken is a result the report carries.**
///
/// A reader is owed what was examined, not only what was wrong. This document breaks
/// 14-003 and not 13-004, so the report says both — one broken, one sound.
#[test]
fn a_condition_that_came_out_sound_is_in_the_report() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            // H1 then H3: 14-003 is broken, 14-002 is not.
            "<< /Type /StructElem /S /H1 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructElem /S /H3 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructTreeRoot /K [4 0 R 5 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "14-003"),
        vec![Outcome::Broken],
        "the skipped heading level was not reported"
    );
    assert_eq!(
        outcomes(&report, "13-004"),
        vec![Outcome::Sound],
        "a condition checked and not broken is missing from the report"
    );
    assert_eq!(
        outcomes(&report, "14-002"),
        vec![Outcome::Sound],
        "the first numbered heading is <H1> and 14-002 was not reported sound"
    );
}

/// **Sound and not-looked-at are different answers, and the report keeps them apart.**
///
/// Every condition the report calls sound has to be one the scope says was checked. A
/// report that called an unexamined condition sound would say a document conforms on the
/// strength of work nobody did — which is the shape this whole item exists to remove.
#[test]
fn nothing_is_called_sound_that_was_not_checked() {
    for fixture in [breaks_everything(), breaks_nothing(), untagged()] {
        let doc = opened(fixture);
        let report = doc.audit_ua2_report().expect("it audits");

        for finding in &report.findings {
            if finding.outcome == Outcome::Sound {
                assert!(
                    report.scope.checked.contains(&finding.checkpoint),
                    "{} is reported sound and the scope does not say it was checked",
                    finding.checkpoint
                );
            }
        }
        // **A condition is broken or sound, never both** — and this counts rather than
        // collecting into a set, because a set is exactly what hides the defect. A first
        // version of this test gathered the checkpoints into a `BTreeSet` and looked one
        // up with `find`, and a mutation that reported every condition sound *including
        // the broken ones* passed all eight tests: the set collapsed the duplicate and
        // `find` returned the first of the pair.
        //
        // **One sound row per condition and no more**, which is W-21g's rule; a broken
        // one may repeat, because a document with four untagged figures breaks 13-004
        // four times and a reader wants all four.
        for condition in &report.scope.checked {
            let came_to = outcomes(&report, condition);
            let sound = came_to.iter().filter(|o| **o == Outcome::Sound).count();
            assert!(sound <= 1, "{condition} is reported sound {sound} times: {came_to:?}");
            assert!(
                sound == 0 || came_to.len() == 1,
                "{condition} is reported both sound and not: {came_to:?}"
            );
        }
    }
}

/// A document with no structure tree has that tree's conditions examined by nothing.
///
/// Reporting them as sound because the walk found no elements to break them would be the
/// emptiest kind of pass.
#[test]
fn an_untagged_document_has_no_structure_condition_to_call_sound() {
    let doc = opened(untagged());
    let report = doc.audit_ua2_report().expect("it audits");

    for condition in FROM_STRUCTURE_TREE {
        assert!(
            outcomes(&report, condition).is_empty(),
            "{condition} is decided by walking a structure tree this document has not got, \
             and it was reported anyway: {:?}",
            outcomes(&report, condition)
        );
    }
}

/// **A condition waiting for a reader is not "nothing found".**
///
/// 28-005 is the first condition with a producer: it reads "a form field does not have a
/// `TU` entry **and** does not have an alternative description (in the form of an `Alt`
/// entry in the enclosing structure element)", and the second half is reached through an
/// `/OBJR`, which nothing here follows. So a field with no `/TU` is handed to a reader
/// with what was found, and is not counted as nothing to act on.
#[test]
fn a_condition_left_for_a_reader_is_not_nothing_found() {
    let doc = opened(breaks_everything());
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "28-005"),
        vec![Outcome::ForAReader],
        "a form field with no /TU was not left for a reader"
    );
    let row =
        report.findings.iter().find(|f| f.checkpoint == "28-005").expect("28-005 is in the report");
    // **A suspicion carries its evidence or it is not shown.** The field's name is what
    // lets a reader agree or disagree; "28-005 suspected" is not something to act on.
    assert!(
        row.message.contains("Given name"),
        "the row does not say which field it is about: {}",
        row.message
    );

    let waiting = AuditReport {
        findings: vec![AuditFinding {
            checkpoint: "28-005".into(),
            severity: "Warning".into(),
            outcome: Outcome::ForAReader,
            message: "the field is yours to judge".into(),
            handle_id: None,
        }],
        scope: AuditScope {
            checked: MatterhornAuditor::CHECKED.iter().map(|c| (*c).to_string()).collect(),
            in_protocol: MatterhornAuditor::IN_PROTOCOL,
        },
    };
    assert!(
        !waiting.found_nothing(),
        "a condition handed to a reader to decide was reported as nothing to act on"
    );
}

/// **A field that states its `/TU` decides the condition rather than deferring it.**
///
/// 28-005 is a conjunction, and its first half is reachable from here: a field with a
/// `/TU` does not break it whatever its enclosing structure element says. Deferring that
/// to a reader would hand over work this engine has already done.
#[test]
fn a_form_field_that_states_its_tu_settles_the_condition() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /FT /Tx /T (Given name) /TU (Your given name, as on your passport) >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "28-005"),
        vec![Outcome::Sound],
        "a form field stating its /TU was not settled"
    );
}

/// **A `/Lang` an ancestor states reaches the element under it (14.9.2.2).**
///
/// The catalogue states none here and the `<Sect>` above the `<Span>` does, so the
/// `/ActualText` has a language and 11-002 is not broken. Checking the element's own
/// `/Lang` alone would report a document that is conforming.
#[test]
fn a_language_stated_on_an_ancestor_reaches_the_element() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /Sect /P 6 0 R /Lang (cy) /K [5 0 R] >>",
            "<< /Type /StructElem /S /Span /P 4 0 R /Pg 3 0 R /ActualText (ibid.) >>",
            "<< /Type /StructTreeRoot /K [4 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "11-002"),
        vec![Outcome::Sound],
        "a /Lang on the enclosing element did not reach the text under it"
    );
}

/// **A `<Figure>` carrying `/ActualText` and no `/Alt` does not break 13-004.**
///
/// The condition is "`<Figure>` tag alternative **or replacement** text missing", and
/// this read `/Alt` alone — so a figure whose replacement text is what 7.3 paragraph 3
/// allows was reported as breaking a condition it does not break. 17-002 really is `/Alt`
/// alone, which is why the two are not one test spelt twice.
#[test]
fn a_figure_with_replacement_text_and_no_alternative_text_is_sound() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /Lang (en-GB) >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /Figure /P 6 0 R /Pg 3 0 R /ActualText (Figure 1) >>",
            "<< /Type /StructElem /S /Formula /P 6 0 R /Pg 3 0 R /ActualText (x) >>",
            "<< /Type /StructTreeRoot /K [4 0 R 5 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "13-004"),
        vec![Outcome::Sound],
        "a figure with replacement text was reported as having none"
    );
    // And the formula beside it, whose condition names `/Alt` and nothing else, is broken
    // by the same entry that settles the figure.
    assert_eq!(
        outcomes(&report, "17-002"),
        vec![Outcome::Broken],
        "17-002 names an Alt attribute and /ActualText was taken for one"
    );
}

/// **A heading level is a number, not a character.**
///
/// The test for `<Hn>` was `tag.len() == 2`, which stops at `<H9>`: a document going
/// `<H1>` to `<H10>` skipped eight levels and `<H10>` was not a heading at all, so 14-003
/// had nothing to compare. 14-005 is about a seventh level and higher, so a document deep
/// enough to need `<H10>` is one this was written for.
#[test]
fn a_heading_past_the_ninth_level_is_still_a_heading() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /H1 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructElem /S /H10 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructTreeRoot /K [4 0 R 5 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "14-003"),
        vec![Outcome::Broken],
        "<H10> after <H1> skips eight levels and was not reported"
    );
    // And the first numbered heading is still <H1>, so 14-002 is not dragged in with it.
    assert_eq!(outcomes(&report, "14-002"), vec![Outcome::Sound], "14-002 fired on <H1> first");
}

/// A tagged one-page document drawing `content`, with an image and a form to draw.
///
/// The structure tree is one `<P>` claiming `/MCID 0`, so a mark under that sequence is
/// tagged and anything outside it is not.
fn page_drawing(content: &str) -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 8 0 R /Lang (en-GB) >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /Resources << /XObject << /Im0 5 0 R /Fm0 6 0 R >> \
                         /Properties << /Pr1 << /MCID 4 >> /Pr2 << /Foo 1 >> >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray \
           /BitsPerComponent 8 /Length 1 >>\nstream\n0\nendstream"
            .to_string(),
        "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length 0 >>\nstream\n\nendstream"
            .to_string(),
        "<< /Type /StructElem /S /P /P 8 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructElem /S /Span /P 8 0 R /Pg 3 0 R >>".to_string(),
        "<< /Type /StructTreeRoot /K [7 0 R] >>".to_string(),
    ])
    .into_iter()
    .collect()
}

/// **A form XObject under neither a tag nor an artefact is a place to look, not a
/// verdict.**
///
/// Its own content stream may open the sequences, and this walk does not descend into it.
/// Reporting 01-005 broken here would be a finding against a file that conforms; saying
/// nothing would be the silence this phase keeps removing.
#[test]
fn a_form_xobject_under_no_tag_is_left_for_a_reader() {
    let doc = opened(page_drawing("q /Fm0 Do Q"));
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "01-005"),
        vec![Outcome::ForAReader],
        "a form drawn under neither was decided rather than handed over"
    );
    let row = report.findings.iter().find(|f| f.checkpoint == "01-005").expect("01-005 is there");
    assert!(
        row.message.contains("Fm0"),
        "the row does not say which XObject it is about: {}",
        row.message
    );
}

/// **An image under neither is decided, because an image holds no marks of its own.**
///
/// The same `Do` operator, and the opposite answer: what makes the form undecidable is
/// its content stream, and an image XObject has none. Treating the two alike would either
/// lose a real finding or invent one.
#[test]
fn an_image_under_no_tag_is_decided() {
    let doc = opened(page_drawing("q /Im0 Do Q"));
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "01-005"),
        vec![Outcome::Broken],
        "an image drawn under neither a tag nor an /Artifact was not reported"
    );
}

/// **A property list named through `/Properties` carries its `/MCID` all the same.**
///
/// `/P /Pr1 BDC` is the same tagging as `/P <</MCID 4>> BDC`, and reading the name as "no
/// `/MCID`" would report every mark on a page written that way as untagged — a document
/// that conforms, reported broken throughout.
#[test]
fn a_named_property_list_tags_what_is_under_it() {
    let tagged = opened(page_drawing("/P /Pr1 BDC 0 0 5 5 re f EMC"));
    assert_eq!(
        outcomes(&tagged.audit_ua2_report().expect("it audits"), "01-005"),
        vec![Outcome::Sound],
        "a named property list carrying an /MCID did not tag what was under it"
    );

    // And one that carries no `/MCID` tags nothing, which is what makes the lookup a
    // lookup rather than a way of passing anything with a name in it.
    let untagged = opened(page_drawing("/P /Pr2 BDC 0 0 5 5 re f EMC"));
    assert_eq!(
        outcomes(&untagged.audit_ua2_report().expect("it audits"), "01-005"),
        vec![Outcome::Broken],
        "a named property list with no /MCID was taken for tagging"
    );
}

/// **A sequence with no `/MCID` tags nothing.**
///
/// A structure element reaches content by `/MCID` and by nothing else, so a sequence
/// without one is not "tagged as real content" however much it looks like a tag. This is
/// the arm that makes 01-005 fire on a file that looks well formed.
///
/// **Both well-formed spellings, because the malformed one proves nothing.** This was
/// `/Span BDC` — `BDC` takes a tag *and* a property list, so with one operand the
/// sublimator finds no tag and emits no sequence at all. The mark came out untagged
/// because the operator was dropped, not because the arm under test said so: a mutation
/// making that arm report `Tagged` left this test passing. `BMC` takes the tag alone and
/// is the well-formed way to open a sequence without a property list.
#[test]
fn a_sequence_with_no_mcid_tags_nothing() {
    for content in ["/Span BMC 0 0 5 5 re f EMC", "/Span <</Foo 1>> BDC 0 0 5 5 re f EMC"] {
        let doc = opened(page_drawing(content));
        assert_eq!(
            outcomes(&doc.audit_ua2_report().expect("it audits"), "01-005"),
            vec![Outcome::Broken],
            "a sequence with no /MCID was taken for tagging: {content}"
        );
    }
}

/// **The two nesting conditions are told apart, and from the third.**
///
/// 01-003 and 01-004 are the same question asked in both directions, which is exactly the
/// shape that gets implemented once and reported twice.
#[test]
fn the_nesting_conditions_are_told_apart() {
    let artifact_in_tagged =
        opened(page_drawing("/P <</MCID 0>> BDC /Artifact BMC 0 0 5 5 re f EMC EMC"));
    let report = artifact_in_tagged.audit_ua2_report().expect("it audits");
    assert_eq!(outcomes(&report, "01-003"), vec![Outcome::Broken], "01-003 did not fire");
    assert_eq!(outcomes(&report, "01-004"), vec![Outcome::Sound], "01-004 fired the wrong way");

    let tagged_in_artifact =
        opened(page_drawing("/Artifact BMC /P <</MCID 0>> BDC 0 0 5 5 re f EMC EMC"));
    let report = tagged_in_artifact.audit_ua2_report().expect("it audits");
    assert_eq!(outcomes(&report, "01-004"), vec![Outcome::Broken], "01-004 did not fire");
    assert_eq!(outcomes(&report, "01-003"), vec![Outcome::Sound], "01-003 fired the wrong way");

    // Neither is broken by the two side by side, which is the ordinary shape of a tagged
    // page: content under its tag, furniture under an artefact.
    let beside =
        opened(page_drawing("/P <</MCID 0>> BDC 0 0 5 5 re f EMC /Artifact BMC 9 9 5 5 re f EMC"));
    let report = beside.audit_ua2_report().expect("it audits");
    for condition in FROM_CONTENT {
        assert_eq!(
            outcomes(&report, condition),
            vec![Outcome::Sound],
            "{condition} fired on a page whose marks are each under one sequence"
        );
    }
}

/// **14-006 counts a node's children, not its descendants.**
///
/// Two `<Sect>`s under one `<Sect>`, each with a heading of its own, is two nodes with one
/// heading each. Counting `<H>` anywhere beneath would report the outer one as holding
/// two, which is how a check about a node becomes a check about a document.
#[test]
fn one_heading_per_node_counts_children_not_descendants() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 9 0 R /Lang (en-GB) >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Type /StructElem /S /Sect /P 9 0 R /K [5 0 R 7 0 R] >>",
            "<< /Type /StructElem /S /Sect /P 4 0 R /K [6 0 R] >>",
            "<< /Type /StructElem /S /H /P 5 0 R /Pg 3 0 R >>",
            "<< /Type /StructElem /S /Sect /P 4 0 R /K [8 0 R] >>",
            "<< /Type /StructElem /S /H /P 7 0 R /Pg 3 0 R >>",
            "<< /Type /StructTreeRoot /K [4 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        outcomes(&report, "14-006"),
        vec![Outcome::Sound],
        "a heading in each of two child nodes was counted as two in one node"
    );
    // And no numbered heading anywhere, so 14-007 has nothing to pair the <H>s with.
    assert_eq!(outcomes(&report, "14-007"), vec![Outcome::Sound], "14-007 fired on <H> alone");
}

/// **Checkpoint 06 is left out because ingestion answers it, not because it is hard.**
///
/// 06-001 is "document does not contain an XMP metadata stream" and 06-003 is "XMP
/// metadata stream does not contain dc:title". `metadata::settle` writes a packet into
/// the catalogue as a file is ingested, and promotes `/Info`'s `/Title` into it — so the
/// document this auditor reads is one where both have already been repaired, because a
/// document here is one normalised state and not the bytes it came from (ADR-0013).
/// 06-001 would answer sound for every file this engine can open, and 06-003 would answer
/// sound for a file whose only title is in the deprecated dictionary, which is a file that
/// breaks it.
///
/// **A check that cannot fail is what this phase has spent itself removing**, so neither
/// is claimed. This test holds that reason to the code: the day ingestion stops writing
/// the packet, it fails, and the two conditions become checkable.
#[test]
fn checkpoint_06_is_left_out_because_ingestion_answers_it() {
    for condition in ["06-001", "06-003"] {
        assert!(
            !MatterhornAuditor::CHECKED.contains(&condition),
            "{condition} is claimed, and what it asks about has been answered before the \
             auditor is called"
        );
    }
    let file = untagged();
    // The file states no `/Metadata`. `survey` reads the bytes rather than the ingested
    // document, which is the whole of the difference being measured here.
    let raw = fepdf::CatalogReport::survey(&file).expect("the catalogue reads");
    assert!(
        raw.entries.iter().all(|entry| entry.key != "Metadata"),
        "the fixture states a /Metadata of its own, so this measures nothing"
    );
    // The document ingested from it does.
    let ingested = opened(file);
    assert!(
        ingested.inner().catalog().expect("the catalogue reads").metadata.is_some(),
        "ingestion no longer writes an XMP packet, so 06-001 and 06-003 can be answered \
         about the file and belong in CHECKED"
    );
}

/// The protocol, as text, for the two tests that read it.
///
/// It is `docs/specs/Matterhorn-Protocol-1-1.pdf`, which is untracked
/// (`docs/specs/README.md` says where to get it at no cost). Without it these cannot
/// check anything, and they say so rather than passing.
fn protocol_text() -> String {
    let path = "../../docs/specs/Matterhorn-Protocol-1-1.pdf";
    let Ok(bytes) = std::fs::read(path) else {
        panic!("{path} is not in this working copy; docs/specs/README.md says where it is");
    };
    let protocol = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the protocol opens");
    let pages = protocol.page_count().expect("it counts");
    (0..pages)
        .map(|page| protocol.extract_text(page).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where the Index column states a number, as the rest of that table row.
///
/// **Not the first occurrence in the document.** 01-007 and 28-005 are named in the
/// Document History before their own tables — "moved 08-003 to 01-007" and "Failure
/// conditions 28-002, 28-004, 28-005 specified more precisely" — so looking for the first
/// occurrence lands on a sentence *about* the number rather than the row that defines it,
/// and the two conditions would have failed a test that is checking the right thing.
/// Index is the table's first column, so a row starts its line.
fn index_rows<'a>(text: &'a str, number: &str) -> Vec<&'a str> {
    text.match_indices(number)
        .filter(|(at, _)| *at == 0 || text.as_bytes()[at - 1] == b'\n')
        .map(|(at, _)| &text[at..])
        .collect()
}

/// **Every number this auditor reports says, in the protocol, what this auditor checks.**
///
/// All three were wrong until 2026-09-21 and nothing noticed, because nothing compared
/// them to the document. A wrong number is not a gap in the audit — it is a finding filed
/// against a different defect, and a reader or a tool that looks the number up is told
/// something untrue.
#[test]
fn every_number_reported_means_in_the_protocol_what_it_is_used_for() {
    let text = protocol_text();

    // What each number has to be followed by in the protocol's own words. Short enough to
    // survive the line breaks a two-column table puts in — the Section, Type and How
    // columns interrupt the first line of every row — and long enough to be that
    // condition and no other.
    let says = [
        ("01-003", "Content marked as Artifact is present inside tagged content"),
        ("01-004", "Tagged content is present inside content marked as Artifact"),
        ("01-005", "Content is neither marked as Artifact nor tagged as real"),
        ("01-007", "Suspects entry has a value of true"),
        ("07-001", "does not contain a DisplayDocTitle"),
        ("07-002", "contains a DisplayDocTitle entry with a"),
        ("11-002", "Natural language for text in Alt, ActualText and"),
        ("13-004", "alternative or replacement text missing"),
        ("14-002", "Does use numbered headings, but the first"),
        ("14-003", "Numbered heading levels in descending"),
        ("14-006", "A node contains more than one <H> tag"),
        ("14-007", "Document uses both <H> and <H#> tags"),
        ("17-002", "<Formula> tag is missing an Alt attribute"),
        ("28-005", "A form field does not have a TU entry and does not"),
    ];
    assert_eq!(
        says.len(),
        MatterhornAuditor::CHECKED.len(),
        "a failure condition was added to the auditor and not to this list"
    );

    for (number, words) in says {
        assert!(
            MatterhornAuditor::CHECKED.contains(&number),
            "{number} is checked for here and the auditor does not name it"
        );
        let rows = index_rows(&text, number);
        assert!(!rows.is_empty(), "the protocol has no table row for {number}");
        assert!(
            rows.iter().any(|row| row.chars().take(160).collect::<String>().contains(words)),
            "the protocol says {number} is something else: {:?}",
            rows.iter().map(|row| row.chars().take(160).collect::<String>()).collect::<Vec<_>>()
        );
    }

    // And what it is a protocol for, which is the reason none of this measures UA-2.
    assert!(
        text.contains("specified in PDF/UA-1"),
        "the protocol no longer says which standard it is about"
    );
}

/// **The protocol's tables enumerate 137 failure conditions and its prose says 136.**
///
/// `IN_PROTOCOL` is the denominator of every "N of M checked" this engine prints, so what
/// it counts has to be the conditions that exist rather than a sentence about them. The
/// sentence is version 1.02's: 1.1's own Document History records "Failure condition
/// 13-008 added", 13-008 is marked `H`, and counting the `How` column gives 87 `M` and 48
/// `H` beside the 2 with no test — one more `H` than the sentence's 47.
///
/// The prose is asserted too, so that an edition correcting the sentence is noticed here
/// rather than silently agreed with.
#[test]
fn every_failure_condition_in_the_protocol_is_counted() {
    let text = protocol_text();

    let numbers: BTreeSet<&str> = text
        .lines()
        .filter_map(|line| line.get(..6))
        .filter(|head| {
            let bytes = head.as_bytes();
            bytes[..2].iter().all(u8::is_ascii_digit)
                && bytes[2] == b'-'
                && bytes[3..].iter().all(u8::is_ascii_digit)
        })
        .collect();

    // **Contiguous from 001, checkpoint by checkpoint**, which is what makes counting
    // distinct numbers a count of the conditions rather than of whatever matched: a gap
    // would mean the extraction lost a row, and a stray match would show as a checkpoint
    // whose highest index exceeds how many it has.
    let mut counted = 0;
    for checkpoint in 1..=31 {
        let here: Vec<&str> = numbers
            .iter()
            .copied()
            .filter(|n| n.starts_with(&format!("{checkpoint:02}-")))
            .collect();
        assert!(!here.is_empty(), "checkpoint {checkpoint:02} has no failure conditions");
        for (index, number) in here.iter().enumerate() {
            assert_eq!(
                *number,
                format!("{checkpoint:02}-{:03}", index + 1),
                "checkpoint {checkpoint:02} is not numbered from 001 without a gap: {here:?}"
            );
        }
        counted += here.len();
    }
    assert_eq!(numbers.len(), counted, "a number matched outside checkpoints 01 to 31");

    assert_eq!(counted, 137, "the protocol's tables no longer enumerate what this counts");
    assert_eq!(MatterhornAuditor::IN_PROTOCOL, counted, "the stated total drifted from the tables");

    // The sentence that disagrees, and the change that explains it.
    assert!(
        text.contains("136 failure conditions"),
        "the protocol's prose no longer says 136, so the reason IN_PROTOCOL disagrees with \
         it has gone and the record explaining it needs rereading"
    );
    assert!(
        text.contains("Failure condition 13-008 added"),
        "the Document History no longer records the condition version 1.1 added, which is \
         the whole of why the tables and the prose differ by one"
    );
}
