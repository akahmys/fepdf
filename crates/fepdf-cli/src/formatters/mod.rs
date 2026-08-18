pub mod catalog;
pub mod encryption;
pub mod interactive;
pub mod structure;
pub mod summary;

use anyhow::Result;
use fepdf::PdfDocument;

/// Prints the decisions taken while reading, in the same shape for every command.
///
/// One function rather than one per report, so a caller sees the same block from
/// `info`, `audit`, `catalog`, `interactive` and `structure`. Before this, only the
/// audit showed them, and it showed them laundered: every decision reached the summary
/// as a compliance issue at `Warning`, whatever severity the engine had assigned.
pub fn render_decisions_text(decisions: &[fepdf::Decision]) {
    println!("\n--- [ DECISIONS TAKEN READING (5.3) ] ---");
    if decisions.is_empty() {
        println!("  none — the file was read without departing from the standard");
        return;
    }
    let count = |s: fepdf::Severity| decisions.iter().filter(|d| d.severity == s).count();
    println!(
        "  {} ambiguities, {} repairs, {} violations",
        count(fepdf::Severity::Ambiguity),
        count(fepdf::Severity::Repaired),
        count(fepdf::Severity::Violation)
    );
    for d in decisions {
        println!("  {d}");
    }
}

pub fn render_decisions_markdown(decisions: &[fepdf::Decision]) {
    println!("\n## Decisions taken reading\n");
    if decisions.is_empty() {
        println!("None — the file was read without departing from the standard.");
        return;
    }
    println!("| Severity | Clause | Found | Action |");
    println!("| :--- | :--- | :--- | :--- |");
    for d in decisions {
        println!("| {:?} | {} | {} | {} |", d.severity, d.clause, d.found, d.action);
    }
}

/// Writes the document and reports what the write cost.
///
/// `/P` is a declaration and not a lock — 7.6.4.1 puts obeying it at `should` — so
/// nothing is refused. But decryption drops `/Encrypt`, the permissions go with it, and
/// the engine used to rewrite a document saying "do not modify" into one saying nothing
/// at all, in silence.
///
/// stderr, so a redirected `>` output is unchanged.
pub fn save_reporting_permissions(
    doc: &PdfDocument,
    output: &std::path::Path,
    save_options: &fepdf::SaveOptions,
) -> Result<()> {
    let decisions =
        doc.save_with_options(output, "2.0", save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    report_write_decisions(&decisions);
    Ok(())
}

/// Prints what a write cost, if anything.
pub fn report_write_decisions(decisions: &[fepdf::Decision]) {
    for decision in decisions {
        eprintln!("{decision}");
    }
}
