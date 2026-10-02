//! Where the view stands: the anchor kept across a re-layout, centring, and scrolling to a page.

use super::{Anchor, DisplayMode, PDFView, PageLayout, ScrollDirection};

impl PDFView {
    /// The page, and the point on it, that the view is anchored to in the layout in hand.
    ///
    /// The anchor point is wherever the last zoom was anchored — the cursor, for a wheel
    /// or a pinch — falling back to the middle of the window for a change that no gesture
    /// caused.
    #[must_use]
    pub fn take_anchor(&self, viewport: egui::Rect, layouts: &[PageLayout]) -> Option<Anchor> {
        let at = self.last_anchor.unwrap_or_else(|| viewport.center());
        let origin = self.get_origin(viewport);
        let layout = self.layout_under(at, origin, self.zoom, layouts)?;
        let page_screen_min = origin + layout.rect.min.to_vec2() * self.zoom;
        // **Clamped onto the page, because an anchor is a point *of* a page.** The cursor
        // is often beside one rather than on it — in a margin, in the gap between two
        // tiles — and `layout_under` then answers with the nearest page and an offset
        // outside it. Where the cursor is on the page this changes nothing; where it is
        // not, it puts the page's nearest edge under the cursor rather than leaving the
        // page off to one side of it.
        let local = (at - page_screen_min) / self.zoom;
        let size = layout.rect.size();
        let local = egui::vec2(local.x.clamp(0.0, size.x), local.y.clamp(0.0, size.y));
        Some(Anchor { page: layout.index, local })
    }

    /// Asks for `page` to be put in the middle of the window once the pages have been
    /// laid out again.
    ///
    /// **The intent, not the placement.** The layout the page will be placed into does
    /// not exist yet — the zoom that changes the arrangement has not been applied and the
    /// rectangles have not been recomputed — so the gesture records what it wants and
    /// `compute_layouts` answers it. Doing it by hand was three steps in the caller
    /// (`set_zoom`, recompute, scroll) and the one path across that boundary that did not
    /// go through the anchor.
    pub fn open_page(&mut self, page: usize) {
        self.centre_next = Some(page);
    }

    /// Puts the anchored point back under the cursor, in the new arrangement.
    ///
    /// **The cursor is the centre of every zoom, including the one that changes the
    /// arrangement.** That is the whole rule, and it is the same sentence for both: the
    /// point under the cursor does not move. The column and the grid put the same page in
    /// quite different places, so the view moves by whatever difference that makes — for
    /// a tile in the first column reaching a cursor on the right, the width of the window.
    ///
    /// Bounded by `clamp_pan` alone, which keeps the document on screen and nothing
    /// narrower: the grid's alignment says where its `x = 0` is, not where the grid has
    /// to sit.
    pub fn restore_anchor(
        &mut self,
        anchor: Option<Anchor>,
        viewport: egui::Rect,
        layouts: &[PageLayout],
    ) {
        self.arranged_as_tiles = !self.is_page_view();
        // A double-click, or a document that has just opened, asked for a page in the
        // middle. **The intent is left standing until there is a window to centre in**:
        // the first layout after a document loads runs before the viewport is known, and
        // taking the intent there would spend it on a rectangle of no size.
        if viewport.area() > 0.0
            && let Some(page) = self.centre_next.take()
            && self.centre_on(page, viewport, layouts)
        {
            return;
        }
        let Some(anchor) = anchor else { return };
        let Some(layout) = layouts.get(anchor.page) else { return };
        let at = self.last_anchor.unwrap_or_else(|| viewport.center());
        let origin_no_pan = self.get_origin_no_pan(viewport);
        self.pan = (at - origin_no_pan) - (layout.rect.min.to_vec2() + anchor.local) * self.zoom;
        self.active_page = anchor.page;
    }

