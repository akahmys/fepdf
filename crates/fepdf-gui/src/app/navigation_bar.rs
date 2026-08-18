//! Floating page navigation and zoom controls overlay for `FepdfApp`.

use super::FepdfApp;
use crate::view::DisplayMode;

impl FepdfApp {
    pub(crate) fn render_floating_navigation_bar(
        // RR-15 Limit: GUI - Floating page and zoom control overlay bar
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
    ) {
        let has_spread = self.view.display_mode == DisplayMode::TwoPageSpread;
        let content_width = if has_spread { 770.0 } else { 600.0 };
        let overlay_width = content_width + 30.0;
        let overlay_height = 36.0;
        let overlay_rect = egui::Rect::from_min_size(
            egui::pos2(viewport_rect.center().x - overlay_width / 2.0, viewport_rect.top() + 16.0),
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

        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(overlay_rect).layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
        ));
        child_ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let spacer = (ui.available_width() - content_width).max(0.0) / 2.0;
                ui.add_space(spacer);

                let current_page = if self.view.display_mode == DisplayMode::SinglePage {
                    self.view.active_page
                } else {
                    self.view.visible_pages.first().copied().unwrap_or(self.view.active_page)
                };

                // Page Navigation Icon Directions (Adapts to Vertical/Horizontal and L2R/R2L binding)
                let (first_icon, prev_icon, next_icon, last_icon) = match self.view.scroll_direction
                {
                    crate::view::ScrollDirection::Vertical => ("▲▲", "▲", "▼", "▼▼"),
                    crate::view::ScrollDirection::Horizontal => {
                        if self.view.binding_direction == crate::view::BindingDirection::RightToLeft
                        {
                            ("▶▶", "▶", "◀", "◀◀")
                        } else {
                            ("◀◀", "◀", "▶", "▶▶")
                        }
                    }
                };

                if ui
                    .add_sized(egui::vec2(24.0, 24.0), egui::Button::new(first_icon))
                    .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_first_page"))
                    .clicked()
                {
                    self.view.scroll_to_page(0, &self.page_layouts);
                }

                // Previous page logic (reversed in RTL Horizontal layout)
                let go_prev =
                    ui.add_sized(egui::vec2(24.0, 24.0), egui::Button::new(prev_icon)).clicked();
                if go_prev {
                    if self.view.display_mode == DisplayMode::TwoPageSingle {
                        let spread = self.view.get_spread_indices(current_page, self.total_pages);
                        if let Some(&first_idx) = spread.first() {
                            let is_r2l = self.view.scroll_direction
                                == crate::view::ScrollDirection::Horizontal
                                && self.view.binding_direction
                                    == crate::view::BindingDirection::RightToLeft;
                            if is_r2l {
                                let last_idx = spread.last().copied().unwrap_or(first_idx);
                                if last_idx + 1 < self.total_pages {
                                    self.view.scroll_to_page(last_idx + 1, &self.page_layouts);
                                }
                            } else if first_idx > 0 {
                                self.view.scroll_to_page(first_idx - 1, &self.page_layouts);
                            }
                        }
                    } else if self.view.scroll_direction == crate::view::ScrollDirection::Horizontal
                        && self.view.binding_direction == crate::view::BindingDirection::RightToLeft
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
                let go_next =
                    ui.add_sized(egui::vec2(24.0, 24.0), egui::Button::new(next_icon)).clicked();
                if go_next {
                    if self.view.display_mode == DisplayMode::TwoPageSingle {
                        let spread = self.view.get_spread_indices(current_page, self.total_pages);
                        if let Some(&first_idx) = spread.first() {
                            let is_r2l = self.view.scroll_direction
                                == crate::view::ScrollDirection::Horizontal
                                && self.view.binding_direction
                                    == crate::view::BindingDirection::RightToLeft;
                            if is_r2l {
                                if first_idx > 0 {
                                    self.view.scroll_to_page(first_idx - 1, &self.page_layouts);
                                }
                            } else {
                                let last_idx = spread.last().copied().unwrap_or(first_idx);
                                if last_idx + 1 < self.total_pages {
                                    self.view.scroll_to_page(last_idx + 1, &self.page_layouts);
                                }
                            }
                        }
                    } else if self.view.scroll_direction == crate::view::ScrollDirection::Horizontal
                        && self.view.binding_direction == crate::view::BindingDirection::RightToLeft
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
                    .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_last_page"))
                    .clicked()
                {
                    self.view.scroll_to_page(self.total_pages - 1, &self.page_layouts);
                }

                ui.separator();

                // 1. Display Mode Toggle Group
                let is_continuous = self.view.display_mode == DisplayMode::Continuous;
                let cont_btn = egui::Button::new(egui::RichText::new("\u{e0ff}").size(14.0))
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
                let single_btn = egui::Button::new(egui::RichText::new("\u{e12c}").size(14.0))
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
                let spread_btn = egui::Button::new(egui::RichText::new("\u{e05f}").size(14.0))
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
                let is_vert = self.view.scroll_direction == crate::view::ScrollDirection::Vertical;
                let vert_btn =
                    egui::Button::new(egui::RichText::new("\u{e37d}").size(14.0)).selected(is_vert);
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
                let horiz_btn = egui::Button::new(egui::RichText::new("\u{e24a}").size(14.0))
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

                    if ui.checkbox(&mut self.view.cover_page_alone, "表紙単独").changed() {
                        self.compute_layouts();
                        let active = self.view.active_page;
                        self.view.scroll_to_page(active, &self.page_layouts);
                    }

                    let is_rtl =
                        self.view.binding_direction == crate::view::BindingDirection::RightToLeft;
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
                    .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_zoom_out"))
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
                    .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_zoom_in"))
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
                    .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_fit_width"))
                    .clicked()
                {
                    self.fit_to_width(viewport_rect);
                }
                if ui
                    .add_sized(
                        egui::vec2(24.0, 24.0),
                        egui::Button::new(egui::RichText::new("\u{e1c7}").size(14.0)),
                    )
                    .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_fit_height"))
                    .clicked()
                {
                    self.fit_to_height(viewport_rect);
                }

                if ui
                    .add_sized(
                        egui::vec2(24.0, 24.0),
                        egui::Button::new(egui::RichText::new("\u{e148}").size(14.0)),
                    )
                    .on_hover_text(self.locale_mgr.tr(&self.active_language, "cmd_reset_view"))
                    .clicked()
                {
                    self.reset_view();
                }
            },
        );
    }
}
