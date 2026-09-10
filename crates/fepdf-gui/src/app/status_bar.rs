//! The status bar: what the engine is doing on the left, what the view is doing on the
//! right.
//!
//! **The right-hand cluster is built in reverse.** `Layout::right_to_left` places the
//! first widget furthest right, so the groups below are added last-on-screen first. That
//! inversion is stated once, here, and the groups are named — reading fourteen anonymous
//! buttons backwards is how a control that draws nothing went unnoticed.

use super::FepdfApp;
use super::icons::{glyph, icon_button};
use super::theme::{colors, size, space, text};
use crate::view::{BindingDirection, DisplayMode};

impl FepdfApp {
    pub(crate) fn render_status_bar(&mut self, ui: &mut egui::Ui) {
        let has_doc = self.total_pages > 0;
        let current_page = if self.view.display_mode == DisplayMode::SinglePage {
            self.view.active_page
        } else {
            self.view.visible_pages.first().copied().unwrap_or(self.view.active_page)
        };

        egui::Panel::bottom("status_bar").default_size(size::STATUS).resizable(false).show_inside(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = space::ITEM;
                    self.status_indicators(ui, has_doc);
                    self.view_controls(ui, has_doc, current_page);
                });
            },
        );
    }

    /// The left of the bar: what the renderer left out, which view mode is in force,
    /// and the reading-order toggle.
    fn status_indicators(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        // Pages the renderer left out say so here rather than in the decision sidebar:
        // see `FepdfApp::pages_left_out` for why that is not a `Decision`.
        if self.pages_left_out > 0 {
            let notice = self
                .locale_mgr
                .tr(&self.active_language, "status_pages_over_budget")
                .replacen("{}", &self.pages_left_out.to_string(), 1);
            ui.label(egui::RichText::new(notice).size(text::SMALL).color(colors::note::WARN));
        } else {
            ui.label(
                egui::RichText::new(self.locale_mgr.tr(&self.active_language, "status_ready"))
                    .size(text::SMALL),
            );
        }

        // Which mode the view is in. Nothing announces that content interactions have
        // stopped below `PDFView::TILE_ZOOM`; this is where a reader finds out which
        // side of that boundary they are on.
        if has_doc {
            ui.add_space(space::SECTION);
            let key =
                if self.view.is_page_view() { "status_mode_page" } else { "status_mode_tile" };
            ui.label(
                egui::RichText::new(self.locale_mgr.tr(&self.active_language, key))
                    .size(text::SMALL)
                    .color(colors::steel::MUTED),
            );
        }

        ui.add_space(space::SECTION);

        let reading_txt = if self.show_reading_order {
            self.locale_mgr.tr(&self.active_language, "reading_order_enabled")
        } else {
            self.locale_mgr.tr(&self.active_language, "reading_order_disabled")
        };
        let reading = egui::RichText::new(reading_txt).size(text::SMALL);
        if ui.selectable_label(self.show_reading_order, reading).clicked() {
            self.show_reading_order = !self.show_reading_order;
        }
    }

    /// The right of the bar, added in reverse of how it reads: see the module note.
    fn view_controls(&mut self, ui: &mut egui::Ui, has_doc: bool, current_page: usize) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(space::ITEM);
            if !has_doc {
                return;
            }
            self.document_group(ui);
            ui.add_space(space::SECTION);
            self.mode_group(ui);
            ui.add_space(space::SECTION);
            self.fit_group(ui);
            ui.add_space(space::SECTION);
            self.zoom_group(ui);
            ui.add_space(space::SECTION);
            self.page_group(ui, current_page);
        });
    }

    /// Rightmost: what to do to the document itself.
    fn document_group(&mut self, ui: &mut egui::Ui) {
        if ui.add(icon_button(glyph::ROTATE, false)).on_hover_text("ページを右に90°回転").clicked()
        {
            self.rotate_selected_pages(fepdf::Quarter::Q90);
        }

        let is_r2l = self.view.binding_direction == BindingDirection::RightToLeft;
        let label = egui::RichText::new(if is_r2l { "縦" } else { "横" }).size(text::BODY);
        let binding = egui::Button::new(label)
            .min_size(egui::vec2(size::ICON, size::ICON))
            .corner_radius(super::theme::radius::CONTROL)
            .selected(is_r2l);
        if ui
            .add(binding)
            .on_hover_text(if is_r2l {
                "縦書き / 右開き順 (R2L) — クリックで横書きに切替"
            } else {
                "横書き / 左開き順 (LTR) — クリックで縦書きに切替"
            })
            .clicked()
        {
            self.view.binding_direction =
                if is_r2l { BindingDirection::LeftToRight } else { BindingDirection::RightToLeft };
            self.compute_layouts();
        }
    }

    /// How the pages are arranged. Added spread-first, so it reads continuous, single,
    /// spread.
    fn mode_group(&mut self, ui: &mut egui::Ui) {
        for (mode, icon, tip) in [
            (DisplayMode::TwoPageSpread, glyph::PAGE_SPREAD, "見開き表示"),
            (DisplayMode::SinglePage, glyph::PAGE_SINGLE, "単一ページ表示"),
            (DisplayMode::Continuous, glyph::PAGE_CONTINUOUS, "連続スクロール"),
        ] {
            let selected = self.view.display_mode == mode;
            if ui.add(icon_button(icon, selected)).on_hover_text(tip).clicked() {
                self.view.display_mode = mode;
                self.compute_layouts();
            }
        }
    }

    /// Fit the spread to the window.
    fn fit_group(&mut self, ui: &mut egui::Ui) {
        let viewport = self.last_viewport_rect.unwrap_or_else(|| ui.max_rect());
        if ui.add(icon_button(glyph::FIT_HEIGHT, false)).on_hover_text("高さに合わせる").clicked()
        {
            self.fit_to_height(viewport);
        }
        if ui.add(icon_button(glyph::FIT_WIDTH, false)).on_hover_text("幅に合わせる").clicked()
        {
            self.fit_to_width(viewport);
        }
    }

    /// Zoom in, the current factor, zoom out — reading out, factor, in.
    fn zoom_group(&mut self, ui: &mut egui::Ui) {
        let viewport = self.last_viewport_rect.unwrap_or_else(|| ui.max_rect());
        let center = viewport.center();

        if ui.add(icon_button(glyph::ZOOM_IN, false)).on_hover_text("拡大").clicked() {
            self.view.zoom_at(self.view.zoom_step_up(), center, viewport, &self.page_layouts);
        }

        let label = egui::RichText::new(self.view.zoom_label()).size(text::SMALL);
        let reset = egui::Button::new(label)
            .min_size(egui::vec2(size::ICON * 1.5, size::ICON))
            .corner_radius(super::theme::radius::CONTROL);
        if ui.add(reset).on_hover_text("ズームリセット (100%)").clicked() {
            self.view.zoom_at(1.0, center, viewport, &self.page_layouts);
        }

        if ui.add(icon_button(glyph::ZOOM_OUT, false)).on_hover_text("縮小").clicked() {
            self.view.zoom_at(self.view.zoom_step_down(), center, viewport, &self.page_layouts);
        }
    }

    /// The page counter and the four buttons around it, reading first, previous, `n/N`,
    /// next, last.
    fn page_group(&mut self, ui: &mut egui::Ui, current_page: usize) {
        if ui.add(icon_button(glyph::PAGE_LAST, false)).on_hover_text("最後のページへ").clicked()
        {
            self.view.scroll_to_page(self.total_pages - 1, &self.page_layouts);
        }

        if ui.add(icon_button(glyph::PAGE_NEXT, false)).on_hover_text("次のページ").clicked()
            && current_page + 1 < self.total_pages
        {
            self.view.scroll_to_page(current_page + 1, &self.page_layouts);
        }

        ui.label(
            egui::RichText::new(format!("{}/{}", current_page + 1, self.total_pages))
                .size(text::SMALL)
                .color(colors::steel::TEXT),
        );

        if ui.add(icon_button(glyph::PAGE_PREV, false)).on_hover_text("前のページ").clicked()
            && current_page > 0
        {
            self.view.scroll_to_page(current_page - 1, &self.page_layouts);
        }

        if ui.add(icon_button(glyph::PAGE_FIRST, false)).on_hover_text("最初のページへ").clicked()
        {
            self.view.scroll_to_page(0, &self.page_layouts);
        }
    }
}
