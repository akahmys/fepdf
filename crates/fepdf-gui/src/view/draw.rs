//! What the viewport draws: the bench, the sheets, and every overlay over them.
//!
//! **Separated from what the viewport *is*.** `view.rs` had grown to 2,451 lines holding
//! four subjects — the view's state, what moves it, what the pointer does to it, and this
//! — and a file that long is one nobody reads the middle of. The state stays in the
//! parent module, which is why the fields below are reachable from here without being
//! public to the crate.

use super::{PDFView, PageLayout, PagePixels};
use crate::app::theme::canvas;
use crate::app::theme::colors;
use crate::app::theme::radius;
use crate::app::theme::space;
use std::collections::BTreeMap;

/// The sentences this view draws in its own voice, in the reader's language.
///
/// **A struct because there turned out to be two.** The placeholder card's was already
/// passed in, with a comment saying why — this type holds a view and not a locale — and
/// the signature field's was drawn in English beside it. One more positional `&str` would
/// have made the next one easy to forget in the same way.
pub struct Words<'a> {
    /// What a page that has not finished rendering says, with `{}` for its number.
    pub placeholder: &'a str,
    /// What is drawn across a signature field the reader is placing.
    pub signature: &'a str,
}

impl PDFView {
    pub fn show_virtual(
        // RR-15 Limit: Dispatcher - Renders a virtualized grid layout of PDF pages and overlays highlights/signals
        &mut self,
        ui: &mut egui::Ui,
        layouts: &[PageLayout],
        pixels: &PagePixels<'_>,
        viewport_rect: egui::Rect, // Unified viewport rect from app.rs
        scenes: &std::collections::BTreeMap<usize, std::sync::Arc<vello::Scene>>,
        highlights: &BTreeMap<usize, Vec<egui::Rect>>,
        redaction_highlights: &BTreeMap<usize, Vec<egui::Rect>>,
        active_redaction_drag: &Option<(usize, egui::Rect)>,
        structural_highlight: &Option<(usize, egui::Rect)>,
        signature_highlight: &Option<(usize, egui::Rect)>,
        selected_pages: &std::collections::BTreeSet<usize>,
        ust_registry: &crate::sidebar::USTRegistry,
        show_reading_order: bool,
        marquee_rect: Option<egui::Rect>,
        // What this view says in its own words, in the reader's language. **Passed in
        // rather than read here**: this type holds a view, not a locale. There were two
        // of these and only the first was passed; the second was drawn in English.
        words: &Words<'_>,
    ) {
        // A second copy of the block `App::ui` carried stood here, to stop "flashing
        // orange/red borders" — egui's old default selection colour, which
        // `theme::apply_global_styles` now sets deliberately. Four of its five lines
        // restated that, and the fifth set `selection.stroke` to `NONE`, whose colour is
        // transparent: the same setting that painted every selected widget's label
        // invisible in the chrome.
        let response = ui.allocate_rect(viewport_rect, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, viewport_rect, layouts);
        self.clamp_pan(viewport_rect, layouts);

        // 1. The bench, and the grid over it.
        //
        // **Both are drawn only where they can be seen.** In the viewport path the vello
        // texture below covers the whole viewport — it has to, because a storage texture
        // clears to `(0,0,0,0)` and egui's opaque shader renders that as black — so a
        // fill and a grid drawn here are painted and then hidden. Vello draws both for
        // that path, from `canvas::grid_lines`, so the two benches cannot drift apart.
        let covered = matches!(pixels, PagePixels::Viewport(Some(_)));
        if !covered {
            ui.painter().rect_filled(viewport_rect, radius::FLAT, colors::paper::CANVAS);
            Self::draw_canvas_grid(ui.painter(), viewport_rect, self.pan, self.zoom);

            // 2. Drop shadows and solid pure-white page backings
            self.draw_page_backings(ui.painter(), viewport_rect, layouts, scenes);
        }

        // 3. Unified viewport texture covering workspace. In thumbnail mode there is no
        //    such texture: each page paints its own inside the loop below.
        if let PagePixels::Viewport(Some(tid)) = pixels {
            ui.painter().image(
                *tid,
                viewport_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                // Not a colour: `Painter::image` multiplies the texture by this, and
                // white is the identity. Tinting the document would be the one thing
                // this window must never do. One of UI-9's three exemptions.
                egui::Color32::WHITE,
            );
        }

        let mut new_visible = Vec::new();

        let current = self.current_page(viewport_rect, layouts);
        for (layout, page_rect) in self.visible_page_rects(viewport_rect, layouts) {
            new_visible.push(layout.index);
            let is_selected = selected_pages.contains(&layout.index);

            // A thumbnail is this page's whole appearance, so it is painted before the
            // placeholder decides whether anything is missing.
            let thumbnail = match pixels {
                PagePixels::Thumbnails(map) => map.get(&layout.index).copied(),
                PagePixels::Viewport(_) => None,
            };
            if let Some(tid) = thumbnail {
                ui.painter().image(
                    tid,
                    page_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    // The identity multiplier again, and the second of UI-9's
                    // three exemptions.
                    egui::Color32::WHITE,
                );
            } else if thumbnail.is_none() && !scenes.contains_key(&layout.index) {
                Self::draw_placeholder_card(
                    ui.painter(),
                    page_rect,
                    layout.index,
                    words.placeholder,
                );
            } else if matches!(pixels, PagePixels::Thumbnails(_)) {
                // The scene is ready but its thumbnail is not yet: this frame made its
                // quota. Say so rather than showing a blank page backing.
                Self::draw_placeholder_card(
                    ui.painter(),
                    page_rect,
                    layout.index,
                    words.placeholder,
                );
            }

            // Page selection border. Selecting pages is the tile view's, so showing a
            // selection is too: a selection made there survives being zoomed into and
            // would otherwise mark a page the reader cannot select, deselect, or act
            // on. The state is kept, only not drawn.
            //
            // **Every sheet carries an edge, not only the ones in the grid.** White
            // paper on `paper::CANVAS` measures 1.09:1, which is not a boundary — in the
            // page view the sheet had none at all and its margin ran into the bench.
            // `steel::EDGE` is 3.43:1 against the bench (WCAG 1.4.11), and a selection replaces it with
            // the accent rather than adding a second line beside it.
            let (width, colour) = if is_selected && self.selects_pages() {
                (2.0_f32, colors::rust::ACCENT)
            } else {
                (1.0_f32, colors::steel::EDGE)
            };
            ui.painter().rect_stroke(
                page_rect,
                radius::FLAT,
                egui::Stroke::new(width, colour),
                egui::StrokeKind::Outside,
            );

            // Page number. The gap it sits in is in page units and the number is in
            // screen pixels, so the space shrinks with the zoom while the digits do
            // not: at 33% a 48-unit gap is 16 pixels and the number lands on the page
            // below. The gap is passed so the number can decline to be drawn.
            let gap = if self.is_page_view() {
                crate::app::FepdfApp::PAGE_GAP
            } else {
                crate::app::FepdfApp::TILE_ROW_GAP
            };
            Self::draw_page_number_badge(
                ui,
                page_rect,
                layout.index,
                (is_selected && self.selects_pages(), layout.index == current),
                self.zoom,
                gap * self.zoom,
            );

            // Overlays
            self.draw_selection_highlights(ui, layout.index, page_rect, layout, highlights);
            self.draw_redaction_highlights(ui, layout.index, redaction_highlights);
            self.draw_active_redaction_drag(ui, layout.index, active_redaction_drag);
            self.draw_structural_highlight(ui, layout.index, structural_highlight);
            self.draw_signature_highlight(ui, layout.index, signature_highlight, words);

            if show_reading_order && let Some(ref root) = ust_registry.root {
                Self::draw_semantic_borders(
                    ui,
                    page_rect,
                    self.zoom,
                    layout.rect.height(),
                    root,
                    layout.index,
                    ust_registry.selected_node_id,
                );
                self.draw_reading_order_bar(ui, page_rect, root, layout.index);
            }
        }

        Self::draw_marquee_overlay(ui.painter(), marquee_rect);

        self.visible_pages = new_visible;
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
    }

