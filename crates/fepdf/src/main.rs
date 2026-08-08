//! fepdf: The Universal PDF Toolkit.
//!
//! (ISO 32000-2:2020 Compliance & Optimization Engine)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ferruginous_core::Object;
use ferruginous_core::handle::Handle;
use ferruginous_sdk::{PdfDocument, PdfStandard, TraceContext};
use inquire::Confirm;
use std::path::PathBuf;

/// Common options for PDF ingestion/reading
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

impl From<IngestArgs> for ferruginous_core::ingest::IngestionOptions {
    fn from(args: IngestArgs) -> Self {
        Self {
            active_refinement: !args.no_refinement,
            sublime_metadata: !args.no_metadata_recovery,
            color_policy: if args.relaxed_color {
                ferruginous_core::ingest::ColorPolicy::Relaxed
            } else {
                ferruginous_core::ingest::ColorPolicy::Strict
            },
            force_fallback: args.force_fallback,
            password: None,
            progress_callback: None,
        }
    }
}

/// Common options for PDF writing/optimization
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

impl From<SaveArgs> for ferruginous_sdk::SaveOptions {
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
                "utf8" => ferruginous_sdk::StringEncoding::Utf8,
                _ => ferruginous_sdk::StringEncoding::Utf16BE,
            },
            creation_date: None,
            dry_run: args.dry_run,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "fepdf")]
