//! What the audit looked at, beside what it found.
//!
//! **A clean report from a check that was never run is the worst answer this engine can
//! give**, and it was the answer it gave. `audit_ua2` returned findings and a caller had
//! no way to tell "nothing is wrong" from "almost nothing was examined": the Matterhorn
//! Protocol 1.1 is 31 checkpoints comprised of 136 failure conditions, and this auditor
//! reports two.
//!
//! `PdfStandard::UA2` writes a conformance claim into the catalogue, and that claim is a
//! statement about 136 things. Saying which two were looked at is not a nicety; it is the
//! difference between a report and an assurance nobody checked.
//!
//! **And the numbers have to be the right ones.** All three were wrong until 2026-09-21,
//! cited from memory: 14-001 is "Headings are not tagged" where this checks that levels
//! are not skipped, which is 14-003, and 13-001 is graphics not tagged as a `<Figure>`
//! where this checks the missing alternative text, which is 13-004. The third — a
//! structure element naming a page that is not there — matched **no** failure condition,
//! because a broken reference is not a way to fail PDF/UA-1, and it went.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_doc::{AuditFinding, AuditReport, AuditScope, MatterhornAuditor, Outcome};

fn opened(bytes: Vec<u8>) -> PdfDocument {
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// A tagged document that breaks every failure condition this auditor looks at.
///
/// **One element per check, so that a failure names which one is not found.** The figure
/// has no alternative text (13-004), and the headings go H1 then H3 (14-003).
fn broken_tagging() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 7 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /StructElem /S /H1 /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructElem /S /H3 /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructElem /S /Figure /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructTreeRoot /K [4 0 R 5 0 R 6 0 R] >>",
    ])
    .into_iter()
    .collect()
}

/// **The report says how much of the protocol it looked at.**
///
/// A couple of 136 failure conditions. A reader told "no findings" and not told that
/// would have been told the document conforms.
#[test]
fn the_report_says_how_much_of_the_protocol_it_checked() {
    let doc = opened(broken_tagging());
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        report.scope.in_protocol, 136,
        "the protocol's size is not what this was written about"
    );
    assert_eq!(
        report.scope.checked.len(),
        2,
        "the scope does not name the two failure conditions this auditor looks at"
    );
    assert!(
        report.scope.checked.len() < report.scope.in_protocol,
        "the scope claims the whole protocol"
    );
}

/// **Every checkpoint the audit reports is one the scope names.**
///
/// Adding a check and forgetting to say so is how a scope stops being true, and it is
/// invisible from the outside: the report grows and the promise does not.
#[test]
fn the_scope_names_every_checkpoint_reported() {
    let doc = opened(broken_tagging());
    let report = doc.audit_ua2_report().expect("it audits");
    assert!(!report.findings.is_empty(), "the fixture is not broken, so this checks nothing");

    for finding in &report.findings {
        assert!(
            report.scope.checked.contains(&finding.checkpoint),
            "the audit reported {} and the scope does not name it: {:?}",
            finding.checkpoint,
            report.scope.checked
        );
    }
}

/// And every checkpoint the scope names is one the audit can report.
///
/// The other direction: a scope naming a checkpoint nothing looks at is a promise of
/// work that is not done, which is the defect this item exists to remove rather than
/// to move.
#[test]
fn every_checkpoint_the_scope_names_is_one_the_audit_reports() {
    let doc = opened(broken_tagging());
    let report = doc.audit_ua2_report().expect("it audits");
    let reported: Vec<&str> = report.findings.iter().map(|f| f.checkpoint.as_str()).collect();

    for named in &MatterhornAuditor::CHECKED {
        assert!(
            reported.contains(named),
            "the scope names {named} and the fixture that breaks every check did not \
             report it: {reported:?}"
        );
    }
}

/// **A document with no structure tree says which check it failed, not "no findings".**
///
/// It is not a tagged PDF, which is the one thing this can say without looking at a
/// checkpoint — and the scope still says what it would have looked at rather than
/// claiming it did.
#[test]
fn an_untagged_document_is_told_apart_from_a_clean_one() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(!report.found_nothing(), "an untagged document came back with nothing said");
    assert_eq!(report.scope.in_protocol, 136, "the scope forgot the protocol");
}

