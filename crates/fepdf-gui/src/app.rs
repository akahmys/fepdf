//! Central state container and egui UI dispatch loop for `fepdf-gui`.

use crate::interaction::{SelectionManager, TextSpan};
use crate::redaction::RedactionManager;
use crate::sidebar::{SidebarPanel, USTRegistry};
use crate::vello_egui::VelloRenderer;
use crate::view::{DisplayMode, PDFView, PageLayout};
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
    pub cert_password: String,
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
    pub doc_metadata: Option<fepdf_sdk::MetadataInfo>,
    pub doc_file_size: Option<usize>,
    pub doc_version: Option<String>,
    pub doc_security_method: Option<String>,
    pub doc_permissions: Option<i32>,
    pub doc_page_sizes: Vec<(f64, f64)>,
    pub doc_fonts: Vec<fepdf_sdk::FontSummary>,
}

impl FepdfApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // RR-15 Limit: GUI - App state creation and initialization
        let vello_renderer =
            cc.wgpu_render_state.as_ref().and_then(|rs| VelloRenderer::new(&rs.device));
        let (tx_req, rx_req) = channel();
        let (tx_res, rx_res) = channel();

        // Load system CJK/Japanese fonts for egui to support Japanese characters properly
        let mut fonts = egui::FontDefinitions::default();

        // Load Lucide icon font
        let lucide_data = include_bytes!("../assets/lucide.ttf");
        fonts
            .font_data
            .insert("lucide".to_owned(), egui::FontData::from_static(lucide_data).into());
        if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            families.push("lucide".to_owned());
        }
        if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            families.push("lucide".to_owned());
        }

        let mut paths = vec![
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf".to_owned(),
            "/System/Library/Fonts/Hiragino Sans GB.ttc".to_owned(),
            "/Library/Fonts/Arial Unicode.ttf".to_owned(),
        ];

        if let Ok(win_dir) = std::env::var("windir") {
            paths.push(format!(r"{win_dir}\Fonts\msgothic.ttc"));
            paths.push(format!(r"{win_dir}\Fonts\yugothm.ttc"));
            paths.push(format!(r"{win_dir}\Fonts\meiryo.ttc"));
        } else {
            paths.push(r"C:\Windows\Fonts\msgothic.ttc".to_owned());
            paths.push(r"C:\Windows\Fonts\yugothm.ttc".to_owned());
            paths.push(r"C:\Windows\Fonts\meiryo.ttc".to_owned());
        }

        paths.push("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf".to_owned());
        paths.push("/usr/share/fonts/opentype/ipafont-gothic/ipag.otf".to_owned());
        paths.push("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc".to_owned());
        paths.push("/usr/share/fonts/TTF/NotoSansCJK-Regular.ttc".to_owned());

        for path in &paths {
            if let Ok(font_data) = std::fs::read(path) {
                log::info!("Successfully loaded CJK font from {path}");
                fonts
                    .font_data
                    .insert("cjk".to_owned(), egui::FontData::from_owned(font_data).into());
                if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    families.insert(0, "cjk".to_owned());
                }
                if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                    families.insert(0, "cjk".to_owned());
                }
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_visuals(egui::Visuals::light());

        cc.egui_ctx.global_style_mut(|style| {
            style.visuals.selection.stroke = egui::Stroke::NONE;
            style.visuals.selection.bg_fill =
                egui::Color32::from_rgba_unmultiplied(120, 125, 135, 45);
            style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0_f32, egui::Color32::from_gray(210));
        });

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
            cert_password: String::new(),
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
            active_language: "ja".to_string(), // Defaulting to Japanese since the user requested in Japanese
            show_settings_modal: false,
            doc_metadata: None,
            doc_file_size: None,
            doc_version: None,
            doc_security_method: None,
            doc_permissions: None,
            doc_page_sizes: Vec::new(),
            doc_fonts: Vec::new(),
        }
    }

    fn inject_tag_to_tree(&mut self, tag: &str, req: &crate::interaction::PendingTagRequest) {
        let new_node = crate::sidebar::USTNode {
            id: self.ust_registry.next_node_id,
            tag: tag.to_string(),
            title: if req.text.len() > 30 {
                format!("{}...", &req.text[..30])
            } else {
                req.text.clone()
            },
            alt_text: if tag == "Figure" { Some(req.text.clone()) } else { None },
            rect: Some([
                req.combined_rect.min.x,
                req.combined_rect.min.y,
                req.combined_rect.max.x,
                req.combined_rect.max.y,
            ]),
            page_index: Some(req.page_index),
            handle_index: None,
            children: Vec::new(),
        };
        self.ust_registry.next_node_id += 1;

        if let Some(ref mut root) = self.ust_registry.root {
            root.children.push(new_node);
        }

        self.error = Some(format!("Successfully created <{tag}> tag"));
    }

    pub fn open_file(&mut self, path: PathBuf, ctx: &egui::Context) {
        if let Ok(bytes) = std::fs::read(&path) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            self.open_file_bytes(bytes::Bytes::from(bytes), name, ctx);
        }
    }

    pub fn open_file_bytes(
        &mut self,
        data: bytes::Bytes,
        name: Option<String>,
        ctx: &egui::Context,
    ) {
        self.error = None;
        self.total_pages = 0;
        self.page_layouts.clear();
        self.scenes.clear();
        self.request_queue.clear();
        self.selection_manager.clear();
        self.page_spans.clear();
        self.ust_registry.clear();
        self.selected_pages.clear();
        self.last_selected_page = None;
        self.clear_thumbnails_pending = true;
        self.is_loading = true;
        self.loading_message = "1/4: Decrypting and normalizing document...".to_string();
        self.doc_metadata = None;
        self.doc_file_size = None;
        self.doc_version = None;
        self.doc_security_method = None;
        self.doc_permissions = None;
        self.doc_page_sizes.clear();
        self.doc_fonts.clear();
        self.reset_view();
        let _ = self.tx_worker.send(WorkerRequest::Open { data, name });
        ctx.request_repaint();
    }

    pub fn reset_view(&mut self) {
        self.view.zoom = 1.0;
        self.view.pan = egui::Vec2::ZERO;
    }

    pub fn fit_to_width(&mut self, viewport_rect: egui::Rect) {
        let current_page =
            self.view.visible_pages.first().copied().unwrap_or(self.view.active_page);
        let mut indices = vec![current_page];
        if self.view.display_mode == crate::view::DisplayMode::TwoPageSpread
            || self.view.display_mode == crate::view::DisplayMode::TwoPageSingle
        {
            if self.view.cover_page_alone {
                if current_page > 0 {
                    let pair_start = ((current_page - 1) / 2) * 2 + 1;
                    indices = vec![pair_start];
                    if pair_start + 1 < self.total_pages {
                        indices.push(pair_start + 1);
                    }
                }
            } else {
                let pair_start = (current_page / 2) * 2;
                indices = vec![pair_start];
                if pair_start + 1 < self.total_pages {
                    indices.push(pair_start + 1);
                }
            }
        }

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        for &idx in &indices {
            if let Some(layout) = self.page_layouts.get(idx) {
                min_x = min_x.min(layout.rect.min.x);
                max_x = max_x.max(layout.rect.max.x);
            }
        }

        let spread_w = max_x - min_x;
        if spread_w > 0.0 && min_x < f32::MAX {
            let target_zoom = (viewport_rect.width() - 40.0) / spread_w;
            self.view.zoom = target_zoom.clamp(0.1, 10.0);
            self.view.pan.x = 0.0;
        }
    }

    pub fn fit_to_height(&mut self, viewport_rect: egui::Rect) {
        let current_page =
            self.view.visible_pages.first().copied().unwrap_or(self.view.active_page);
        let mut indices = vec![current_page];
        if self.view.display_mode == crate::view::DisplayMode::TwoPageSpread
            || self.view.display_mode == crate::view::DisplayMode::TwoPageSingle
        {
            if self.view.cover_page_alone {
                if current_page > 0 {
                    let pair_start = ((current_page - 1) / 2) * 2 + 1;
                    indices = vec![pair_start];
                    if pair_start + 1 < self.total_pages {
                        indices.push(pair_start + 1);
                    }
                }
            } else {
                let pair_start = (current_page / 2) * 2;
                indices = vec![pair_start];
                if pair_start + 1 < self.total_pages {
                    indices.push(pair_start + 1);
                }
            }
        }

        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for &idx in &indices {
            if let Some(layout) = self.page_layouts.get(idx) {
                min_y = min_y.min(layout.rect.min.y);
                max_y = max_y.max(layout.rect.max.y);
            }
        }

        let spread_h = max_y - min_y;
        if spread_h > 0.0 && min_y < f32::MAX {
            let target_zoom = (viewport_rect.height() - 40.0) / spread_h;
            self.view.zoom = target_zoom.clamp(0.1, 10.0);
            if self.view.display_mode == DisplayMode::Continuous
                || self.view.display_mode == DisplayMode::TwoPageSpread
                || self.view.display_mode == DisplayMode::TwoPageSingle
            {
                self.view.pan.y = -min_y * self.view.zoom;
            } else {
                self.view.pan.y = 0.0;
            }
            self.view.pan.x = 0.0;
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
                    } = *loaded;
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
                WorkerResponse::DocumentSaved { path } => {
                    self.error = Some(format!(
                        "Successfully exported compliant PDF to {}",
                        path.file_name().unwrap_or(path.as_os_str()).display()
                    ));
                    ctx.request_repaint();
                }
                WorkerResponse::Error(err) => {
                    self.is_loading = false;
                    self.error = Some(err);
                }
            }
        }
    }

    /// Recomputes page rectangles from `doc_self.doc_page_sizes` and the current view mode.
    ///
    /// Reads the sizes from `self` rather than taking them as a parameter: every
    /// caller passed `self.doc_self.doc_page_sizes`, and doing so through a `&mut self`
    /// method forced a clone of the whole vector at each of the fourteen call
    /// sites -- one entry per page, on every layout change.
    fn compute_layouts(&mut self) {
        // RR-15 Limit: GUI
        use crate::view::{BindingDirection, DisplayMode, ScrollDirection};
        let mut layouts =
            vec![PageLayout { index: 0, rect: egui::Rect::NOTHING }; self.doc_page_sizes.len()];

        if self.view.display_mode == DisplayMode::TwoPageSpread
            || self.view.display_mode == DisplayMode::TwoPageSingle
        {
            let mut current_offset = 0.0;
            let gap = 20.0;
            let inner_gap = 8.0;
            let mut i = if self.view.cover_page_alone && !self.doc_page_sizes.is_empty() {
                let (w, h) = self.doc_page_sizes[0];
                let w = w as f32;
                let h = h as f32;
                let rect = if self.view.scroll_direction == ScrollDirection::Vertical {
                    egui::Rect::from_min_size(
                        egui::pos2(-w / 2.0, current_offset),
                        egui::vec2(w, h),
                    )
                } else {
                    egui::Rect::from_min_size(
                        egui::pos2(current_offset, -h / 2.0),
                        egui::vec2(w, h),
                    )
                };
                layouts[0] = PageLayout { index: 0, rect };
                if self.view.scroll_direction == ScrollDirection::Vertical {
                    current_offset += h + gap;
                } else {
                    current_offset += w + gap;
                }
                1
            } else {
                0
            };

            while i < self.doc_page_sizes.len() {
                if i + 1 < self.doc_page_sizes.len() {
                    let (w1, h1) = self.doc_page_sizes[i];
                    let (w2, h2) = self.doc_page_sizes[i + 1];
                    let w1 = w1 as f32;
                    let w2 = w2 as f32;
                    let h1 = h1 as f32;
                    let h2 = h2 as f32;

                    let total_w = w1 + w2 + inner_gap;
                    let max_h = h1.max(h2);

                    let (rect1, rect2) = if self.view.scroll_direction == ScrollDirection::Vertical
                    {
                        let y1 = current_offset + (max_h - h1) / 2.0;
                        let y2 = current_offset + (max_h - h2) / 2.0;
                        if self.view.binding_direction == BindingDirection::RightToLeft {
                            let r1 = egui::Rect::from_min_size(
                                egui::pos2(inner_gap / 2.0, y1),
                                egui::vec2(w1, h1),
                            );
                            let r2 = egui::Rect::from_min_size(
                                egui::pos2(-total_w / 2.0, y2),
                                egui::vec2(w2, h2),
                            );
                            (r1, r2)
                        } else {
                            let r1 = egui::Rect::from_min_size(
                                egui::pos2(-total_w / 2.0, y1),
                                egui::vec2(w1, h1),
                            );
                            let r2 = egui::Rect::from_min_size(
                                egui::pos2(inner_gap / 2.0, y2),
                                egui::vec2(w2, h2),
                            );
                            (r1, r2)
                        }
                    } else {
                        let x1 = current_offset;
                        if self.view.binding_direction == BindingDirection::RightToLeft {
                            let r1 = egui::Rect::from_min_size(
                                egui::pos2(x1 + w2 + inner_gap, -h1 / 2.0),
                                egui::vec2(w1, h1),
                            );
                            let r2 = egui::Rect::from_min_size(
                                egui::pos2(x1, -h2 / 2.0),
                                egui::vec2(w2, h2),
                            );
                            (r1, r2)
                        } else {
                            let r1 = egui::Rect::from_min_size(
                                egui::pos2(x1, -h1 / 2.0),
                                egui::vec2(w1, h1),
                            );
                            let r2 = egui::Rect::from_min_size(
                                egui::pos2(x1 + w1 + inner_gap, -h2 / 2.0),
                                egui::vec2(w2, h2),
                            );
                            (r1, r2)
                        }
                    };

                    layouts[i] = PageLayout { index: i, rect: rect1 };
                    layouts[i + 1] = PageLayout { index: i + 1, rect: rect2 };

                    if self.view.scroll_direction == ScrollDirection::Vertical {
                        current_offset += max_h + gap;
                    } else {
                        current_offset += total_w + gap;
                    }
                    i += 2;
                } else {
                    let (w, h) = self.doc_page_sizes[i];
                    let w = w as f32;
                    let h = h as f32;
                    let rect = if self.view.scroll_direction == ScrollDirection::Vertical {
                        egui::Rect::from_min_size(
                            egui::pos2(-w / 2.0, current_offset),
                            egui::vec2(w, h),
                        )
                    } else {
                        egui::Rect::from_min_size(
                            egui::pos2(current_offset, -h / 2.0),
                            egui::vec2(w, h),
                        )
                    };
                    layouts[i] = PageLayout { index: i, rect };
                    if self.view.scroll_direction == ScrollDirection::Vertical {
                        current_offset += h + gap;
                    } else {
                        current_offset += w + gap;
                    }
                    i += 1;
                }
            }
        } else if self.view.display_mode == DisplayMode::SinglePage {
            for (i, &(w, h)) in self.doc_page_sizes.iter().enumerate() {
                let w = w as f32;
                let h = h as f32;
                let rect = if self.view.scroll_direction == ScrollDirection::Vertical {
                    egui::Rect::from_min_size(egui::pos2(-w / 2.0, 0.0), egui::vec2(w, h))
                } else {
                    egui::Rect::from_min_size(egui::pos2(0.0, -h / 2.0), egui::vec2(w, h))
                };
                layouts[i] = PageLayout { index: i, rect };
            }
        } else {
            let mut current_offset = 0.0;
            let gap = 20.0;
            for (i, &(w, h)) in self.doc_page_sizes.iter().enumerate() {
                let w = w as f32;
                let h = h as f32;
                let rect = if self.view.scroll_direction == ScrollDirection::Vertical {
                    let r = egui::Rect::from_min_size(
                        egui::pos2(-w / 2.0, current_offset),
                        egui::vec2(w, h),
                    );
                    current_offset += h + gap;
                    r
                } else {
                    let r = egui::Rect::from_min_size(
                        egui::pos2(current_offset, -h / 2.0),
                        egui::vec2(w, h),
                    );
                    current_offset += w + gap;
                    r
                };
                layouts[i] = PageLayout { index: i, rect };
            }
        }
        self.page_layouts = layouts;
    }

    pub fn reorder_page(&mut self, from: usize, to: usize) {
        if from >= self.total_pages || to >= self.total_pages || from == to {
            return;
        }

        if from < self.doc_page_sizes.len() && to < self.doc_page_sizes.len() {
            let item = self.doc_page_sizes.remove(from);
            self.doc_page_sizes.insert(to, item);
        }

        self.scenes.clear();
        self.raw_texts.clear();
        self.page_spans.clear();
        self.clear_thumbnails_pending = true;

        self.compute_layouts();

        let mut new_selected = BTreeSet::new();
        for &idx in &self.selected_pages {
            if idx == from {
                new_selected.insert(to);
            } else if from < to && idx > from && idx <= to {
                new_selected.insert(idx - 1);
            } else if to < from && idx >= to && idx < from {
                new_selected.insert(idx + 1);
            } else {
                new_selected.insert(idx);
            }
        }
        self.selected_pages = new_selected;

        if let Some(last) = self.last_selected_page {
            if last == from {
                self.last_selected_page = Some(to);
            } else if from < to && last > from && last <= to {
                self.last_selected_page = Some(last - 1);
            } else if to < from && last >= to && last < from {
                self.last_selected_page = Some(last + 1);
            }
        }

        let _ = self.tx_worker.send(WorkerRequest::ReorderPages { from, to });
    }

    pub fn duplicate_page(&mut self, index: usize) {
        if index >= self.total_pages {
            return;
        }

        let page_size = self.doc_page_sizes.get(index).copied().unwrap_or((595.0, 842.0));
        self.doc_page_sizes.insert(index + 1, page_size);
        self.total_pages += 1;

        self.scenes.clear();
        self.raw_texts.clear();
        self.page_spans.clear();
        self.clear_thumbnails_pending = true;

        self.compute_layouts();

        self.selected_pages.clear();
        self.selected_pages.insert(index + 1);
        self.last_selected_page = Some(index + 1);

        let _ = self.tx_worker.send(WorkerRequest::DuplicatePage { index });
    }

    pub fn remove_selected_pages(&mut self) {
        if self.selected_pages.is_empty() || self.total_pages <= 1 {
            return;
        }
        if self.selected_pages.len() >= self.total_pages {
            return;
        }

        let mut indices: Vec<usize> = self.selected_pages.iter().copied().collect();
        indices.sort_unstable_by(|a, b| b.cmp(a));

        for &idx in &indices {
            if idx < self.doc_page_sizes.len() {
                self.doc_page_sizes.remove(idx);
            }
        }

        self.total_pages -= indices.len();
        self.scenes.clear();
        self.raw_texts.clear();
        self.page_spans.clear();
        self.clear_thumbnails_pending = true;

        self.compute_layouts();

        self.selected_pages.clear();
        self.last_selected_page = None;

        if self.view.active_page >= self.total_pages {
            self.view.active_page = self.total_pages.saturating_sub(1);
        }

        let _ = self.tx_worker.send(WorkerRequest::RemovePages { indices });
    }

    pub fn rotate_pages(&mut self, indices: Vec<usize>, delta: fepdf_sdk::Quarter) {
        if indices.is_empty() || self.total_pages == 0 {
            return;
        }

        let is_90_or_270 = delta.to_degrees() % 180 != 0;
        for &idx in &indices {
            if idx < self.doc_page_sizes.len() {
                if is_90_or_270 {
                    let (w, h) = self.doc_page_sizes[idx];
                    self.doc_page_sizes[idx] = (h, w);
                }
                self.scenes.remove(&idx);
                self.raw_texts.remove(&idx);
                self.page_spans.remove(&idx);
            }
        }

        self.clear_thumbnails_pending = true;
        self.compute_layouts();

        let _ = self.tx_worker.send(WorkerRequest::RotatePages { indices, delta });
    }

    #[allow(dead_code)]
    pub fn rotate_selected_pages(&mut self, delta: fepdf_sdk::Quarter) {
        let targets = if !self.selected_pages.is_empty() {
            self.selected_pages.iter().copied().collect()
        } else if self.total_pages > 0 && self.view.active_page < self.total_pages {
            vec![self.view.active_page]
        } else {
            Vec::new()
        };
        self.rotate_pages(targets, delta);
    }

    pub fn rotate_page_action(&mut self, clicked_idx: usize, delta: fepdf_sdk::Quarter) {
        let targets = if self.selected_pages.contains(&clicked_idx) {
            self.selected_pages.iter().copied().collect()
        } else {
            vec![clicked_idx]
        };
        self.rotate_pages(targets, delta);
    }

    fn check_gpu_support(&self, ui: &mut egui::Ui, frame: &mut eframe::Frame) -> bool {
        let has_wgpu = frame.wgpu_render_state().is_some();
        let has_vello = self.vello_renderer.is_some();
        if !has_wgpu || !has_vello {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.colored_label(egui::Color32::RED, "GPU Compute Support Error");
                    ui.add_space(20.0);
                    ui.label("fepdf relies on high-performance GPU Compute Shaders (Vello) to render PDFs.");
                    ui.label("Either WebGPU is not supported by your hardware/browser, or your GPU drivers do not support compute shaders.");
                    ui.add_space(20.0);
                    ui.label("Please ensure WebGPU or WebGL2-Compute support is enabled, and your graphics drivers are up to date.");
                });
            });
            false
        } else {
            true
        }
    }

    fn queue_visible_pages(&mut self) {
        // Collect visible pages and calculate pre-render lookahead indices
        let mut render_targets = std::collections::BTreeSet::new();
        if self.view.display_mode == DisplayMode::SinglePage {
            let active = self.view.active_page;
            render_targets.insert(active);
            if active > 0 {
                render_targets.insert(active - 1);
            }
            if active + 1 < self.total_pages {
                render_targets.insert(active + 1);
            }
        } else if self.view.display_mode == DisplayMode::TwoPageSingle {
            let spread_indices =
                self.view.get_spread_indices(self.view.active_page, self.total_pages);
            for &idx in &spread_indices {
                render_targets.insert(idx);
            }
            // Pre-render pages before and after the spread
            if let Some(&first_idx) = spread_indices.first()
                && first_idx > 0
            {
                render_targets.insert(first_idx - 1);
            }
            if let Some(&last_idx) = spread_indices.last()
                && last_idx + 1 < self.total_pages
            {
                render_targets.insert(last_idx + 1);
            }
        } else {
            for &visible_index in &self.view.visible_pages {
                render_targets.insert(visible_index);

                // Lookahead pre-rendering: queue previous page and next page in the background
                if visible_index > 0 {
                    render_targets.insert(visible_index - 1);
                }
                if visible_index + 1 < self.total_pages {
                    render_targets.insert(visible_index + 1);
                }
            }
        }

        // Queue rendering requests to the worker thread
        for index in render_targets {
            if !self.scenes.contains_key(&index) && !self.request_queue.contains(&index) {
                let scale = 2.0;
                self.request_queue.insert(index);
                let _ = self.tx_worker.send(WorkerRequest::RenderPage { index, scale });
            }
        }
    }

    fn handle_signature_placement_interaction(
        &mut self,
        ui: &mut egui::Ui,
        visible_index: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
    ) {
        let response = ui.allocate_rect(page_screen_rect, egui::Sense::drag());
        let screen_pos = ui.input(|i| i.pointer.hover_pos());

        if response.drag_started()
            && let Some(pos) = screen_pos
        {
            let pdf_pos = SelectionManager::screen_to_pdf(page_screen_rect, zoom, unscaled_h, pos);
            self.signature_position =
                Some((visible_index, egui::Rect::from_min_max(pdf_pos, pdf_pos)));
        }

        if response.dragged()
            && let Some(pos) = screen_pos
            && let Some((sig_idx, sig_rect)) = &mut self.signature_position
            && *sig_idx == visible_index
        {
            let pdf_pos = SelectionManager::screen_to_pdf(page_screen_rect, zoom, unscaled_h, pos);
            let start_pos = sig_rect.min;
            *sig_rect = egui::Rect::from_two_pos(start_pos, pdf_pos);
        }

        if response.drag_stopped() {
            self.is_placing_signature = false;
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    fn handle_page_interactions(
        // RR-15 Limit: GUI - Unified egui pointer and canvas coordinate interaction loop
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) {
        let visible_pages = self.view.visible_pages.clone();
        let active_spread = self.view.get_spread_indices(self.view.active_page, self.total_pages);
        for &visible_index in &visible_pages {
            if self.view.display_mode == DisplayMode::SinglePage
                && visible_index != self.view.active_page
            {
                continue;
            }
            if self.view.display_mode == DisplayMode::TwoPageSingle
                && !active_spread.contains(&visible_index)
            {
                continue;
            }
            if let Some(layout) = self.page_layouts.get(visible_index) {
                let origin = self.view.get_origin(viewport_rect);
                let page_screen_rect = egui::Rect::from_min_size(
                    origin + layout.rect.min.to_vec2() * zoom,
                    layout.rect.size() * zoom,
                );
                let unscaled_h = layout.rect.height();

                if self.is_placing_signature {
                    self.handle_signature_placement_interaction(
                        ui,
                        visible_index,
                        page_screen_rect,
                        unscaled_h,
                        zoom,
                    );
                } else if self.caliper_tool.is_active {
                    if let Some(spans) = self.page_spans.get(&visible_index) {
                        self.caliper_tool.handle_interaction(
                            ui,
                            visible_index,
                            page_screen_rect,
                            unscaled_h,
                            zoom,
                            &mut self.cad_snap_engine,
                            spans,
                        );
                        self.caliper_tool.draw_overlay(ui, page_screen_rect, unscaled_h, zoom);
                    }
                } else if self.redaction_manager.is_active {
                    self.redaction_manager.handle_interaction(
                        ui,
                        visible_index,
                        page_screen_rect,
                        unscaled_h,
                        zoom,
                    );
                } else if let Some(spans) = self.page_spans.get(&visible_index) {
                    if self.selection_manager.is_tagging_brush_active {
                        self.selection_manager.handle_tagging_brush_interaction(
                            ui,
                            visible_index,
                            page_screen_rect,
                            unscaled_h,
                            spans,
                            zoom,
                        );
                    } else {
                        self.selection_manager.handle_interaction(
                            ui,
                            visible_index,
                            page_screen_rect,
                            unscaled_h,
                            spans,
                            zoom,
                        );
                    }
                }
            }
        }
    }

    fn get_structural_highlight(
        &self,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) -> Option<(usize, egui::Rect)> {
        let selected_id = self.ust_registry.selected_node_id?;
        if let Some(ref root) = self.ust_registry.root
            && root.id == selected_id
        {
            return None;
        }
        let (page_idx, rect) = self.ust_registry.find_placement_by_id(selected_id)?;
        let layout = self.page_layouts.get(page_idx)?;
        let origin = self.view.get_origin(viewport_rect);
        let page_screen_rect = egui::Rect::from_min_size(
            origin + layout.rect.min.to_vec2() * zoom,
            layout.rect.size() * zoom,
        );
        let unscaled_h = layout.rect.height();
        let screen_min = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(rect[0], rect[3]),
        );
        let screen_max = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(rect[2], rect[1]),
        );
        Some((page_idx, egui::Rect::from_min_max(screen_min, screen_max)))
    }

    fn get_signature_highlight(
        &self,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) -> Option<(usize, egui::Rect)> {
        let (sig_page, sig_rect) = self.signature_position?;
        let layout = self.page_layouts.get(sig_page)?;
        let origin = self.view.get_origin(viewport_rect);
        let page_screen_rect = egui::Rect::from_min_size(
            origin + layout.rect.min.to_vec2() * zoom,
            layout.rect.size() * zoom,
        );
        let unscaled_h = layout.rect.height();
        let screen_min = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(sig_rect.min.x, sig_rect.max.y),
        );
        let screen_max = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(sig_rect.max.x, sig_rect.min.y),
        );
        Some((sig_page, egui::Rect::from_min_max(screen_min, screen_max)))
    }

    fn draw_view_with_highlights(
        // RR-15 Limit: GUI
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        zoom: f32,
        viewport_texture_id: Option<egui::TextureId>,
    ) {
        // Gather redaction highlights
        let mut redaction_highlights = BTreeMap::new();
        let mut active_redaction_drag = None;

        let visible_pages = self.view.visible_pages.clone();
        let active_spread = self.view.get_spread_indices(self.view.active_page, self.total_pages);
        for &visible_index in &visible_pages {
            if self.view.display_mode == DisplayMode::SinglePage
                && visible_index != self.view.active_page
            {
                continue;
            }
            if self.view.display_mode == DisplayMode::TwoPageSingle
                && !active_spread.contains(&visible_index)
            {
                continue;
            }
            if let Some(layout) = self.page_layouts.get(visible_index) {
                let origin = self.view.get_origin(viewport_rect);
                let page_screen_rect = egui::Rect::from_min_size(
                    origin + layout.rect.min.to_vec2() * zoom,
                    layout.rect.size() * zoom,
                );
                let unscaled_h = layout.rect.height();

                let (completed, active_drag) = self.redaction_manager.get_screen_highlights(
                    visible_index,
                    page_screen_rect,
                    unscaled_h,
                    zoom,
                );
                if !completed.is_empty() {
                    redaction_highlights.insert(visible_index, completed);
                }
                if let Some(drag_rect) = active_drag {
                    active_redaction_drag = Some((visible_index, drag_rect));
                }
            }
        }

        let structural_highlight = self.get_structural_highlight(viewport_rect, zoom);
        let signature_highlight = self.get_signature_highlight(viewport_rect, zoom);

        self.view.show_virtual(
            ui,
            &self.page_layouts,
            viewport_texture_id,
            viewport_rect,
            &self.scenes,
            &self.selection_manager.highlights,
            &redaction_highlights,
            &active_redaction_drag,
            &structural_highlight,
            &signature_highlight,
            &self.ust_registry,
            self.show_reading_order,
        );
    }

    fn render_document_panel(
        // RR-15 Limit: GUI - Renders document panel, handles centering, page layouts, and vello texture projection
        &mut self,
        ui: &mut egui::Ui,
        rs: &egui_wgpu::RenderState,
        viewport_rect: egui::Rect,
    ) {
        if let Some(center_id) = self.ust_registry.pending_center_node_id.take()
            && let Some((page_idx, rect)) = self.ust_registry.find_placement_by_id(center_id)
            && let Some(layout) = self.page_layouts.get(page_idx)
        {
            self.view.center_on_rect(viewport_rect, layout, rect);
        }

        let vello_renderer = match self.vello_renderer.as_mut() {
            Some(r) => r,
            None => return,
        };
        vello_renderer.next_frame(rs);

        let zoom = self.view.zoom;

        // Collect visible pages and their scenes
        let mut visible_pages_data = Vec::new();
        let origin = self.view.get_origin(viewport_rect);

        let active_spread = self.view.get_spread_indices(self.view.active_page, self.total_pages);
        for layout in &self.page_layouts {
            if self.view.display_mode == DisplayMode::SinglePage
                && layout.index != self.view.active_page
            {
                continue;
            }
            if self.view.display_mode == DisplayMode::TwoPageSingle
                && !active_spread.contains(&layout.index)
            {
                continue;
            }
            let page_screen_rect = egui::Rect::from_min_size(
                origin + layout.rect.min.to_vec2() * zoom,
                layout.rect.size() * zoom,
            );

            if viewport_rect.intersects(page_screen_rect)
                && let Some(scene) = self.scenes.get(&layout.index)
            {
                let unscaled_size = egui::vec2(layout.rect.width(), layout.rect.height());
                visible_pages_data.push((
                    layout.index,
                    Arc::clone(scene),
                    page_screen_rect,
                    unscaled_size,
                ));
            }
        }

        let scale_factor = ui.ctx().pixels_per_point();
        let viewport_texture_id = vello_renderer.render_viewport(
            rs,
            &visible_pages_data,
            viewport_rect,
            scale_factor,
            zoom,
        );

        self.handle_page_interactions(ui, viewport_rect, zoom);
        self.draw_view_with_highlights(ui, viewport_rect, zoom, viewport_texture_id);
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

                // Floating page & zoom overlay at the top-center of the CentralPanel
                let has_spread = self.view.display_mode == DisplayMode::TwoPageSpread;
                let content_width = if has_spread { 770.0 } else { 600.0 };
                let overlay_width = content_width + 30.0;
                let overlay_height = 36.0;
                let overlay_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        viewport_rect.center().x - overlay_width / 2.0,
                        viewport_rect.top() + 16.0,
                    ),
                    egui::vec2(overlay_width, overlay_height),
                );

                // Rounded semi-transparent background card
                ui.painter().rect_filled(
                    overlay_rect,
                    6.0,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 220),
                );
                ui.painter().rect_stroke(
                    overlay_rect,
                    6.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_gray(200)),
                    egui::StrokeKind::Outside,
                );

                let mut child_ui = ui.new_child(
                    egui::UiBuilder::new().max_rect(overlay_rect).layout(
                        egui::Layout::left_to_right(egui::Align::Center)
                            .with_main_align(egui::Align::Center),
                    ),
                );
                child_ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center)
                        .with_main_align(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let spacer = (ui.available_width() - content_width).max(0.0) / 2.0;
                        ui.add_space(spacer);

                        let current_page = if self.view.display_mode == DisplayMode::SinglePage {
                            self.view.active_page
                        } else {
                            self.view
                                .visible_pages
                                .first()
                                .copied()
                                .unwrap_or(self.view.active_page)
                        };

                        // Page Navigation Icon Directions (Adapts to Vertical/Horizontal and L2R/R2L binding)
                        let (first_icon, prev_icon, next_icon, last_icon) =
                            match self.view.scroll_direction {
                                crate::view::ScrollDirection::Vertical => ("▲▲", "▲", "▼", "▼▼"),
                                crate::view::ScrollDirection::Horizontal => {
                                    if self.view.binding_direction
                                        == crate::view::BindingDirection::RightToLeft
                                    {
                                        ("▶▶", "▶", "◀", "◀◀")
                                    } else {
                                        ("◀◀", "◀", "▶", "▶▶")
                                    }
                                }
                            };

                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), egui::Button::new(first_icon))
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_first_page"),
                            )
                            .clicked()
                        {
                            self.view.scroll_to_page(0, &self.page_layouts);
                        }

                        // Previous page logic (reversed in RTL Horizontal layout)
                        let go_prev = ui
                            .add_sized(egui::vec2(24.0, 24.0), egui::Button::new(prev_icon))
                            .clicked();
                        if go_prev {
                            if self.view.display_mode == DisplayMode::TwoPageSingle {
                                let spread =
                                    self.view.get_spread_indices(current_page, self.total_pages);
                                if let Some(&first_idx) = spread.first() {
                                    let is_r2l = self.view.scroll_direction
                                        == crate::view::ScrollDirection::Horizontal
                                        && self.view.binding_direction
                                            == crate::view::BindingDirection::RightToLeft;
                                    if is_r2l {
                                        let last_idx = spread.last().copied().unwrap_or(first_idx);
                                        if last_idx + 1 < self.total_pages {
                                            self.view
                                                .scroll_to_page(last_idx + 1, &self.page_layouts);
                                        }
                                    } else if first_idx > 0 {
                                        self.view.scroll_to_page(first_idx - 1, &self.page_layouts);
                                    }
                                }
                            } else if self.view.scroll_direction
                                == crate::view::ScrollDirection::Horizontal
                                && self.view.binding_direction
                                    == crate::view::BindingDirection::RightToLeft
                            {
                                if current_page + 1 < self.total_pages {
                                    self.view.scroll_to_page(current_page + 1, &self.page_layouts);
                                }
                            } else if current_page > 0 {
                                self.view.scroll_to_page(current_page - 1, &self.page_layouts);
                            }
                        }

                        ui.allocate_ui_with_layout(
                            egui::vec2(75.0, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(75.0);
                                ui.set_max_width(75.0);
                                ui.centered_and_justified(|ui| {
                                    ui.label(format!("{}/{}", current_page + 1, self.total_pages));
                                });
                            },
                        );

                        // Next page logic (reversed in RTL Horizontal layout)
                        let go_next = ui
                            .add_sized(egui::vec2(24.0, 24.0), egui::Button::new(next_icon))
                            .clicked();
                        if go_next {
                            if self.view.display_mode == DisplayMode::TwoPageSingle {
                                let spread =
                                    self.view.get_spread_indices(current_page, self.total_pages);
                                if let Some(&first_idx) = spread.first() {
                                    let is_r2l = self.view.scroll_direction
                                        == crate::view::ScrollDirection::Horizontal
                                        && self.view.binding_direction
                                            == crate::view::BindingDirection::RightToLeft;
                                    if is_r2l {
                                        if first_idx > 0 {
                                            self.view
                                                .scroll_to_page(first_idx - 1, &self.page_layouts);
                                        }
                                    } else {
                                        let last_idx = spread.last().copied().unwrap_or(first_idx);
                                        if last_idx + 1 < self.total_pages {
                                            self.view
                                                .scroll_to_page(last_idx + 1, &self.page_layouts);
                                        }
                                    }
                                }
                            } else if self.view.scroll_direction
                                == crate::view::ScrollDirection::Horizontal
                                && self.view.binding_direction
                                    == crate::view::BindingDirection::RightToLeft
                            {
                                if current_page > 0 {
                                    self.view.scroll_to_page(current_page - 1, &self.page_layouts);
                                }
                            } else if current_page + 1 < self.total_pages {
                                self.view.scroll_to_page(current_page + 1, &self.page_layouts);
                            }
                        }

                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), egui::Button::new(last_icon))
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_last_page"),
                            )
                            .clicked()
                        {
                            self.view.scroll_to_page(self.total_pages - 1, &self.page_layouts);
                        }

                        ui.separator();

                        // 1. Display Mode Toggle Group
                        let is_continuous = self.view.display_mode == DisplayMode::Continuous;
                        let cont_btn =
                            egui::Button::new(egui::RichText::new("\u{e0ff}").size(14.0))
                                .selected(is_continuous);
                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), cont_btn)
                            .on_hover_text("連続スクロール")
                            .clicked()
                        {
                            self.view.display_mode = DisplayMode::Continuous;
                            self.compute_layouts();
                            let active = self.view.active_page;
                            self.view.scroll_to_page(active, &self.page_layouts);
                        }

                        let is_single = self.view.display_mode == DisplayMode::SinglePage;
                        let single_btn =
                            egui::Button::new(egui::RichText::new("\u{e12c}").size(14.0))
                                .selected(is_single);
                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), single_btn)
                            .on_hover_text("単一ページ表示")
                            .clicked()
                        {
                            self.view.display_mode = DisplayMode::SinglePage;
                            self.compute_layouts();
                            let active = self.view.active_page;
                            self.view.scroll_to_page(active, &self.page_layouts);
                        }

                        let is_spread = self.view.display_mode == DisplayMode::TwoPageSpread;
                        let spread_btn =
                            egui::Button::new(egui::RichText::new("\u{e05f}").size(14.0))
                                .selected(is_spread);
                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), spread_btn)
                            .on_hover_text("見開き連続表示")
                            .clicked()
                        {
                            self.view.display_mode = DisplayMode::TwoPageSpread;
                            self.compute_layouts();
                            let active = self.view.active_page;
                            self.view.scroll_to_page(active, &self.page_layouts);
                        }

                        let is_spread_single = self.view.display_mode == DisplayMode::TwoPageSingle;
                        let spread_single_btn =
                            egui::Button::new(egui::RichText::new("\u{e80a}").size(14.0))
                                .selected(is_spread_single);
                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), spread_single_btn)
                            .on_hover_text("見開き単一表示")
                            .clicked()
                        {
                            self.view.display_mode = DisplayMode::TwoPageSingle;
                            self.compute_layouts();
                            let active = self.view.active_page;
                            self.view.scroll_to_page(active, &self.page_layouts);
                        }

                        ui.separator();

                        // 2. Scroll Direction Toggle Group
                        let is_vert =
                            self.view.scroll_direction == crate::view::ScrollDirection::Vertical;
                        let vert_btn =
                            egui::Button::new(egui::RichText::new("\u{e37d}").size(14.0))
                                .selected(is_vert);
                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), vert_btn)
                            .on_hover_text("縦方向スクロール")
                            .clicked()
                        {
                            self.view.scroll_direction = crate::view::ScrollDirection::Vertical;
                            self.compute_layouts();
                            let active = self.view.active_page;
                            self.view.scroll_to_page(active, &self.page_layouts);
                        }

                        let is_horiz =
                            self.view.scroll_direction == crate::view::ScrollDirection::Horizontal;
                        let horiz_btn =
                            egui::Button::new(egui::RichText::new("\u{e24a}").size(14.0))
                                .selected(is_horiz);
                        if ui
                            .add_sized(egui::vec2(24.0, 24.0), horiz_btn)
                            .on_hover_text("横方向スクロール")
                            .clicked()
                        {
                            self.view.scroll_direction = crate::view::ScrollDirection::Horizontal;
                            self.compute_layouts();
                            let active = self.view.active_page;
                            self.view.scroll_to_page(active, &self.page_layouts);
                        }

                        // 3. Two-Page Spread Specific Controls
                        if has_spread {
                            ui.separator();

                            if ui.checkbox(&mut self.view.cover_page_alone, "表紙単独").changed()
                            {
                                self.compute_layouts();
                                let active = self.view.active_page;
                                self.view.scroll_to_page(active, &self.page_layouts);
                            }

                            let is_rtl = self.view.binding_direction
                                == crate::view::BindingDirection::RightToLeft;
                            let binding_label = if is_rtl { "右綴じ" } else { "左綴じ" };
                            if ui
                                .add_sized(
                                    egui::vec2(48.0, 24.0),
                                    egui::Button::new(binding_label).selected(is_rtl),
                                )
                                .clicked()
                            {
                                self.view.binding_direction = if is_rtl {
                                    crate::view::BindingDirection::LeftToRight
                                } else {
                                    crate::view::BindingDirection::RightToLeft
                                };
                                self.compute_layouts();
                                let active = self.view.active_page;
                                self.view.scroll_to_page(active, &self.page_layouts);
                            }
                        }

                        ui.separator();

                        if ui
                            .add_sized(
                                egui::vec2(24.0, 24.0),
                                egui::Button::new(egui::RichText::new("\u{e1b7}").size(14.0)),
                            )
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_zoom_out"),
                            )
                            .clicked()
                        {
                            self.view.zoom = (self.view.zoom / 1.2).clamp(0.1, 10.0);
                        }
                        ui.allocate_ui_with_layout(
                            egui::vec2(45.0, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(45.0);
                                ui.set_max_width(45.0);
                                ui.centered_and_justified(|ui| {
                                    ui.label(format!("{:.0}%", self.view.zoom * 100.0));
                                });
                            },
                        );
                        if ui
                            .add_sized(
                                egui::vec2(24.0, 24.0),
                                egui::Button::new(egui::RichText::new("\u{e1b6}").size(14.0)),
                            )
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_zoom_in"),
                            )
                            .clicked()
                        {
                            self.view.zoom = (self.view.zoom * 1.2).clamp(0.1, 10.0);
                        }

                        ui.separator();

                        if ui
                            .add_sized(
                                egui::vec2(24.0, 24.0),
                                egui::Button::new(egui::RichText::new("\u{e1c6}").size(14.0)),
                            )
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_fit_width"),
                            )
                            .clicked()
                        {
                            self.fit_to_width(viewport_rect);
                        }
                        if ui
                            .add_sized(
                                egui::vec2(24.0, 24.0),
                                egui::Button::new(egui::RichText::new("\u{e1c7}").size(14.0)),
                            )
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_fit_height"),
                            )
                            .clicked()
                        {
                            self.fit_to_height(viewport_rect);
                        }

                        if ui
                            .add_sized(
                                egui::vec2(24.0, 24.0),
                                egui::Button::new(egui::RichText::new("\u{e148}").size(14.0)),
                            )
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "cmd_reset_view"),
                            )
                            .clicked()
                        {
                            self.reset_view();
                        }
                    },
                );
            } else if self.is_loading {
                ui.centered_and_justified(|ui| {
                    ui.label(&self.loading_message);
                });
            } else {
                // Keep the central panel blank at startup as requested
            }
        });
    }

    fn show_export_wizard_window(&mut self, ctx: &egui::Context) {
        crate::export_wizard::ExportWizard::show(self, ctx);
    }

    /// Bulk front end for the redaction pipeline: pattern-matched text spans are pushed
    /// into the same `RedactionManager::zones` the manual brush fills, so the export
    /// wizard's "burn redactions" path consumes both identically.
    fn show_redaction_studio_window(&mut self, ctx: &egui::Context) {
        let Self {
            redaction_studio_panel,
            raw_texts,
            page_spans,
            redaction_manager,
            locale_mgr,
            active_language,
            show_redaction_studio,
            ..
        } = self;

        let title = locale_mgr.tr(active_language, "redaction_studio_title");
        egui::Window::new(format!("🔍 {title}"))
            .open(show_redaction_studio)
            .resizable(true)
            .default_width(420.0)
            .default_height(360.0)
            .show(ctx, |ui| {
                redaction_studio_panel.show(
                    ui,
                    raw_texts,
                    page_spans,
                    redaction_manager,
                    locale_mgr,
                    active_language,
                );
            });
    }
}

