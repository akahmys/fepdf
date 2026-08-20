use anyhow::{Context, Result};
use fepdf::PdfDocument;
use std::path::{Path, PathBuf};

use crate::args::IngestArgs;
use crate::formatters::catalog::render_catalog_markdown;
use crate::formatters::catalog::render_catalog_text;
use crate::formatters::encryption::{render_encryption_markdown, render_encryption_text};
use crate::formatters::interactive::{render_interactive_markdown, render_interactive_text};
use crate::formatters::structure::{render_structure_markdown, render_structure_text};
use crate::formatters::summary::{render_summary_markdown, render_summary_text};
use crate::util::parse_page_range;

pub fn handle_info(input: PathBuf, format: String, ingest: IngestArgs) -> Result<()> {
    if format == "text" {
        println!("fepdf info: Analyzing {}", input.display());
    }
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let summary = doc.get_summary().map_err(|e| anyhow::anyhow!("{e:?}"))?;

    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&summary)?),
        "markdown" => render_summary_markdown(&summary, &input, false)?,
        _ => render_summary_text(&doc, &summary, false, false)?,
    }
    Ok(())
}

pub fn handle_audit(input: PathBuf, format: String, ingest: IngestArgs) -> Result<()> {
    if format == "text" {
        println!("fepdf audit: Performing compliance check on {}", input.display());
    }
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let summary = doc.get_summary().map_err(|e| anyhow::anyhow!("{e:?}"))?;

    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&summary)?),
        "markdown" => render_summary_markdown(&summary, &input, true)?,
        _ => render_summary_text(&doc, &summary, true, false)?,
    }
    Ok(())
}

pub fn handle_text(input: PathBuf, pages: Option<String>, ingest: IngestArgs) -> Result<()> {
    println!("fepdf text: Extracting text from {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_indices = parse_page_range(&range_str, page_count)?;

    // On stderr, not stdout: this command's output is meant to be piped, and a
    // decisions block in the middle of extracted text would corrupt it. Silence here
    // would be worse — text pulled from a file the engine had to repair is exactly the
    // case a caller needs told about.
    let taken_reading = doc.decisions();
    if !taken_reading.is_empty() {
        eprintln!("--- [ DECISIONS TAKEN READING (5.3) ] ---");
        for d in &taken_reading {
            eprintln!("  {d}");
        }
    }

    // One page that will not extract does not make the other 845 unreadable. This
    // returned on the first failure, so `samples/fy05.pdf` — whose page 128 fails with
    // "Expected number" — yielded 127 pages of its 846 and exited non-zero, having
    // printed no hint that the rest existed. The failure goes to stderr with the others,
    // and the exit status reports that something was lost.
    let mut failed = Vec::new();
    for idx in target_indices {
        match doc.extract_text(idx) {
            Ok(text) => println!("\n--- [ PAGE {} ] ---\n{}", idx + 1, text),
            Err(e) => {
                eprintln!("  page {}: no text extracted — {e:?}", idx + 1);
                failed.push(idx + 1);
            }
        }
    }
    // Interpreting a page is reading too, and what it decides arrives after the text
    // rather than before it — an image whose filter this engine cannot decode is
    // skipped mid-stream (ADR-0018). Reported apart from the block above because the
    // two answer different questions: one is what the *file* needed to be read at all,
    // the other is what this run of the interpreter gave up on.
    let taken_interpreting = doc.decisions();
    if taken_interpreting.len() > taken_reading.len() {
        eprintln!("--- [ DECISIONS TAKEN INTERPRETING (5.3) ] ---");
        for d in &taken_interpreting[taken_reading.len()..] {
            eprintln!("  {d}");
        }
    }

    if !failed.is_empty() {
        eprintln!(
            "  {} of {page_count} pages yielded no text: {}",
            failed.len(),
            failed.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
        );
        anyhow::bail!("{} pages could not be extracted", failed.len());
    }
    Ok(())
}

/// Reports the document catalogue (7.7.2) entry by entry, and the gaps.
///
/// `--all` adds every Table 29 key the file does not carry, which turns the report
/// from "what is in this document" into "what the engine understands at all".
pub fn handle_catalog(input: &Path, format: &str, all: bool) -> Result<()> {
    let data = std::fs::read(input).with_context(|| "Failed to read input")?;
    let report = fepdf::CatalogReport::survey(&data).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&report)?),
        "markdown" => render_catalog_markdown(&report, input),
        _ => render_catalog_text(&report, input, all),
    }
    Ok(())
}