#[command(author = "Ferruginous Developers")]
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
async fn main() -> Result<()> { // RR-15 Limit: Dispatcher - CLIs top level command dispatcher routing to handlers
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
    println!("fepdf merge: Combining {} files into {:?}", inputs.len(), output);
    let mut sources = Vec::new();
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    for path in inputs {
        let data = std::fs::read(&path).with_context(|| format!("Failed to read {:?}", path))?;
        let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        sources.push(doc);
    }

    let merged = PdfDocument::merge(sources).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let save_options: ferruginous_sdk::SaveOptions = save.into();
    merged
        .save_with_options(&output, "2.0", &save_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    println!("SUCCESS: Merged output saved to {:?}", output);
    Ok(())
}

fn handle_split(
    input: PathBuf,
    output: PathBuf,
    pages: Option<String>,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf split: Extracting pages from {:?}", input);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_indices = parse_page_range(&range_str, page_count)?;

    let extracted = doc.extract_pages(target_indices).map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let save_options: ferruginous_sdk::SaveOptions = save.into();
    extracted
        .save_with_options(&output, "2.0", &save_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    println!("SUCCESS: Extracted output saved to {:?}", output);
    Ok(())
}

fn render_summary_markdown(
    summary: &ferruginous_sdk::DocumentSummary,
    input: &std::path::Path,
    audit: bool,
) -> Result<()> {
    println!("# Document Summary: {:?}", input.file_name().unwrap_or_default());
    render_general_info(summary);
    render_font_audit(summary);

    if audit {
        render_compliance_markdown(summary)?;
    }
    Ok(())
}

fn render_general_info(summary: &ferruginous_sdk::DocumentSummary) {
    println!("\n## General Information");
    println!("\n| Property | Value |");
    println!("| :--- | :--- |");
    println!("| Version | {} |", summary.version);
    println!("| Total Pages | {} |", summary.page_count);
    if let Some(v) = &summary.metadata.title {
        println!("| Title | {} |", v);
    }
    if let Some(v) = &summary.metadata.author {
        println!("| Author | {} |", v);
    }
    if let Some(v) = &summary.metadata.subject {
        println!("| Subject | {} |", v);
    }
    if let Some(v) = &summary.metadata.keywords {
        println!("| Keywords | {} |", v);
    }
    if let Some(v) = &summary.metadata.creator {
        println!("| Creator | {} |", v);
    }
    if let Some(v) = &summary.metadata.producer {
        println!("| Producer | {} |", v);
    }
}

fn render_font_audit(summary: &ferruginous_sdk::DocumentSummary) {
    let embedded_count = summary.fonts.iter().filter(|f| f.is_embedded).count();
    let total_fonts = summary.fonts.len();

    println!("\n## Font Audit (Embedded: {}/{})", embedded_count, total_fonts);
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

fn render_compliance_markdown(summary: &ferruginous_sdk::DocumentSummary) -> Result<()> {
    let errors = summary
        .compliance
        .issues
        .iter()
        .filter(|i| {
            matches!(
                i.severity,
                ferruginous_sdk::IssueSeverity::Error | ferruginous_sdk::IssueSeverity::Critical
            )
        })
        .count();
    let warnings = summary
        .compliance
        .issues
        .iter()
        .filter(|i| matches!(i.severity, ferruginous_sdk::IssueSeverity::Warning))
        .count();
    println!("\n## Compliance Audit (UA-2)");
    println!("**Summary**: {} Errors, {} Warnings", errors, warnings);

    if !summary.compliance.issues.is_empty() {
        println!("\n| Severity | Standard | Message |");
        println!("| :--- | :--- | :--- |");
        for issue in &summary.compliance.issues {
            let icon = match issue.severity {
                ferruginous_sdk::IssueSeverity::Critical => "🚨",
                ferruginous_sdk::IssueSeverity::Error => "❌",
                ferruginous_sdk::IssueSeverity::Warning => "⚠️",
                ferruginous_sdk::IssueSeverity::Info => "ℹ️",
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
            println!("- **Clause {}**", clause);
        }
    }
    Ok(())
}

fn render_summary_text(
    doc: &PdfDocument,
    summary: &ferruginous_sdk::DocumentSummary,
    audit: bool,
    structure: bool,
) -> Result<()> {
    render_general_text(summary);
    render_font_text(summary);

    if audit {
        render_audit_text(summary);
    }

    if structure {
        let tree = doc.print_structure().map_err(|e| anyhow::anyhow!("{:?}", e))?;
        println!("\n--- [ DOCUMENT STRUCTURE ] ---\n{}", tree);
    }
    Ok(())
}

fn handle_info(input: PathBuf, format: String, ingest: IngestArgs) -> Result<()> {
    if format == "text" {
        println!("fepdf info: Analyzing {:?}", input);
    }
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let summary = doc.get_summary().map_err(|e| anyhow::anyhow!("{:?}", e))?;

    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&summary)?),
        "markdown" => render_summary_markdown(&summary, &input, false)?,
        _ => render_summary_text(&doc, &summary, false, false)?,
    }
    Ok(())
}

fn handle_audit(input: PathBuf, format: String, ingest: IngestArgs) -> Result<()> {
    if format == "text" {
        println!("fepdf audit: Performing compliance check on {:?}", input);
    }
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let summary = doc.get_summary().map_err(|e| anyhow::anyhow!("{:?}", e))?;

    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&summary)?),
        "markdown" => render_summary_markdown(&summary, &input, true)?,
        _ => render_summary_text(&doc, &summary, true, false)?,
    }
    Ok(())
}

fn print_dictionary_fields(
    arena: &ferruginous_core::PdfArena,
    h: Handle<std::collections::BTreeMap<Handle<ferruginous_core::PdfName>, Object>>,
) {
    if let Some(dict) = arena.get_dict(h) {
        println!("Type: Dictionary");
        for (k, v) in dict.iter() {
            let name = arena
                .get_name(*k)
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| format!("Unknown_{:?}", k));
            let val_str = match v {
                ferruginous_core::Object::Name(vh) => arena
                    .get_name(*vh)
                    .map(|n| format!("Name(/{})", n.as_str()))
                    .unwrap_or_else(|| format!("{:?}", v)),
                _ => format!("{:?}", v),
            };
            println!("  /{} -> {}", name, val_str);
        }
    } else {
        println!("Error: Dictionary handle {:?} not found in arena", h);
    }
}

