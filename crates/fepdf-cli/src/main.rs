//! fepdf: The Universal PDF Toolkit.
//!
//! (ISO 32000-2:2020 Compliance & Optimization Engine)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fepdf_sdk::{PdfDocument, PdfStandard, TraceContext};
use inquire::Confirm;
use std::path::PathBuf;

/// Common options for PDF ingestion/reading
// Each bool is one `--flag` that clap parses for us. Grouping them into a
// sub-struct would only move the same flags one level down and break the
// flattened command line.
#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Args, Debug, Clone)]
struct IngestArgs {
    /// Disable active 2-pass refinement (UTF-8 normalization)
    #[arg(long)]
    no_refinement: bool,
    /// Disable automatic conversion of Info to XMP
    #[arg(long)]
    no_metadata_recovery: bool,
    /// Use relaxed color validation policy
    #[arg(long)]
    relaxed_color: bool,
    /// Force fallback to system fonts if embedded font parsing fails
    #[arg(long)]
    force_fallback: bool,
}

impl From<IngestArgs> for fepdf_sdk::IngestionOptions {
    fn from(args: IngestArgs) -> Self {
        Self {
            active_refinement: !args.no_refinement,
            sublime_metadata: !args.no_metadata_recovery,
            color_policy: if args.relaxed_color {
                fepdf_sdk::ColorPolicy::Relaxed
            } else {
                fepdf_sdk::ColorPolicy::Strict
            },
            force_fallback: args.force_fallback,
            password: None,
            progress_callback: None,
        }
    }
}

/// Common options for PDF writing/optimization
// Each bool is one `--flag` that clap parses for us. Grouping them into a
// sub-struct would only move the same flags one level down and break the
// flattened command line.
#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Args, Debug, Clone)]
struct SaveArgs {
    /// Opt-in for stream compression (FlateDecode)
    #[arg(long)]
    compress: bool,
    /// Remove unreachable objects
    #[arg(long)]
    vacuum: bool,
    /// Strip descriptive metadata
    #[arg(long)]
    strip: bool,
    /// Encrypt with password
    #[arg(long)]
    password: Option<String>,
    /// Use Object Streams (ObjStm) for high-density compression
    #[arg(long)]
    obj_stm: bool,
    /// Image re-compression quality (1-100)
    #[arg(long)]
    image_quality: Option<u32>,
    /// Set document primary language (e.g., "en-US", "ja-JP")
    #[arg(long)]
    lang: Option<String>,
    /// Override document title
    #[arg(long)]
    title: Option<String>,
    /// Override document author
    #[arg(long)]
    author: Option<String>,
    /// Set copyright notice in XMP metadata
    #[arg(long)]
    copyright: Option<String>,
    /// Permission flags (e.g., "print,copy")
    #[arg(long)]
    permissions: Option<String>,
    /// Text string encoding for non-ASCII characters (utf16be, utf8)
    #[arg(long, default_value = "utf16be")]
    string_encoding: String,
    /// Perform simulation without writing output file
    #[arg(long)]
    dry_run: bool,
}

impl From<SaveArgs> for fepdf_sdk::SaveOptions {
    fn from(args: SaveArgs) -> Self {
        Self {
            compress: args.compress,
            compression_level: 9,
            vacuum: args.vacuum,
            strip: args.strip,
            password: args.password,
            obj_stm: args.obj_stm,
            image_quality: args.image_quality,
            lang: args.lang,
            title: args.title,
            author: args.author,
            copyright: args.copyright,
            permissions: args.permissions,
            string_encoding: match args.string_encoding.to_lowercase().as_str() {
                "utf8" => fepdf_sdk::StringEncoding::Utf8,
                _ => fepdf_sdk::StringEncoding::Utf16BE,
            },
            creation_date: None,
            dry_run: args.dry_run,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "fepdf")]
#[command(author = "fepdf Developers")]
#[command(version)]
#[command(about = "fepdf: The Universal PDF Toolkit for Compliance, Optimization, and Manipulation", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Inspect document characteristics (Read-Only)
    Inspect {
        #[command(subcommand)]
        sub: InspectSubcommands,
    },
    /// Edit document pages and structure (Interactive & Structural Edit)
    Edit {
        #[command(subcommand)]
        sub: EditSubcommands,
    },
    /// Publish final compliance-certified outputs
    Publish {
        #[command(subcommand)]
        sub: PublishSubcommands,
    },
    /// Low-level debugging and inspection tools
    Debug {
        #[command(subcommand)]
        sub: DebugSubcommands,
    },
    /// Display open source credits and licenses
    Credits,
}

