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
            let (ref_w, ref_h) = self.doc_page_sizes.first().copied().unwrap_or((595.0, 842.0));
            let ref_w = ref_w as f32;
            let ref_h = ref_h as f32;
            let zoom = self.view.zoom();
            let is_r2l = self.view.binding_direction == BindingDirection::RightToLeft;
            let gap_x = 24.0_f32;
            let gap_y = 24.0_f32;

            if self.view.scroll_direction == ScrollDirection::Vertical {
                let cols = if zoom < 0.65 {
                    let page_screen_w = (ref_w + gap_x) * zoom;
                    (viewport_w / page_screen_w.max(1.0)).floor().max(1.0) as usize
                } else {
                    1
                };

                if cols <= 1 {
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
                    // Multi-column tile grid (seamlessly transitions when zoomed out)
                    let total_grid_w = (cols as f32 - 1.0).mul_add(gap_x, cols as f32 * ref_w);
                    let start_x = -total_grid_w / 2.0;

                    for (i, &(page_w, page_h)) in self.doc_page_sizes.iter().enumerate() {
                        let row = i / cols;
                        let col = if is_r2l { (cols - 1) - (i % cols) } else { i % cols };
                        let width = page_w as f32;
                        let height = page_h as f32;

                        let pos_x =
                            (col as f32).mul_add(ref_w + gap_x, start_x) + (ref_w - width) / 2.0;
                        let pos_y = (row as f32).mul_add(ref_h + gap_y, (ref_h - height) / 2.0);

                        let page_rect = egui::Rect::from_min_size(
                            egui::pos2(pos_x, pos_y),
                            egui::vec2(width, height),
                        );
                        layouts[i] = PageLayout { index: i, rect: page_rect };
                    }
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
}