    /// Moves from `from` to `page` by exactly the distance between them.
    ///
    /// **The button changes the page, not the composition.** Placing the new page against
    /// an edge or in the middle re-frames the window every time it is pressed: a reader
    /// looking a third of the way down page 8 presses *next* and the view jumps to a
    /// different arrangement of page 9, which is a second thing happening that they did
    /// not ask for. Moving by the pitch between the two leaves the window exactly where it
    /// was over the page and changes only which page is under it.
    ///
    /// `from` is `None` when the page the view was on is not in the layout it is being
    /// placed into, and then the page's near edge is where it starts.
    pub(super) fn place_along_the_scroll(&mut self, page: egui::Rect, from: Option<egui::Rect>) {
        if self.scroll_direction == ScrollDirection::Vertical {
            self.pan.y = match from {
                Some(from) => (page.min.y - from.min.y).mul_add(-self.zoom, self.pan.y),
                None => -page.min.y * self.zoom,
            };
            self.pan.x = 0.0;
        } else {
            self.pan.x = match from {
                Some(from) => (page.min.x - from.min.x).mul_add(-self.zoom, self.pan.x),
                None => -page.min.x * self.zoom,
            };
            self.pan.y = 0.0;
        }
    }

    pub fn center_on_rect(
        &mut self,
        viewport_rect: egui::Rect,
        page_layout: &PageLayout,
        rect: [f32; 4],
    ) {
        let pdf_center_x = f32::midpoint(rect[0], rect[2]);
        let pdf_center_y = f32::midpoint(rect[1], rect[3]);

        // The page's own point as the page is shown: box origin and turn taken into account.
        let local = page_layout.frame.shown(egui::pos2(pdf_center_x, pdf_center_y));

        // In virtual space (relative to layout center/top):
        let page_local_pos = page_layout.rect.min + local.to_vec2();

        // We want origin + page_local_pos * zoom = viewport_rect.center()
        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        self.pan = viewport_rect.center().to_vec2()
            - origin_no_pan.to_vec2()
            - page_local_pos.to_vec2() * self.zoom;
    }

    /// Keeps the page the view is on inside a document of `total` pages.
    ///
    /// **The pages can go away under the view.** An extraction moves them out, a deletion
    /// removes them, and until this runs `active_page` names a page that is not there —
    /// which in the spread view is an extent taken over nothing.
    pub const fn keep_page_inside(&mut self, total: usize) {
        if self.active_page >= total {
            self.active_page = total.saturating_sub(1);
        }
    }

    /// Ends whatever journey the page was on: nothing is arriving, nothing is adrift.
    ///
    /// **Anything that puts the view somewhere outright ends the travelling.** The offset
    /// a page is carrying is measured from where that page belonged, and a view sent to
    /// another page — by the page buttons, by a bookmark, by the crosshair — has left that
    /// somewhere behind: kept, it displaces the page it was sent to by as much as a window
    /// and then eases it in from nowhere in particular.
    pub(super) fn nothing_in_flight(&mut self) {
        self.adrift = 0.0;
        self.arriving = false;
    }

    /// Brings the page the reader is on to the middle of the window.
    pub fn centre_current_page(&mut self, viewport: egui::Rect, layouts: &[PageLayout]) {
        let page = self.current_page();
        self.centre_on(page, viewport, layouts);
    }

    /// Puts `page` in the middle of the window, and says whether there was such a page.
    ///
    /// **One home for the act.** The crosshair button, a double-click on the bench and a
    /// document that has just opened all want the same two lines, and three copies of
    /// them is three places for the rule to drift (UI-12).
    pub(super) fn centre_on(
        &mut self,
        page: usize,
        viewport: egui::Rect,
        layouts: &[PageLayout],
    ) -> bool {
        let Some(layout) = layouts.get(page) else { return false };
        let origin = self.get_origin_no_pan(viewport);
        self.pan = viewport.center() - origin - layout.rect.center().to_vec2() * self.zoom;
        self.active_page = page;
        self.nothing_in_flight();
        true
    }