    fn draw_canvas_grid(
        painter: &egui::Painter,
        viewport_rect: egui::Rect,
        pan: egui::Vec2,
        zoom: f32,
    ) {
        let stroke = egui::Stroke::new(1.0_f32, canvas::grid_colour());
        let offset = viewport_rect.min.to_vec2();
        for [from, to] in canvas::grid_lines(viewport_rect.size(), pan, zoom) {
            painter.line_segment([from + offset, to + offset], stroke);
        }
    }

    fn draw_page_backings(
        &self,
        painter: &egui::Painter,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
        scenes: &std::collections::BTreeMap<usize, std::sync::Arc<vello::Scene>>,
    ) {
        for (layout, page_rect) in self.visible_page_rects(viewport_rect, layouts) {
            if scenes.contains_key(&layout.index) {
                for offset in 1..=4 {
                    painter.rect_filled(
                        page_rect.translate(egui::vec2(
                            f32::from(offset) * 1.5,
                            f32::from(offset) * 1.5,
                        )),
                        4.0,
                        colors::tint(colors::steel::TEXT, 20 - offset * 4),
                    );
                }
            }
            // **Only the thumbnail path sees this.** In the viewport path an opaque
            // vello texture covers the whole viewport a step later, so this fill and the
            // shadow above it are painted and then hidden; the sheet the reader sees
            // there is vello's. Its edge is therefore drawn after the texture, below.
            painter.rect_filled(page_rect, radius::FLAT, colors::paper::WHITE);
        }
    }

