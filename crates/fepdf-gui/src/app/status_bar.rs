//! Status bar footer rendering for `FepdfApp` with navigation and zoom controls.

use super::FepdfApp;
use crate::view::{BindingDirection, DisplayMode};

impl FepdfApp {
    pub(crate) fn render_status_bar(&mut self, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - Bottom status bar with status indicators, page navigation, and zoom controls
        let has_doc = self.total_pages > 0;
        let current_page = if self.view.display_mode == DisplayMode::SinglePage {
            self.view.active_page
        } else {
            self.view.visible_pages.first().copied().unwrap_or(self.view.active_page)
        };

        egui::Panel::bottom("status_bar").default_size(28.0).resizable(false).show_inside(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;

                    // 1. Left side: Status text & Reading Order toggle
                    //
                    // Pages the renderer left out say so here rather than in the decision
                    // sidebar: see `FepdfApp::pages_left_out` for why that is not a
                    // `Decision`.
                    if self.pages_left_out > 0 {
                        let notice = self
                            .locale_mgr
                            .tr(&self.active_language, "status_pages_over_budget")
                            .replacen("{}", &self.pages_left_out.to_string(), 1);
                        ui.label(
                            egui::RichText::new(notice)
                                .size(12.0)
                                .color(super::theme::colors::STATUS_WARN_TEXT),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(
                                self.locale_mgr.tr(&self.active_language, "status_ready"),
                            )
                            .size(12.0),
                        );
                    }

                    // Which mode the view is in. Nothing announces that content
                    // interactions have stopped below `PDFView::LEGIBLE_ZOOM`; this is
                    // where a reader finds out which half of that boundary they are on.
                    if has_doc {
                        ui.separator();
                        let key = if self.view.is_reading_view() {
                            "status_mode_reading"
                        } else {
                            "status_mode_overview"
                        };
                        ui.label(
                            egui::RichText::new(self.locale_mgr.tr(&self.active_language, key))
                                .size(12.0)
                                .weak(),
                        );
                    }

                    ui.separator();

                    let reading_txt = if self.show_reading_order {
                        self.locale_mgr.tr(&self.active_language, "reading_order_enabled")
                    } else {
                        self.locale_mgr.tr(&self.active_language, "reading_order_disabled")
                    };
                    if ui.selectable_label(self.show_reading_order, reading_txt).clicked() {
                        self.show_reading_order = !self.show_reading_order;
                    }

                    // 2. Right side controls: Zoom & Display Mode
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);

                        if has_doc {
                            // Rotate 90° Clockwise
                            if ui
                                .add_sized(
                                    egui::vec2(22.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e253}").size(12.0)),
                                )
                                .on_hover_text("ページを右に90°回転")
                                .clicked()
                            {
                                self.rotate_selected_pages(fepdf::Quarter::Q90);
                            }

                            // Layout direction: Horizontal (LTR) vs Vertical (R2L)
                            let is_r2l =
                                self.view.binding_direction == BindingDirection::RightToLeft;
                            if ui
                                .add_sized(
                                    egui::vec2(22.0, 20.0),
                                    egui::Button::new(
                                        egui::RichText::new(if is_r2l { "縦" } else { "横" })
                                            .size(11.0),
                                    )
                                    .selected(is_r2l),
                                )
                                .on_hover_text(if is_r2l {
                                    "縦書き / 右開き順 (R2L) — クリックで横書きに切替"
                                } else {
                                    "横書き / 左開き順 (LTR) — クリックで縦書きに切替"
                                })
                                .clicked()
                            {
                                self.view.binding_direction = if is_r2l {
                                    BindingDirection::LeftToRight
                                } else {
                                    BindingDirection::RightToLeft
                                };
                                self.compute_layouts();
                            }

                            ui.separator();

                            // View modes
                            let is_spread = self.view.display_mode == DisplayMode::TwoPageSpread;
                            if ui
                                .add_sized(
                                    egui::vec2(22.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e05f}").size(12.0))
                                        .selected(is_spread),
                                )
                                .on_hover_text("見開き表示")
                                .clicked()
                            {
                                self.view.display_mode = DisplayMode::TwoPageSpread;
                                self.compute_layouts();
                            }

                            let is_single = self.view.display_mode == DisplayMode::SinglePage;
                            if ui
                                .add_sized(
                                    egui::vec2(22.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e12c}").size(12.0))
                                        .selected(is_single),
                                )
                                .on_hover_text("単一ページ表示")
                                .clicked()
                            {
                                self.view.display_mode = DisplayMode::SinglePage;
                                self.compute_layouts();
                            }

                            let is_continuous = self.view.display_mode == DisplayMode::Continuous;
                            if ui
                                .add_sized(
                                    egui::vec2(22.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e0ff}").size(12.0))
                                        .selected(is_continuous),
                                )
                                .on_hover_text("連続スクロール")
                                .clicked()
                            {
                                self.view.display_mode = DisplayMode::Continuous;
                                self.compute_layouts();
                            }

                            ui.separator();

                            // Fit Width / Height
                            let viewport = self.last_viewport_rect.unwrap_or(ui.max_rect());
                            if ui
                                .add_sized(
                                    egui::vec2(22.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e1c7}").size(12.0)),
                                )
                                .on_hover_text("高さに合わせる")
                                .clicked()
                            {
                                self.fit_to_height(viewport);
                            }

                            if ui
                                .add_sized(
                                    egui::vec2(22.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e1c6}").size(12.0)),
                                )
                                .on_hover_text("幅に合わせる")
                                .clicked()
                            {
                                self.fit_to_width(viewport);
                            }

                            let viewport_rect =
                                self.last_viewport_rect.unwrap_or_else(|| ui.max_rect());
                            let center = viewport_rect.center();

                            // Zoom In / Reset / Out
                            if ui
                                .add_sized(
                                    egui::vec2(20.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e1b6}").size(12.0)),
                                )
                                .on_hover_text("拡大")
                                .clicked()
                            {
                                self.view.zoom_at(
                                    self.view.zoom_step_up(),
                                    center,
                                    viewport_rect,
                                    &self.page_layouts,
                                );
                            }

                            let zoom_label = format!("{:.0}%", self.view.zoom() * 100.0);
                            if ui
                                .add_sized(egui::vec2(42.0, 20.0), egui::Button::new(zoom_label))
                                .on_hover_text("ズームリセット (100%)")
                                .clicked()
                            {
                                self.view.zoom_at(1.0, center, viewport_rect, &self.page_layouts);
                            }

                            if ui
                                .add_sized(
                                    egui::vec2(20.0, 20.0),
                                    egui::Button::new(egui::RichText::new("\u{e1b7}").size(12.0)),
                                )
                                .on_hover_text("縮小")
                                .clicked()
                            {
                                self.view.zoom_at(
                                    self.view.zoom_step_down(),
                                    center,
                                    viewport_rect,
                                    &self.page_layouts,
                                );
                            }

                            ui.separator();

                            // 3. Center Page Navigation
                            if ui
                                .add_sized(egui::vec2(22.0, 20.0), egui::Button::new("⏭"))
                                .on_hover_text("最後のページへ")
                                .clicked()
                            {
                                self.view.scroll_to_page(self.total_pages - 1, &self.page_layouts);
                            }

                            if ui
                                .add_sized(egui::vec2(22.0, 20.0), egui::Button::new("▶"))
                                .on_hover_text("次のページ")
                                .clicked()
                                && current_page + 1 < self.total_pages
                            {
                                self.view.scroll_to_page(current_page + 1, &self.page_layouts);
                            }

                            ui.label(format!("{}/{}", current_page + 1, self.total_pages));

                            if ui
                                .add_sized(egui::vec2(22.0, 20.0), egui::Button::new("◀"))
                                .on_hover_text("前のページ")
                                .clicked()
                                && current_page > 0
                            {
                                self.view.scroll_to_page(current_page - 1, &self.page_layouts);
                            }

                            if ui
                                .add_sized(egui::vec2(22.0, 20.0), egui::Button::new("⏮"))
                                .on_hover_text("最初のページへ")
                                .clicked()
                            {
                                self.view.scroll_to_page(0, &self.page_layouts);
                            }

                            ui.separator();
                        }
                    });
                });
            },
        );
    }
}
