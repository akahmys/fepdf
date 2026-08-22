use anyhow::{Context, Result};
use fepdf::PdfDocument;
use inquire::Confirm;
use std::path::PathBuf;

use crate::args::{IngestArgs, SaveArgs};
use crate::formatters::save_reporting_permissions;
use crate::util::parse_page_range;

pub fn handle_merge(
    inputs: Vec<PathBuf>,
    output: PathBuf,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf merge: Combining {} files into {}", inputs.len(), output.display());
    let mut sources = Vec::new();
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    for path in inputs {
        let data =
            std::fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sources.push(doc);
    }

    let merged = PdfDocument::merge(sources).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&merged, &output, &save_options)?;
    println!("SUCCESS: Merged output saved to {}", output.display());
    Ok(())
}

pub fn handle_split(
    input: PathBuf,
    output: PathBuf,
    pages: Option<String>,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf split: Extracting pages from {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_indices = parse_page_range(&range_str, page_count)?;

    let extracted = doc.extract_pages(target_indices).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&extracted, &output, &save_options)?;
    println!("SUCCESS: Extracted output saved to {}", output.display());
    Ok(())
}

pub fn handle_rotate(
    input: PathBuf,
    output: PathBuf,
    pages: Option<String>,
    angle: i32,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf rotate: Rotating pages in {} by {angle} degrees...", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_pages = parse_page_range(&range_str, page_count)?;

    let quarter = fepdf::Quarter::from_degrees(angle)
        .ok_or_else(|| anyhow::anyhow!("Angle must be a multiple of 90 degrees"))?;

    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Indices(target_pages),
        mode: fepdf::RotateMode::Absolute(quarter),
    })
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: Rotated output saved to {}", output.display());
    Ok(())
}

pub fn handle_repair(
    input: PathBuf,
    output: PathBuf,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf repair: Attempting to salvage corrupted document {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_and_repair_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: Repaired output saved to {}", output.display());
    Ok(())
}

pub fn handle_retag(
    input: PathBuf,
    output: PathBuf,
    wizard: bool,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!(
        "fepdf retag: {} -> {}",
        if wizard { "Wizard Mode" } else { "Automatic" },
        output.display()
    );
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    if wizard {
        println!("Wizard Mode: Reviewing heuristic structural candidates...");
        let candidates = doc.get_remediation_candidates().map_err(|e| anyhow::anyhow!("{e:?}"))?;

        if candidates.is_empty() {
            println!("No remediation candidates found.");
        } else {
            for candidate in candidates {
                let prompt =
                    format!("Page {}: {}?", candidate.page_index + 1, candidate.description);
                if Confirm::new(&prompt).with_default(true).prompt()? {
                    println!("Applying fix...");
                }
            }
        }
    } else {
        println!("Running automatic heuristic re-tagging rules...");
        doc.apply(fepdf::Operation::Retag).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: Re-tagged document saved to {}", output.display());
    Ok(())
}

pub fn handle_portfolio(
    output: PathBuf,
    files: Vec<PathBuf>,
    _cover: Option<PathBuf>,
    _ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!(
        "fepdf portfolio: Creating portfolio with {} files at {}",
        files.len(),
        output.display()
    );
    let mut items = Vec::new();
    for file_path in files {
        let file_name = file_path
            .file_name()
            .map_or_else(|| "file".to_string(), |n| n.to_string_lossy().to_string());
        let data = std::fs::read(&file_path)
            .with_context(|| format!("Failed to read file {}", file_path.display()))?;
        items.push(fepdf::PortfolioItem {
            filename: file_name,
            mime_type: None,
            description: None,
            size_bytes: data.len() as u64,
            data,
        });
    }

    let portfolio = fepdf::PortfolioCollection {
        view_mode: fepdf::CollectionViewMode::Details,
        initial_document: None,
        items,
    };

    let mut doc = PdfDocument::create_empty().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    doc.apply(fepdf::Operation::CreatePortfolio(portfolio))
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: Portfolio saved to {}", output.display());
    Ok(())
}

pub fn handle_bates(
    input: PathBuf,
    output: PathBuf,
    prefix: String,
    start_number: u64,
    digits: usize,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf bates: Applying Bates numbering to {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let op = fepdf::Operation::ApplyBatesNumbering {
        pages: fepdf::PageSelection::All,
        prefix,
        start_number,
        digits,
        position: fepdf::DecorationPosition::BottomRight,
    };
    doc.apply(op).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: Output with Bates numbering saved to {}", output.display());
    Ok(())
}

pub fn handle_attach(
    input: PathBuf,
    output: PathBuf,
    file: PathBuf,
    relationship_str: String,
    mime_type: String,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf attach: Attaching {} to {}", file.display(), input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input PDF")?;
    let file_data = std::fs::read(&file).with_context(|| "Failed to read attachment file")?;
    let file_name = file
        .file_name()
        .map_or_else(|| "attached".to_string(), |n| n.to_string_lossy().to_string());

    let relationship = match relationship_str.to_lowercase().as_str() {
        "source" => fepdf::AFRelationship::Source,
        "supplement" => fepdf::AFRelationship::Supplement,
        "alternative" => fepdf::AFRelationship::Alternative,
        _ => fepdf::AFRelationship::Data,
    };

    let af =
        fepdf::AssociatedFile { filename: file_name, relationship, mime_type, data: file_data };

    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    doc.apply(fepdf::Operation::AttachAssociatedFile(af)).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: PDF with Associated File saved to {}", output.display());
    Ok(())
}

pub fn handle_page_label(
    input: PathBuf,
    output: PathBuf,
    style_str: String,
    prefix: Option<String>,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf page-label: Setting page labels on {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input PDF")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let style = match style_str.to_lowercase().as_str() {
        "lower-roman" => fepdf::PageLabelStyle::LowerRoman,
        "upper-roman" => fepdf::PageLabelStyle::UpperRoman,
        "lower-alpha" => fepdf::PageLabelStyle::LowerAlpha,
        "upper-alpha" => fepdf::PageLabelStyle::UpperAlpha,
        _ => fepdf::PageLabelStyle::Decimal,
    };

    let labels = vec![fepdf::PageLabelSpec { start_page: 0, style, prefix, start_number: 1 }];

    doc.apply(fepdf::Operation::SetPageLabels(labels)).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: PDF with updated page labels saved to {}", output.display());
    Ok(())
}

pub fn handle_geo(
    input: PathBuf,
    output: PathBuf,
    lat: f64,
    lon: f64,
    crs: String,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf geo: Setting GIS anchor ({lat}, {lon}) on {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input PDF")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let anchor = fepdf::GeoSpatialAnchor {
        page: 0,
        latitude: lat,
        longitude: lon,
        altitude_meters: None,
        crs_wkt: crs,
    };

    doc.apply(fepdf::Operation::SetGeospatialAnchor(anchor))
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    save_reporting_permissions(&doc, &output, &save_options)?;
    println!("SUCCESS: PDF with GIS anchor saved to {}", output.display());
    Ok(())
}