    fn draw_placeholder_card(
        painter: &egui::Painter,
        page_rect: egui::Rect,
        page_index: usize,
        placeholder: &str,
    ) {
        painter.rect_filled(page_rect, radius::CONTROL, colors::paper::WHITE);
        painter.rect_stroke(
            page_rect,
            4.0,
            egui::Stroke::new(1.0_f32, colors::steel::EDGE),
            egui::StrokeKind::Inside,
        );
        // **The sentence if the card can hold it, the icon if it cannot.** A tile at the
        // zoom floor is 119 points wide and "page 12 is still drawing" is not; the text
        // used to be drawn anyway, overflowing the sheet it was about. Measured rather
        // than decided by view mode, so a narrow page in the page view answers the same
        // way a tile does.
        let words = placeholder.replace("{}", &(page_index + 1).to_string());
        let galley = painter.layout_no_wrap(
            words,
            egui::FontId::proportional(crate::app::theme::text::HEAD),
            colors::steel::MUTED,
        );
        if says_it_in_words(galley.size().x, page_rect.width()) {
            painter.galley(page_rect.center() - galley.size() / 2.0, galley, colors::steel::MUTED);
            return;
        }
        painter.text(
            page_rect.center(),
            egui::Align2::CENTER_CENTER,
            crate::app::icons::glyph::LOADING,
            egui::FontId::new(
                crate::app::theme::size::GLYPH.min(page_rect.width() / 3.0),
                crate::app::theme::icon_family(),
            ),
            colors::steel::EDGE,
        );
    }