    /// The page the reader is on, which is the page they went to.
    ///
    /// **It used to depend on the arrangement**, because a column of pages is a surface
    /// where the middle of the window is where you are, and a grid is a chooser where it
    /// is not — so this read `page_at_middle` for the column and `active_page` for the
    /// tiles. With the column gone both answer the same way: the page view stacks its
    /// pages on the origin and draws one, and the tiles have a page that was chosen.
    /// `page_at_middle` went with the column, having nothing left to be the middle of.
    ///
    /// It takes nothing now. The viewport and the layouts were what `page_at_middle`
    /// needed, and a signature that asks for what it does not use is a signature that
    /// says the answer might depend on them.
    #[must_use]
    pub const fn current_page(&self) -> usize {
        self.active_page
    }

    /// Moves the view to `page_index` and makes it the current one.
    ///
    /// **By the distance between the two pages, so that the view does not move.** See
    /// [`Self::place_along_the_scroll`].
    /// It takes no viewport: the tiles move by the pitch between two layouts and the page
    /// view centres on one, and neither reads the window.
    pub fn scroll_to_page(&mut self, page_index: usize, layouts: &[PageLayout]) {
        let was = self.current_page();
        self.active_page = page_index;
        // **The tiles are the one arrangement left that scrolls.** Going to a page in
        // them moves by the pitch between the two, which is what kept a reader's place on
        // the page while the page changed under them; the page view stacks its pages on
        // the origin, so there is nowhere to move along and the page is simply centred.
        if !self.is_page_view() {
            if let Some(layout) = layouts.get(page_index) {
                let from = layouts.get(was).map(|l| l.rect);
                self.place_along_the_scroll(layout.rect, from);
            }
        } else {
            // **A page and a spread are centred by the same two lines**, over whatever is
            // shown: one page, or the pair the page is half of. They were written out
            // twice, a `min`/`max` loop each, beside a third arm that nothing could reach
            // once a page view meant one of two arrangements.
            let ((min_x, max_x), (min_y, max_y)) = self.shown_extent(layouts);
            self.pan.x = -f32::midpoint(min_x, max_x) * self.zoom;
            self.pan.y = -f32::midpoint(min_y, max_y) * self.zoom;
        }
        self.nothing_in_flight();
    }

    /// The pages the viewport shows, each with the rect it occupies on screen.
    ///
    /// **Which pages are shown is one decision**, and it stood written out in `draw_pages`,
    /// in `draw_page_backings`, and — in a copy that had not been told the grid moved to
    /// the zoom — in the app's `collect_visible_pages_data`, which is what hands the
    /// renderer its work. A page one drew and another did not showed as a backing with no
    /// page on it, or as a tile that span for ever waiting for pixels nobody had asked for.
    /// Whether the page view shows `index`: it is the page, or one of the spread's pair.
    pub(super) fn shows(&self, index: usize, total: usize) -> bool {
        match self.display_mode {
            DisplayMode::SinglePage => index == self.active_page,
            DisplayMode::TwoPageSingle => {
                self.get_spread_indices(self.active_page, total).contains(&index)
            }
        }
    }

    pub(crate) fn visible_page_rects<'a>(
        &self,
        viewport_rect: egui::Rect,
        layouts: &'a [PageLayout],
    ) -> Vec<(&'a PageLayout, egui::Rect)> {
        let origin = self.get_origin(viewport_rect);
        layouts
            .iter()
            // **The tiles show the document, whatever the mode is.** A grid is a chooser
            // and a chooser that hid every page but the active one would be a page.
            .filter(|layout| !self.is_page_view() || self.shows(layout.index, layouts.len()))
            .map(|layout| {
                (
                    layout,
                    egui::Rect::from_min_size(
                        origin + layout.rect.min.to_vec2() * self.zoom,
                        layout.rect.size() * self.zoom,
                    ),
                )
            })
            .filter(|(_, page_rect)| viewport_rect.intersects(*page_rect))
            .collect()
    }
}
