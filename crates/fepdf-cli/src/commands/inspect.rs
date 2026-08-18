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
    let decisions = doc.decisions();
    if !decisions.is_empty() {
        eprintln!("--- [ DECISIONS TAKEN READING (5.3) ] ---");
        for d in decisions {
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
