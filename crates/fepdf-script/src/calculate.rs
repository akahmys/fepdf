//! Running a form's calculation order (ISO 32000-2, 12.6.3).
//!
//! `/CO` supplies the order — the engine already read it, at the one site that records
//! the `Violation` this module exists to stop producing. What it could not do was run
//! the scripts.
//!
//! **The recursion guard is not "do not calculate a field twice".** 12.6.3 permits
//! A → B → A: the effects of a field action are limited only by the action itself, and
//! two fields may legitimately depend on each other. So the guard is a bounded iteration
//! count, and reaching it is a fact about the document that gets recorded rather than a
//! silent stop.

use crate::host::{DocumentHandle, ScriptEnvironment, ScriptError, ScriptHost};
use fepdf::{FormFieldSpec, FormValue, Operation, Says, Trigger};

/// How many passes over the calculation order are made before giving up.
///
/// A pass runs every field once, so this bounds a cycle rather than forbidding one. Four
/// is past anything a form converges in and short of a number that would hide a runaway.
const MAX_PASSES: usize = 4;

/// What running the calculation order did.
#[derive(Debug, Default, Clone)]
pub struct CalculationReport {
    /// Field names whose calculation produced a value, in the order they were written.
    pub calculated: Vec<String>,
    /// Passes actually made. More than one means a value changed on the pass before.
    pub passes: usize,
    /// Whether the bound was reached with values still changing.
    pub stopped_early: bool,
}

/// Runs every field in `/CO`, in order, until nothing changes.
///
/// # Errors
/// The first script that will not complete stops the run: a form whose later fields are
/// computed from an earlier one that failed would otherwise be filled with values derived
/// from a value that was never produced.
pub fn run_calculations(
    handle: &DocumentHandle,
    environment: &ScriptEnvironment,
) -> Result<CalculationReport, ScriptError> {
    let order = handle.with(|doc| fepdf::calculation_order(doc.inner()));
    let mut report = CalculationReport::default();
    if order.is_empty() {
        return Ok(report);
    }
    let scripts = handle.with(collect_scripts);

    for pass in 1..=MAX_PASSES {
        report.passes = pass;
        let mut changed = false;
        for field in &order {
            let Some(source) = scripts.get(field) else { continue };
            if run_one(handle, environment, field, source)? {
                changed = true;
                if !report.calculated.contains(field) {
                    report.calculated.push(field.clone());
                }
            }
        }
        if !changed {
            return Ok(report);
        }
    }
    // Still changing at the bound. 12.6.3 permits the cycle that causes this, so the
    // document is not wrong; the run is incomplete, and a caller has to be told which.
    report.stopped_early = true;
    handle.with(|doc| {
        doc.inner().record(fepdf::Decision::violation(
            "12.6.3",
            format!("the calculation order still changed values after {MAX_PASSES} passes"),
            "stopped and kept the values from the last pass; a field may be stale",
        ));
    });
    Ok(report)
}

/// Every field's `/AA /C` script, by field name.
fn collect_scripts(doc: &fepdf::PdfDocument) -> std::collections::BTreeMap<String, String> {
    let mut scripts = std::collections::BTreeMap::new();
    let Ok(report) = fepdf::ActionReport::of(doc.inner()) else {
        return scripts;
    };
    for action in &report.actions {
        // The field walk and the annotation walk reach the same dictionary when a field
        // is its own widget, which is the common case; only the one that names the field
        // is usable here.
        let Trigger::FieldEvent { field: Some(name), event } = &action.trigger else {
            continue;
        };
        if event != "C" {
            continue;
        }
        if let Some(Says::Script(source)) = &action.says {
            scripts.insert(name.clone(), source.clone());
        }
    }
    scripts
}

/// Runs one field's calculation and writes the result back. Reports whether it changed.
fn run_one(
    handle: &DocumentHandle,
    environment: &ScriptEnvironment,
    field: &str,
    source: &str,
) -> Result<bool, ScriptError> {
    let before = handle.with(|doc| field_value(doc, field));
    let host = ScriptHost::new(handle.clone(), environment.clone());
    let Some(produced) = host.run_calculation(source, before.as_deref())? else {
        return Ok(false);
    };
    if before.as_deref() == Some(produced.as_str()) {
        return Ok(false);
    }
    // Writes go through the vocabulary. There is no third path (ADR-0025).
    handle
        .with_mut(|doc| {
            doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
                name: field.to_string(),
                value: FormValue::Text(produced.clone()),
            }))
        })
        .map_err(|e| ScriptError::DidNotComplete(e.to_string()))?;
    Ok(true)
}

/// A field's current `/V`, as text.
fn field_value(doc: &fepdf::PdfDocument, field: &str) -> Option<String> {
    fepdf::field_value(doc.inner(), field)
}
