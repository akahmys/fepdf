use anyhow::{Context, Result};
use fepdf::{PdfDocument, PdfStandard};
use std::path::PathBuf;

use crate::args::{IngestArgs, SaveArgs};
use crate::formatters::{render_decisions_text, report_write_decisions};

#[allow(clippy::too_many_arguments)]
pub fn handle_upgrade(
    input: PathBuf,
    output: PathBuf,
    standard: Option<String>,
    icc_profile: Option<PathBuf>,
    linearize: bool,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf upgrade: {} -> {}", output.display(), input.display());
    if save.dry_run {
        println!("DRY RUN: Simulation mode enabled. No file will be written.");
    }

    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let mut doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

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
        doc.apply(fepdf::Operation::Upgrade { standard: std })
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();

    // Both branches write, so both owe the notice. An earlier version called
    // `permissions_lost_on_write` directly here and so reported only half of what a
    // write costs once signatures joined it — which is the reason `save_*` returns the
    // decisions rather than leaving each call site to remember what to ask for.
    let decisions = if linearize {
        doc.save_linearized(&output, "2.0", &save_options).map_err(|e| anyhow::anyhow!("{e:?}"))?
    } else {
        doc.save_with_options(&output, "2.0", &save_options)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?
    };
    report_write_decisions(&decisions);
    println!("SUCCESS: Output saved to {}", output.display());
    Ok(())
}

pub fn handle_render(
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
    let ingest_options: fepdf::IngestionOptions = ingest.into();
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
            system_fonts.insert(fepdf::FallbackFontType::Serif, std::sync::Arc::new(data));
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
            system_fonts.insert(fepdf::FallbackFontType::SansSerif, arc.clone());
            system_fonts.entry(fepdf::FallbackFontType::Default).or_insert(arc);
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

#[allow(clippy::too_many_arguments)]
pub fn handle_sign(
    input: PathBuf,
    output: PathBuf,
    certificate: PathBuf,
    private_key: PathBuf,
    reason: Option<String>,
    location: Option<String>,
    name: Option<String>,
    page: usize,
    ingest: IngestArgs,
    save: SaveArgs,
) -> Result<()> {
    println!("fepdf sign: {} -> {}", input.display(), output.display());
    let data = std::fs::read(&input).with_context(|| "Failed to read input")?;
    let ingest_options: fepdf::IngestionOptions = ingest.into();
    let doc = PdfDocument::open_with_options(data.into(), &ingest_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let sign_options = fepdf::SignOptions {
        reason,
        location,
        name,
        page_index: page.saturating_sub(1),
        certificate: Some(
            std::fs::read(&certificate).with_context(|| "Failed to read the certificate")?,
        ),
        private_key: Some(
            std::fs::read(&private_key).with_context(|| "Failed to read the private key")?,
        ),
        ..Default::default()
    };

    save.check()?;
    let save_options: fepdf::SaveOptions = save.into();
    let decisions = doc
        .save_signed(&output, "2.0", &save_options, &sign_options)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;

    println!("SUCCESS: Signed document saved to {}", output.display());
    // The signature covers the output, so anything the write gave up is inside what was
    // signed. A caller has to see that before trusting the file.
    report_write_decisions(&decisions);
    Ok(())
}

pub fn handle_verify_signature(input: PathBuf, ingest: IngestArgs) -> Result<()> {
    println!("fepdf verify-signature: {}", input.display());
    let _ = ingest;
    let data = std::fs::read(&input).with_context(|| "Failed to read input PDF")?;
    // The byte layer, not `PdfDocument`: `/ByteRange` names offsets into this file, and
    // a `Document` has already normalised them out of existence (ADR-0013).
    let report = fepdf::SignatureReport::survey(&data).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    println!("\n--- [ SIGNATURES (12.8) ] ---");
    if report.signatures.is_empty() {
        println!("  no signature — {} unsigned signature fields", report.unsigned_fields);
    }
    for (n, s) in report.signatures.iter().enumerate() {
        let field = s.field.clone().unwrap_or_else(|| format!("(unnamed field {})", n + 1));
        match &s.refused {
            None => println!("  {field}: verifies"),
            Some(why) => println!("  {field}: REFUSED — {why}"),
        }
        if let Some(signer) = &s.signer {
            println!("    signer                   {signer}");
        }
        if let Some(sub_filter) = &s.sub_filter {
            println!("    /SubFilter               {sub_filter}");
        }
        if let Some(at) = &s.signed_at {
            println!("    /M                       {at} (the document's word)");
        }
        let (covered, total) = s.covered;
        if s.covers_whole_file {
            println!("    covers                   the whole file, {covered} of {total} bytes");
        } else {
            println!(
                "    covers                   {covered} of {total} bytes — NOT the whole file"
            );
        }
    }
    if report.unsigned_fields > 0 && !report.signatures.is_empty() {
        println!("  {} further signature fields hold no signature", report.unsigned_fields);
    }

    // What was not asked is as important as what was. A verified signature here says
    // the bytes have not changed since it was made and that it is bound to the
    // certificate it carries; it says nothing about whether that certificate should be
    // believed, which needs a trust store this engine does not have.
    println!("\n  Not checked: whether the certificate is trusted, was valid when it");
    println!("  signed, or has since been revoked.");

    render_decisions_text(&report.decisions);
    Ok(())
}