fn print_stream_fields(
    arena: &ferruginous_core::PdfArena,
    h: Handle<std::collections::BTreeMap<Handle<ferruginous_core::PdfName>, Object>>,
    data: std::sync::Arc<ferruginous_core::SublimatedData>,
) {
    if let Some(dict) = arena.get_dict(h) {
        println!("Type: Stream");
        for (k, v) in dict.iter() {
            let name = arena
                .get_name(*k)
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| format!("Unknown_{:?}", k));
            println!("  /{} -> {:?}", name, v);
        }
        let raw_bytes = arena.get_stream_bytes(&data).unwrap_or_default();
        println!("Raw Length: {} bytes", raw_bytes.len());

        if let Ok(decoded) = arena.process_filters(&raw_bytes, &dict) {
            println!("Decoded Length: {} bytes", decoded.len());
            if decoded.len() < 2000 {
                println!("\n--- [ DECODED CONTENT ] ---\n{}", String::from_utf8_lossy(&decoded));
            } else {
                println!(
                    "\n--- [ DECODED CONTENT (PREVIEW) ] ---\n{}",
                    String::from_utf8_lossy(&decoded[..2000])
                );
                println!("... (truncated)");
            }
        }
    } else {
        println!("Error: Stream dictionary handle {:?} not found in arena", h);
    }
}

fn handle_debug_dump(input: PathBuf, obj_id: u32, _gen_num: u16, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug dump: Object {} from {:?}", obj_id, input);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let arena = doc.inner().arena();
    let id = ferruginous_core::Object::Reference(Handle::new(obj_id));
    let resolved = id.resolve(arena);

    println!("\n--- [ OBJECT {} ] ---", obj_id);
    match resolved {
        ferruginous_core::Object::Dictionary(h) => {
            print_dictionary_fields(arena, h);
        }
        ferruginous_core::Object::Stream(h, data) => {
            print_stream_fields(arena, h, data);
        }
        _ => println!("{:?}", resolved),
    }

    Ok(())
}

fn handle_debug_structure(input: PathBuf, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug structure: Hierarchical tree for {:?}", input);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let tree = doc.print_structure().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    println!("\n--- [ DOCUMENT STRUCTURE ] ---\n{}", tree);
    Ok(())
}

