use anyhow::Result;
use fepdf::PdfDocument;

use super::{render_decisions_markdown, render_decisions_text};

pub fn render_general_text(summary: &fepdf::DocumentSummary) {
    println!("\n--- [ DOCUMENT SUMMARY ] ---");
    println!("Version:    {}", summary.version);
    println!("Pages:      {}", summary.page_count);
    if let Some(v) = &summary.metadata.title {
        println!("Title:      {v}");
    }
    if let Some(v) = &summary.metadata.author {
        println!("Author:     {v}");
    }
}

pub fn render_general_info(summary: &fepdf::DocumentSummary) {
    println!("\n## General Information");
    println!("\n| Property | Value |");
    println!("| :--- | :--- |");
    println!("| Version | {} |", summary.version);
    println!("| Total Pages | {} |", summary.page_count);
    if let Some(v) = &summary.metadata.title {
        println!("| Title | {v} |");
    }
    if let Some(v) = &summary.metadata.author {
        println!("| Author | {v} |");
    }
    if let Some(v) = &summary.metadata.subject {
        println!("| Subject | {v} |");
    }
    if let Some(v) = &summary.metadata.keywords {
        println!("| Keywords | {v} |");
    }
    if let Some(v) = &summary.metadata.creator {
        println!("| Creator | {v} |");
    }
    if let Some(v) = &summary.metadata.producer {
        println!("| Producer | {v} |");
    }
}

pub fn render_font_text(summary: &fepdf::DocumentSummary) {
    println!("\n--- [ FONT AUDIT ] ---");
    let embedded_count = summary.fonts.iter().filter(|f| f.is_embedded).count();
    println!("Total Fonts: {} (Embedded: {})", summary.fonts.len(), embedded_count);

    if summary.fonts.is_empty() {
        println!("No fonts detected.");
    } else {
        println!(
            "{:<30} | {:<10} | {:<4} | {:<4} | {:<4} | {:<4} | {:<10}",
            "Font Name", "Type", "Emb", "T3", "Sub", "ToU", "Encoding"
        );
        println!(
            "{:-<30}-+-{:-<10}-+-{:-<4}-+-{:-<4}-+-{:-<4}-+-{:-<4}-+-{:-<10}",
            "", "", "", "", "", "", ""
        );
        for f in &summary.fonts {
            println!(
                "{:<30} | {:<10} | {:<4} | {:<4} | {:<4} | {:<4} | {:<10}",
                f.name,
                f.font_type,
                if f.is_embedded { "✅" } else { "❌" },
                if f.is_type3 { "T3" } else { "−" },
                if f.is_subset { "✅" } else { "−" },
                if f.has_to_unicode { "✅" } else { "❌" },
                f.encoding
            );
        }
    }
}

pub fn render_font_audit(summary: &fepdf::DocumentSummary) {
    let embedded_count = summary.fonts.iter().filter(|f| f.is_embedded).count();
    let total_fonts = summary.fonts.len();

    println!("\n## Font Audit (Embedded: {embedded_count}/{total_fonts})");
    if total_fonts > 0 {
        println!("\n| Font Name | Type | Embedded | Subset | Encoding |");
        println!("| :--- | :--- | :--- | :--- | :--- |");
        for f in &summary.fonts {
            println!(
                "| {} | {} | {} | {} | {} | {} |",
                f.name,
                f.font_type,
                if f.is_embedded { "✅" } else { "❌" },
                if f.is_type3 { "T3" } else { "−" },
                if f.is_subset { "✅" } else { "−" },
                f.encoding
            );
        }
    }
}

pub fn render_compliance_markdown(summary: &fepdf::DocumentSummary) -> Result<()> {
    let errors = summary
        .compliance
        .issues
        .iter()
        .filter(|i| {
            matches!(i.severity, fepdf::IssueSeverity::Error | fepdf::IssueSeverity::Critical)
        })
        .count();
    let warnings = summary
        .compliance
        .issues
        .iter()
        .filter(|i| matches!(i.severity, fepdf::IssueSeverity::Warning))
        .count();
    println!("\n## Compliance Audit (UA-2)");
    println!("**Summary**: {errors} Errors, {warnings} Warnings");

    if !summary.compliance.issues.is_empty() {
        println!("\n| Severity | Standard | Message |");
        println!("| :--- | :--- | :--- |");
        for issue in &summary.compliance.issues {
            let icon = match issue.severity {
                fepdf::IssueSeverity::Critical => "🚨",
                fepdf::IssueSeverity::Error => "❌",
                fepdf::IssueSeverity::Warning => "⚠️",
                fepdf::IssueSeverity::Info => "ℹ️",
            };
            println!("| {} {:?} | {} | {} |", icon, issue.severity, issue.standard, issue.message);
        }
    } else {
        println!("\n✅ No compliance issues found.");
    }

    if !summary.compliance.iso_clauses.is_empty() {
        println!("\n## Validated ISO 32000-2 Clauses");
        println!("The following structural components were validated against the specification:");
        for clause in &summary.compliance.iso_clauses {
            println!("- **Clause {clause}**");
        }
    }
    Ok(())
}

pub fn render_audit_text(summary: &fepdf::DocumentSummary) {
    println!("\n--- [ COMPLIANCE AUDIT ] ---");
    if summary.compliance.issues.is_empty() {
        println!("SUCCESS: No major issues found.");
    } else {
        for issue in &summary.compliance.issues {
            println!("[{:?}] {:<10} | {}", issue.severity, issue.standard, issue.message);
        }
    }
    if !summary.compliance.iso_clauses.is_empty() {
        println!("\n--- [ ISO 32000-2 COMPLIANCE ] ---");
        println!("Validated Clauses: {}", summary.compliance.iso_clauses.join(", "));
    }
}

pub fn render_summary_text(
    doc: &PdfDocument,
    summary: &fepdf::DocumentSummary,
    audit: bool,
    structure: bool,
) -> Result<()> {
    render_general_text(summary);
    render_font_text(summary);

    // Every text report carries the log, not only the audit: a caller must be able to
    // tell "this loaded" from "this was conforming" whichever command they reached for.
    render_decisions_text(&summary.decisions);

    if audit {
        render_audit_text(summary);
    }

    if structure {
        let tree = doc.print_structure().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        println!("\n--- [ DOCUMENT STRUCTURE ] ---\n{tree}");
    }
    Ok(())
}

pub fn render_summary_markdown(
    summary: &fepdf::DocumentSummary,
    input: &std::path::Path,
    audit: bool,
) -> Result<()> {
    println!("# Document Summary: {}", input.file_name().unwrap_or_default().display());
    render_general_info(summary);
    render_font_audit(summary);
    render_decisions_markdown(&summary.decisions);

    if audit {
        render_compliance_markdown(summary)?;
    }
    Ok(())
}
