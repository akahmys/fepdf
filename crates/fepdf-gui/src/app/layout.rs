//! View and page layout calculations for `FepdfApp`.

use super::FepdfApp;
use crate::view::{BindingDirection, DisplayMode, PageLayout, ScrollDirection};

impl FepdfApp {
    pub fn reset_view(&mut self) {
        self.view.set_zoom(1.0);
        self.view.pan = egui::Vec2::ZERO;
    }

    pub fn fit_to_width(&mut self, viewport_rect: egui::Rect) {
        let current_page =
            self.view.visible_pages.first().copied().unwrap_or(self.view.active_page);
        let mut indices = vec![current_page];
        if self.view.display_mode == DisplayMode::TwoPageSpread
            || self.view.display_mode == DisplayMode::TwoPageSingle
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
            self.view.set_zoom(target_zoom);
            self.view.pan.x = 0.0;
        }
    }

    pub fn fit_to_height(&mut self, viewport_rect: egui::Rect) {
        let current_page =
            self.view.visible_pages.first().copied().unwrap_or(self.view.active_page);
        let mut indices = vec![current_page];
        if self.view.display_mode == DisplayMode::TwoPageSpread
            || self.view.display_mode == DisplayMode::TwoPageSingle
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
            self.view.set_zoom(target_zoom);
            if self.view.display_mode == DisplayMode::Continuous
                || self.view.display_mode == DisplayMode::TwoPageSpread
                || self.view.display_mode == DisplayMode::TwoPageSingle
            {
                self.view.pan.y = -min_y * self.view.zoom();
            } else {
                self.view.pan.y = 0.0;
            }
            self.view.pan.x = 0.0;
        }
    }

    /// How many pages a row of tiles holds.
    ///
    /// **Fixed, so that zooming does not rearrange the document.** The count used to come
    /// from the viewport width divided by the zoom, which meant every zoom step reflowed
    /// the grid and pages changed rows underneath the cursor — zooming was a rearrangement
    /// that happened to also change scale. With the count fixed, the arrangement lives in
    /// page space and does not depend on the zoom or the window at all, so zooming is what
    /// it looks like: moving towards or away from one fixed sheet of pages.
    ///
    /// The cost is that the grid no longer fits itself to the window. Ten A4 pages across
    /// is 6,430 page units, which at 22% is 1,415 pixels and fills an ordinary window, at
    /// 25% overflows it, and at the zoom floor occupies less than half of it. That is what
    /// a fixed arrangement looks like from different distances.
    pub const TILE_COLUMNS: usize = 10;

    /// The space between rows of tiles, in page units.
    ///
    /// **Wider than the space between columns, because a row break is a bigger break than a
    /// column one.** With both at [`Self::PAGE_GAP`] the grid read as an even field of
    /// pages and the eye had nothing to travel along; the page numbers sit in this space
    /// too, and needed room to be read as labels rather than as a line of their own.
    pub const TILE_ROW_GAP: f32 = 144.0;

    /// The space between pages, in page units.
    ///
    /// **It is in page units, so it shrinks with the zoom.** At the floor the old 24 drew
    /// as 2.4 pixels and neighbouring pages read as one sheet; a page in the tile grid is
    /// told apart by its gap and its border, and at that size the gap was doing none of the
    /// work. Widening it costs nothing at reading zoom, where 24 was already narrower than
    /// most of the margins inside the pages it separated.
    pub const PAGE_GAP: f32 = 72.0;

    /// Recomputes page rectangles from `doc_page_sizes` and the current view mode.
    pub fn compute_layouts(&mut self) {
        // RR-15 Limit: GUI
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
            // Continuous display: one column of pages, or a fixed grid of tiles. Neither
            // arrangement reads the zoom or the window — only which of the two applies
            // does — so zooming moves the view over a layout that is holding still.
            let gap_x = Self::PAGE_GAP;
            let gap_y = if self.view.is_page_view() { Self::PAGE_GAP } else { Self::TILE_ROW_GAP };

            if self.view.scroll_direction == ScrollDirection::Vertical {
                if self.view.is_page_view() {
                    Self::column_rows(&self.doc_page_sizes, gap_y, &mut layouts);
                } else {
                    Self::grid_rows(
                        &self.doc_page_sizes,
                        Self::TILE_COLUMNS,
                        gap_x,
                        gap_y,
                        self.view.binding_direction == BindingDirection::RightToLeft,
                        &mut layouts,
                    );
                }
            } else {
                let mut current_offset = 0.0;
                for (i, &(w, h)) in self.doc_page_sizes.iter().enumerate() {
                    let w = w as f32;
                    let h = h as f32;
                    let r = egui::Rect::from_min_size(
                        egui::pos2(current_offset, -h / 2.0),
                        egui::vec2(w, h),
                    );
                    current_offset += w + gap_x;
                    layouts[i] = PageLayout { index: i, rect: r };
                }
            }
        }
        self.page_layouts = layouts;
    }

    /// Stacks the pages in one column, each centred on `x = 0`.
    ///
    /// **A page sits somewhere quite different here than it does in the grid**, which is
    /// why crossing between the two views has to recompute before it scrolls: page 25 is at
    /// `2 * 954` in a ten-wide grid and at `25 * 890` in a column, twelve times further
    /// down. Scrolling to a page with the layout of the view being left from lands near the
    /// top of the document.
    fn column_rows(sizes: &[(f64, f64)], gap_y: f32, layouts: &mut [PageLayout]) {
        let mut offset_y = 0.0_f32;
        for (i, &(w, h)) in sizes.iter().enumerate() {
            let (w, h) = (w as f32, h as f32);
            layouts[i] = PageLayout {
                index: i,
                rect: egui::Rect::from_min_size(egui::pos2(-w / 2.0, offset_y), egui::vec2(w, h)),
            };
            offset_y += h + gap_y;
        }
    }

    /// Places pages left to right in rows of `columns`.
    ///
    /// **The cell used to be page 1's size, for every page.** The column count, the pitch
    /// and the centring all came from `doc_page_sizes.first()`, so a page wider than
    /// `ref_w + 2 * gap_x` was drawn on top of its neighbour — an A3 landscape sheet in an
    /// A4 document overlapped the next page by 274pt, 46% of its width. Rows of actual
    /// widths cannot overlap, because each page is placed after the one before it.
    ///
    /// **The count is fixed rather than fitted to the window.** Fitting made the layout a
    /// function of the zoom, so zooming reflowed the grid and moved pages between rows
    /// while the reader was trying to look at one; see [`Self::TILE_COLUMNS`].
    ///
    /// Both corpora are documents of a single page size — 9 samples and 515 external
    /// files, no mixed-size document among them — where this is an even grid. Mixed sizes
    /// give ragged rows, which is the case that was previously drawn overlapping.
    fn grid_rows(
        sizes: &[(f64, f64)],
        columns: usize,
        gap_x: f32,
        gap_y: f32,
        is_r2l: bool,
        layouts: &mut [PageLayout],
    ) {
        let columns = columns.max(1);
        let mut offset_y = 0.0_f32;

        for row in (0..sizes.len()).collect::<Vec<_>>().chunks(columns) {
            let row_w = row.iter().enumerate().fold(0.0_f32, |w, (n, &i)| {
                let page_w = sizes[i].0 as f32;
                if n == 0 { page_w } else { w + gap_x + page_w }
            });
            offset_y +=
                Self::place_row(sizes, row, row_w, offset_y, gap_x, is_r2l, layouts) + gap_y;
        }
    }

    /// Lays one row out centred on `x = 0`, and answers its height.
    ///
    /// Pages of differing heights are centred against the tallest, which is what the fixed
    /// grid did against its cell and the one part of it that was right.
    fn place_row(
        sizes: &[(f64, f64)],
        row: &[usize],
        row_w: f32,
        offset_y: f32,
        gap_x: f32,
        is_r2l: bool,
        layouts: &mut [PageLayout],
    ) -> f32 {
        let row_h = row.iter().map(|&i| sizes[i].1 as f32).fold(0.0_f32, f32::max);
        let mut x = -row_w / 2.0;
        for &i in row {
            let (w, h) = (sizes[i].0 as f32, sizes[i].1 as f32);
            // A right-bound book's grid runs right to left, for the same reason its
            // spread does: the reader's eye starts at the right edge. Mirroring the row
            // about its own centre puts page order there without moving the row.
            let pos_x = if is_r2l { -x - w } else { x };
            let rect = egui::Rect::from_min_size(
                egui::pos2(pos_x, offset_y + (row_h - h) / 2.0),
                egui::vec2(w, h),
            );
            layouts[i] = PageLayout { index: i, rect };
            x += w + gap_x;
        }
        row_h
    }
}