/// Reports what protects the document (7.6), and how far the engine conforms.
pub fn handle_encryption(
    input: &Path,
    format: &str,
    password: &str,
    certificate: Option<PathBuf>,
    private_key: Option<PathBuf>,
) -> Result<()> {
    let data = std::fs::read(input).with_context(|| "Failed to read input")?;
    let identity = match (certificate, private_key) {
        (Some(certificate), Some(key)) => Some(
            fepdf::RecipientIdentity::from_der(
                &std::fs::read(&certificate).with_context(|| "Failed to read the certificate")?,
                &std::fs::read(&key).with_context(|| "Failed to read the private key")?,
            )
            .map_err(|e| anyhow::anyhow!("{e:?}"))?,
        ),
        _ => None,
    };
    let report = fepdf::EncryptionReport::survey(
        &data,
        fepdf::Credentials { password, recipient: identity.as_ref() },
    )
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&report)?),
        "markdown" => render_encryption_markdown(&report, input),
        _ => render_encryption_text(&report, input),
    }
    Ok(())
}

/// Reports what a reader could interact with (clause 12).
pub fn handle_interactive(input: &Path, format: &str) -> Result<()> {
    let data = std::fs::read(input).with_context(|| "Failed to read input")?;
    let report = fepdf::InteractiveReport::survey(&data).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&report)?),
        "markdown" => render_interactive_markdown(&report, input),
        _ => render_interactive_text(&report, input),
    }
    Ok(())
}

/// Reports the file's layout (ISO 32000-2, 7.5) and every decision taken reading it.
///
/// Takes no `IngestArgs`: this describes the file as written, before normalisation,
/// so the ingestion options have nothing to act on. It also does not build a
/// `Document`, which is why it stays fast on a file with 341,321 objects.
pub fn handle_structure(input: &Path, format: &str) -> Result<()> {
    let data = std::fs::read(input).with_context(|| "Failed to read input")?;
    let structure = fepdf::FileStructure::survey(&data).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&structure)?),
        "markdown" => render_structure_markdown(&structure, input),
        _ => render_structure_text(&structure, input),
    }
    Ok(())
}

/// A whole-number percentage, or `None` when nothing was presented — which is not the
/// same as none of it being read, and must not print as 0%.
fn percent(part: usize, whole: usize) -> Option<usize> {
    (whole > 0).then(|| part * 100 / whole)
}

/// Reports the coverage index over a set of files (`fepdf-model::coverage`).
///
/// Takes many inputs where every other `inspect` subcommand takes one, because the
/// figure is about a *corpus*: the denominator is the constructs those files present,
/// and one file's denominator is not a measurement of anything.
///
/// A file that will not survey is counted and named rather than dropped. A coverage
/// figure that quietly skipped what it could not read would improve every time the
/// engine got worse.
pub fn handle_coverage(inputs: &[PathBuf], format: &str, unread: bool) -> Result<()> {
    if inputs.is_empty() {
        anyhow::bail!("give at least one file: the figure is over a corpus");
    }
    let mut total = fepdf::Coverage::default();
    let mut measured = 0usize;
    let mut refused: Vec<String> = Vec::new();
    for input in inputs {
        match std::fs::read(input)
            .map_err(|e| e.to_string())
            .and_then(|data| fepdf::Coverage::of(&data).map_err(|e| format!("{e:?}")))
        {
            Ok(c) => {
                total.merge(&c);
                measured += 1;
            }
            Err(why) => refused.push(format!("{}: {why}", input.display())),
        }
    }

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&total.axes())?);
        return Ok(());
    }

    render_coverage(&total, measured, inputs.len(), unread);
    for why in &refused {
        eprintln!("  not measured — {why}");
    }
    Ok(())
}

/// The axes, the total, and what the total is not.
fn render_coverage(total: &fepdf::Coverage, measured: usize, of: usize, unread: bool) {
    println!("fepdf coverage: {measured} of {of} files");
    println!("\n  {:<22} {:<8} {:>9} {:>6}  of them", "axis", "clause", "presented", "read");
    for axis in total.axes() {
        println!(
            "  {:<22} {:<8} {:>9} {:>6}  {}",
            axis.axis,
            axis.clause,
            axis.presented,
            axis.read,
            percent(axis.read, axis.presented)
                .map_or_else(|| "—  nothing presented".to_string(), |p| format!("{p}%"))
        );
    }
    let (read, presented) = total.total();
    if let Some(p) = percent(read, presented) {
        println!("\n  {read} of {presented} constructs read — {p}%");
    }
    println!(
        "  A proxy for understanding, not a measure of it: it says nothing about \n  \
         whether what was read was read correctly (ADR-0019)."
    );

    if !unread {
        return;
    }
    for (axis, _) in fepdf::COVERAGE_AXES {
        let missing = total.unread(axis);
        if !missing.is_empty() {
            println!("\n  no reader — {axis} ({})", missing.len());
            for construct in &missing {
                println!("      {construct}");
            }
        }
    }
}

pub fn handle_tree(input: PathBuf, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug structure: Hierarchical tree for {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let tree = doc.print_structure().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("\n--- [ DOCUMENT STRUCTURE ] ---\n{tree}");
    Ok(())
}
