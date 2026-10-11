//! fepdf: The Universal PDF Toolkit.
//!
//! (ISO 32000-2:2020 Compliance & Optimization Engine)

mod args;
mod commands;
mod formatters;
mod util;

use anyhow::Result;
use clap::Parser;

use crate::args::{
    Cli, Commands, DebugSubcommands, EditSubcommands, InspectSubcommands, PublishSubcommands,
};
use crate::commands::{credits, debug, edit, inspect, publish};

#[tokio::main]
async fn main() -> Result<()> {
    // RR-15 Limit: Dispatcher - CLIs top level command dispatcher routing to handlers
    env_logger::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect { sub } => match sub {
            InspectSubcommands::Info { input, format, ingest } => {
                inspect::handle_info(input, format, ingest)?;
            }
            InspectSubcommands::Audit { input, format, ingest } => {
                inspect::handle_audit(input, format, ingest)?;
            }
            InspectSubcommands::Text { input, pages, ingest } => {
                inspect::handle_text(input, pages, ingest)?;
            }
            InspectSubcommands::Catalog { input, format, all } => {
                inspect::handle_catalog(&input, &format, all)?;
            }
            InspectSubcommands::Encryption {
                input,
                format,
                password,
                certificate,
                private_key,
            } => {
                inspect::handle_encryption(&input, &format, &password, certificate, private_key)?;
            }
            InspectSubcommands::Interactive { input, format } => {
                inspect::handle_interactive(&input, &format)?;
            }
            InspectSubcommands::Structure { input, format } => {
                inspect::handle_structure(&input, &format)?;
            }
            InspectSubcommands::Actions { input, format, full } => {
                inspect::handle_actions(&input, &format, full)?;
            }
            InspectSubcommands::Coverage { inputs, format, unread } => {
                inspect::handle_coverage(&inputs, &format, unread)?;
            }
            InspectSubcommands::Tree { input, ingest } => {
                inspect::handle_tree(input, ingest)?;
            }
        },
        Commands::Edit { sub } => match sub {
            EditSubcommands::Merge { inputs, output, ingest, save } => {
                edit::handle_merge(inputs, output, ingest, save)?;
            }
            EditSubcommands::Split { input, output, pages, ingest, save } => {
                edit::handle_split(input, output, pages, ingest, save)?;
            }
            EditSubcommands::Rotate { input, output, pages, angle, ingest, save } => {
                edit::handle_rotate(input, output, pages, angle, ingest, save)?;
            }
            EditSubcommands::Repair { input, output, ingest, save } => {
                edit::handle_repair(input, output, ingest, save)?;
            }
            EditSubcommands::Tag { input, output, wizard, ingest, save } => {
                edit::handle_retag(input, output, wizard, ingest, save)?;
            }
            EditSubcommands::Portfolio { output, files, cover, ingest, save } => {
                edit::handle_portfolio(output, files, cover, ingest, save)?;
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
                edit::handle_bates(input, output, prefix, start_number, digits, ingest, save)?;
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
                edit::handle_attach(input, output, file, relationship, mime_type, ingest, save)?;
            }
            EditSubcommands::Images { images, output, into, at, sheet, ingest, save } => {
                let placing = edit::Placing { into, at, sheet };
                edit::handle_images(&images, &output, placing, ingest, save)?;
            }
            EditSubcommands::FdfExport { input, output, ingest } => {
                edit::handle_fdf_export(&input, &output, ingest)?;
            }
            EditSubcommands::FdfImport { input, fdf, output, ingest, save } => {
                edit::handle_fdf_import(&input, &fdf, &output, ingest, save)?;
            }
            EditSubcommands::PageLabel { input, output, style, prefix, ingest, save } => {
                edit::handle_page_label(input, output, style, prefix, ingest, save)?;
            }
            EditSubcommands::Geo { input, output, lat, lon, crs, ingest, save } => {
                edit::handle_geo(input, output, lat, lon, crs, ingest, save)?;
            }
            EditSubcommands::TextLayer { input, output, page, json, ingest, save } => {
                edit::handle_text_layer(&input, &output, page, &json, ingest, save)?;
            }
        },
        Commands::Publish { sub } => match sub {
            PublishSubcommands::Upgrade {
                input,
                output,
                standard,
                icc_profile,
                linearize,
                ingest,
                save,
            } => {
                publish::handle_upgrade(
                    input,
                    output,
                    standard,
                    icc_profile,
                    linearize,
                    ingest,
                    save,
                )?;
            }
            PublishSubcommands::Render { input, output, page, cpu, ingest } => {
                publish::handle_render(input, output, page, cpu, ingest)?;
            }
            PublishSubcommands::Sign { sign, ingest, save } => {
                publish::handle_sign(sign, ingest, save)?;
            }
            PublishSubcommands::VerifySignature { input, ingest } => {
                publish::handle_verify_signature(input, ingest)?;
            }
        },
        Commands::Debug { sub } => match sub {
            DebugSubcommands::Dump { input, obj, gen_num, ingest } => {
                debug::handle_debug_dump(input, obj, gen_num, ingest)?;
            }
            DebugSubcommands::Stats { input, ingest } => {
                debug::handle_debug_stats(input, ingest)?;
            }
            DebugSubcommands::FontExtract { input, obj_num, output, ingest } => {
                debug::handle_extract_font(input, obj_num, output, ingest)?;
            }
            DebugSubcommands::TraceGlyph { input, unicode, font, ingest } => {
                debug::handle_debug_trace_glyph(input, unicode, font, ingest)?;
            }
        },
        Commands::Credits => {
            credits::handle_credits()?;
        }
    }

    Ok(())
}