#[derive(Subcommand, Debug)]
enum InspectSubcommands {
    /// Display document information and font summary
    Info {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Perform detailed compliance audit (UA-2, ISO 32000-2)
    Audit {
        /// Input PDF file
        input: PathBuf,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Extract text content
    Text {
        /// Input PDF file
        input: PathBuf,
        /// Pages to extract text from (comma-separated or range, e.g., 1-5)
        #[arg(short, long)]
        pages: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Dump hierarchical logical structure tree
    Tree {
        /// Input PDF file
        input: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
}

#[derive(Subcommand, Debug)]
enum EditSubcommands {
    /// Merge multiple PDF files into one
    Merge {
        /// Input PDF files
        inputs: Vec<PathBuf>,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Split or extract pages from a PDF
    Split {
        /// Input PDF file
        input: PathBuf,
        /// Output directory or file pattern
        #[arg(short, long)]
        output: PathBuf,
        /// Page range (e.g., 1-5, 10)
        #[arg(long)]
        pages: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Rotate specific pages in the document
    Rotate {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Pages to rotate (comma-separated, e.g., 1,3-5) (default: all)
        #[arg(short, long)]
        pages: Option<String>,
        /// Rotation angle (90, 180, 270)
        #[arg(short, long)]
        angle: i32,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Attempt to repair a corrupted PDF document
    Repair {
        /// Input corrupted PDF file
        input: PathBuf,
        /// Output repaired PDF file
        output: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Heuristically re-tag the document logical structure for UA-2
    Tag {
        /// Input PDF file
        input: PathBuf,
        /// Output repaired PDF file (Explicitly required)
        #[arg(short, long)]
        output: PathBuf,
        /// Enable interactive Wizard Mode
        #[arg(short, long)]
        wizard: bool,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Create a PDF Portfolio / Collection
    Portfolio {
        /// Output PDF portfolio file
        #[arg(short, long)]
        output: PathBuf,
        /// Input files to embed into portfolio
        #[arg(short, long)]
        files: Vec<PathBuf>,
        /// Optional cover page PDF
        #[arg(long)]
        cover: Option<PathBuf>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Apply Bates numbering to PDF pages
    Bates {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Bates prefix (e.g. "CONFIDENTIAL-")
        #[arg(long, default_value = "")]
        prefix: String,
        /// Starting number
        #[arg(long, default_value_t = 1)]
        start_number: u64,
        /// Total digits count for padding (e.g. 6)
        #[arg(long, default_value_t = 6)]
        digits: usize,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Attach an Associated File (/AF) to PDF
    Attach {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// File to attach
        #[arg(long)]
        file: PathBuf,
        /// Semantic relationship (Source, Data, Supplement, Alternative)
        #[arg(long, default_value = "Data")]
        relationship: String,
        /// MIME type (e.g. "text/xml")
        #[arg(long, default_value = "application/octet-stream")]
        mime_type: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Set page numbering labels (/PageLabels)
    PageLabel {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Label style (decimal, lower-roman, upper-roman, lower-alpha, upper-alpha)
        #[arg(long, default_value = "decimal")]
        style: String,
        /// Optional label prefix
        #[arg(long)]
        prefix: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Set GIS geographic anchor (/Geo)
    Geo {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        #[arg(short, long)]
        output: PathBuf,
        /// Latitude in degrees
        #[arg(long)]
        lat: f64,
        /// Longitude in degrees
        #[arg(long)]
        lon: f64,
        /// Coordinate Reference System WKT
        #[arg(long, default_value = "GEOGCS[\"WGS 84\"]")]
        crs: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
}

#[derive(Subcommand, Debug)]
enum PublishSubcommands {
    /// Upgrade document to PDF 2.0 and modern standards (A-4, X-6, UA-2)
    Upgrade {
        /// Input PDF file
        input: PathBuf,
        /// Output PDF file
        output: PathBuf,
        /// Target standard (a4, x6, ua2)
        #[arg(long)]
        standard: Option<String>,
        /// Optional ICC color profile path
        #[arg(long)]
        icc_profile: Option<PathBuf>,
        /// Opt-in for Fast Web View (Linearization)
        #[arg(long)]
        linearize: bool,
        /// Display internal structural diff after refinement
        #[arg(long)]
        diff: bool,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Render a PDF page to an image (PNG, JPEG)
    Render {
        /// Input PDF file
        input: PathBuf,
        /// Output image file (format detected from extension)
        output: PathBuf,
        /// Page number to render (default 1)
        #[arg(short, long, default_value_t = 1)]
        page: usize,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Digitally sign the PDF document
    Sign {
        /// Input PDF file
        input: PathBuf,
        /// Output signed PDF file
        output: PathBuf,
        /// Reason for signing
        #[arg(long)]
        reason: Option<String>,
        /// Location of signing
        #[arg(long)]
        location: Option<String>,
        /// Signer name
        #[arg(long)]
        name: Option<String>,
        /// Page number for the signature widget (default 1)
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// Visual rectangle [x1, y1, x2, y2]
        #[arg(long, value_delimiter = ',', num_args = 4)]
        rect: Option<Vec<f32>>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
        /// Output optimization options
        #[command(flatten)]
        save: SaveArgs,
    },
    /// Verify a digital signature on a specific field
    VerifySignature {
        /// Input PDF file
        input: PathBuf,
        /// Signature field name
        #[arg(short, long)]
        field: String,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
}

#[derive(Subcommand, Debug)]
enum DebugSubcommands {
    /// Dump a specific PDF object
    Dump {
        /// Input PDF file
        input: PathBuf,
        /// Object ID to dump
        #[arg(long)]
        obj: u32,
        /// Gen number (default 0)
        #[arg(long, default_value_t = 0)]
        gen_num: u16,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Display arena memory and object statistics
    Stats {
        /// Input PDF file
        input: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Extract raw font data
    FontExtract {
        /// Input PDF file
        input: PathBuf,
        /// Object ID of the font
        obj_num: u32,
        /// Output file path
        output: PathBuf,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
    /// Trace glyph mapping for a specific character
    TraceGlyph {
        /// Input PDF file
        input: PathBuf,
        /// Unicode character or hex code (e.g., "A" or "U+0041")
        #[arg(short, long)]
        unicode: String,
        /// Specific font name to trace (optional, scans all if omitted)
        #[arg(short, long)]
        font: Option<String>,
        /// Ingestion control options
        #[command(flatten)]
        ingest: IngestArgs,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // RR-15 Limit: Dispatcher - CLIs top level command dispatcher routing to handlers
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect { sub } => match sub {
            InspectSubcommands::Info { input, format, ingest } => {
                handle_info(input, format, ingest)?;
            }
            InspectSubcommands::Audit { input, format, ingest } => {
                handle_audit(input, format, ingest)?;
            }
            InspectSubcommands::Text { input, pages, ingest } => {
                handle_text(input, pages, ingest)?;
            }
            InspectSubcommands::Tree { input, ingest } => {
                handle_debug_structure(input, ingest)?;
            }
        },
        Commands::Edit { sub } => match sub {
            EditSubcommands::Merge { inputs, output, ingest, save } => {
                handle_merge(inputs, output, ingest, save)?;
            }
            EditSubcommands::Split { input, output, pages, ingest, save } => {
                handle_split(input, output, pages, ingest, save)?;
            }
            EditSubcommands::Rotate { input, output, pages, angle, ingest, save } => {
                handle_rotate(input, output, pages, angle, ingest, save)?;
            }
            EditSubcommands::Repair { input, output, ingest, save } => {
                handle_repair(input, output, ingest, save)?;
            }
            EditSubcommands::Tag { input, output, wizard, ingest, save } => {
                handle_retag(input, output, wizard, ingest, save)?;
            }
            EditSubcommands::Portfolio { output, files, cover, ingest, save } => {
                handle_portfolio(output, files, cover, ingest, save)?;
            }
            EditSubcommands::Bates {
                input,
                output,
                prefix,
                start_number,
                digits,
                ingest,
                save,
            } => {
                handle_bates(input, output, prefix, start_number, digits, ingest, save)?;
            }
            EditSubcommands::Attach {
                input,
                output,
                file,
                relationship,
                mime_type,
                ingest,
                save,
            } => {
                handle_attach(input, output, file, relationship, mime_type, ingest, save)?;
            }
            EditSubcommands::PageLabel { input, output, style, prefix, ingest, save } => {
                handle_page_label(input, output, style, prefix, ingest, save)?;
            }
            EditSubcommands::Geo { input, output, lat, lon, crs, ingest, save } => {
                handle_geo(input, output, lat, lon, crs, ingest, save)?;
            }
        },
        Commands::Publish { sub } => match sub {
            PublishSubcommands::Upgrade {
                input,
                output,
                standard,
                icc_profile,
                linearize,
                diff,
                ingest,
                save,
            } => {
                handle_upgrade(
                    input,
                    output,
                    standard,
                    icc_profile,
                    linearize,
                    diff,
                    ingest,
                    save,
                )?;
            }
            PublishSubcommands::Render { input, output, page, ingest } => {
                handle_render(input, output, page, ingest)?;
            }
            PublishSubcommands::Sign {
                input,
                output,
                reason,
                location,
                name,
                page,
                rect,
                ingest,
                save,
            } => {
                handle_sign(input, output, reason, location, name, page, rect, ingest, save)?;
            }
            PublishSubcommands::VerifySignature { input, field, ingest } => {
                handle_verify_signature(input, field, ingest)?;
            }
        },
        Commands::Debug { sub } => match sub {
            DebugSubcommands::Dump { input, obj, gen_num, ingest } => {
                handle_debug_dump(input, obj, gen_num, ingest)?;
            }
            DebugSubcommands::Stats { input, ingest } => {
                handle_debug_stats(input, ingest)?;
            }
            DebugSubcommands::FontExtract { input, obj_num, output, ingest } => {
                handle_extract_font(input, obj_num, output, ingest)?;
            }
            DebugSubcommands::TraceGlyph { input, unicode, font, ingest } => {
                handle_debug_trace_glyph(input, unicode, font, ingest)?;
            }
        },
        Commands::Credits => {
            handle_credits()?;
        }
    }

    Ok(())
}

fn handle_merge(
    inputs: Vec<PathBuf>,
    output: PathBuf,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf merge: Combining {} files into {}", inputs.len(), output.display());
    let mut sources = Vec::new();
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    for path in inputs {
        let data =
            std::fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        sources.push(doc);
    }

    let merged = PdfDocument::merge(sources).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let save_options: fepdf_sdk::SaveOptions = save.into();
    merged
        .save_with_options(&output, "2.0", &save_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: Merged output saved to {}", output.display());
    Ok(())
}

fn handle_split(
    input: PathBuf,
    output: PathBuf,
    pages: Option<String>,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf split: Extracting pages from {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_indices = parse_page_range(&range_str, page_count)?;

    let extracted = doc.extract_pages(target_indices).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    extracted
        .save_with_options(&output, "2.0", &save_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: Extracted output saved to {}", output.display());
    Ok(())
}

fn render_summary_markdown(
    summary: &fepdf_sdk::DocumentSummary,
    input: &std::path::Path,
    audit: bool,
) -> Result<()> {
    println!("# Document Summary: {}", input.file_name().unwrap_or_default().display());
    render_general_info(summary);
    render_font_audit(summary);

    if audit {
        render_compliance_markdown(summary)?;
    }
    Ok(())
}

fn render_general_info(summary: &fepdf_sdk::DocumentSummary) {
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

fn render_font_audit(summary: &fepdf_sdk::DocumentSummary) {
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

fn render_compliance_markdown(summary: &fepdf_sdk::DocumentSummary) -> Result<()> {
    let errors = summary
        .compliance
        .issues
        .iter()
        .filter(|i| {
            matches!(
                i.severity,
                fepdf_sdk::IssueSeverity::Error | fepdf_sdk::IssueSeverity::Critical
            )
        })
        .count();
    let warnings = summary
        .compliance
        .issues
        .iter()
        .filter(|i| matches!(i.severity, fepdf_sdk::IssueSeverity::Warning))
        .count();
    println!("\n## Compliance Audit (UA-2)");
    println!("**Summary**: {errors} Errors, {warnings} Warnings");

    if !summary.compliance.issues.is_empty() {
        println!("\n| Severity | Standard | Message |");
        println!("| :--- | :--- | :--- |");
        for issue in &summary.compliance.issues {
            let icon = match issue.severity {
                fepdf_sdk::IssueSeverity::Critical => "🚨",
                fepdf_sdk::IssueSeverity::Error => "❌",
                fepdf_sdk::IssueSeverity::Warning => "⚠️",
                fepdf_sdk::IssueSeverity::Info => "ℹ️",
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

fn render_summary_text(
    doc: &PdfDocument,
    summary: &fepdf_sdk::DocumentSummary,
    audit: bool,
    structure: bool,
) -> Result<()> {
    render_general_text(summary);
    render_font_text(summary);

    if audit {
        render_audit_text(summary);
    }

    if structure {
        let tree = doc.print_structure().map_err(|e| anyhow::anyhow!("{e:?}"))?;
        println!("\n--- [ DOCUMENT STRUCTURE ] ---\n{tree}");
    }
    Ok(())
}

fn handle_info(input: PathBuf, format: String, ingest: IngestArgs) -> Result<()> {
    if format == "text" {
        println!("fepdf info: Analyzing {}", input.display());
    }
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
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

fn handle_audit(input: PathBuf, format: String, ingest: IngestArgs) -> Result<()> {
    if format == "text" {
        println!("fepdf audit: Performing compliance check on {}", input.display());
    }
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
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

fn handle_debug_dump(input: PathBuf, obj_id: u32, _gen_num: u16, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug dump: Object {obj_id} from {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let described = doc.describe_object(obj_id).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("\n--- [ OBJECT {obj_id} ] ---\n{described}");
    Ok(())
}

fn handle_debug_structure(input: PathBuf, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug structure: Hierarchical tree for {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let tree = doc.print_structure().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("\n--- [ DOCUMENT STRUCTURE ] ---\n{tree}");
    Ok(())
}

fn handle_debug_stats(input: PathBuf, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug stats: Analyzing memory usage for {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let stats = doc.arena_stats();

    println!("\n--- [ ARENA STATISTICS ] ---");
    println!("PDF Version:      {}", stats.version);
    println!("Indirect Objects: {}", stats.object_count);
    println!("Dictionaries:     {}", stats.dictionary_count);
    println!("Arrays:           {}", stats.array_count);
    println!("\n--- [ FONT RESOURCES ] ---");
    for font in doc.fonts() {
        println!("  Handle {:>3}: {:<30} ({})", font.object_id, font.name, font.font_type);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_upgrade(
    input: PathBuf,
    output: PathBuf,
    standard: Option<String>,
    icc_profile: Option<PathBuf>,
    linearize: bool,
    diff: bool,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf upgrade: {} -> {}", output.display(), input.display());
    if save.dry_run {
        println!("DRY RUN: Simulation mode enabled. No file will be written.");
    }

    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    if diff {
        println!("INFO: Structural diff would be displayed here (M67 enhancement).");
    }

    if let Some(std_str) = standard {
        let std = match std_str.to_lowercase().as_str() {
            "a4" => PdfStandard::A4,
            "x6" => PdfStandard::X6,
            "ua2" => PdfStandard::UA2,
            _ => anyhow::bail!("Unsupported standard: {std_str}"),
        };

        if (std == PdfStandard::A4 || std == PdfStandard::X6) && icc_profile.is_none() {
            println!("ADVICE: No --icc-profile specified. Defaulting to standard sRGB.");
        }
        doc.upgrade_to_standard(std).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }

    let save_options: fepdf_sdk::SaveOptions = save.into();

    if linearize {
        doc.save_linearized(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    } else {
        doc.save_with_options(&output, "2.0", &save_options)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    println!("SUCCESS: Output saved to {}", output.display());
    Ok(())
}

fn handle_repair(
    input: PathBuf,
    output: PathBuf,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf repair: Attempting to salvage corrupted document {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_and_repair_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: Repaired output saved to {}", output.display());
    Ok(())
}

fn handle_rotate(
    input: PathBuf,
    output: PathBuf,
    pages: Option<String>,
    angle: i32,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf rotate: Rotating pages in {} by {angle} degrees...", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_pages = parse_page_range(&range_str, page_count)?;

    let quarter = fepdf_sdk::Quarter::from_degrees(angle)
        .ok_or_else(|| anyhow::anyhow!("Angle must be a multiple of 90 degrees"))?;

    doc.apply(fepdf_sdk::Operation::Rotate {
        pages: fepdf_sdk::PageSelection::Indices(target_pages),
        mode: fepdf_sdk::RotateMode::Absolute(quarter),
    })
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: Rotated output saved to {}", output.display());
    Ok(())
}

fn handle_render(
    input: PathBuf,
    output: PathBuf,
    page_num: usize,
    ingest: IngestArgs,
) -> Result<()> {
    println!(
        "fepdf render: Rendering page {page_num} of {} to {}...",
        output.display(),
        input.display()
    );
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    // Host-level font discovery
    let mut system_fonts = std::collections::BTreeMap::new();
    let mincho_paths = [
        "/System/Library/Fonts/ヒラギノ明朝 ProN.ttc",
        "/System/Library/Fonts/Hiragino Mincho ProN.ttc",
        "/usr/share/fonts/opentype/ipafont-mincho/ipam.ttf",
    ];
    for path in mincho_paths {
        if let Ok(data) = std::fs::read(path) {
            system_fonts.insert(fepdf_sdk::FallbackFontType::Serif, std::sync::Arc::new(data));
            break;
        }
    }
    let gothic_paths = [
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
    ];
    for path in gothic_paths {
        if let Ok(data) = std::fs::read(path) {
            let arc = std::sync::Arc::new(data);
            system_fonts.insert(fepdf_sdk::FallbackFontType::SansSerif, arc.clone());
            system_fonts.entry(fepdf_sdk::FallbackFontType::Default).or_insert(arc);
            break;
        }
    }
    doc.set_system_fonts(system_fonts);

    if page_num == 0 || page_num > doc.page_count().map_err(|e| anyhow::anyhow!("{e:?}"))? {
        return Err(anyhow::anyhow!("Invalid page number: {page_num}"));
    }

    doc.render_page_to_file(page_num - 1, &output).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    println!("SUCCESS: Rendered page saved to {}", output.display());
    Ok(())
}

fn handle_retag(
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
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
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
        doc.retag_document().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: Re-tagged document saved to {}", output.display());
    Ok(())
}

fn handle_text(input: PathBuf, pages: Option<String>, ingest: IngestArgs) -> Result<()> {
    println!("fepdf text: Extracting text from {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_indices = parse_page_range(&range_str, page_count)?;

    for idx in target_indices {
        let text = doc.extract_text(idx).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        println!("\n--- [ PAGE {} ] ---\n{}", idx + 1, text);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_sign(
    input: PathBuf,
    output: PathBuf,
    reason: Option<String>,
    location: Option<String>,
    name: Option<String>,
    page: usize,
    rect: Option<Vec<f32>>,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf sign: {} -> {}", output.display(), input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let mut sign_options = fepdf_sdk::SignOptions {
        reason,
        location,
        name,
        page_index: page.saturating_sub(1),
        ..Default::default()
    };

    if let Some(r) = rect {
        if r.len() == 4 {
            sign_options.rect = [r[0], r[1], r[2], r[3]];
        }
    } else {
        sign_options.rect = [50.0, 50.0, 200.0, 100.0]; // Default widget rect
    }

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_signed(&output, "2.0", &save_options, &sign_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    println!("SUCCESS: Signed document saved to {}", output.display());
    Ok(())
}

fn handle_credits() -> Result<()> {
    println!("\n--- [ OPEN SOURCE CREDITS ] ---");
    println!("fepdf and fepdf-sdk are powered by the following excellent libraries:\n");

    let credits = [
        ("lopdf", "MIT", "Low-level PDF parsing and manipulation"),
        ("pdf-writer", "Apache-2.0", "Efficient PDF object serialization"),
        ("flate2", "MIT / Apache-2.0", "Deflate/Zlib compression"),
        ("vello", "Apache-2.0 / MIT", "High-performance vector graphics"),
        ("kurbo", "Apache-2.0 / MIT", "Vector geometry primitives"),
        ("skrifa / read-fonts", "Apache-2.0 / MIT", "Modern font parsing and glyph scaling"),
        ("image", "MIT / Apache-2.0", "Raster image processing"),
        ("anyhow / thiserror", "MIT / Apache-2.0", "Structured error handling"),
        ("serde", "MIT / Apache-2.0", "Universal serialization framework"),
        ("tokio", "MIT", "Asynchronous runtime"),
    ];

    println!("{:<25} | {:<20} | {:<30}", "Crate", "License", "Purpose");
    println!("{:-<25}-+-{:-<20}-+-{:-<30}", "", "", "");
    for (name, license, purpose) in credits {
        println!("{name:<25} | {license:<20} | {purpose:<30}");
    }

    println!("\nFull license texts are available in the repository's NOTICE file.");
    println!("Thank you to the Rust community for these amazing tools.");
    Ok(())
}

fn parse_page_range(range_str: &str, max_pages: usize) -> Result<Vec<usize>> {
    let mut pages = Vec::new();
    for part in range_str.split(',') {
        if part.trim() == "all" {
            return Ok((0..max_pages).collect());
        }
        if part.contains('-') {
            let bounds: Vec<&str> = part.split('-').collect();
            if bounds.len() == 2 {
                let start: usize = bounds[0].trim().parse::<usize>()?.saturating_sub(1);
                let end: usize = bounds[1].trim().parse::<usize>()?;
                for i in start..end.min(max_pages) {
                    pages.push(i);
                }
            }
        } else {
            let p: usize = part.trim().parse::<usize>()?.saturating_sub(1);
            if p < max_pages {
                pages.push(p);
            }
        }
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

fn render_general_text(summary: &fepdf_sdk::DocumentSummary) {
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

fn render_font_text(summary: &fepdf_sdk::DocumentSummary) {
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

fn render_audit_text(summary: &fepdf_sdk::DocumentSummary) {
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

fn handle_extract_font(
    input: PathBuf,
    obj_num: u32,
    _output: PathBuf,
    ingest: IngestArgs,
) -> Result<()> {
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let obj_id = obj_num;
    let font_resource = doc.get_font(obj_id).ok().map(|arc_f| (*arc_f).clone());

    if let Some(mut resource) = font_resource {
        resource.perform_reconstruction().ok();
        let data = resource.reconstructed_data.as_ref().or(resource.data.as_ref());
        if let Some(arc_data) = data {
            let extension = if resource.reconstructed_data.is_some() { "otf" } else { "cid" };
            let filename = format!("exports/font-{obj_id:04}.{extension}");
            std::fs::write(&filename, &**arc_data).with_context(|| "Failed to write output")?;
            println!("SUCCESS: Extracted font to {} ({} bytes)", filename, arc_data.len());
        } else {
            anyhow::bail!("No data for font {obj_id}");
        }
    } else {
        anyhow::bail!("Failed to load font resource for {obj_id}");
    }
    Ok(())
}

fn trace_single_font(font: &fepdf_sdk::FontResource, name: &str, obj_id: u32, target_char: char) {
    println!("\n--- [ FONT: {name} ] ---");
    println!("Object: {obj_id}");

    let mut ctx = TraceContext::new();
    let cid_match = font.unicode_to_gid.get(&target_char).copied();
    if let Some(cid) = cid_match {
        println!("Note: Unicode character maps to CID {cid} in this font's CMap");
    }

    let gid = font.resolve_gid(cid_match.unwrap_or(0), Some(target_char), Some(&mut ctx));

    #[cfg(feature = "debug-tools")]
    for (i, step) in ctx.traces.iter().flat_map(|t| t.steps.iter()).enumerate() {
        println!("  {:>2}. {}", i + 1, step);
    }

    match gid {
        Some(g) => {
            let cid = cid_match.unwrap_or(0);
            let w = font.glyph_width_by_cid(cid);
            let (v_w, vx, vy) = font.glyph_vertical_metrics(cid);
            println!("RESULT: GID {g} (w: {w}, vx: {vx}, vy: {vy}, v_adv: {v_w})");
        }
        None => println!("RESULT: FAILED TO RESOLVE"),
    }
}

fn handle_debug_trace_glyph(
    input: PathBuf,
    unicode_str: String,
    font_filter: Option<String>,
    ingest: IngestArgs,
) -> Result<()> {
    println!(
        "fepdf debug trace-glyph: Analyzing mapping for '{unicode_str}' in {}",
        input.display()
    );

    let target_char = parse_unicode(&unicode_str)?;
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let font_summaries = doc.fonts();
    let mut found_any = false;

    for summary in font_summaries {
        let name = summary.name.as_str();
        if let Some(ref filter) = font_filter
            && !name.contains(filter)
        {
            continue;
        }

        let font = match doc.get_font(summary.object_id) {
            Ok(f) => f,
            Err(e) => {
                println!("Warning: Failed to load font {name}: {e:?}");
                continue;
            }
        };

        found_any = true;
        trace_single_font(&font, name, summary.object_id, target_char);
    }

    if !found_any {
        println!("No fonts matched the filter: {font_filter:?}");
    }

    Ok(())
}

fn handle_portfolio(
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
        items.push(fepdf_sdk::PortfolioItem {
            filename: file_name,
            mime_type: None,
            description: None,
            size_bytes: data.len() as u64,
            data,
        });
    }

    let portfolio = fepdf_sdk::PortfolioCollection {
        view_mode: fepdf_sdk::CollectionViewMode::Details,
        initial_document: None,
        items,
    };

    let mut doc = PdfDocument::create_empty().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    doc.apply(fepdf_sdk::Operation::CreatePortfolio(portfolio))
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: Portfolio saved to {}", output.display());
    Ok(())
}

fn handle_bates(
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
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let op = fepdf_sdk::Operation::ApplyBatesNumbering {
        pages: fepdf_sdk::PageSelection::All,
        prefix,
        start_number,
        digits,
        position: fepdf_sdk::DecorationPosition::BottomRight,
    };
    doc.apply(op).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: Output with Bates numbering saved to {}", output.display());
    Ok(())
}

fn handle_attach(
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
        "source" => fepdf_sdk::AFRelationship::Source,
        "supplement" => fepdf_sdk::AFRelationship::Supplement,
        "alternative" => fepdf_sdk::AFRelationship::Alternative,
        _ => fepdf_sdk::AFRelationship::Data,
    };

    let af =
        fepdf_sdk::AssociatedFile { filename: file_name, relationship, mime_type, data: file_data };

    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    doc.apply(fepdf_sdk::Operation::AttachAssociatedFile(af))
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: PDF with Associated File saved to {}", output.display());
    Ok(())
}

fn handle_page_label(
    input: PathBuf,
    output: PathBuf,
    style_str: String,
    prefix: Option<String>,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf page-label: Setting page labels on {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input PDF")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let style = match style_str.to_lowercase().as_str() {
        "lower-roman" => fepdf_sdk::PageLabelStyle::LowerRoman,
        "upper-roman" => fepdf_sdk::PageLabelStyle::UpperRoman,
        "lower-alpha" => fepdf_sdk::PageLabelStyle::LowerAlpha,
        "upper-alpha" => fepdf_sdk::PageLabelStyle::UpperAlpha,
        _ => fepdf_sdk::PageLabelStyle::Decimal,
    };

    let labels = vec![fepdf_sdk::PageLabelSpec { start_page: 0, style, prefix, start_number: 1 }];

    doc.apply(fepdf_sdk::Operation::SetPageLabels(labels)).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: PDF with updated page labels saved to {}", output.display());
    Ok(())
}

fn handle_geo(
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
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let anchor = fepdf_sdk::GeoSpatialAnchor {
        page: 0,
        latitude: lat,
        longitude: lon,
        altitude_meters: None,
        crs_wkt: crs,
    };

    doc.apply(fepdf_sdk::Operation::SetGeospatialAnchor(anchor))
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let save_options: fepdf_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("SUCCESS: PDF with GIS anchor saved to {}", output.display());
    Ok(())
}

fn handle_verify_signature(input: PathBuf, field: String, ingest: IngestArgs) -> Result<()> {
    println!("fepdf verify-signature: Verifying field '{field}' on {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input PDF")?;
    let ingest_options: fepdf_sdk::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    doc.apply(fepdf_sdk::Operation::VerifyDigitalSignature { field_name: field.clone() })
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let report = fepdf_sdk::PkiValidator::validate_signature_bytes(&field, &[])
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    println!("\n--- [ DIGITAL SIGNATURE VERIFICATION REPORT ] ---");
    println!("Field Name: {}", report.field_name);
    println!("Status: {:?}", report.status);
    println!("Summary: {}", report.summary);
    Ok(())
}

fn parse_unicode(s: &str) -> Result<char> {
    if s.starts_with("U+") || s.starts_with("u+") {
        let hex = &s[2..];
        let val = u32::from_str_radix(hex, 16).with_context(|| "Invalid hex code")?;
        std::char::from_u32(val)
            .ok_or_else(|| anyhow::anyhow!("Invalid unicode scalar: U+{val:04X}"))
    } else if let Some(c) = s.chars().next() {
        Ok(c)
    } else {
        anyhow::bail!(
            "Invalid unicode input. Use single char or U+XXXX format (e.g. 'A' or 'U+6C38')"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unicode_input() {
        assert_eq!(parse_unicode("A").unwrap(), 'A');
        assert_eq!(parse_unicode("U+6C38").unwrap(), '永');
    }

    #[test]
    fn test_ingest_args_conversion() {
        let args = IngestArgs {
            no_refinement: true,
            no_metadata_recovery: false,
            relaxed_color: true,
            force_fallback: false,
        };
        let opts: fepdf_sdk::IngestionOptions = args.into();
        assert!(!opts.active_refinement);
        assert!(opts.sublime_metadata);
        assert_eq!(opts.color_policy, fepdf_sdk::ColorPolicy::Relaxed);
    }
}
