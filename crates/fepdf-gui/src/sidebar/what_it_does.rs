//! What a document is, and what it does when opened.
//!
//! **Three of `fepdf inspect`'s ten reports had no counterpart in this window** — measured
//! 2026-09-09: `encryption`, `actions` and `coverage`. The first two matter most to
//! someone who has just been sent a PDF and does not know where it came from: what runs
//! without them asking, and what the file is allowed to reach.
//!
//! The security handler was already carried into the app by `handle_open` and shown
//! nowhere.

use fepdf::{ActionReport, Coverage, SignatureReport};

/// What the panel is showing, filled by a `Survey` and cleared when a document opens.
#[derive(Default)]
pub struct Survey {
    pub actions: Option<Box<ActionReport>>,
    pub coverage: Option<Coverage>,
    /// The file's signatures and whether each verifies (12.8).
    pub signatures: Option<Box<SignatureReport>>,
    /// Whether a survey has been asked for, so the panel asks once rather than per frame.
    pub asked: bool,
}

/// Draws the panel. `request` is called at most once per document.
pub fn show(
    ui: &mut egui::Ui,
    survey: &mut Survey,
    security_method: Option<&String>,
    permissions: Option<i32>,
    tr: &dyn Fn(&str) -> String,
    request: &dyn Fn(),
) {
    if !survey.asked {
        survey.asked = true;
        request();
    }
    protection(ui, security_method, permissions, tr);
    ui.add_space(crate::app::theme::space::GROUP);
    let Some(report) = survey.actions.as_ref() else { return };
    if let Some(signed) = survey.signatures.as_ref() {
        signatures(ui, signed, tr);
        ui.add_space(crate::app::theme::space::GROUP);
    }
    automatic(ui, report, tr);
    ui.add_space(crate::app::theme::space::GROUP);
    capabilities(ui, report, tr);
    ui.add_space(crate::app::theme::space::GROUP);
    coverage(ui, survey.coverage.as_ref(), tr);
}

/// 7.6: what protects the document, which the app has had since it opened.
fn protection(
    ui: &mut egui::Ui,
    security_method: Option<&String>,
    permissions: Option<i32>,
    tr: &dyn Fn(&str) -> String,
) {
    ui.heading(tr("survey_protection"));
    match security_method {
        Some(method) if !method.is_empty() && method != "None" => {
            ui.label(method);
            if let Some(bits) = permissions {
                // The integer, not a reading of it: /P's meaning depends on the handler's
                // revision (Table 22), and naming the bits wrongly is worse than showing
                // what the file says.
                ui.label(
                    egui::RichText::new(format!("{} /P {bits}", tr("survey_permissions")))
                        .size(crate::app::theme::text::SMALL)
                        .weak(),
                );
            }
        }
        _ => {
            ui.label(
                egui::RichText::new(tr("survey_unprotected"))
                    .size(crate::app::theme::text::SMALL)
                    .weak(),
            );
        }
    }
}

/// 12.8: who signed it, whether each signature still verifies, and how much it covers.
///
/// **What was not asked is said beside what was.** A signature that verifies says the
/// bytes it covers have not changed and that it is bound to the certificate it carries; it
/// says nothing about whether that certificate should be believed, which needs a trust
/// store this engine does not have. A signature covering less than the whole file says
/// that something was appended after it, and the panel says so rather than a bare "valid".
fn signatures(ui: &mut egui::Ui, report: &SignatureReport, tr: &dyn Fn(&str) -> String) {
    ui.heading(tr("survey_signatures"));
    if report.signatures.is_empty() {
        ui.label(tr("survey_signatures_none"));
    }
    for check in &report.signatures {
        let field = check.field.clone().unwrap_or_else(|| tr("survey_signature_unnamed"));
        let verdict = match &check.refused {
            None => tr("survey_signature_verifies"),
            Some(why) => tr("survey_signature_refused").replacen("{}", why, 1),
        };
        ui.label(format!("{field}: {verdict}"));
        let small =
            |text: String| egui::RichText::new(text).size(crate::app::theme::text::SMALL).weak();
        if let Some(signer) = &check.signer {
            ui.label(small(tr("survey_signature_signer").replacen("{}", signer, 1)));
        }
        if let Some(at) = &check.signed_at {
            ui.label(small(tr("survey_signature_at").replacen("{}", at, 1)));
        }
        let (covered, total) = check.covered;
        let extent = if check.covers_whole_file {
            tr("survey_signature_whole").replacen("{}", &total.to_string(), 1)
        } else {
            tr("survey_signature_partial").replacen("{}", &covered.to_string(), 1).replacen(
                "{}",
                &total.to_string(),
                1,
            )
        };
        ui.label(small(extent));
    }
    if report.unsigned_fields > 0 {
        let unsigned = report.unsigned_fields.to_string();
        ui.label(tr("survey_signatures_unsigned").replacen("{}", &unsigned, 1));
    }
    if !report.signatures.is_empty() {
        ui.label(
            egui::RichText::new(tr("survey_signature_note"))
                .size(crate::app::theme::text::SMALL)
                .weak(),
        );
    }
}

/// What fires without the reader doing anything (12.6.4.17, 12.6.3).
fn automatic(ui: &mut egui::Ui, report: &ActionReport, tr: &dyn Fn(&str) -> String) {
    ui.heading(tr("survey_automatic"));
    let unprompted = report.without_interaction();
    if unprompted.is_empty() {
        ui.label(
            egui::RichText::new(tr("survey_automatic_none"))
                .size(crate::app::theme::text::SMALL)
                .weak(),
        );
        return;
    }
    for action in &unprompted {
        ui.label(
            egui::RichText::new(&action.kind).color(super::super::app::theme::colors::note::WARN),
        );
    }
}

/// What the document is able to do at all (12.6).
fn capabilities(ui: &mut egui::Ui, report: &ActionReport, tr: &dyn Fn(&str) -> String) {
    ui.heading(tr("survey_capabilities"));
    let found = report.capabilities();
    if found.is_empty() {
        ui.label(
            egui::RichText::new(tr("survey_capabilities_none"))
                .size(crate::app::theme::text::SMALL)
                .weak(),
        );
    }
    for (capability, count) in &found {
        ui.horizontal(|ui| {
            ui.label(capability.label());
            ui.label(egui::RichText::new(format!("{count}")).weak());
        });
    }
    if report.unreadable > 0 {
        ui.label(
            egui::RichText::new(format!("{} {}", report.unreadable, tr("survey_unreadable")))
                .size(crate::app::theme::text::SMALL)
                .weak(),
        );
    }
}

/// ADR-0019's proxy: the share of what the file presents whose contents are read.
fn coverage(ui: &mut egui::Ui, coverage: Option<&Coverage>, tr: &dyn Fn(&str) -> String) {
    ui.heading(tr("survey_coverage"));
    let Some(coverage) = coverage else { return };
    for axis in coverage.axes() {
        ui.horizontal(|ui| {
            ui.label(axis.axis);
            let share =
                axis.fraction().map_or_else(|| "—".to_string(), |f| format!("{:.0}%", f * 100.0));
            ui.label(
                egui::RichText::new(format!("{} / {} · {share}", axis.read, axis.presented)).weak(),
            );
        });
    }
    ui.label(
        egui::RichText::new(tr("survey_coverage_note")).size(crate::app::theme::text::SMALL).weak(),
    );
}
