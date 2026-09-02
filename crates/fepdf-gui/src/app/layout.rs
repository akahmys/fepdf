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

    /// The space between pages, in page units.
    ///
    /// **It is in page units, so it shrinks with the zoom.** At the floor the old 24 drew
    /// as 2.4 pixels and neighbouring pages read as one sheet; a page in the tile grid is
    /// told apart by its gap and its border, and at that size the gap was doing none of the
    /// work. Widening it costs nothing at reading zoom, where 24 was already narrower than
    /// most of the margins inside the pages it separated.
    pub const PAGE_GAP: f32 = 48.0;

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
            // Continuous Display Mode: Dynamically tile pages in 1..N columns based on zoom and viewport width
            let viewport_w = self.last_viewport_rect.map_or(1000.0, |r| r.width());
            let zoom = self.view.zoom();
            let is_r2l = self.view.binding_direction == BindingDirection::RightToLeft;
            let gap_x = Self::PAGE_GAP;
            let gap_y = Self::PAGE_GAP;

            if self.view.scroll_direction == ScrollDirection::Vertical {
                if self.view.is_reading_view() {
                    // Standard single vertical column
                    let mut current_offset = 0.0;
                    for (i, &(page_w, page_h)) in self.doc_page_sizes.iter().enumerate() {
                        let width = page_w as f32;
                        let height = page_h as f32;
                        let page_rect = egui::Rect::from_min_size(
                            egui::pos2(-width / 2.0, current_offset),
                            egui::vec2(width, height),
                        );
                        current_offset += height + gap_y;
                        layouts[i] = PageLayout { index: i, rect: page_rect };
                    }
                } else {
                    Self::flow_rows(
                        &self.doc_page_sizes,
                        viewport_w / zoom.max(f32::EPSILON),
                        gap_x,
                        gap_y,
                        is_r2l,
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

    /// Places pages left to right in rows, each row taking as many as fit in `available_w`.
    ///
    /// **The cell used to be page 1's size, for every page.** The column count, the pitch
    /// and the centring all came from `doc_page_sizes.first()`, so a page wider than
    /// `ref_w + 2 * gap_x` was drawn on top of its neighbour — an A3 landscape sheet in an
    /// A4 document overlapped the next page by 274pt, 46% of its width. Rows of actual
    /// widths cannot overlap, because a row is closed before the page that would not fit.
    ///
    /// **On a document whose pages are all one size this is the old layout exactly**: equal
    /// widths make every row hold the same number, and the row height is that one height.
    /// Both corpora are entirely such documents — 9 samples and 515 external files, no
    /// mixed-size document among them — so the case that changes is the one that was wrong.
    ///
    /// `available_w` is the viewport width in page units, so the row fills the window at
    /// the current zoom. Every row takes at least one page, or a sheet wider than the
    /// window would close a row it was never in.
    fn flow_rows(
        sizes: &[(f64, f64)],
        available_w: f32,
        gap_x: f32,
        gap_y: f32,
        is_r2l: bool,
        layouts: &mut [PageLayout],
    ) {
        let mut row: Vec<usize> = Vec::new();
        let mut row_w = 0.0_f32;
        let mut offset_y = 0.0_f32;

        for i in 0..sizes.len() {
            let w = sizes[i].0 as f32;
            let next_w = if row.is_empty() { w } else { row_w + gap_x + w };
            if !row.is_empty() && next_w > available_w {
                offset_y +=
                    Self::place_row(sizes, &row, row_w, offset_y, gap_x, is_r2l, layouts) + gap_y;
                row.clear();
                row_w = 0.0;
            }
            row_w = if row.is_empty() { w } else { row_w + gap_x + w };
            row.push(i);
        }
        if !row.is_empty() {
            Self::place_row(sizes, &row, row_w, offset_y, gap_x, is_r2l, layouts);
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
            // Right-to-left mirrors the row about its own centre, so page order runs from
            // the right edge inwards without the rest of the placement changing.
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

#[cfg(test)]
mod flow {
    use super::super::FepdfApp;
    use crate::view::PageLayout;

    /// Rows are compared by position rather than by exact equality: the values come from
    /// the same arithmetic and do match exactly, but a layout test that depends on that
    /// would be a layout test that breaks on an unrelated reordering of the sum.
    fn same(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    /// The real gap, so that changing it is covered rather than shadowed by a literal.
    const GAP: f32 = FepdfApp::PAGE_GAP;

    fn lay(sizes: &[(f64, f64)], available_w: f32, is_r2l: bool) -> Vec<egui::Rect> {
        let mut layouts = vec![PageLayout { index: 0, rect: egui::Rect::NOTHING }; sizes.len()];
        FepdfApp::flow_rows(sizes, available_w, GAP, GAP, is_r2l, &mut layouts);
        layouts.into_iter().map(|l| l.rect).collect()
    }

    /// No two pages may overlap. This is the whole defect: the old grid drew an A3
    /// landscape sheet 274pt on top of the A4 page beside it.
    #[test]
    fn pages_of_differing_widths_do_not_overlap() {
        let sizes = [(595.0, 842.0), (1191.0, 842.0), (595.0, 842.0), (297.0, 420.0)];
        let rects = lay(&sizes, 2600.0, false);
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
            }
        }
    }

    /// A uniform document must lay out exactly as the fixed grid did, since that is every
    /// document in both corpora: 9 samples and 515 external files, no mixed sizes among
    /// them. A change there would be a regression nothing else would catch.
    #[test]
    fn uniform_pages_give_uniform_rows() {
        let sizes = [(595.0, 842.0); 7];
        // Four across, and a fifth would not fit, whatever the gap is set to.
        let width = 4.0_f32.mul_add(595.0, 3.0 * GAP);
        let rects = lay(&sizes, width + 1.0, false);
        let first_row_y = rects[0].min.y;
        assert_eq!(rects.iter().filter(|r| same(r.min.y, first_row_y)).count(), 4);
        assert!(same(rects[4].min.y, 842.0 + GAP), "the second row clears the first");
        let pitch = rects[1].min.x - rects[0].min.x;
        assert!((pitch - (595.0 + GAP)).abs() < 0.01, "even pitch within a row: {pitch}");
    }

    /// Rows hold what fits, so a row of wide pages holds fewer than a row of narrow ones.
    /// A fixed column count could not do this, which is what made the old layout overlap.
    #[test]
    fn rows_hold_different_numbers_of_pages() {
        let sizes = [
            (1200.0, 800.0),
            (1200.0, 800.0),
            (300.0, 400.0),
            (300.0, 400.0),
            (300.0, 400.0),
            (300.0, 400.0),
        ];
        let rects = lay(&sizes, 2500.0, false);
        let rows: std::collections::BTreeSet<i64> =
            rects.iter().map(|r| r.min.y.round() as i64).collect();
        assert_eq!(rows.len(), 2, "two wide pages, then four narrow ones");
        assert!(same(rects[0].min.y, rects[1].min.y));
        assert!(same(rects[2].min.y, rects[5].min.y));
        assert!(rects[2].min.y > rects[0].min.y);
    }

    /// A page wider than the window still gets a row, rather than closing a row it was
    /// never placed in and being laid out on top of the previous one.
    #[test]
    fn a_page_wider_than_the_window_takes_its_own_row() {
        let sizes = [(595.0, 842.0), (5000.0, 842.0), (595.0, 842.0)];
        let rects = lay(&sizes, 800.0, false);
        assert!(rects[1].min.y > rects[0].min.y);
        assert!(rects[2].min.y > rects[1].min.y);
        assert!(!rects[0].intersects(rects[1]) && !rects[1].intersects(rects[2]));
    }

    /// Right to left mirrors the row about its own centre: page order runs from the right
    /// edge inwards, and the row still occupies the same span.
    #[test]
    fn right_to_left_reverses_within_the_row_only() {
        let sizes = [(595.0, 842.0); 3];
        let ltr = lay(&sizes, 2000.0, false);
        let r2l = lay(&sizes, 2000.0, true);
        assert!(r2l[0].min.x > r2l[2].min.x, "page 1 sits to the right of page 3");
        assert!((ltr[0].min.x - r2l[2].min.x).abs() < 0.01, "the same span, mirrored");
        assert!(same(r2l[0].min.y, r2l[2].min.y), "still one row");
    }

    /// **The gap has to survive the zoom it exists for.** It is in page units, so at the
    /// floor it is multiplied by 0.1: the previous 24 drew as 2.4 pixels and neighbouring
    /// pages read as one sheet. This ties the two constants together, so that lowering the
    /// zoom floor or narrowing the gap has to answer for the overview it produces.
    #[test]
    fn the_gap_is_still_visible_at_the_smallest_zoom() {
        let floor = 0.1_f32;
        let on_screen = FepdfApp::PAGE_GAP * floor;
        assert!(on_screen >= 4.0, "the gap draws as {on_screen} pixels at the zoom floor");
    }

    /// Pages shorter than the tallest in their row are centred against it, not left to
    /// sit on the row's top edge.
    #[test]
    fn a_short_page_is_centred_against_the_tallest_in_its_row() {
        let sizes = [(400.0, 800.0), (400.0, 400.0)];
        let rects = lay(&sizes, 1000.0, false);
        assert!(same(rects[0].min.y, 0.0));
        assert!((rects[1].min.y - 200.0).abs() < 0.01, "centred: {:?}", rects[1].min.y);
    }
}
