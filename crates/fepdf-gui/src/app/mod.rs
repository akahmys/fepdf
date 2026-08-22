//! Central state container and egui UI dispatch loop for `fepdf-gui`.

mod layout;
mod modals;
mod navigation_bar;
mod page_ops;
mod side_panels;
mod status_bar;
pub mod theme;
mod view_panel;

use crate::interaction::{SelectionManager, TextSpan};
use crate::redaction::RedactionManager;
use crate::sidebar::{SidebarPanel, USTRegistry};
use crate::vello_egui::VelloRenderer;
use crate::view::{PDFView, PageLayout};
use crate::worker::{WorkerRequest, WorkerResponse, run_worker};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use vello::Scene;

pub struct FepdfApp {
    pub tx_worker: Sender<WorkerRequest>,
    pub rx_worker: Receiver<WorkerResponse>,

    pub total_pages: usize,
    pub page_layouts: Vec<PageLayout>,

    pub view: PDFView,
    pub error: Option<String>,
    pub pdf_name: Option<String>,

    pub vello_renderer: Option<VelloRenderer>,
    pub scenes: BTreeMap<usize, Arc<Scene>>,
    pub request_queue: BTreeSet<usize>,

    pub selection_manager: SelectionManager,
    pub page_spans: BTreeMap<usize, Vec<TextSpan>>,

    pub ust_registry: USTRegistry,
    pub sidebar_panel: SidebarPanel,

    pub redaction_manager: RedactionManager,
    pub redaction_studio_panel: crate::redaction_studio::RedactionStudioPanel,
    pub show_redaction_studio: bool,
    pub show_export_wizard: bool,
    pub export_compress: bool,
    pub export_linearize: bool,
    pub export_vacuum: bool,
    pub export_upgrade_pdf20: bool,
    pub export_apply_tags: bool,
    pub export_burn_redactions: bool,
    pub raw_texts: BTreeMap<usize, String>, // page_index -> raw extracted text

    // Digital Signature & Placement
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub signature_position: Option<(usize, egui::Rect)>, // (page_index, rect in PDF user space)
    pub is_placing_signature: bool,

    // CAD snappers & Inspector
    pub cad_snap_engine: crate::cad_canvas::CadSnapEngine,
    pub caliper_tool: crate::cad_canvas::CaliperTool,
    pub arlington_inspector: crate::inspector::ArlingtonInspectorPanel,
    pub show_inspector: bool,

    // Selection management
    pub selected_pages: BTreeSet<usize>,
    pub last_selected_page: Option<usize>,
    pub clear_thumbnails_pending: bool,
    pub invalidated_thumbnails: BTreeSet<usize>,
    pub is_loading: bool,
    pub loading_message: String,
    pub show_reading_order: bool,
    pub show_command_palette: bool,
    pub command_palette_search: String,
    pub last_viewport_rect: Option<egui::Rect>,
    pub show_about_modal: bool,
    pub locale_mgr: crate::locale::LocaleManager,
    pub active_language: String,
    pub show_settings_modal: bool,
    pub doc_metadata: Option<fepdf::MetadataInfo>,
    pub doc_file_size: Option<usize>,
    pub doc_version: Option<String>,
    pub doc_security_method: Option<String>,
    pub doc_permissions: Option<i32>,
    pub doc_page_sizes: Vec<(f64, f64)>,
    pub doc_fonts: Vec<fepdf::FontSummary>,
    /// What to present for optional content (6.3.2.3), refreshed whenever a layer is
    /// toggled so the checkboxes show the state actually in force.
    pub layers: Vec<fepdf::LayerRow>,
}

