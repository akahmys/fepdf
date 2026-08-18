use anyhow::{Context, Result};
use fepdf::{PdfDocument, TraceContext};
use std::path::PathBuf;

use crate::args::IngestArgs;
use crate::util::parse_unicode;

pub fn handle_debug_dump(
    input: PathBuf,
    obj_id: u32,
    _gen_num: u16,
    ingest: IngestArgs,
) -> Result<()> {
    println!("fepdf debug dump: Object {obj_id} from {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let described = doc.describe_object(obj_id).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    println!("\n--- [ OBJECT {obj_id} ] ---\n{described}");
    Ok(())
}

pub fn handle_debug_stats(input: PathBuf, ingest: IngestArgs) -> Result<()> {
    println!("fepdf debug stats: Analyzing memory usage for {}", input.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
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

pub fn handle_extract_font(
    input: PathBuf,
    obj_num: u32,
    _output: PathBuf,
    ingest: IngestArgs,
) -> Result<()> {
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
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

pub fn trace_single_font(font: &fepdf::FontResource, name: &str, obj_id: u32, target_char: char) {
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

pub fn handle_debug_trace_glyph(
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
    let ingest_options: fepdf::IngestionOptions = ingest.into();
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
