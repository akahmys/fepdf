//! fepdf: the flagship desktop PDF 2.0 editor.
//!
//! Builds on `egui` + `eframe` + `wgpu`, rendering page content through the Vello
//! compute rasteriser in [`fepdf-render`]. Document work runs on a background
//! worker thread ([`worker`]) so the canvas stays responsive.
//!
//! # Lint policy
//!
//! Every suppression below is crate-wide because the underlying cause is crate-wide.
//! Anything narrower is suppressed at the individual site with its own justification;
//! see `CODING.md` for the RR-15 rules these sit under.

// PDF user space is `f64` (ISO 32000-2) while egui and Vello work in `f32` screen
// space. Narrowing at that boundary is intentional and occurs at ~45 call sites.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless
)]
// egui convention: widget helpers take `&mut egui::Ui` even when the current body only
// paints, so that adding an `ui.add(..)` call later is not a breaking signature change.
#![allow(clippy::needless_pass_by_ref_mut)]
// Panel and dispatcher entry points thread whole-application state through a single
// call per frame. Grouping the arguments into structs would only move the same field
// list one frame down the stack without reducing coupling.
#![allow(
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    clippy::ref_option
)]
// `eframe::Error` is large and owned by the framework; `main` must return it as-is.
#![allow(clippy::result_large_err)]

mod app;
mod cad_canvas;
mod command_palette;
mod export_wizard;
mod inspector;
mod interaction;
mod locale;
mod redaction;
mod redaction_studio;
mod sidebar;
mod vello_egui;
mod view;
mod worker;

use app::FepdfApp;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    env_logger::init();

    let pdf_path = std::env::args().nth(1).map(PathBuf::from);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 900.0])
            .with_title("fepdf"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "fepdf-gui",
        native_options,
        Box::new(|cc| {
            let mut app = FepdfApp::new(cc);
            if let Some(path) = pdf_path {
                app.open_file(path, &cc.egui_ctx);
            }
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
}
