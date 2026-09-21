//! What the audit looked at, beside what it found.
//!
//! **A clean report from a check that was never run is the worst answer this engine can
//! give**, and it was the answer it gave. `audit_ua2` returned findings and a caller had
//! no way to tell "nothing is wrong" from "almost nothing was examined": the Matterhorn
//! protocol has 136 checkpoints and this auditor reports three.
//!
//! `PdfStandard::UA2` writes a conformance claim into the catalogue, and that claim is a
//! statement about 136 things. Saying which three were looked at is not a nicety; it is
//! the difference between a report and a assurance nobody checked.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_doc::MatterhornAuditor;

fn opened(bytes: Vec<u8>) -> PdfDocument {
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// A tagged document that breaks every checkpoint this auditor looks at.
///
/// **One element per check, so that a failure names which one is not found.** The figure
/// has no `/Alt` (13-001), the headings go H1 then H3 (14-001), and the last element's
/// `/Pg` names object 99, which is not in the file (01-002).
fn broken_tagging() -> Vec<u8> {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 7 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /StructElem /S /H1 /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructElem /S /H3 /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructElem /S /Figure /P 7 0 R /Pg 3 0 R >>",
        "<< /Type /StructTreeRoot /K [4 0 R 5 0 R 6 0 R 8 0 R] >>",
        "<< /Type /StructElem /S /P /P 7 0 R /Pg 99 0 R >>",
    ])
    .into_iter()
    .collect()
}

/// **The report says how much of the protocol it looked at.**
///
/// Three of 136. A reader told "no findings" and not told that would have been told the
/// document conforms.
#[test]
fn the_report_says_how_much_of_the_protocol_it_checked() {
    let doc = opened(broken_tagging());
    let report = doc.audit_ua2_report().expect("it audits");

    assert_eq!(
        report.scope.in_protocol, 136,
        "the protocol's size is not what this was written about"
    );
    assert_eq!(report.scope.checked.len(), 3, "the scope does not name three checkpoints");
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