/// A tagged document that breaks neither condition this auditor looks at.
///
/// The headings go H1 then H2 (14-003 wants no level skipped) and the figure carries its
/// alternative text (13-004).
fn sound_tagging() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 7 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /StructElem /S /H1 /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructElem /S /H2 /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructElem /S /Figure /P 7 0 R /Pg 3 0 R /Alt (a duck) >>",
        "<< /Type /StructTreeRoot /K [4 0 R 5 0 R 6 0 R] >>",
    ])
    .into_iter()
    .collect()
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
    let doc = opened(sound_tagging());
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(
        report.found_nothing(),
        "a document breaking neither checked condition was reported as breaking one: {:?}",
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

/// **Every number this auditor reports says, in the protocol, what this auditor checks.**
///
/// All three were wrong until 2026-09-21 and nothing noticed, because nothing compared
/// them to the document. A wrong number is not a gap in the audit — it is a finding filed
/// against a different defect, and a reader or a tool that looks the number up is told
/// something untrue.
///
/// The protocol is `docs/specs/Matterhorn-Protocol-1-1.pdf`, which is untracked
/// (`docs/specs/README.md` says where to get it at no cost). Without it this cannot
/// check anything, and it says so rather than passing.
#[test]
fn every_number_reported_means_in_the_protocol_what_it_is_used_for() {
    let path = "../../docs/specs/Matterhorn-Protocol-1-1.pdf";
    let Ok(bytes) = std::fs::read(path) else {
        panic!("{path} is not in this working copy; docs/specs/README.md says where it is");
    };
    let protocol = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the protocol opens");
    let pages = protocol.page_count().expect("it counts");
    let text: String = (0..pages)
        .map(|page| protocol.extract_text(page).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ");

    // What each number has to be followed by in the protocol's own words. Short enough to
    // survive the line breaks a two-column table puts in, long enough to be that
    // condition and no other.
    let says = [
        ("13-004", "alternative or replacement text missing"),
        ("14-003", "Numbered heading levels in descending"),
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
        let at = text
            .find(number)
            .unwrap_or_else(|| panic!("the protocol does not contain {number} at all"));
        let after: String = text[at..].chars().take(160).collect();
        assert!(after.contains(words), "the protocol says {number} is something else: {after:?}");
    }

    // The protocol's own statement of its size, which `IN_PROTOCOL` claims.
    assert!(
        text.contains("136 failure conditions"),
        "the protocol no longer states the count this auditor reports against"
    );
    assert_eq!(MatterhornAuditor::IN_PROTOCOL, 136, "the stated total drifted from the protocol");

    // And what it is a protocol for, which is the reason none of this measures UA-2.
    assert!(
        text.contains("specified in PDF/UA-1"),
        "the protocol no longer says which standard it is about"
    );
}

/// **A condition checked and not broken is a result the report carries.**
///
/// A reader is owed what was examined, not only what was wrong. This document breaks
/// 14-003 and not 13-004, so the report says both — one broken, one sound — and the two
/// together are the whole of what was looked at.
#[test]
fn a_condition_that_came_out_sound_is_in_the_report() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            // H1 then H3: 14-003 is broken.
            "<< /Type /StructElem /S /H1 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructElem /S /H3 /P 6 0 R /Pg 3 0 R >>",
            "<< /Type /StructTreeRoot /K [4 0 R 5 0 R] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    let outcome = |condition: &str| {
        report.findings.iter().find(|f| f.checkpoint == condition).map_or_else(
            || panic!("{condition} is in neither column: {:?}", report.findings),
            |f| f.outcome,
        )
    };
    assert_eq!(outcome("14-003"), Outcome::Broken, "the skipped heading level was not reported");
    assert_eq!(
        outcome("13-004"),
        Outcome::Sound,
        "a condition checked and not broken is missing from the report"
    );
}

/// **Sound and not-looked-at are different answers, and the report keeps them apart.**
///
/// Every condition the report calls sound has to be one the scope says was checked. A
/// report that called an unexamined condition sound would say a document conforms on the
/// strength of work nobody did — which is the shape this whole item exists to remove.
#[test]
fn nothing_is_called_sound_that_was_not_checked() {
    let doc = opened(broken_tagging());
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
    // version of this test gathered the checkpoints into a `BTreeSet` and looked one up
    // with `find`, and a mutation that reported every condition sound *including the
    // broken ones* passed all eight tests: the set collapsed the duplicate and `find`
    // returned the first of the pair.
    let mut sound_and_broken = Vec::new();
    for condition in &report.scope.checked {
        let outcomes: Vec<Outcome> = report
            .findings
            .iter()
            .filter(|f| &f.checkpoint == condition)
            .map(|f| f.outcome)
            .collect();
        assert_eq!(
            outcomes.len(),
            1,
            "{condition} is reported {} times, as {outcomes:?}",
            outcomes.len()
        );
        if outcomes.contains(&Outcome::Sound) && outcomes.contains(&Outcome::Broken) {
            sound_and_broken.push(condition.clone());
        }
    }
    assert!(sound_and_broken.is_empty(), "reported both sound and broken: {sound_and_broken:?}");
}

/// A document with no structure tree has nothing examined, so nothing is sound.
///
/// It is not a tagged PDF, and reporting the conditions as sound because the walk found
/// no elements to break them would be the emptiest kind of pass.
#[test]
fn an_untagged_document_has_nothing_to_call_sound() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        ])
        .into_iter()
        .collect(),
    );
    let report = doc.audit_ua2_report().expect("it audits");

    assert!(
        report.findings.iter().all(|f| f.outcome != Outcome::Sound),
        "an untagged document had a condition called sound: {:?}",
        report.findings
    );
}

/// **A condition waiting for a reader is not "nothing found".**
///
/// Dropping `ForAReader` from [`AuditReport::found_nothing`] failed no test, because no
/// condition emits one yet: `CHECKED` is two machine-decided conditions, and the ones the
/// protocol leaves to a person are W-21d. The variant is in the method's contract and in
/// the panel's second section, so it is tested here against a report built by hand rather
/// than left until a producer exists.
#[test]
fn a_condition_left_for_a_reader_is_not_nothing_found() {
    let waiting = AuditReport {
        findings: vec![
            AuditFinding {
                checkpoint: "13-004".into(),
                severity: "Pass".into(),
                outcome: Outcome::Sound,
                message: "13-004 was checked and this document does not break it".into(),
                handle_id: None,
            },
            AuditFinding {
                checkpoint: "14-003".into(),
                severity: "Warning".into(),
                outcome: Outcome::ForAReader,
                message: "the heading levels are yours to judge".into(),
                handle_id: None,
            },
        ],
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