impl FepdfApp {
    fn apply_global_styles(&self, ctx: &egui::Context) {
        ctx.set_visuals(egui::Visuals::light());
        ctx.global_style_mut(|style| {
            style.visuals.selection.stroke = egui::Stroke::NONE;
            style.visuals.selection.bg_fill =
                egui::Color32::from_rgba_unmultiplied(120, 125, 135, 45);
            style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
            style.visuals.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0_f32, egui::Color32::from_gray(210));
        });
    }

    fn render_left_side_panels(&mut self, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - Renders left sidebar icon bar, context panels, and inspector panel
        // 1. Left Icon Bar (Vertical column, full height)
        let ctx = ui.ctx().clone();

        egui::Panel::left("left_icon_bar").resizable(false).default_size(50.0).show_inside(
            ui,
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 8.0);
                    ui.add_space(8.0);

                    // 1. Load PDF
                    let load_btn = egui::Button::new(egui::RichText::new("\u{e247}").size(16.0))
                        .min_size(egui::vec2(36.0, 36.0));
                    if ui
                        .add(load_btn)
                        .on_hover_text(
                            self.locale_mgr.tr(&self.active_language, "tooltip_load_pdf"),
                        )
                        .clicked()
                        && let Some(p) =
                            rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
                    {
                        if self.total_pages > 0 {
                            if let Ok(exe) = std::env::current_exe() {
                                let _ = std::process::Command::new(exe).arg(p).spawn();
                            }
                        } else {
                            self.open_file(p, &ctx);
                        }
                    }

                    // 2. Export PDF & Inspector (Enabled only when doc loaded)
                    let has_doc = self.total_pages > 0;
                    ui.add_enabled_ui(has_doc, |ui| {
                        let export_btn =
                            egui::Button::new(egui::RichText::new("\u{e14d}").size(16.0))
                                .min_size(egui::vec2(36.0, 36.0));
                        if ui
                            .add(export_btn)
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_export_pdf"),
                            )
                            .clicked()
                        {
                            self.show_export_wizard = true;
                        }

                        let mut inspector_btn =
                            egui::Button::new(egui::RichText::new("\u{e151}").size(16.0))
                                .min_size(egui::vec2(36.0, 36.0));
                        if self.show_inspector {
                            inspector_btn = inspector_btn
                                .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_gray(80)));
                        }
                        if ui
                            .add(inspector_btn)
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_inspector"),
                            )
                            .clicked()
                        {
                            self.show_inspector = !self.show_inspector;
                        }
                    });

                    ui.separator();

                    // 3. 7 Sidebar Navigation Tabs (DocumentInfo, Bookmarks, Attachments, Structure, Properties, AltText, Audit)
                    self.sidebar_panel.show_icon_bar(ui, &self.locale_mgr, &self.active_language);

                    let current_height = ui.available_height();
                    if current_height > 100.0 {
                        ui.add_space(current_height - 90.0);
                    }

                    // 4. Settings Button
                    let settings_btn =
                        egui::Button::new(egui::RichText::new("\u{e30b}").size(16.0))
                            .min_size(egui::vec2(36.0, 36.0));
                    if ui
                        .add(settings_btn)
                        .on_hover_text(
                            self.locale_mgr.tr(&self.active_language, "tooltip_settings"),
                        )
                        .clicked()
                    {
                        self.show_settings_modal = true;
                    }

                    // 5. About (Help) Button
                    let about_btn = egui::Button::new(egui::RichText::new("\u{e082}").size(16.0))
                        .min_size(egui::vec2(36.0, 36.0));
                    if ui
                        .add(about_btn)
                        .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_about"))
                        .clicked()
                    {
                        self.show_about_modal = true;
                    }
                });
            },
        );

        let locale_mgr = &self.locale_mgr;
        let active_lang = &self.active_language;

        // 2. Context Panel (resizable, automatic size adjusting)
        if self.sidebar_panel.context_panel_open {
            egui::Panel::left("context_panel")
                .resizable(true)
                .show_separator_line(true)
                .size_range(260.0..=900.0)
                .show_inside(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    egui::Frame::NONE.inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        self.sidebar_panel.show(
                            ui,
                            &mut self.ust_registry,
                            &self.tx_worker,
                            &self.pdf_name,
                            self.total_pages,
                            &self.doc_metadata,
                            self.doc_file_size,
                            &self.doc_version,
                            &self.doc_security_method,
                            self.doc_permissions,
                            &self.doc_page_sizes,
                            &self.doc_fonts,
                            locale_mgr,
                            active_lang,
                        );
                    });
                });
        }

        // 3. Arlington Dictionary Inspector (Left side, next to context panel)
        if self.show_inspector {
            let selected_tag = self.ust_registry.selected_node_id.and_then(|id| {
                if let Some(ref root) = self.ust_registry.root {
                    crate::sidebar::USTRegistry::find_node_by_id_recursive(root, id)
                        .map(|n| n.tag.as_str())
                } else {
                    None
                }
            });
            egui::Panel::left("inspector_panel")
                .resizable(true)
                .show_separator_line(true)
                .default_size(280.0)
                .size_range(200.0..=450.0)
                .show_inside(ui, |ui| {
                    egui::Frame::NONE.inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        self.arlington_inspector.show(ui, selected_tag, locale_mgr, active_lang);
                    });
                });
        }
    }

    fn render_status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar").default_size(28.0).resizable(false).show_inside(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(self.locale_mgr.tr(&self.active_language, "status_ready"));
                    ui.separator();
                    if self.total_pages > 0 {
                        let current_page = self.view.visible_pages.first().copied().unwrap_or(0);
                        let indicator = self
                            .locale_mgr
                            .tr(&self.active_language, "page_indicator")
                            .replacen("{}", &(current_page + 1).to_string(), 1)
                            .replacen("{}", &self.total_pages.to_string(), 1);
                        ui.label(indicator);
                    } else {
                        ui.label(self.locale_mgr.tr(&self.active_language, "no_doc_loaded"));
                    }
                    ui.separator();
                    if self.show_reading_order {
                        ui.label(
                            self.locale_mgr.tr(&self.active_language, "reading_order_enabled"),
                        );
                    } else {
                        ui.label(
                            self.locale_mgr.tr(&self.active_language, "reading_order_disabled"),
                        );
                    }
                });
            },
        );
    }

    fn show_about_modal_window(&mut self, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Displays the application metadata/about modal
        if self.show_about_modal {
            let mut show_about = true;
            let about_title = self.locale_mgr.tr(&self.active_language, "about_title");
            egui::Window::new(about_title)
                .open(&mut show_about)
                .resizable(false)
                .collapsible(false)
                .default_width(320.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(
                                self.locale_mgr.tr(&self.active_language, "about_app_name"),
                            )
                            .strong()
                            .size(18.0),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} 0.1.0",
                                self.locale_mgr.tr(&self.active_language, "about_version")
                            ))
                            .weak(),
                        );
                        ui.add_space(8.0);
                        ui.label(self.locale_mgr.tr(&self.active_language, "about_description"));
                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(
                                self.locale_mgr.tr(&self.active_language, "about_third_party"),
                            )
                            .strong(),
                        );
                        ui.add_space(4.0);
                    });

                    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        let credits = [
                            ("lopdf", "MIT License", "Low-level PDF parsing"),
                            ("pdf-writer", "Apache-2.0 License", "PDF object serialization"),
                            ("vello", "Apache-2.0 / MIT", "GPU vector graphics"),
                            ("egui / eframe", "MIT / Apache-2.0", "GUI library"),
                            ("Lucide Icons", "ISC License", "Icon font asset"),
                        ];
                        for (name, license, purpose) in credits {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(name).strong());
                                ui.label(format!("({license})"));
                            });
                            ui.label(egui::RichText::new(purpose).weak());
                            ui.add_space(4.0);
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        if ui
                            .button(self.locale_mgr.tr(&self.active_language, "about_close"))
                            .clicked()
                        {
                            self.show_about_modal = false;
                        }
                    });
                });
            if !show_about {
                self.show_about_modal = false;
            }
        }
    }

    fn render_overlay_windows(&mut self, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Renders various overlay windows, tool wizards, and popup alerts
        if self.show_export_wizard {
            self.show_export_wizard_window(ctx);
        }

        if self.show_redaction_studio {
            self.show_redaction_studio_window(ctx);
        }

        // Show Command Palette window overlay
        crate::command_palette::CommandPalette::show(self, ctx);

        // Show interactive Create Semantic Tag popup dialog on visual tag selector brush highlights
        if let Some(req) = self.selection_manager.pending_tag_request.clone() {
            let mut show_popup = true;
            let popup_title = self.locale_mgr.tr(&self.active_language, "tag_popup_title");
            egui::Window::new(popup_title)
                .open(&mut show_popup)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(self.locale_mgr.tr(&self.active_language, "tag_popup_selected"));
                    ui.group(|ui| {
                        ui.label(&req.text);
                    });
                    ui.add_space(5.0);
                    ui.label(self.locale_mgr.tr(&self.active_language, "tag_popup_instruction"));

                    ui.horizontal(|ui| {
                        if ui.button("H1").clicked() {
                            self.inject_tag_to_tree("H1", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                        if ui.button("H2").clicked() {
                            self.inject_tag_to_tree("H2", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                        if ui.button("P").clicked() {
                            self.inject_tag_to_tree("P", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                        if ui.button("Figure").clicked() {
                            self.inject_tag_to_tree("Figure", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                    });

                    if ui
                        .button(self.locale_mgr.tr(&self.active_language, "tag_popup_cancel"))
                        .clicked()
                    {
                        self.selection_manager.pending_tag_request = None;
                    }
                });
            if !show_popup {
                self.selection_manager.pending_tag_request = None;
            }
        }

        // Show Settings Modal
        if self.show_settings_modal {
            let mut show_settings = true;
            let title = self.locale_mgr.tr(&self.active_language, "settings_title");
            egui::Window::new(title)
                .open(&mut show_settings)
                .resizable(false)
                .collapsible(false)
                .default_width(280.0)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                self.locale_mgr
                                    .tr(&self.active_language, "settings_language_label"),
                            );
                            let current_lang = self.active_language.clone();
                            egui::ComboBox::from_id_salt("settings_lang_combobox")
                                .selected_text(&current_lang)
                                .show_ui(ui, |ui| {
                                    for lang in self.locale_mgr.available_languages() {
                                        ui.selectable_value(
                                            &mut self.active_language,
                                            lang.clone(),
                                            lang,
                                        );
                                    }
                                });
                        });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            if ui
                                .button(self.locale_mgr.tr(&self.active_language, "settings_close"))
                                .clicked()
                            {
                                self.show_settings_modal = false;
                            }
                        });
                    });
                });
            if !show_settings {
                self.show_settings_modal = false;
            }
        }

        // Show About Modal
        self.show_about_modal_window(ctx);
    }
}

impl eframe::App for FepdfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // RR-15 Limit: Dispatcher - Main application UI shell layout routing layout panels and windows
        let ctx = ui.ctx().clone();
        self.apply_global_styles(&ctx);

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