fn handle_debug_stats(input: PathBuf, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug stats: Analyzing memory usage for {:?}", input);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let stats = doc.inner().arena().get_stats();

    println!("\n--- [ ARENA STATISTICS ] ---");
    println!("PDF Version:      {}", stats.version);
    println!("Indirect Objects: {}", stats.object_count);
    println!("Dictionaries:     {}", stats.dictionary_count);
    println!("Arrays:           {}", stats.array_count);
    println!("\n--- [ FONT RESOURCES ] ---");
    for font in doc.inner().fonts() {
        println!("  Handle {:>3}: {:<30} ({})", font.handle.index(), font.name, font.font_type);
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
    println!("fepdf upgrade: {:?} -> {:?}", input, output);
    if save.dry_run {
        println!("DRY RUN: Simulation mode enabled. No file will be written.");
    }

    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    if diff {
        println!("INFO: Structural diff would be displayed here (M67 enhancement).");
    }

    if let Some(std_str) = standard {
        let std = match std_str.to_lowercase().as_str() {
            "a4" => PdfStandard::A4,
            "x6" => PdfStandard::X6,
            "ua2" => PdfStandard::UA2,
            _ => anyhow::bail!("Unsupported standard: {}", std_str),
        };

        if (std == PdfStandard::A4 || std == PdfStandard::X6) && icc_profile.is_none() {
            println!("ADVICE: No --icc-profile specified. Defaulting to standard sRGB.");
        }
        doc.upgrade_to_standard(std).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    }

    let save_options: ferruginous_sdk::SaveOptions = save.into();

    if linearize {
        doc.save_linearized(&output, "2.0", &save_options)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    } else {
        doc.save_with_options(&output, "2.0", &save_options)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    }
    println!("SUCCESS: Output saved to {:?}", output);
    Ok(())
}

fn handle_repair(
    input: PathBuf,
    output: PathBuf,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf repair: Attempting to salvage corrupted document {:?}", input);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_and_repair_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let save_options: ferruginous_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    println!("SUCCESS: Repaired output saved to {:?}", output);
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
    println!("fepdf rotate: Rotating pages in {:?} by {} degrees...", input, angle);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_pages = parse_page_range(&range_str, page_count)?;

    for idx in target_pages {
        doc.set_page_rotation(idx, angle).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    }

    let save_options: ferruginous_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    println!("SUCCESS: Rotated output saved to {:?}", output);
    Ok(())
}

fn handle_render(
    input: PathBuf,
    output: PathBuf,
    page_num: usize,
    ingest: IngestArgs,
) -> Result<()> {
    println!("fepdf render: Rendering page {} of {:?} to {:?}...", page_num, input, output);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    // Host-level font discovery
    let mut system_fonts = std::collections::BTreeMap::new();
    let mincho_paths = [
        "/System/Library/Fonts/ヒラギノ明朝 ProN.ttc",
        "/System/Library/Fonts/Hiragino Mincho ProN.ttc",
        "/usr/share/fonts/opentype/ipafont-mincho/ipam.ttf",
    ];
    for path in mincho_paths {
        if let Ok(data) = std::fs::read(path) {
            system_fonts
                .insert(ferruginous_sdk::FallbackFontType::Serif, std::sync::Arc::new(data));
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
            system_fonts.insert(ferruginous_sdk::FallbackFontType::SansSerif, arc.clone());
            system_fonts.entry(ferruginous_sdk::FallbackFontType::Default).or_insert(arc);
            break;
        }
    }
    doc.set_system_fonts(system_fonts);

    if page_num == 0 || page_num > doc.page_count().map_err(|e| anyhow::anyhow!("{:?}", e))? {
        return Err(anyhow::anyhow!("Invalid page number: {}", page_num));
    }

    doc.render_page_to_file(page_num - 1, &output).map_err(|e| anyhow::anyhow!("{:?}", e))?;

    println!("SUCCESS: Rendered page saved to {:?}", output);
    Ok(())
}

fn handle_retag(
    input: PathBuf,
    output: PathBuf,
    wizard: bool,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf retag: {} -> {:?}", if wizard { "Wizard Mode" } else { "Automatic" }, output);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    if wizard {
        println!("Wizard Mode: Reviewing heuristic structural candidates...");
        let candidates =
            doc.get_remediation_candidates().map_err(|e| anyhow::anyhow!("{:?}", e))?;

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
        doc.retag_document().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    }

    let save_options: ferruginous_sdk::SaveOptions = save.into();
    doc.save_with_options(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{:?}", e))?;
    println!("SUCCESS: Re-tagged document saved to {:?}", output);
    Ok(())
}

fn handle_text(input: PathBuf, pages: Option<String>, ingest: IngestArgs) -> Result<()> {
    println!("fepdf text: Extracting text from {:?}", input);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let page_count = doc.page_count().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let range_str = pages.unwrap_or_else(|| "all".to_string());
    let target_indices = parse_page_range(&range_str, page_count)?;

    for idx in target_indices {
        let text = doc.extract_text(idx).map_err(|e| anyhow::anyhow!("{:?}", e))?;
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
    println!("fepdf sign: {:?} -> {:?}", input, output);
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let mut sign_options = ferruginous_sdk::SignOptions {
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

    let save_options: ferruginous_sdk::SaveOptions = save.into();
    doc.save_signed(&output, "2.0", &save_options, &sign_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    println!("SUCCESS: Signed document saved to {:?}", output);
    Ok(())
}

fn handle_credits() -> Result<()> {
    println!("\n--- [ OPEN SOURCE CREDITS ] ---");
    println!("fepdf and ferruginous-sdk are powered by the following excellent libraries:\n");

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
        println!("{:<25} | {:<20} | {:<30}", name, license, purpose);
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
    pages.sort();
    pages.dedup();
    Ok(pages)
}

fn render_general_text(summary: &ferruginous_sdk::DocumentSummary) {
    println!("\n--- [ DOCUMENT SUMMARY ] ---");
    println!("Version:    {}", summary.version);
    println!("Pages:      {}", summary.page_count);
    if let Some(v) = &summary.metadata.title {
        println!("Title:      {}", v);
    }
    if let Some(v) = &summary.metadata.author {
        println!("Author:     {}", v);
    }
}

fn render_font_text(summary: &ferruginous_sdk::DocumentSummary) {
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

fn render_audit_text(summary: &ferruginous_sdk::DocumentSummary) {
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
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let obj_id = obj_num;
    let handle = Handle::new(obj_id);
    let obj = doc.inner().arena().get_object(handle);

    let font_resource = if let Some(Object::Dictionary(dh)) = obj {
        if let Some(dict) = doc.inner().arena().get_dict(dh) {
            ferruginous_core::font::FontResource::load(&dict, doc.inner()).ok()
        } else {
            None
        }
    } else {
        None
    };

    if let Some(mut resource) = font_resource {
        resource.perform_reconstruction().ok();
        let data = resource.reconstructed_data.as_ref().or(resource.data.as_ref());
        if let Some(arc_data) = data {
            let extension = if resource.reconstructed_data.is_some() { "otf" } else { "cid" };
            let filename = format!("exports/font-{:04}.{}", obj_id, extension);
            std::fs::write(&filename, &**arc_data).with_context(|| "Failed to write output")?;
            println!("SUCCESS: Extracted font to {} ({} bytes)", filename, arc_data.len());
        } else {
            anyhow::bail!("No data for font {}", obj_id);
        }
    } else {
        anyhow::bail!("Failed to load font resource for {}", obj_id);
    }
    Ok(())
}

fn trace_single_font(
    font: &ferruginous_core::font::FontResource,
    name: &str,
    handle: Handle<Object>,
    target_char: char,
) {
    println!("\n--- [ FONT: {} ] ---", name);
    println!("Handle: {:?}", handle);

    let mut ctx = TraceContext::new();
    let cid_match = font.unicode_to_gid.get(&target_char).copied();
    if let Some(cid) = cid_match {
        println!("Note: Unicode character maps to CID {} in this font's CMap", cid);
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
            println!("RESULT: GID {} (w: {}, vx: {}, vy: {}, v_adv: {})", g, w, vx, vy, v_w);
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
    println!("fepdf debug trace-glyph: Analyzing mapping for '{}' in {:?}", unicode_str, input);

    let target_char = parse_unicode(&unicode_str)?;
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: ferruginous_core::ingest::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let font_summaries = doc.inner().fonts();
    let mut found_any = false;

    for summary in font_summaries {
        let name = summary.name.as_str();
        if let Some(ref filter) = font_filter
            && !name.contains(filter)
        {
            continue;
        }

        let font = match doc.inner().get_font(summary.handle) {
            Ok(f) => f,
            Err(e) => {
                println!("Warning: Failed to load font {}: {:?}", name, e);
                continue;
            }
        };

        found_any = true;
        trace_single_font(&font, name, summary.handle, target_char);
    }

    if !found_any {
        println!("No fonts matched the filter: {:?}", font_filter);
    }

    Ok(())
}

fn parse_unicode(s: &str) -> Result<char> {
    if s.starts_with("U+") || s.starts_with("u+") {
        let hex = &s[2..];
        let val = u32::from_str_radix(hex, 16).with_context(|| "Invalid hex code")?;
        std::char::from_u32(val)
            .ok_or_else(|| anyhow::anyhow!("Invalid unicode scalar: U+{:04X}", val))
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
        let opts: ferruginous_core::ingest::IngestionOptions = args.into();
        assert!(!opts.active_refinement);
        assert!(opts.sublime_metadata);
        assert_eq!(opts.color_policy, ferruginous_core::ingest::ColorPolicy::Relaxed);
    }
}