impl FepdfApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // RR-15 Limit: GUI - App state creation and initialization
        let vello_renderer =
            cc.wgpu_render_state.as_ref().and_then(|rs| VelloRenderer::new(&rs.device));
        let (tx_req, rx_req) = channel();
        let (tx_res, rx_res) = channel();

        // Configure system fonts, icon font, and visual theme
        theme::configure_fonts_and_styles(&cc.egui_ctx);

        let egui_ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            run_worker(rx_req, tx_res, egui_ctx);
        });

        Self {
            tx_worker: tx_req,
            rx_worker: rx_res,
            total_pages: 0,
            page_layouts: Vec::new(),
            view: PDFView::new(),
            error: None,
            pdf_name: None,
            vello_renderer,
            scenes: BTreeMap::new(),
            request_queue: BTreeSet::new(),
            selection_manager: SelectionManager::new(),
            page_spans: BTreeMap::new(),
            ust_registry: USTRegistry::new(),
            sidebar_panel: SidebarPanel::new(),
            redaction_manager: RedactionManager::new(),
            redaction_studio_panel: crate::redaction_studio::RedactionStudioPanel::new(),
            show_redaction_studio: false,
            show_export_wizard: false,
            export_compress: true,
            export_linearize: true,
            export_vacuum: true,
            export_upgrade_pdf20: true,
            export_apply_tags: true,
            export_burn_redactions: true,
            raw_texts: BTreeMap::new(),

            // Signature Defaults
            cert_path: None,
            key_path: None,
            signature_position: None,
            is_placing_signature: false,

            // CAD & Inspector Defaults
            cad_snap_engine: crate::cad_canvas::CadSnapEngine::new(),
            caliper_tool: crate::cad_canvas::CaliperTool::new(),
            arlington_inspector: crate::inspector::ArlingtonInspectorPanel::new(),
            show_inspector: false,

            // Selection Defaults
            selected_pages: BTreeSet::new(),
            last_selected_page: None,
            clear_thumbnails_pending: false,
            invalidated_thumbnails: BTreeSet::new(),
            is_loading: false,
            loading_message: String::new(),
            show_reading_order: true,
            show_command_palette: false,
            command_palette_search: String::new(),
            last_viewport_rect: None,
            show_about_modal: false,
            locale_mgr: crate::locale::LocaleManager::new(),
            active_language: "ja".to_string(),
            show_settings_modal: false,
            doc_metadata: None,
            doc_file_size: None,
            doc_version: None,
            doc_security_method: None,
            doc_permissions: None,
            doc_page_sizes: Vec::new(),
            doc_fonts: Vec::new(),
            layers: Vec::new(),
        }
    }

    fn process_worker_messages(&mut self, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Handle asynchronous background messages
        while let Ok(msg) = self.rx_worker.try_recv() {
            match msg {
                WorkerResponse::LoadingProgress { message } => {
                    self.loading_message = message;
                    ctx.request_repaint();
                }
                WorkerResponse::LayersChanged { layers } => {
                    // The page must be drawn again: a layer's state decides what the
                    // interpreter paints, and every cached scene predates the toggle.
                    self.layers = layers;
                    self.scenes.clear();
                    self.raw_texts.clear();
                    self.page_spans.clear();
                }
                WorkerResponse::DocumentLoaded(loaded) => {
                    let crate::worker::LoadedDocument {
                        name,
                        num_pages,
                        page_sizes,
                        ust_root,
                        file_size,
                        version,
                        metadata,
                        security_method,
                        permissions,
                        fonts,
                        viewer_direction,
                        layers,
                    } = *loaded;
                    self.layers = layers;
                    if name.is_some() {
                        self.pdf_name = name;
                    }
                    self.total_pages = num_pages;
                    self.scenes.clear();
                    self.raw_texts.clear();
                    self.page_spans.clear();
                    self.clear_thumbnails_pending = true;
                    if let Some(ref dir) = viewer_direction {
                        if dir.eq_ignore_ascii_case("R2L") {
                            self.view.binding_direction =
                                crate::view::BindingDirection::RightToLeft;
                        } else {
                            self.view.binding_direction =
                                crate::view::BindingDirection::LeftToRight;
                        }
                    } else {
                        self.view.binding_direction = crate::view::BindingDirection::LeftToRight;
                    }
                    self.doc_page_sizes = page_sizes;
                    self.compute_layouts();

                    self.doc_file_size = Some(file_size);
                    self.doc_version = Some(version);
                    self.doc_metadata = Some(metadata);
                    self.doc_security_method = Some(security_method);
                    self.doc_permissions = permissions;
                    self.doc_fonts = fonts;

                    // Load parsed accessibility tag tree
                    self.ust_registry.root = ust_root;

                    // Kick off Matterhorn compliance audit asynchronously in the background
                    let _ = self.tx_worker.send(WorkerRequest::Audit);

                    self.is_loading = false;
                    ctx.request_repaint();
                }
                WorkerResponse::PageRendered { index, scene, text, spans, .. } => {
                    self.scenes.insert(index, scene);
                    self.request_queue.remove(&index);
                    self.invalidated_thumbnails.insert(index);

                    if let Some(text) = text {
                        self.raw_texts.insert(index, text);
                    }

                    if let Some(spans) = spans {
                        self.page_spans.insert(index, spans);
                    } else if let Some(text) = self.raw_texts.get(&index)
                        && let Some(layout) = self.page_layouts.get(index)
                    {
                        let size = layout.rect.size();
                        let spans = SelectionManager::generate_spans_for_page(text, size.x, size.y);
                        self.page_spans.insert(index, spans);
                    }

                    ctx.request_repaint();
                }
                WorkerResponse::AuditFindings { findings } => {
                    self.ust_registry.audit_findings = findings;
                    ctx.request_repaint();
                }
                WorkerResponse::DocumentSaved { path, notices } => {
                    let name = path.file_name().unwrap_or(path.as_os_str()).display();
                    self.error = Some(if notices.is_empty() {
                        format!("Successfully exported compliant PDF to {name}")
                    } else {
                        format!("Exported to {name}\n{}", notices.join("\n"))
                    });
                    ctx.request_repaint();
                }
                WorkerResponse::Error(err) => {
                    self.is_loading = false;
                    self.error = Some(err);
                }
            }
        }
    }

    fn update_vello(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // RR-15 Limit: Dispatcher
        let ctx = ui.ctx().clone();
        self.process_worker_messages(&ctx);

        if !self.check_gpu_support(ui, frame) {
            return;
        }

        let rs = match frame.wgpu_render_state() {
            Some(state) => state,
            None => return,
        };

        if self.clear_thumbnails_pending {
            if let Some(ref mut r) = self.vello_renderer {
                r.clear_thumbnails(rs);
            }
            self.clear_thumbnails_pending = false;
        }

        if !self.invalidated_thumbnails.is_empty()
            && let Some(ref mut r) = self.vello_renderer
        {
            for page_idx in std::mem::take(&mut self.invalidated_thumbnails) {
                r.invalidate_thumbnail(rs, page_idx);
            }
        }

        self.queue_visible_pages();

        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            let bg_color = egui::Color32::from_rgb(235, 237, 240);
            ui.painter().rect_filled(ui.max_rect(), 0.0, bg_color);

            if let Some(err) = &self.error {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(egui::Color32::RED, err);
                });
            } else if !self.page_layouts.is_empty() {
                let viewport_rect = ui.max_rect();
                self.last_viewport_rect = Some(viewport_rect);
                self.render_document_panel(ui, rs, viewport_rect);
                self.render_floating_navigation_bar(ui, viewport_rect);
            } else if self.is_loading {
                ui.centered_and_justified(|ui| {
                    ui.label(&self.loading_message);
                });
            } else {
                // Keep the central panel blank at startup as requested
            }
        });
    }
}

impl eframe::App for FepdfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // RR-15 Limit: Dispatcher - Main application UI shell layout routing layout panels and windows
        let ctx = ui.ctx().clone();
        theme::apply_global_styles(&ctx);

        // Ensure the style overrides are active on the root UI visuals immediately
        let visuals = ui.visuals_mut();
        visuals.selection.stroke = egui::Stroke::NONE;
        visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(120, 125, 135, 45);
        visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;

        let entire_rect = ui.max_rect();
        ui.painter().rect_filled(entire_rect, 0.0, ui.visuals().window_fill);

        self.render_left_side_panels(ui);
        if self.view.scroll_direction == crate::view::ScrollDirection::Vertical {
            crate::thumbnail_sidebar::ThumbnailSidebar::show(self, ui, frame);
        } else {
            crate::thumbnail_sidebar::ThumbnailSidebar::show_horizontal(self, ui, frame);
        }
        self.render_status_bar(ui);

        self.update_vello(ui, frame);
        self.render_overlay_windows(&ctx);
    }
}