/// A row break is the bigger break of the two, and a build says so.
///
/// **At module scope, because an associated constant nobody reads is never evaluated.**
/// The same assertion inside `impl FepdfApp` compiled happily with the rows closer together
/// than the columns; inside a `#[cfg(test)]` module it would only be checked when the tests
/// were built. Here every build refuses the pair.
const _ROW_BREAK_IS_BIGGER: () =
    assert!(FepdfApp::TILE_ROW_GAP > FepdfApp::PAGE_GAP, "rows must be further apart than columns");

#[cfg(test)]
mod tiles {
    use super::super::FepdfApp;
    use crate::view::PageLayout;

    const GAP: f32 = FepdfApp::PAGE_GAP;
    const COLS: usize = FepdfApp::TILE_COLUMNS;
    const ROW_GAP: f32 = FepdfApp::TILE_ROW_GAP;

    /// Rows are compared by position rather than by exact equality: the values come from
    /// the same arithmetic and do match exactly, but a layout test that depends on that
    /// would break on an unrelated reordering of the sum.
    fn same(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn lay(sizes: &[(f64, f64)]) -> Vec<egui::Rect> {
        laid(sizes, false)
    }

    fn column(sizes: &[(f64, f64)]) -> Vec<egui::Rect> {
        let mut layouts = vec![PageLayout { index: 0, rect: egui::Rect::NOTHING }; sizes.len()];
        FepdfApp::column_rows(sizes, GAP, &mut layouts);
        layouts.into_iter().map(|l| l.rect).collect()
    }

    fn laid(sizes: &[(f64, f64)], is_r2l: bool) -> Vec<egui::Rect> {
        let mut layouts = vec![PageLayout { index: 0, rect: egui::Rect::NOTHING }; sizes.len()];
        FepdfApp::grid_rows(sizes, COLS, GAP, ROW_GAP, is_r2l, &mut layouts);
        layouts.into_iter().map(|l| l.rect).collect()
    }

    /// **The layout takes no zoom and no viewport.** This is the whole point of fixing the
    /// column count: the arrangement lives in page space, so zooming is a change of scale
    /// over something holding still rather than a rearrangement that also changes scale.
    /// The signature is the guarantee — there is nothing to pass that could vary.
    #[test]
    fn the_arrangement_depends_on_nothing_that_zooming_changes() {
        let sizes = [(595.0, 842.0); 25];
        assert_eq!(lay(&sizes), lay(&sizes));
    }

    /// Every row holds the column count, and the last holds the remainder.
    #[test]
    fn rows_hold_the_column_count() {
        let sizes = [(595.0, 842.0); 25];
        let rects = lay(&sizes);
        let first = rects[0].min.y;
        assert_eq!(rects.iter().filter(|r| same(r.min.y, first)).count(), COLS);
        assert!(same(rects[COLS].min.y, 842.0 + ROW_GAP), "the second row clears the first");
        let last = rects[20].min.y;
        assert_eq!(rects.iter().filter(|r| same(r.min.y, last)).count(), 25 - 2 * COLS);
    }

    /// No two pages may overlap. This is the defect the fixed cell had: an A3 landscape
    /// sheet was drawn 274pt on top of the A4 page beside it.
    #[test]
    fn pages_of_differing_widths_do_not_overlap() {
        let sizes = [(595.0, 842.0), (1191.0, 842.0), (595.0, 842.0), (297.0, 420.0)];
        let rects = lay(&sizes);
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
            }
        }
    }

    /// A uniform document gives an even grid: equal widths mean an equal pitch.
    #[test]
    fn uniform_pages_give_an_even_pitch() {
        let rects = lay(&[(595.0, 842.0); 20]);
        let pitch = rects[1].min.x - rects[0].min.x;
        assert!(same(pitch, 595.0 + GAP), "even pitch within a row: {pitch}");
        assert!(same(rects[2].min.x - rects[1].min.x, pitch));
    }

    /// A left-bound book's grid runs left to right, and a right-bound book's runs the other
    /// way, for the same reason its spread does.
    #[test]
    fn the_grid_runs_the_way_the_book_is_bound() {
        let ltr = laid(&[(595.0, 842.0); 3], false);
        assert!(ltr[0].min.x < ltr[1].min.x && ltr[1].min.x < ltr[2].min.x);

        let r2l = laid(&[(595.0, 842.0); 3], true);
        assert!(r2l[0].min.x > r2l[1].min.x && r2l[1].min.x > r2l[2].min.x);
        assert!(same(ltr[0].min.x, r2l[2].min.x), "the same span, mirrored");
        assert!(same(r2l[0].min.y, r2l[2].min.y), "still one row");
    }

    /// Pages shorter than the tallest in their row are centred against it, and the next
    /// row clears the tallest rather than the first.
    #[test]
    fn a_short_page_is_centred_and_the_row_clears_the_tallest() {
        let mut sizes = vec![(400.0, 400.0); COLS];
        sizes[3] = (400.0, 800.0);
        sizes.push((400.0, 400.0));
        let rects = lay(&sizes);
        assert!(same(rects[3].min.y, 0.0), "the tallest sets the row");
        assert!(same(rects[0].min.y, 200.0), "and the others are centred against it");
        assert!(same(rects[COLS].min.y, 800.0 + ROW_GAP), "the next row clears the tallest");
    }

    /// **A page is in a different place in the two arrangements**, which is what makes the
    /// order of operations matter when a double-click crosses between them: the zoom
    /// changes the view, the layout has to be rebuilt for it, and only then does scrolling
    /// to the page mean the right place. Scrolling first lands near the front of the
    /// document, because a grid packs ten pages into the height a column gives one.
    #[test]
    fn a_page_sits_somewhere_else_in_the_other_arrangement() {
        let sizes = [(595.0, 842.0); 40];

        let grid = lay(&sizes);
        let column = column(&sizes);

        assert!(same(grid[25].min.y, 2.0 * (842.0 + ROW_GAP)), "row 2 of a ten-wide grid");
        assert!(same(column[25].min.y, 25.0 * (842.0 + GAP)), "the 26th page of a column");
        assert!(
            column[25].min.y > grid[25].min.y * 10.0,
            "and the two are an order of magnitude apart, not a rounding difference"
        );
    }

    /// A column keeps the pages in order, one under the next, centred.
    #[test]
    fn a_column_stacks_the_pages_in_order() {
        let column = column(&[(595.0, 842.0), (297.0, 420.0), (595.0, 842.0)]);

        assert!(same(column[0].min.y, 0.0));
        assert!(same(column[1].min.y, 842.0 + GAP), "the next page clears this one");
        assert!(same(column[2].min.y, 842.0 + GAP + 420.0 + GAP), "by its own height");
        for r in &column {
            assert!(same(r.center().x, 0.0), "each page centred, whatever its width");
        }
    }

    /// **The gaps have to hold the page number at the smallest zoom each view reaches.**
    /// They are in page units and the number is in screen pixels, so the space shrinks with
    /// the zoom while the digits do not: at 33% a 48-unit gap was 16 pixels and the number
    /// was dropped rather than drawn on the page below. Each gap is sized from
    /// `PAGE_NUMBER_SPACE` at its own floor, so the number is drawn at every zoom.
    #[test]
    fn the_gaps_hold_a_page_number_at_every_zoom() {
        let needed = crate::view::PDFView::PAGE_NUMBER_SPACE;

        // The page view reaches down to the first step above the tile boundary.
        let lowest_page_step = crate::view::PDFView::ZOOM_STEPS
            .iter()
            .copied()
            .find(|s| *s >= crate::view::PDFView::TILE_ZOOM)
            .expect("a step above the boundary");
        let between_pages = FepdfApp::PAGE_GAP * lowest_page_step;
        assert!(between_pages >= needed, "pages are {between_pages} apart at {lowest_page_step}");

        // The tile view reaches the floor.
        let between_rows = FepdfApp::TILE_ROW_GAP * crate::view::PDFView::ZOOM_FLOOR;
        assert!(between_rows >= needed, "rows are {between_rows} apart at the floor");
    }
}