    fn draw_marquee_overlay(painter: &egui::Painter, marquee_rect: Option<egui::Rect>) {
        if let Some(m_rect) = marquee_rect {
            painter.rect_filled(m_rect, radius::FLAT, colors::rust::wash());
            painter.rect_stroke(
                m_rect,
                0.0,
                egui::Stroke::new(1.5_f32, colors::rust::ACCENT),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// The page's number, under it, and whether the reader is on it.
    ///
    /// **Current and selected are two different things, and only one of them is the
    /// accent.** A selection is what an operation would act on; the current page is where
    /// the reader is — the page the arrows step from and the page the view was showing
    /// before it came out to the grid. Rust means "you are touching this" (UI-10) and is
    /// already spoken for, so *current* is a rule under the number instead. It inherits
    /// the number's colour, so a page that is both reads as both without a third mark.
    #[allow(clippy::fn_params_excessive_bools)]
    fn draw_page_number_badge(
        ui: &mut egui::Ui,
        page_rect: egui::Rect,
        page_index: usize,
        marks: (bool, bool),
        zoom: f32,
        gap_px: f32,
    ) {
        let (is_selected, is_current) = marks;
        let badge_text = format!("{}", page_index + 1);
        let font_size = if zoom < Self::TILE_ZOOM { 11.0 } else { 12.0 };
        // The number is set on the canvas, not in a chip. A filled rounded rectangle with
        // a border around a two-digit number is a control the reader cannot press, and a
        // grid of them reads as a row of buttons between the rows of pages.
        // **The current page's number is set in the darkest steel, not the faintest.**
        // A page number is a label and labels are muted; the one the reader is on is not
        // a label but an answer, and it carries the rule below it, which was legible and
        // easy to miss on its own.
        let colour = if is_selected {
            colors::rust::ACCENT
        } else if is_current {
            colors::steel::TEXT
        } else {
            colors::steel::MUTED
        };

        let galley =
            ui.painter().layout_no_wrap(badge_text, egui::FontId::proportional(font_size), colour);
        // The same figure the gaps are sized from, so that the two cannot drift: a gap
        // narrowed below what a number needs stops drawing numbers rather than putting one
        // on the page below.
        if gap_px < Self::PAGE_NUMBER_SPACE {
            return;
        }
        let offset = 6.0_f32.min((gap_px - galley.size().y).max(0.0) / 2.0);
        let size = galley.size();
        let at = egui::pos2(page_rect.center().x - size.x / 2.0, page_rect.max.y + offset);
        ui.painter().galley(at, galley, colour);
        if is_current {
            // Two points thick and a little wider than the digits, so that a single `1`
            // is as plainly marked as a `23`.
            let under = at.y + size.y;
            let reach = space::ITEM;
            ui.painter().line_segment(
                [egui::pos2(at.x - reach, under), egui::pos2(at.x + size.x + reach, under)],
                egui::Stroke::new(2.0_f32, colour),
            );
        }
    }

    /// Draws what is selected on this page.
    ///
    /// **Mapped to the screen here, every frame, from the page's own coordinates.** The
    /// rects used to be computed once when the drag made them and drawn as they stood, so
    /// zooming or panning left the selection behind over whatever had moved into its
    /// place. A selection belongs to the page.
    fn draw_selection_highlights(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        page_rect: egui::Rect,
        layout: &PageLayout,
        highlights: &BTreeMap<usize, Vec<egui::Rect>>,
    ) {
        let Some(on_page) = highlights.get(&page_index) else {
            return;
        };
        let unscaled_h = layout.rect.height();
        for span in on_page {
            let rect = crate::interaction::SelectionManager::highlight_rect(
                page_rect, self.zoom, unscaled_h, *span,
            );
            ui.painter().rect_filled(rect, radius::FLAT, colors::rust::wash());
        }
    }

    fn draw_redaction_highlights(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        redaction_highlights: &BTreeMap<usize, Vec<egui::Rect>>,
    ) {
        if let Some(redact_rects) = redaction_highlights.get(&page_index) {
            for redact_rect in redact_rects {
                // **Drawn as what it will become.** Burning a redaction writes black, so
                // the fill is black and needs no colour of its own; the accent on the
                // edge is the part that says it can still be taken back.
                // Black because burning writes black, not because black was chosen: this
                // is the document's future state rather than a colour of this window's.
                // The third of UI-9's exemptions.
                ui.painter().rect_filled(*redact_rect, radius::FLAT, egui::Color32::BLACK);
                canvas::haloed_rect(
                    ui.painter(),
                    *redact_rect,
                    radius::FLAT,
                    egui::Stroke::new(1.5_f32, colors::rust::ACCENT),
                );
                if redact_rect.width() > 60.0 && redact_rect.height() > 12.0 {
                    ui.painter().text(
                        redact_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "[REDACTED]",
                        egui::FontId::monospace(crate::app::theme::text::SMALL),
                        colors::paper::WHITE,
                    );
                }
            }
        }
    }

    fn draw_active_redaction_drag(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        active_redaction_drag: &Option<(usize, egui::Rect)>,
    ) {
        if let Some((active_page, drag_rect)) = active_redaction_drag
            && *active_page == page_index
        {
            ui.painter().rect_filled(
                *drag_rect,
                radius::FLAT,
                colors::tint(colors::rust::ACCENT, 60),
            );
            canvas::haloed_rect(
                ui.painter(),
                *drag_rect,
                radius::FLAT,
                egui::Stroke::new(1.5_f32, colors::rust::ACCENT),
            );
        }
    }

    fn draw_structural_highlight(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        structural_highlight: &Option<(usize, egui::Rect)>,
    ) {
        if let Some((highlight_page, highlight_rect)) = structural_highlight
            && *highlight_page == page_index
        {
            let time = ui.ctx().input(|i| i.time);
            let pulse = (time * 6.0).sin().abs() as f32;
            let fill_opacity = 20 + (pulse * 35.0) as u8;
            let stroke_w = 2.0 + pulse * 2.0;

            canvas::haloed_rect(
                ui.painter(),
                *highlight_rect,
                radius::FLAT,
                egui::Stroke::new(stroke_w, colors::rust::ACCENT),
            );
            ui.painter().rect_filled(
                *highlight_rect,
                radius::FLAT,
                colors::tint(colors::rust::ACCENT, fill_opacity),
            );
            ui.ctx().request_repaint();
        }
    }

    fn draw_signature_highlight(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        signature_highlight: &Option<(usize, egui::Rect)>,
        words: &Words<'_>,
    ) {
        if let Some((sig_page, sig_rect)) = signature_highlight
            && *sig_page == page_index
        {
            // A second terracotta `(226, 135, 67)` stood here, near enough to the accent
            // to read as it and far enough to be a different colour. The field is being
            // placed by the reader, so it is the accent.
            let diagonal = egui::Stroke::new(1.0_f32, colors::tint(colors::rust::ACCENT, 100));
            ui.painter().rect_filled(
                *sig_rect,
                radius::CONTROL,
                colors::tint(colors::rust::ACCENT, 30),
            );
            canvas::haloed_rect(
                ui.painter(),
                *sig_rect,
                radius::CONTROL,
                egui::Stroke::new(2.0_f32, colors::rust::ACCENT),
            );
            ui.painter().line_segment([sig_rect.left_top(), sig_rect.right_bottom()], diagonal);
            ui.painter().line_segment([sig_rect.right_top(), sig_rect.left_bottom()], diagonal);
            canvas::haloed_text(
                ui.painter(),
                sig_rect.center(),
                egui::Align2::CENTER_CENTER,
                words.signature,
                egui::FontId::monospace(crate::app::theme::text::BODY),
                colors::rust::ACCENT,
            );
        }
    }
    /// One element's rectangle, from PDF user space to the screen.
    fn element_rect(
        page_rect: egui::Rect,
        zoom: f32,
        unscaled_h: f32,
        rect: [f32; 4],
    ) -> egui::Rect {
        let corner = |x: f32, y: f32| {
            crate::interaction::SelectionManager::pdf_to_screen(
                page_rect,
                zoom,
                unscaled_h,
                egui::pos2(x, y),
            )
        };
        egui::Rect::from_min_max(corner(rect[0], rect[3]), corner(rect[2], rect[1]))
    }

    /// **Filtered by page.** A structure tree covers the whole document, and this walks
    /// all of it once per visible page — so without the test below, page 2 is drawn with
    /// page 1's boxes on top of it. That went unseen for as long as no element had a
    /// rectangle at all, which was until they were derived from marked content.
    fn draw_semantic_borders(
        ui: &mut egui::Ui,
        page_rect: egui::Rect,
        zoom: f32,
        unscaled_h: f32,
        node: &crate::sidebar::USTNode,
        page_index: usize,
        selected_id: Option<usize>,
    ) {
        let here = node.page_index == Some(page_index);
        if let Some(rect) = node.rect
            && here
        {
            let element_rect = Self::element_rect(page_rect, zoom, unscaled_h, rect);

            // **The box says which tag it is, instead of being coloured for it.** Four
            // hues stood here — and again, verbatim, in `collect_nodes_for_reading_order`
            // — so a reader had to remember that purple meant `Figure`. The name is two
            // characters wide and needs remembering by nobody.
            let is_selected = Some(node.id) == selected_id;
            let (width, colour) = if is_selected {
                (2.0_f32, colors::rust::ACCENT)
            } else {
                (1.0_f32, colors::steel::EDGE)
            };
            canvas::haloed_rect(
                ui.painter(),
                element_rect,
                radius::CONTROL,
                egui::Stroke::new(width, colour),
            );
            if element_rect.width() > 48.0 && element_rect.height() > 16.0 {
                canvas::haloed_text(
                    ui.painter(),
                    element_rect.left_top() + egui::vec2(3.0, 2.0),
                    egui::Align2::LEFT_TOP,
                    &node.tag,
                    egui::FontId::monospace(crate::app::theme::text::SMALL),
                    colour,
                );
            }
        }

        for child in &node.children {
            Self::draw_semantic_borders(
                ui,
                page_rect,
                zoom,
                unscaled_h,
                child,
                page_index,
                selected_id,
            );
        }
    }

    /// The tags in reading order.
    ///
    /// **The colour it used to carry alongside each tag was the same four-hue table as
    /// `draw_semantic_borders`, written out a second time.** Each chip is labelled
    /// `"3: Figure"`, so the hue restated the label beside it.
    fn collect_nodes_for_reading_order(
        node: &crate::sidebar::USTNode,
        page_index: usize,
        list: &mut Vec<String>,
    ) {
        if node.rect.is_some() && node.page_index == Some(page_index) {
            list.push(node.tag.clone());
        }
        for child in &node.children {
            Self::collect_nodes_for_reading_order(child, page_index, list);
        }
    }

    fn draw_reading_order_bar(
        &self,
        ui: &mut egui::Ui,
        page_rect: egui::Rect,
        root_node: &crate::sidebar::USTNode,
        page_index: usize,
    ) {
        let mut list = Vec::new();
        Self::collect_nodes_for_reading_order(root_node, page_index, &mut list);

        if list.is_empty() {
            return;
        }

        let bar_height = 24.0;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(page_rect.left(), page_rect.bottom() + 8.0),
            egui::vec2(page_rect.width(), bar_height),
        );

        ui.painter().rect_filled(bar_rect, radius::CONTROL, colors::steel::TEXT);

        let mut x_offset = bar_rect.left() + 4.0;
        for (i, tag) in list.iter().enumerate() {
            let label = format!("{}: {}", i + 1, tag);
            let text_gal = ui.painter().layout_no_wrap(
                label.clone(),
                egui::FontId::proportional(crate::app::theme::text::SMALL),
                colors::paper::WHITE,
            );
            let block_width = text_gal.size().x + 12.0;

            if x_offset + block_width > bar_rect.right() - 4.0 {
                break;
            }

            let block_rect = egui::Rect::from_min_size(
                egui::pos2(x_offset, bar_rect.top() + 3.0),
                egui::vec2(block_width, bar_height - 6.0),
            );

            ui.painter().rect_filled(block_rect, radius::CONTROL, colors::steel::MUTED);
            ui.painter().text(
                block_rect.center(),
                egui::Align2::CENTER_CENTER,
                &label,
                egui::FontId::proportional(crate::app::theme::text::SMALL),
                colors::paper::WHITE,
            );

            x_offset += block_width + 6.0;
        }
    }
}

/// Whether a card that is still drawing can say so in words, or has to use the icon.
///
/// **Measured, not decided by the view mode.** A tile at the zoom floor is 119 points
/// wide and "page 12 is still drawing" is not, so the sentence used to be drawn over the
/// sheet it was about and past both its edges. A narrow page in the page view answers the
/// same way a tile does, which is the point of asking the width rather than the mode.
fn says_it_in_words(text_width: f32, card_width: f32) -> bool {
    text_width + space::PANE < card_width
}

#[cfg(test)]
mod placeholder_fit {
    use super::says_it_in_words;

    /// **A tile cannot hold a sentence, and a page can.** At the zoom floor an A4 tile is
    /// 119 points across; the placeholder used to draw its sentence there anyway, over the
    /// sheet it was about and past both of its edges.
    #[test]
    fn a_tile_uses_the_icon_and_a_page_uses_the_words() {
        let sentence = 180.0; // what "page 12 is still drawing" measures at HEAD
        assert!(!says_it_in_words(sentence, 119.0), "a tile tried to hold the sentence");
        assert!(says_it_in_words(sentence, 612.0), "a page fell back to the icon");
    }

    /// The margin is part of the question: a sentence that exactly fills a card is a
    /// sentence touching both its edges.
    #[test]
    fn a_sentence_that_only_just_fits_does_not() {
        assert!(!says_it_in_words(100.0, 101.0));
        assert!(says_it_in_words(100.0, 200.0));
    }
}
