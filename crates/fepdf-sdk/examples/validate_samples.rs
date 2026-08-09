//! Opens every PDF under `samples/` and reports the ones that fail to ingest.
//!
//! A regression sweep over the sample corpus: each file must load through the
//! Sublimation pipeline and expose a resolvable catalogue and page count. Exits
//! non-zero when any file fails.
//!
//! `samples/` is gitignored, so this runs against whatever corpus the machine
//! holds rather than a fixed set — it is a local sweep, not a CI gate.
//!
//! Run from the workspace root: `cargo run -p fepdf-sdk --example validate_samples`

use fepdf_sdk::PdfDocument;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples_dir = Path::new("samples");
    if !samples_dir.exists() {
        println!("No samples/ directory here; run from the workspace root.");
        return Ok(());
    }

    let mut files = Vec::new();
    collect_pdfs(samples_dir, &mut files)?;
    files.sort();
    println!("Validating {} PDF files.\n", files.len());

    let mut failures = 0;
    for file in &files {
        match validate(file) {
            Ok(pages) => println!("  OK      {} ({pages} pages)", file.display()),
            Err(e) => {
                println!("  FAILED  {}: {e}", file.display());
                failures += 1;
            }
        }
    }

    if failures > 0 {
        println!("\n{failures} of {} files failed.", files.len());
        std::process::exit(1);
    }
    println!("\nAll {} files validated.", files.len());
    Ok(())
}

fn collect_pdfs(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_pdfs(&path, files)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("pdf") {
            files.push(path);
        }
    }
    Ok(())
}

/// Ingests one file and returns its page count.
fn validate(path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let doc = PdfDocument::open(data.into())?;

    // A document that ingests but has no catalogue would still fail every later
    // operation, so treat that as a validation failure rather than a success.
    if doc.inner().catalog_handle().is_none() {
        return Err("no /Root catalogue could be resolved".into());
    }

    Ok(doc.page_count()?)
}
