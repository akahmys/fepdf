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

/// The window and the wgpu device the viewer asks for.
///
/// **The limits are taken from the adapter rather than from wgpu's defaults.** A page
/// whose scene exceeds the default storage-buffer binding comes back as a fully
/// transparent texture with `render_to_texture` reporting success — `headless.rs` carries
/// the same story for `samples/volvo_xc90.pdf` pages 10 and 389 — so the device is built
/// with whatever the adapter will actually give.
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 900.0])
            .with_title("fepdf"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(egui_wgpu::WgpuSetupCreateNew {
                instance_descriptor: wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::all(),
                    flags: wgpu::InstanceFlags::default(),
                    backend_options: wgpu::BackendOptions::default(),
                    display: None,
                    memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                },
                power_preference: wgpu::PowerPreference::HighPerformance,
                native_adapter_selector: None,
                display_handle: None,
                device_descriptor: std::sync::Arc::new(|adapter| {
                    let adapter_limits = adapter.limits();
                    wgpu::DeviceDescriptor {
                        label: Some("fepdf wgpu device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits {
                            max_buffer_size: adapter_limits.max_buffer_size,
                            max_storage_buffer_binding_size: adapter_limits
                                .max_storage_buffer_binding_size,
                            max_storage_buffers_per_shader_stage: adapter_limits
                                .max_storage_buffers_per_shader_stage,
                            max_compute_workgroups_per_dimension: adapter_limits
                                .max_compute_workgroups_per_dimension,
                            max_storage_textures_per_shader_stage: adapter_limits
                                .max_storage_textures_per_shader_stage,
                            ..wgpu::Limits::default()
                        },
                        memory_hints: wgpu::MemoryHints::Performance,
                        experimental_features: wgpu::ExperimentalFeatures::default(),
                        trace: wgpu::Trace::default(),
                    }
                }),
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let pdf_path = std::env::args().nth(1).map(PathBuf::from);

    let native_options = native_options();

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
