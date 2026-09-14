//! The viewport: where the document is, how far away, and what a pointer does to it.
//!
//! **What it draws is next door.** This file held that too and came to 2,451 lines, which
//! is a file nobody reads the middle of. The state lives here and `draw` borrows it: a
//! child module can see its parent's private fields, so the split cost the struct no
//! visibility it did not already have.
//!
//! The subjects that remain are one each: where the view is (`pan`, `zoom`, the origins),
//! what moves it (the zoom ladder, the anchor a change of arrangement carries, the page
//! stepping), and what the pointer means (`handle_input` and the gestures).

pub mod draw;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct PageLayout {
    pub index: usize,
    pub rect: egui::Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Continuous,
    SinglePage,
    TwoPageSpread,
    TwoPageSingle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingDirection {
    LeftToRight,
    RightToLeft,
}

/// Where the page area gets its pixels for this frame.
///
/// **The two are alternatives, not a flag and a value**, which is why this is a type: in
/// one the whole viewport is a single texture composed from every visible page's vector
/// scene, and in the other each page is its own small texture. Passing both and choosing
/// inside would allow a state that means nothing.
pub enum PagePixels<'a> {
    /// One texture covering the viewport. `None` while it is being created.
    Viewport(Option<egui::TextureId>),
    /// One thumbnail per page, drawn at that page's rect. Pages absent from the map have
    /// not been rendered yet and keep their placeholder.
    Thumbnails(&'a BTreeMap<usize, egui::TextureId>),
}

/// A page, and the point on it the view is holding still.
#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    page: usize,
    /// Where on that page, in unscaled page units from its top-left.
    local: egui::Vec2,
}

pub struct PDFView {
    /// Private, so the only ways to change it are [`Self::set_zoom`] and
    /// [`Self::zoom_at`]. It was `pub`, four callers assigned it directly, and three of
    /// them wrote out the same `clamp(0.1, 10.0)` — a bound repeated is a bound that
    /// drifts, and one caller forgetting it is a view that cannot be zoomed back.
    ///
    /// [`Self::apply_zoom`] is where the bound is applied, and the only place it is
    /// written. `set_zoom` had its own copy of the same `clamp` until it went through
    /// `apply_zoom` too: two is fewer than four and still more than one.
    zoom: f32,
    /// What a continuous gesture has accumulated, before snapping.
    ///
    /// **`zoom` alone would stick to a step and never leave it.** A pinch computes its next
    /// value from the current one, so once snapping had pulled it onto 1.00, every
    /// small delta landed back inside the snap band and was pulled onto 1.00 again.
    /// Accumulating the raw value separately lets the gesture travel through the band.
    /// Nothing renders from this; it is the gesture's own memory.
    zoom_unsnapped: f32,
    pub pan: egui::Vec2,
    pub visible_pages: Vec<usize>,
    pub display_mode: DisplayMode,
    pub active_page: usize,
    pub scroll_direction: ScrollDirection,
    pub binding_direction: BindingDirection,
    pub cover_page_alone: bool,
    pub overscroll_accumulator: egui::Vec2,
    /// Whether the layout the current `pan` was computed against was the tile grid.
    ///
    /// **The two arrangements are different coordinate systems, and `pan` is in one of
    /// them.** Below [`Self::TILE_ZOOM`] a continuous document is laid out as a grid four
    /// columns wide; above it, as one column. Page 8 of a letter-size document sits at
    /// `y = 802` in the grid and at `y = 6,336` in the column, so a zoom that crosses the
    /// boundary leaves the view pointing at whatever else happens to be at the old `y` —
    /// which is the page the reader was not looking at.
    arranged_as_tiles: bool,
    /// Where the last zoom was anchored, which a change of arrangement carries across.
    last_anchor: Option<egui::Pos2>,
    /// A page to put in the middle of the window at the next change of arrangement,
    /// instead of carrying the cursor's anchor across it.
    ///
    /// **Two gestures ask for this and both are double-clicks.** Every other way across
    /// the tile boundary is a zoom, where the cursor is pointing at something and holding
    /// it still is the whole rule. A double-click on the bench points at nothing and what
    /// the reader wants back is the page they were reading; a double-click on a tile
    /// points at one page and says open it. Both are answered by [`Self::open_page`].
    centre_next: Option<usize>,
}

impl PDFView {
    pub fn get_spread_indices(&self, page_index: usize, total_pages: usize) -> Vec<usize> {
        if total_pages == 0 {
            return Vec::new();
        }
        if self.cover_page_alone {
            if page_index == 0 {
                vec![0]
            } else {
                let pair_index = ((page_index - 1) / 2) * 2 + 1;
                let mut spread = vec![pair_index];
                if pair_index + 1 < total_pages {
                    spread.push(pair_index + 1);
                }
                spread
            }
        } else {
            let pair_index = (page_index / 2) * 2;
            let mut spread = vec![pair_index];
            if pair_index + 1 < total_pages {
                spread.push(pair_index + 1);
            }
            spread
        }
    }
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            zoom_unsnapped: 1.0,
            pan: egui::Vec2::ZERO,
            visible_pages: Vec::new(),
            display_mode: DisplayMode::Continuous,
            active_page: 0,
            scroll_direction: ScrollDirection::Vertical,
            binding_direction: BindingDirection::LeftToRight,
            cover_page_alone: true,
            overscroll_accumulator: egui::Vec2::ZERO,
            arranged_as_tiles: false,
            last_anchor: None,
            centre_next: None,
        }
    }
    pub fn get_origin(&self, viewport_rect: egui::Rect) -> egui::Pos2 {
        let origin_x = self.origin_x(viewport_rect);
        let origin_y = if self.scroll_direction == ScrollDirection::Horizontal
            || self.display_mode == DisplayMode::SinglePage
            || self.display_mode == DisplayMode::TwoPageSingle
        {
            viewport_rect.center().y
        } else {
            viewport_rect.min.y + 20.0
        };
        egui::pos2(origin_x, origin_y) + self.pan
    }

    pub fn get_origin_no_pan(&self, viewport_rect: egui::Rect) -> egui::Pos2 {
        let origin_x = self.origin_x(viewport_rect);
        let origin_y = if self.scroll_direction == ScrollDirection::Horizontal
            || self.display_mode == DisplayMode::SinglePage
            || self.display_mode == DisplayMode::TwoPageSingle
        {
            viewport_rect.center().y
        } else {
            viewport_rect.min.y + 20.0
        };
        egui::pos2(origin_x, origin_y)
    }

    /// Where `x = 0` of the layout sits in the window.
    ///
    /// **A sheet is centred and a contact sheet is not.** One page in the middle of the
    /// window is the page being read; ten columns of tiles centred on the same point put
    /// the grid's middle there and hang its two ends off both edges, so the first tile —
    /// the one a reader looks for first — is the one that has gone. The tiles hang from
    /// the binding edge instead, which is the left of the window for a left-bound
    /// document and the right for a right-bound one.
    ///
    /// **Read from the arrangement in hand, not from the zoom.** The zoom crosses
    /// `TILE_ZOOM` one call before the layouts are recomputed, so a origin that asked
    /// `is_page_view()` answered for the column while the grid was still laid out — and
    /// the anchor carried across the change was measured against an origin that belonged
    /// to neither, which came out a page off.
    fn origin_x(&self, viewport_rect: egui::Rect) -> f32 {
        if !self.arranged_as_tiles || self.display_mode != DisplayMode::Continuous {
            return viewport_rect.center().x;
        }
        if self.binding_direction == BindingDirection::RightToLeft {
            viewport_rect.max.x - crate::app::theme::space::PANE
        } else {
            viewport_rect.min.x + crate::app::theme::space::PANE
        }
    }

    /// The current zoom factor. Read freely; changing it goes through
    /// [`Self::set_zoom`] or [`Self::zoom_at`].
    #[must_use]
    pub const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// The bounds a zoom factor is held to, in one place.
    ///
    /// Written out three times before — inside `zoom_at` and twice in the fits that used
    /// to sit beside it — and a bound repeated is a bound that drifts.
    const ZOOM_BOUNDS: std::ops::RangeInclusive<f32> = Self::ZOOM_FLOOR..=10.0;

    /// The smallest zoom the viewer offers.
    ///
    /// **Below it, zooming out stops helping.** The tile view is for arranging pages, and a
    /// tile smaller than this is not one a reader can tell from its neighbour, so the zoom
    /// beyond here buys a fuller screen of things that cannot be used. That is the reason;
    /// what follows is a coincidence that supports it.
    ///
    /// Ten columns of A4 with their gaps is 6,382 page units, which at 20% is 1,276 pixels
    /// and fills an ordinary viewport. Below that the grid also shrinks into the middle of
    /// the screen and shows no more of the document than it did — the arrangement being
    /// fixed (`FepdfApp::TILE_COLUMNS`) is what gives it a floor at all. A layout that
    /// fitted itself to the window had none, which is why the old floor was 0.1.
    ///
    /// The cost is that the tile view is two steps wide, 20% and 25%, and shows 40 to 50
    /// pages rather than the 253 measured at the old floor. More than that is now a
    /// question for the column count, not for the zoom.
    pub const ZOOM_FLOOR: f32 = 0.20;

    /// The zoom the view stops being pages and becomes tiles.
    ///
    /// Below it the pages tile, a drag reorders them, `Cmd+A` selects all of them, and each
    /// page is drawn from its own thumbnail rather than composed as a vector scene. It was
    /// written out at nine call sites across three files before it had a name.
    ///
    /// **It is the only boundary.** There were three — the layout changed at 0.65, the
    /// rendering at 0.33, and what could be done to a page's content at 0.40 — which is
    /// three thresholds where a reader sees one view changing. Above this is the **page
    /// view**: one column, composed from vector scenes, text selectable, reading order
    /// shown. Below it is the **tile view**: flowed into rows, drawn from thumbnails,
    /// dragged to reorder, selected whole.
    ///
    /// **The modes are named for what is on screen, not for what they are for.** They were
    /// "reading" and "overview" while the boundary was 0.65, where reading was a promise the
    /// view could keep; at 0.30 text draws around 3pt, so a mode called "reading" would name
    /// something it cannot deliver. A page and a tile are what the reader can see, and can
    /// check the label against.
    ///
    /// Selection at 0.30 is likewise offered over text nobody can read, which was the
    /// argument for a separate legibility boundary at 0.40. One boundary a reader can see
    /// was judged worth more than a second one they cannot.
    ///
    /// It also cuts what is composed between here and 0.65 from several hundred pages to
    /// the three or four a single column holds.
    pub const TILE_ZOOM: f32 = 0.30;

    /// Where double-clicking out lands: the first step that is unambiguously a tile view.
    pub const TILE_STEP: f32 = 0.25;

    /// The zoom as a percentage, written so that it is never a percentage the view is not
    /// at.
    ///
    /// **`{:.0}%` printed 99.6% as `100%`** — the same string a true 100% shows, over a
    /// page rendered at a different size. Every zoom a gesture or a button produces is a
    /// step, so with the fit buttons gone nothing off the ladder reaches this any more;
    /// one decimal stays because `12.5%` is on the ladder and reads as itself, and a
    /// trailing `.0` is dropped so that a step reads `100%`.
    #[must_use]
    pub fn zoom_label(&self) -> String {
        let percent = self.zoom * 100.0;
        let text = format!("{percent:.1}");
        format!("{}%", text.strip_suffix(".0").unwrap_or(&text))
    }

    /// The screen space a page number needs: its own height, plus a little either side.
    ///
    /// **The number does not scale and the gap it sits in does.** The gap is in page units,
    /// so it shrinks with the zoom while the digits stay the size they are; the gaps are
    /// sized from this figure at the smallest zoom each view reaches, so that the number is
    /// drawn at every zoom rather than dropped where it would not fit.
    pub const PAGE_NUMBER_SPACE: f32 = 20.0;

    /// The zooms the buttons, the keyboard and the menu move between.
    ///
    /// **A multiplier could not reach them.** The buttons used to scale the current zoom by
    /// 1.2, so from any value not already on a step — anything a pinch or a fit had
    /// produced — no number of presses ever arrived at 100%; a separate reset button existed
    /// to paper over it. Stepping along a fixed ladder lands on 100% from anywhere, and the
    /// percentage the status bar prints is then the percentage in force.
    ///
    /// **The spacing is the point.** The first ladder had `0.67` and `0.75` a ratio of 1.12
    /// apart — a press that changed nothing a reader could see — while `0.33` to `0.50` was
    /// 1.52, the largest jump of the lot, sitting just above the mode boundary where the
    /// finest control is wanted. Every ratio here is between 1.19 and 1.34 except the last,
    /// 700 to 1000, which is the end of the range and rarely stepped through.
    ///
    /// It is a doubling spine — 25, 50, 100, 200, 400 — divided by a repeating 1.25 / 1.20 /
    /// 1.33, so the round numbers a reader thinks in are landed on exactly. Chrome and
    /// Firefox both thicken the ladder around the readable range in the same way; Acrobat
    /// does not, going 10, 25, 50, whose 2.5x first step this viewer cannot use.
    ///
    /// **25 and 33 straddle [`Self::TILE_ZOOM`]**, so stepping never lands on the boundary
    /// and leaves the mode undetermined.
    pub const ZOOM_STEPS: [f32; 17] = [
        0.20, 0.25, 0.33, 0.40, 0.50, 0.67, 0.80, 1.00, 1.25, 1.50, 2.00, 2.50, 3.00, 4.00, 5.00,
        7.00, 10.00,
    ];

    /// How close a continuous gesture must come to a step before it is taken to mean it.
    ///
    /// Without this a pinch can stop at 99.4% and be indistinguishable from 100% on screen
    /// and in the label while rendering differently.
    const SNAP_TOLERANCE: f32 = 0.02;

    /// How much further a gesture must travel to leave the step it is sitting on.
    ///
    /// **A detent needs to be sticky or it is a boundary.** The nearest step alone flips at
    /// the midpoint between two, where the smallest wobble in a pinch sends the view back
    /// and forth. In log space each step keeps this much extra reach, so a gesture crossing
    /// the middle has to commit before the view follows.
    const DETENT_STICK: f32 = 0.05;

    /// The first step above the current zoom, or the top of the range.
    #[must_use]
    pub fn zoom_step_up(&self) -> f32 {
        Self::ZOOM_STEPS
            .iter()
            .copied()
            .find(|step| *step > self.zoom * (1.0 + Self::SNAP_TOLERANCE))
            .unwrap_or(*Self::ZOOM_BOUNDS.end())
    }

    /// The first step below the current zoom, or the bottom of the range.
    #[must_use]
    pub fn zoom_step_down(&self) -> f32 {
        Self::ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|step| *step < self.zoom * (1.0 - Self::SNAP_TOLERANCE))
            .unwrap_or(*Self::ZOOM_BOUNDS.start())
    }

    /// The step a continuous gesture at `zoom` means, given the step it is on now.
    ///
    /// **Every zoom a gesture produces is a step.** It used to be pulled onto one only
    /// within [`Self::SNAP_TOLERANCE`], so a pinch spent almost all of its travel between
    /// steps: the view scaled smoothly, the label read a number no ladder contains, and the
    /// steps were something the buttons had and the fingers did not. Answering with the
    /// nearest step makes a pinch click from one to the next, which is the same movement
    /// the buttons make.
    ///
    /// Distance is measured in log space, because the ladder is a ratio: 0.20 and 0.25 are
    /// as far apart to the eye as 4.00 and 5.00, and the midpoint of a step is its
    /// geometric mean, not its average. `current` keeps [`Self::DETENT_STICK`] of extra
    /// reach so that sitting exactly between two steps does not oscillate.
    fn step_for(zoom: f32, current: f32) -> f32 {
        let reach = |step: f32| {
            let distance = (zoom.max(f32::EPSILON) / step).ln().abs();
            if (step - current).abs() < f32::EPSILON {
                distance - Self::DETENT_STICK
            } else {
                distance
            }
        };
        Self::ZOOM_STEPS
            .iter()
            .copied()
            .min_by(|a, b| reach(*a).total_cmp(&reach(*b)))
            .unwrap_or(zoom)
    }

    /// Whether a click on a page selects *the page*. See [`Self::selects_text`].
    ///
    /// **The two must never both be true.** One `Response` covers a page, and page
    /// selection and text selection both read it: while both were live, a drag meant to
    /// select text also selected the page, and `Delete` — which is not gated by mode —
    /// then removed the page the reader had merely clicked in.
    #[must_use]
    pub fn selects_pages(&self) -> bool {
        !self.is_page_view()
    }

    /// Whether a click on a page selects *text on it*. See [`Self::selects_pages`].
    #[must_use]
    pub fn selects_text(&self) -> bool {
        self.is_page_view()
    }

    /// Whether the view is showing pages rather than tiles. See [`Self::TILE_ZOOM`].
    #[must_use]
    pub fn is_page_view(&self) -> bool {
        self.zoom >= Self::TILE_ZOOM
    }

    /// Sets the zoom without moving anything, for a caller that places the view itself.
    ///
    /// **Not the same operation as [`Self::zoom_at`], which is why both exist.** `zoom_at`
    /// keeps a chosen point under the cursor and computes `pan` to do it; this is for
    /// `reset_view` and double-click-to-fit, which set `pan` explicitly on the line after
    /// and would have that work thrown away. Routing them through `zoom_at` would compute
    /// an anchor nobody reads.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.apply_zoom(zoom);
        self.zoom_unsnapped = self.zoom;
    }

    /// Sets `zoom` alone, leaving the gesture's accumulator where it is.
    ///
    /// `zoom_at` records the raw target itself and must not have it overwritten with the
    /// snapped one, which is exactly what `set_zoom` would do.
    fn apply_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(*Self::ZOOM_BOUNDS.start(), *Self::ZOOM_BOUNDS.end());
    }

    /// The zoom a gesture should compute its next value from. See [`Self::zoom_unsnapped`].
    #[must_use]
    pub const fn zoom_before_snapping(&self) -> f32 {
        self.zoom_unsnapped
    }

    /// The page under `center_pos`, else the nearest, else the active one.
    ///
    /// Only pages the current display mode actually shows are candidates: zooming in
    /// single-page mode must not anchor to a page that is not on screen, and a two-page
    /// spread anchors within its own spread.
    fn layout_under<'a>(
        &self,
        center_pos: egui::Pos2,
        current_origin: egui::Pos2,
        old_zoom: f32,
        layouts: &'a [PageLayout],
    ) -> Option<&'a PageLayout> {
        let (mut closest, mut min_dist_sq) = (None, f32::MAX);
        for layout in layouts {
            if self.display_mode == DisplayMode::SinglePage && layout.index != self.active_page {
                continue;
            }
            if self.display_mode == DisplayMode::TwoPageSingle {
                let spread = self.get_spread_indices(self.active_page, layouts.len());
                if !spread.contains(&layout.index) {
                    continue;
                }
            }
            let page_screen_rect = egui::Rect::from_min_size(
                current_origin + layout.rect.min.to_vec2() * old_zoom,
                layout.rect.size() * old_zoom,
            );
            if page_screen_rect.contains(center_pos) {
                return Some(layout);
            }
            let dist_sq = page_screen_rect.distance_sq_to_pos(center_pos);
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest = Some(layout);
            }
        }
        closest.or_else(|| layouts.get(self.active_page))
    }

    /// Zooms to `new_zoom`, anchoring to the page and the point on it under `center_pos`.
    ///
    /// The anchor is remembered, because the layout under it may be about to be replaced
    /// by a differently arranged one: see [`Self::take_anchor`].
    pub fn zoom_at(
        &mut self,
        new_zoom: f32,
        center_pos: egui::Pos2,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
    ) {
        self.last_anchor = Some(center_pos);
        let old_zoom = self.zoom;
        let raw = new_zoom.clamp(*Self::ZOOM_BOUNDS.start(), *Self::ZOOM_BOUNDS.end());
        // Recorded before the early return: a gesture that is crossing a snap band must
        // keep accumulating even on the frames where the snapped zoom does not move.
        self.zoom_unsnapped = raw;
        let new_zoom = Self::step_for(raw, old_zoom);
        if (new_zoom - old_zoom).abs() < f32::EPSILON {
            return;
        }

        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        let current_origin = origin_no_pan + self.pan;
        let target_layout = self.layout_under(center_pos, current_origin, old_zoom, layouts);

        if let Some(layout) = target_layout {
            // Page-anchored zoom: calculate the point on this page in unscaled page coordinates
            let page_screen_min = current_origin + layout.rect.min.to_vec2() * old_zoom;
            let local_offset_doc = (center_pos - page_screen_min) / old_zoom;

            self.apply_zoom(new_zoom);
            // Place the exact same local page point under center_pos after zoom
            self.pan = (center_pos - origin_no_pan)
                - (layout.rect.min.to_vec2() + local_offset_doc) * new_zoom;
        } else {
            let cursor_doc = (center_pos - origin_no_pan - self.pan) / old_zoom;
            self.apply_zoom(new_zoom);
            self.pan = (center_pos - origin_no_pan) - cursor_doc * new_zoom;
        }
    }

    /// Whether the layout about to be computed will be arranged differently from the one
    /// in hand.
    ///
    /// **The two arrangements are different coordinate systems, and `pan` is in one of
    /// them.** Below [`Self::TILE_ZOOM`] a continuous document is laid out as a grid
    /// hanging from the binding edge; above it, as one centred column. Page 8 of a
    /// letter-size document sits at `y = 802` in the grid and at `y = 6,336` in the
    /// column, so a zoom across the boundary leaves the view pointing at whatever else
    /// happens to be at the old `y`.
    #[must_use]
    pub fn arrangement_is_changing(&self) -> bool {
        self.is_page_view() == self.arranged_as_tiles
    }

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
    fn place_along_the_scroll(&mut self, page: egui::Rect, from: Option<egui::Rect>) {
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

    /// Brings the page the reader is on to the middle of the window.
    ///
    /// **In either arrangement, because the question is the same in both.** A column
    /// leaves a page wherever the scroll left it and the grid hangs from its binding edge,
    /// so "where was I" and "put it in front of me" are two different things in each. The
    /// page is the one [`Self::current_page`] names — the same one the counter reads and
    /// the rule under a number marks.
    pub fn centre_current_page(&mut self, viewport: egui::Rect, layouts: &[PageLayout]) {
        let page = self.current_page(viewport, layouts);
        self.centre_on(page, viewport, layouts);
    }

    /// Puts `page` in the middle of the window, and says whether there was such a page.
    ///
    /// **One home for the act.** The crosshair button, a double-click on the bench and a
    /// document that has just opened all want the same two lines, and three copies of
    /// them is three places for the rule to drift (UI-12).
    fn centre_on(&mut self, page: usize, viewport: egui::Rect, layouts: &[PageLayout]) -> bool {
        let Some(layout) = layouts.get(page) else { return false };
        let origin = self.get_origin_no_pan(viewport);
        self.pan = viewport.center() - origin - layout.rect.center().to_vec2() * self.zoom;
        self.active_page = page;
        true
    }

    /// The page the reader is on, by the one rule two things read it by.
    ///
    /// **In the tiles it is the page they are on; in the page view it is the page in the
    /// middle of the window.** The grid is a chooser — its middle holds whichever tile the
    /// scroll left there, which is not where the reader is — and a column of pages is a
    /// surface, where the middle is exactly where they are. The rule under a page number
    /// and the counter in the view controls both answer from here, because two answers to
    /// "which page am I on" is what this replaces.
    #[must_use]
    pub fn current_page(&self, viewport: egui::Rect, layouts: &[PageLayout]) -> usize {
        if self.display_mode == DisplayMode::SinglePage || self.selects_pages() {
            return self.active_page;
        }
        self.page_at_middle(viewport, layouts).unwrap_or(self.active_page)
    }

    /// The page the middle of the viewport is over, which is the one being read.
    ///
    /// Nearest-by-distance rather than strictly containing, so the gaps between pages
    /// answer with the page they are next to rather than with nothing.
    #[must_use]
    pub fn page_at_middle(&self, viewport: egui::Rect, layouts: &[PageLayout]) -> Option<usize> {
        let origin = self.get_origin(viewport);
        self.layout_under(viewport.center(), origin, self.zoom, layouts).map(|layout| layout.index)
    }

    /// Moves the view to `page_index` and makes it the current one.
    ///
    /// **By the distance between the two pages, so that the view does not move.** See
    /// [`Self::place_along_the_scroll`].
    pub fn scroll_to_page(
        &mut self,
        page_index: usize,
        viewport: egui::Rect,
        layouts: &[PageLayout],
    ) {
        let was = self.current_page(viewport, layouts);
        self.active_page = page_index;
        if self.display_mode == DisplayMode::Continuous
            || self.display_mode == DisplayMode::TwoPageSpread
        {
            if let Some(layout) = layouts.get(page_index) {
                let from = layouts.get(was).map(|l| l.rect);
                self.place_along_the_scroll(layout.rect, from);
            }
        } else if self.display_mode == DisplayMode::TwoPageSingle {
            // In TwoPageSingle, we center the active spread's bounding box relative to origin
            let spread_indices = self.get_spread_indices(page_index, layouts.len());
            if !spread_indices.is_empty() {
                let mut min_x = f32::MAX;
                let mut max_x = f32::MIN;
                let mut min_y = f32::MAX;
                let mut max_y = f32::MIN;
                for &idx in &spread_indices {
                    if let Some(layout) = layouts.get(idx) {
                        min_x = min_x.min(layout.rect.min.x);
                        max_x = max_x.max(layout.rect.max.x);
                        min_y = min_y.min(layout.rect.min.y);
                        max_y = max_y.max(layout.rect.max.y);
                    }
                }
                self.pan.x = -f32::midpoint(min_x, max_x) * self.zoom;
                self.pan.y = -f32::midpoint(min_y, max_y) * self.zoom;
            }
        } else if self.display_mode == DisplayMode::SinglePage {
            if let Some(layout) = layouts.get(page_index) {
                // In SinglePage, we center the page on both x and y
                self.pan.x = -layout.rect.center().x * self.zoom;
                self.pan.y = -layout.rect.center().y * self.zoom;
            }
        } else {
            self.pan = egui::Vec2::ZERO;
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

        let unscaled_h = page_layout.rect.height();

        // Convert to egui page-local coordinate system (Y=0 is top)
        let local_x = pdf_center_x;
        let local_y = unscaled_h - pdf_center_y;

        // In virtual space (relative to layout center/top):
        let page_local_pos = page_layout.rect.min + egui::vec2(local_x, local_y);

        // We want origin + page_local_pos * zoom = viewport_rect.center()
        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        self.pan = viewport_rect.center().to_vec2()
            - origin_no_pan.to_vec2()
            - page_local_pos.to_vec2() * self.zoom;
    }

    /// The pages the viewport shows, each with the rect it occupies on screen.
    ///
    /// **Which pages are shown is the display mode's decision**, and it stood written out
    /// in both `draw_pages` and `draw_page_backings` — the same two guards, the same rect
    /// arithmetic, the same intersection test. A page one drew and the other did not would
    /// have shown as a backing with no page on it, or the reverse.
    fn visible_page_rects<'a>(
        &self,
        viewport_rect: egui::Rect,
        layouts: &'a [PageLayout],
    ) -> Vec<(&'a PageLayout, egui::Rect)> {
        let origin = self.get_origin(viewport_rect);
        let active_spread = self.get_spread_indices(self.active_page, layouts.len());
        layouts
            .iter()
            .filter(|layout| match self.display_mode {
                DisplayMode::SinglePage => layout.index == self.active_page,
                DisplayMode::TwoPageSingle => active_spread.contains(&layout.index),
                DisplayMode::Continuous | DisplayMode::TwoPageSpread => true,
            })
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

    fn handle_zoom_gestures(
        &mut self,
        ui: &egui::Ui,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
    ) {
        ui.input(|i| {
            let cursor_pos = i
                .pointer
                .hover_pos()
                .or(i.pointer.latest_pos())
                .filter(|p| viewport_rect.contains(*p))
                .unwrap_or_else(|| viewport_rect.center());

            let zoom_delta = i.zoom_delta();
            #[allow(clippy::float_cmp)]
            if zoom_delta != 1.0 {
                // From the unsnapped value, so the gesture can travel through a step's
                // snap band rather than being pulled back onto it every frame.
                let target = self.zoom_before_snapping() * zoom_delta;
                self.zoom_at(target, cursor_pos, viewport_rect, layouts);
            }

            let is_zoom_modifier = i.modifiers.command || i.modifiers.ctrl;
            let scroll_y = i.smooth_scroll_delta.y;

            if is_zoom_modifier && scroll_y != 0.0 {
                let factor = (scroll_y * 0.005).exp();
                let target = self.zoom_before_snapping() * factor;
                self.zoom_at(target, cursor_pos, viewport_rect, layouts);
            }
        });
    }

    fn handle_scroll_panning(&mut self, ui: &egui::Ui) {
        ui.input(|i| {
            if !i.modifiers.command && !i.modifiers.ctrl {
                let scroll_delta = i.smooth_scroll_delta;
                if self.scroll_direction == ScrollDirection::Horizontal {
                    if scroll_delta.x != 0.0 {
                        self.pan.x += scroll_delta.x;
                    } else {
                        self.pan.x += scroll_delta.y;
                    }
                } else {
                    self.pan += scroll_delta;
                }
            }
        });
    }

    fn handle_input(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
    ) {
        let is_hovered = ui.ctx().input(|i| {
            i.pointer
                .hover_pos()
                .or(i.pointer.latest_pos())
                .is_some_and(|pos| viewport_rect.contains(pos))
        });
        if is_hovered {
            self.handle_zoom_gestures(ui, viewport_rect, layouts);
            self.handle_scroll_panning(ui);
        }
        let shift_down = ui.input(|i| i.modifiers.shift);
        if response.dragged() && (!shift_down || self.is_page_view()) {
            self.pan += response.drag_delta();
        }
        if response.double_clicked() {
            let pos = ui
                .input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()))
                .filter(|p| viewport_rect.contains(*p))
                .unwrap_or_else(|| viewport_rect.center());
            self.double_click_on_the_bench(pos, viewport_rect, layouts);
        }
        if response.drag_stopped()
            || (!response.dragged() && ui.input(|i| i.pointer.any_released()))
        {
            self.overscroll_accumulator = egui::Vec2::ZERO;
        }
    }

    /// Crosses the tile boundary, putting the page the reader is on in the middle.
    ///
    /// **The bench points at nothing, in either direction.** Every other way across that
    /// boundary is a zoom, where the cursor is over something and holding it still is the
    /// whole rule; a double-click on the empty space between pages is over nothing, so
    /// there is nothing to hold still and the answer is the page being read. That was
    /// already the rule going out to the tiles and is now the rule coming back — a
    /// double-click that lands in the page view used to leave it wherever the pointer
    /// happened to be.
    ///
    /// `pos` still anchors the zoom itself, because [`Self::open_page`] is answered after
    /// the pages are laid out again and overrides where the zoom left things.
    pub fn double_click_on_the_bench(
        &mut self,
        pos: egui::Pos2,
        viewport: egui::Rect,
        layouts: &[PageLayout],
    ) {
        self.open_page(self.current_page(viewport, layouts));
        let target_zoom = if self.is_page_view() { Self::TILE_STEP } else { 1.0 };
        self.zoom_at(target_zoom, pos, viewport, layouts);
    }

    /// Moves to the page or spread after the current one, and says whether there was one.
    ///
    /// **A spread steps over both of its pages**: forward from `[1, 2]` is 3, not 2. That
    /// rule, and its mirror in [`Self::page_back`], each stood in two places — once for the
    /// vertical axis and once for the horizontal — inside a single function, which is what
    /// its `RR-15 Limit: GUI` was paying for. Two axes deciding the same thing separately
    /// is the shape [`CODING.md`'s Rule D](../../../CODING.md) names for frontends, arrived
    /// at inside one.
    fn page_forward(&mut self, viewport: egui::Rect, layouts: &[PageLayout]) -> bool {
        let total_pages = layouts.len();
        let next = if self.display_mode == DisplayMode::TwoPageSingle {
            self.get_spread_indices(self.active_page, total_pages)
                .last()
                .copied()
                .filter(|&last| last + 1 < total_pages)
                .map(|last| last + 1)
        } else {
            (self.active_page + 1 < total_pages).then_some(self.active_page + 1)
        };
        self.step_to(next, viewport, layouts)
    }

    /// Moves to the page or spread before the current one, and says whether there was one.
    fn page_back(&mut self, viewport: egui::Rect, layouts: &[PageLayout]) -> bool {
        let prev = if self.display_mode == DisplayMode::TwoPageSingle {
            self.get_spread_indices(self.active_page, layouts.len())
                .first()
                .copied()
                .filter(|&first| first > 0)
                .map(|first| first - 1)
        } else {
            (self.active_page > 0).then_some(self.active_page.saturating_sub(1))
        };
        self.step_to(prev, viewport, layouts)
    }

    /// Scrolls to `target` and forgets what the overscroll had accumulated getting there.
    fn step_to(
        &mut self,
        target: Option<usize>,
        viewport: egui::Rect,
        layouts: &[PageLayout],
    ) -> bool {
        let Some(target) = target else {
            return false;
        };
        self.scroll_to_page(target, viewport, layouts);
        self.overscroll_accumulator = egui::Vec2::ZERO;
        true
    }

    pub fn clamp_pan(&mut self, viewport_rect: egui::Rect, layouts: &[PageLayout]) {
        // RR-15 Limit: GUI
        if layouts.is_empty() {
            return;
        }

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;

        // If SinglePage or TwoPageSingle, only clamp using target layouts
        let target_layouts: Vec<&PageLayout> = if self.display_mode == DisplayMode::SinglePage {
            if let Some(layout) = layouts.get(self.active_page) {
                vec![layout]
            } else {
                layouts.iter().collect()
            }
        } else if self.display_mode == DisplayMode::TwoPageSingle {
            let spread_indices = self.get_spread_indices(self.active_page, layouts.len());
            spread_indices.iter().filter_map(|&idx| layouts.get(idx)).collect()
        } else {
            layouts.iter().collect()
        };

        for layout in &target_layouts {
            min_x = min_x.min(layout.rect.min.x);
            max_x = max_x.max(layout.rect.max.x);
            min_y = min_y.min(layout.rect.min.y);
            max_y = max_y.max(layout.rect.max.y);
        }

        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        let min_overlap = 50.0f32;

        // **The tiles do not float sideways.** The grid hangs from the binding edge, so
        // the only horizontal freedom it has is scrolling towards its far end when it is
        // wider than the window. Without this the anchor carried across a change of
        // arrangement can leave it centred on one column, with the first tile — the one
        // a reader looks for first — pushed off the side they read from.
        if self.arranged_as_tiles && self.display_mode == DisplayMode::Continuous {
            // **The grid's edge is where its `x = 0` is, not where the grid has to sit.**
            // Binding it there took away the only freedom that can hold a reader's place
            // on the page while the pages are re-laid: a tile in the first column cannot
            // stay under a cursor on the right of the window unless the grid may move
            // right. The bound below is the general one — at least `min_overlap` of the
            // document stays on screen — and nothing narrower.
            let min_pan_y =
                max_y.mul_add(-self.zoom, viewport_rect.min.y + min_overlap - origin_no_pan.y);
            let max_pan_y =
                min_y.mul_add(-self.zoom, viewport_rect.max.y - min_overlap - origin_no_pan.y);
            self.pan.y = self.pan.y.clamp(min_pan_y, max_pan_y);
            return;
        }

        let min_pan_x =
            max_x.mul_add(-self.zoom, viewport_rect.min.x + min_overlap - origin_no_pan.x);
        let max_pan_x =
            min_x.mul_add(-self.zoom, viewport_rect.max.x - min_overlap - origin_no_pan.x);
        let clamped_x = self.pan.x.clamp(min_pan_x, max_pan_x);

        let min_pan_y =
            max_y.mul_add(-self.zoom, viewport_rect.min.y + min_overlap - origin_no_pan.y);
        let max_pan_y =
            min_y.mul_add(-self.zoom, viewport_rect.max.y - min_overlap - origin_no_pan.y);
        let clamped_y = self.pan.y.clamp(min_pan_y, max_pan_y);

        if self.display_mode == DisplayMode::SinglePage
            || self.display_mode == DisplayMode::TwoPageSingle
        {
            let threshold = 80.0; // Pull past edge distance threshold

            if self.scroll_direction == ScrollDirection::Vertical {
                let diff_y = self.pan.y - clamped_y;
                if diff_y.abs() > 0.0 {
                    self.overscroll_accumulator.y += diff_y;
                } else {
                    self.overscroll_accumulator.y = 0.0;
                }

                if self.overscroll_accumulator.y.abs() > threshold {
                    if self.overscroll_accumulator.y < 0.0 {
                        // Pulled up / past bottom -> next page/spread
                        if self.page_forward(viewport_rect, layouts) {
                            return;
                        }
                    } else {
                        // Pulled down / past top -> prev page/spread
                        if self.page_back(viewport_rect, layouts) {
                            return;
                        }
                    }
                }
            } else {
                let diff_x = self.pan.x - clamped_x;
                if diff_x.abs() > 0.0 {
                    self.overscroll_accumulator.x += diff_x;
                } else {
                    self.overscroll_accumulator.x = 0.0;
                }

                if self.overscroll_accumulator.x.abs() > threshold {
                    let is_r2l = self.binding_direction == BindingDirection::RightToLeft;

                    if (self.overscroll_accumulator.x < 0.0 && !is_r2l)
                        || (self.overscroll_accumulator.x > 0.0 && is_r2l)
                    {
                        // Go to next page/spread
                        if self.page_forward(viewport_rect, layouts) {
                            return;
                        }
                    } else {
                        // Go to prev page/spread
                        if self.page_back(viewport_rect, layouts) {
                            return;
                        }
                    }
                }
            }
        }

        self.pan.x = clamped_x;
        self.pan.y = clamped_y;
    }
}

#[cfg(test)]
mod arrangement_crossing {
    use super::{DisplayMode, PDFView, PageLayout};

    /// The window the tests place the view in.
    const WINDOW: egui::Rect =
        egui::Rect { min: egui::pos2(0.0, 0.0), max: egui::pos2(1000.0, 800.0) };

    /// The grid a continuous document is laid out as below `TILE_ZOOM`: ten columns
    /// hanging from `x = 0`, which is wider than the window at any useful zoom.
    #[allow(clippy::cast_precision_loss)]
    fn grid(pages: usize) -> Vec<PageLayout> {
        (0..pages)
            .map(|i| PageLayout {
                index: i,
                rect: egui::Rect::from_min_size(
                    egui::pos2((i % 10) as f32 * 632.0, (i / 10) as f32 * 812.0),
                    egui::vec2(612.0, 792.0),
                ),
            })
            .collect()
    }

    /// And the column it is laid out as above it.
    #[allow(clippy::cast_precision_loss)]
    fn column(pages: usize) -> Vec<PageLayout> {
        (0..pages)
            .map(|i| PageLayout {
                index: i,
                rect: egui::Rect::from_min_size(
                    egui::pos2(-306.0, i as f32 * 812.0),
                    egui::vec2(612.0, 792.0),
                ),
            })
            .collect()
    }

    /// Which page is under `at`, and where on it.
    fn under(view: &PDFView, at: egui::Pos2, layouts: &[PageLayout]) -> (usize, egui::Vec2) {
        let origin = view.get_origin(WINDOW);
        let layout = view.layout_under(at, origin, view.zoom(), layouts).expect("a page");
        let page_min = origin + layout.rect.min.to_vec2() * view.zoom();
        (layout.index, (at - page_min) / view.zoom())
    }

    /// **The point under the cursor does not move.** One sentence for every zoom,
    /// including the one that changes the arrangement — which is what a reader expects of
    /// a zoom and took five attempts to arrive at. The column and the grid put the same
    /// page in quite different places, so the view moves by whatever difference that
    /// makes.
    #[test]
    fn the_point_under_the_cursor_does_not_move() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let tiles = grid(25);
        view.set_zoom(0.2);
        view.restore_anchor(None, WINDOW, &tiles);

        let cursor = egui::pos2(700.0, 300.0);
        view.zoom_at(0.33, cursor, WINDOW, &tiles);
        let (chosen, local) = under(&view, cursor, &tiles);
        assert!(view.arrangement_is_changing(), "the crossing went unnoticed");

        let carried = view.take_anchor(WINDOW, &tiles);
        let pages = column(25);
        view.restore_anchor(carried, WINDOW, &pages);

        assert_eq!(view.active_page, chosen, "a different page was carried across");
        let (after, after_local) = under(&view, cursor, &pages);
        assert_eq!(after, chosen, "the cursor came out over a different page");
        assert!(
            (after_local - local).length() < 1.0,
            "the cursor came out at {after_local:?} of the page, not {local:?}"
        );
    }

    /// The same in the other direction: zooming out into the tiles leaves the point the
    /// reader was over where it was, and the grid goes wherever that needs it to.
    #[test]
    fn going_into_the_tiles_leaves_the_point_where_it_was() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(1.0);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(10, WINDOW, &pages);

        let cursor = egui::pos2(700.0, 300.0);
        view.zoom_at(0.25, cursor, WINDOW, &pages);
        let (chosen, local) = under(&view, cursor, &pages);

        let carried = view.take_anchor(WINDOW, &pages);
        let tiles = grid(25);
        view.restore_anchor(carried, WINDOW, &tiles);

        let (after, after_local) = under(&view, cursor, &tiles);
        assert_eq!(after, chosen, "the cursor came out over a different tile");
        assert!((after_local - local).length() < 1.0, "and at {after_local:?}, not {local:?}");
    }

    /// **The grid's edge is a bound, not a position.** A tile in the first column is at
    /// the grid's own `x = 0`, so reaching a cursor on the right needs the grid to move
    /// right — which a grid pinned to the left of the window cannot do. "When it becomes
    /// tiles the whole grid is left-aligned to the display area, so it does not do what
    /// was meant."
    #[test]
    fn a_tile_in_the_first_column_can_still_reach_the_cursor() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(1.0);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(10, WINDOW, &pages); // the grid's first column, second row

        let cursor = egui::pos2(750.0, 300.0);
        view.zoom_at(0.25, cursor, WINDOW, &pages);
        let (chosen, local) = under(&view, cursor, &pages);
        assert_eq!(chosen % 10, 0, "the fixture is not on the grid's first column");

        let carried = view.take_anchor(WINDOW, &pages);
        let tiles = grid(25);
        view.restore_anchor(carried, WINDOW, &tiles);
        assert!(view.pan.x > 1.0, "the grid stayed on its edge: pan.x is {}", view.pan.x);
        let (after, after_local) = under(&view, cursor, &tiles);
        assert_eq!(after, chosen);
        assert!((after_local - local).length() < 1.0, "it came out at {after_local:?}");
    }

    /// **A double-click on the bench coming *back* centres it too**, which it did not.
    ///
    /// Going out to the tiles had centred the page being read since the rule was written;
    /// coming back in went through the cursor's anchor, so a reader who double-clicked the
    /// empty bench at the edge of the window landed on the page view with their page
    /// against that edge. The bench points at nothing in either direction.
    #[test]
    fn a_double_click_on_the_bench_centres_coming_back_as_well() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let tiles = grid(25);
        view.set_zoom(0.2);
        view.restore_anchor(None, WINDOW, &tiles);
        view.active_page = 9;

        // A corner, as far from the middle as the window allows.
        view.double_click_on_the_bench(egui::pos2(950.0, 60.0), WINDOW, &tiles);
        assert!(view.is_page_view(), "the double-click did not cross into the page view");
        let pages = column(25);
        view.restore_anchor(view.take_anchor(WINDOW, &tiles), WINDOW, &pages);

        assert_eq!(view.active_page, 9);
        let middle = view.get_origin(WINDOW) + pages[9].rect.center().to_vec2() * view.zoom();
        assert!(
            (middle - WINDOW.center()).length() < 1.0,
            "page 9 came out centred on {middle:?}, not {:?}",
            WINDOW.center()
        );
    }

    /// **A layout that changes no arrangement still answers the request to centre.**
    ///
    /// This is how a document that has just opened shows its first page in the middle:
    /// nothing has zoomed, so nothing crosses the tile boundary, and the intent used to
    /// sit unanswered until some later zoom spent it somewhere the reader had not asked
    /// for.
    #[test]
    fn a_page_asked_for_is_centred_even_when_the_arrangement_is_not_changing() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(1.0);
        view.restore_anchor(None, WINDOW, &pages);
        assert!(!view.arrangement_is_changing(), "the fixture is mid-crossing");

        view.open_page(6);
        view.restore_anchor(None, WINDOW, &pages);

        let middle = view.get_origin(WINDOW) + pages[6].rect.center().to_vec2() * view.zoom();
        assert!(
            (middle - WINDOW.center()).length() < 1.0,
            "page 6 came out centred on {middle:?}, not {:?}",
            WINDOW.center()
        );
    }

    /// **The request waits for a window to be centred in.**
    ///
    /// The first layout after a document loads runs before the viewport is known. Spending
    /// the request on a rectangle of no size would centre the page on nothing and leave
    /// the reader looking at whatever the pan happened to be.
    #[test]
    fn a_request_to_centre_survives_a_layout_with_no_window_yet() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(1.0);

        view.open_page(6);
        view.restore_anchor(None, egui::Rect::NOTHING, &pages);
        view.restore_anchor(None, WINDOW, &pages);

        let middle = view.get_origin(WINDOW) + pages[6].rect.center().to_vec2() * view.zoom();
        assert!(
            (middle - WINDOW.center()).length() < 1.0,
            "page 6 came out centred on {middle:?}, not {:?}",
            WINDOW.center()
        );
    }

    /// **A double-click on a tile opens that page, in the middle of the window**, by the
    /// same route as the one on the bench. It used to rebuild the layout in the caller and
    /// scroll into it — the one way across the tile boundary that did not go through the
    /// anchor, and so the one whose landing had to be reasoned about separately.
    #[test]
    fn a_double_click_on_a_tile_opens_that_page_in_the_middle() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let tiles = grid(25);
        view.set_zoom(0.2);
        view.restore_anchor(None, WINDOW, &tiles);

        // The gesture: name the page, then leave the tiles.
        view.open_page(17);
        view.set_zoom(1.0);
        assert!(view.arrangement_is_changing(), "the crossing went unnoticed");
        let pages = column(25);
        view.restore_anchor(view.take_anchor(WINDOW, &tiles), WINDOW, &pages);

        assert_eq!(view.active_page, 17);
        let middle = view.get_origin(WINDOW) + pages[17].rect.center().to_vec2() * view.zoom();
        assert!(
            (middle - WINDOW.center()).length() < 1.0,
            "page 17 came out centred on {middle:?}, not {:?}",
            WINDOW.center()
        );
    }

    /// **A double-click on the bench brings the page back to the middle**, wherever the
    /// pointer was. Every other way into the tiles is a zoom, where the cursor is pointing
    /// at something and holding it still is the whole rule; a double-click on the empty
    /// bench points at nothing, and what the reader wants back is the page they were on.
    #[test]
    fn a_double_click_on_the_bench_centres_the_page_being_read() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(1.0);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(10, WINDOW, &pages);

        // The gesture: the page being read is remembered, then the zoom crosses.
        view.centre_next = view.page_at_middle(WINDOW, &pages);
        assert_eq!(view.centre_next, Some(10), "the fixture is not showing page 10");
        view.zoom_at(0.25, egui::pos2(950.0, 60.0), WINDOW, &pages); // a far corner
        let tiles = grid(25);
        view.restore_anchor(view.take_anchor(WINDOW, &pages), WINDOW, &tiles);

        assert_eq!(view.active_page, 10);
        let middle = view.get_origin(WINDOW) + tiles[10].rect.center().to_vec2() * view.zoom();
        assert!(
            (middle - WINDOW.center()).length() < 1.0,
            "the tile came out centred on {middle:?}, not {:?}",
            WINDOW.center()
        );
        assert!(view.centre_next.is_none(), "the request outlived the gesture");
    }

    /// **The button changes the page, not the composition.** Placing the new page against
    /// an edge or in the middle re-frames the window every time it is pressed: a reader a
    /// third of the way down page 8 presses *next* and the view jumps to a different
    /// arrangement of page 9. Moving by the pitch between the two leaves the window where
    /// it was over the page.
    #[test]
    fn going_to_a_page_leaves_the_view_where_it_was_over_it() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(0.5);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(8, WINDOW, &pages);

        // A little way down page 8 — far enough to be off its top edge, near enough that
        // the middle of the window is still over it, so that going to page 9 is going
        // somewhere.
        let into = 40.0_f32;
        view.pan.y = into.mul_add(-view.zoom(), view.pan.y);
        assert_eq!(view.current_page(WINDOW, &pages), 8, "the reader is no longer on page 8");
        let before = (pages[8].rect.min.y + into).mul_add(view.zoom(), view.get_origin(WINDOW).y);

        view.scroll_to_page(9, WINDOW, &pages);
        let after = (pages[9].rect.min.y + into).mul_add(view.zoom(), view.get_origin(WINDOW).y);
        assert!(
            (after - before).abs() < 0.01,
            "the same point of the next page came out at {after}, not {before}"
        );
    }

    /// And the page really did change, which is the half of it the reader asked for.
    #[test]
    fn going_to_a_page_moves_by_the_distance_between_them() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(0.5);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(8, WINDOW, &pages);
        let was = view.pan.y;

        view.scroll_to_page(11, WINDOW, &pages);
        let pitch = (pages[11].rect.min.y - pages[8].rect.min.y) * view.zoom();
        assert!((view.pan.y - (was - pitch)).abs() < 0.01, "moved by {}", was - view.pan.y);
        assert_eq!(view.active_page, 11);
    }

    /// **The same button works in both arrangements, and has something to do in each.**
    /// The grid hangs from its binding edge and the reader scrolls away from the page
    /// they were on; the column leaves it wherever the scroll left it.
    #[test]
    fn centring_brings_the_current_page_to_the_middle_of_the_window() {
        for (zoom, layouts) in [(0.2_f32, grid(25)), (1.0, column(25))] {
            let mut view = PDFView::new();
            view.display_mode = DisplayMode::Continuous;
            view.set_zoom(zoom);
            view.restore_anchor(None, WINDOW, &layouts);
            view.active_page = 12;
            view.pan = egui::vec2(-300.0, -4000.0); // scrolled somewhere else entirely

            let page = view.current_page(WINDOW, &layouts);
            view.centre_current_page(WINDOW, &layouts);
            let middle = view.get_origin(WINDOW) + layouts[page].rect.center().to_vec2() * zoom;
            assert!(
                (middle - WINDOW.center()).length() < 0.01,
                "at {zoom}x the page came out centred on {middle:?}, not {:?}",
                WINDOW.center()
            );
        }
    }

    /// **Two things read "which page am I on" and they read the same rule.** The line
    /// under a page number in the tiles and the counter in the view controls disagreed
    /// once — the counter answered with whatever tile the scroll had left in the middle of
    /// the window, which in a grid is nobody's idea of where they are.
    #[test]
    fn the_current_page_is_the_one_being_read_in_each_arrangement() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(1.0);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(8, WINDOW, &pages);
        assert_eq!(view.current_page(WINDOW, &pages), 8, "the page view reads its middle");

        // In the tiles, scrolling past a page is not being on it.
        let tiles = grid(25);
        view.set_zoom(0.2);
        view.restore_anchor(None, WINDOW, &tiles);
        view.active_page = 8;
        view.pan = egui::vec2(0.0, -2000.0);
        assert_eq!(view.current_page(WINDOW, &tiles), 8, "the grid reads where the reader is");
    }

    /// A zoom that stays on one side of the boundary rearranges nothing, so nothing is
    /// carried and the view is not touched: the reader is zooming, not navigating.
    #[test]
    fn a_zoom_that_crosses_nothing_rearranges_nothing() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let pages = column(25);
        view.set_zoom(0.5);
        view.restore_anchor(None, WINDOW, &pages);
        assert!(!view.arrangement_is_changing());
        view.set_zoom(2.0);
        assert!(!view.arrangement_is_changing());
    }
}

#[cfg(test)]
mod zoom_steps {
    use super::PDFView;

    /// **The bound is applied wherever the zoom is set, and nothing checked it.**
    /// Removing the clamp from `set_zoom` left every other zoom test passing: they step
    /// along the ladder, and the ladder stays inside the bounds by construction. A caller
    /// that sets a zoom directly — a fit computing one from a viewport width — is the case
    /// the bound exists for, and it is the case that had no test.
    #[test]
    fn a_zoom_set_outside_the_bounds_is_brought_back_inside() {
        let mut view = PDFView::new();
        view.set_zoom(1000.0);
        assert!(
            (view.zoom() - *PDFView::ZOOM_BOUNDS.end()).abs() < f32::EPSILON,
            "1000 was kept as {}",
            view.zoom()
        );
        view.set_zoom(-5.0);
        assert!(
            (view.zoom() - *PDFView::ZOOM_BOUNDS.start()).abs() < f32::EPSILON,
            "-5 was kept as {}",
            view.zoom()
        );
        view.set_zoom(2.5);
        assert!(
            (view.zoom() - 2.5).abs() < f32::EPSILON,
            "a zoom inside the bounds is left alone, not {}",
            view.zoom()
        );
    }

    /// The defect the ladder replaces: from any zoom a pinch or a fit had produced,
    /// multiplying by 1.2 never arrived at 100%. Stepping does, from either side.
    #[test]
    fn stepping_reaches_one_hundred_percent_from_anywhere() {
        for start in [0.11_f32, 0.37, 0.83, 0.96, 1.04, 2.7, 9.4] {
            let mut view = PDFView::new();
            view.set_zoom(start);
            let up = start < 1.0;
            for _ in 0..20 {
                let next = if up { view.zoom_step_up() } else { view.zoom_step_down() };
                view.set_zoom(next);
                if (view.zoom() - 1.0).abs() < f32::EPSILON {
                    break;
                }
            }
            assert!(
                (view.zoom() - 1.0).abs() < f32::EPSILON,
                "from {start} the ladder stopped at {}",
                view.zoom()
            );
        }
    }

    /// Stepping moves, and moves the right way. Without this a `find` that matched the
    /// current value would leave the buttons dead.
    #[test]
    fn a_step_always_moves_and_stops_at_the_bounds() {
        let mut view = PDFView::new();
        view.set_zoom(1.0);
        assert!(view.zoom_step_up() > 1.0);
        assert!(view.zoom_step_down() < 1.0);

        view.set_zoom(10.0);
        assert!((view.zoom_step_up() - 10.0).abs() < f32::EPSILON, "held at the ceiling");
        view.set_zoom(PDFView::ZOOM_FLOOR);
        let floor = PDFView::ZOOM_FLOOR;
        assert!((view.zoom_step_down() - floor).abs() < f32::EPSILON, "held at the floor");

        // The ladder and the bounds are the same range, or a press could ask for a zoom
        // the clamp then refuses and the button would look broken at one end.
        assert!(
            (PDFView::ZOOM_STEPS[0] - floor).abs() < f32::EPSILON,
            "the ladder starts at the floor"
        );
        assert!(
            (PDFView::ZOOM_STEPS[PDFView::ZOOM_STEPS.len() - 1] - 10.0).abs() < f32::EPSILON,
            "and ends at the ceiling"
        );
    }

    /// The mode boundary sits between two steps, so no step lands on it and leaves the
    /// mode ambiguous: 25% and 33% straddle 30%, and nothing is exactly 30%.
    #[test]
    fn no_step_lands_on_a_mode_boundary() {
        let mut view = PDFView::new();
        for step in PDFView::ZOOM_STEPS {
            view.set_zoom(step);
            assert!(
                (step - PDFView::TILE_ZOOM).abs() > 0.01,
                "step {step} sits on the only boundary there is"
            );
        }
        view.set_zoom(PDFView::TILE_STEP);
        assert!(!view.is_page_view(), "the double-click destination is a tile view");
        view.set_zoom(0.25);
        assert!(!view.is_page_view(), "the step below the boundary tiles");
        view.set_zoom(0.33);
        assert!(view.is_page_view(), "and the step above it reads");
    }

    /// A gesture that lands within the snap band is taken to mean the step, so the label
    /// and the rendering agree. 0.996 used to print as "100%" while rendering otherwise.
    #[test]
    fn a_gesture_near_a_step_is_taken_to_mean_it() {
        let layouts = [];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut view = PDFView::new();
        view.set_zoom(0.90);
        view.zoom_at(0.996, rect.center(), rect, &layouts);
        assert!((view.zoom() - 1.0).abs() < f32::EPSILON, "snapped: {}", view.zoom());
    }

    /// **A gesture is never between steps.** The whole travel of a pinch used to sit off
    /// the ladder, so the steps were something the buttons had and the fingers did not.
    #[test]
    fn a_gesture_only_ever_rests_on_a_step() {
        let layouts = [];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut view = PDFView::new();
        view.set_zoom(PDFView::ZOOM_FLOOR);

        let mut seen = Vec::new();
        for _ in 0..200 {
            let target = view.zoom_before_snapping() * 1.02;
            view.zoom_at(target, rect.center(), rect, &layouts);
            let z = view.zoom();
            assert!(
                PDFView::ZOOM_STEPS.iter().any(|s| (s - z).abs() < f32::EPSILON),
                "{z} is not a step"
            );
            if seen.last().is_none_or(|last: &f32| (last - z).abs() > f32::EPSILON) {
                seen.push(z);
            }
        }
        assert_eq!(seen, PDFView::ZOOM_STEPS.to_vec(), "and it visits every step, in order");
    }

    /// **The detent is sticky.** Wobbling around the midpoint between two steps must not
    /// send the view back and forth: without the extra reach, a gesture parked between
    /// 1.00 and 1.25 flips on the smallest change.
    #[test]
    fn a_gesture_parked_between_two_steps_does_not_oscillate() {
        let layouts = [];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut view = PDFView::new();
        view.set_zoom(1.0);

        // The geometric midpoint of 1.00 and 1.25, jittered either side of it.
        let middle = (1.0_f32 * 1.25).sqrt();
        for step in [1.0_f32, 1.005, 0.995, 1.004, 0.996] {
            view.zoom_at(middle * step, rect.center(), rect, &layouts);
            assert!((view.zoom() - 1.0).abs() < f32::EPSILON, "left 100% for {}", view.zoom());
        }
    }

    /// Sticky is not stuck: a gesture that keeps going does leave.
    #[test]
    fn a_gesture_that_commits_leaves_the_step() {
        let layouts = [];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut view = PDFView::new();
        view.set_zoom(1.0);
        view.zoom_at(1.20, rect.center(), rect, &layouts);
        assert!((view.zoom() - 1.25).abs() < f32::EPSILON, "stuck at {}", view.zoom());
    }

    /// **A pinch must be able to leave a step.** Snapping used to compute the next value
    /// from the snapped one, so small deltas landed back inside the band and the gesture
    /// stuck at 100% for good.
    #[test]
    fn a_gesture_can_travel_through_a_step() {
        let layouts = [];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut view = PDFView::new();
        view.set_zoom(0.98);
        for _ in 0..40 {
            let target = view.zoom_before_snapping() * 1.004;
            view.zoom_at(target, rect.center(), rect, &layouts);
        }
        assert!(view.zoom() > 1.02, "the gesture stuck at {}", view.zoom());
    }
}

#[cfg(test)]
mod click_ownership {
    use super::PDFView;

    /// **A click belongs to exactly one of them, at every zoom.** Page selection and text
    /// selection read the same `Response` over a page, so a zoom where both are live means
    /// a drag over a sentence also selects the page — and `Delete` is gated by nothing, so
    /// the page the reader clicked in is the page that goes. A zoom where neither is live
    /// means clicking a page does nothing at all.
    #[test]
    fn a_click_on_a_page_is_owned_by_one_of_the_two() {
        let mut view = PDFView::new();
        for zoom in [0.10_f32, 0.25, 0.29, 0.30, 0.33, 0.50, 1.0, 4.0, 10.0] {
            view.set_zoom(zoom);
            assert_ne!(
                view.selects_pages(),
                view.selects_text(),
                "at {zoom} pages={} text={}",
                view.selects_pages(),
                view.selects_text()
            );
        }
    }

    /// And each view owns the one that belongs to it: pages are arranged in the tile view
    /// and read in the page view.
    #[test]
    fn the_tile_view_selects_pages_and_the_page_view_selects_text() {
        let mut view = PDFView::new();
        view.set_zoom(PDFView::TILE_STEP);
        assert!(view.selects_pages() && !view.selects_text());
        view.set_zoom(1.0);
        assert!(view.selects_text() && !view.selects_pages());
    }
}

#[cfg(test)]
mod zoom_label {
    use super::PDFView;

    /// **A zoom that is not 100% must not read `100%`.** `{:.0}%` printed 99.6% as `100%`,
    /// which is the label of a different rendering; the reader had no way to tell the two
    /// apart. Only a fit produces such a value now, and a fit is exactly the case where the
    /// number matters — it is the one the reader did not choose.
    #[test]
    fn a_zoom_near_a_step_does_not_borrow_its_label() {
        let mut view = PDFView::new();
        view.set_zoom(1.0);
        let hundred = view.zoom_label();
        for near in [0.996_f32, 1.004, 0.9951] {
            view.set_zoom(near);
            assert_ne!(view.zoom_label(), hundred, "{near} reads as 100%");
        }
    }

    /// A step reads as a whole number, with no decimal to carry.
    #[test]
    fn a_step_reads_as_itself() {
        let mut view = PDFView::new();
        for (zoom, expected) in [(1.0_f32, "100%"), (0.5, "50%"), (0.2, "20%"), (10.0, "1000%")] {
            view.set_zoom(zoom);
            assert_eq!(view.zoom_label(), expected);
        }
    }

    /// **Every step is a whole percentage, so the decimal belongs to a fit and nothing
    /// else.** That is the point of showing one: a fit is the zoom the reader did not
    /// choose, and the only one that can land between steps.
    #[test]
    fn only_a_fit_carries_a_decimal() {
        let mut view = PDFView::new();
        for step in PDFView::ZOOM_STEPS {
            view.set_zoom(step);
            let label = view.zoom_label();
            assert!(!label.contains('.'), "step {step} reads as {label}");
        }

        view.set_zoom(0.8134);
        assert_eq!(view.zoom_label(), "81.3%", "a fit says where it actually is");
    }
}

#[cfg(test)]
mod overscroll_paging {
    use super::{BindingDirection, DisplayMode, PDFView, PageLayout, ScrollDirection};

    /// Four pages, each 100 wide and 100 tall, spaced 120 apart down the page.
    fn pages() -> Vec<PageLayout> {
        (0..4)
            .map(|i| PageLayout {
                index: i,
                rect: egui::Rect::from_min_max(
                    egui::pos2(-50.0, i as f32 * 120.0),
                    egui::pos2(50.0, (i as f32).mul_add(120.0, 100.0)),
                ),
            })
            .collect()
    }

    /// Pulls the view past an edge from page 1 and says where it landed.
    fn pull_from_page_one(mode: DisplayMode, dir: ScrollDirection, pan: egui::Vec2) -> usize {
        let mut view = PDFView::new();
        view.display_mode = mode;
        view.scroll_direction = dir;
        view.active_page = 1;
        view.pan = pan;
        view.clamp_pan(
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0)),
            &pages(),
        );
        view.active_page
    }

    /// The same pull, in the same direction, must page the same way whether the document
    /// scrolls vertically or horizontally.
    ///
    /// The two axes each carried their own copy of "step to the next page or spread" and
    /// "step to the previous one" — four copies of two decisions inside one 158-line
    /// function, which is what its `RR-15 Limit: GUI` was paying for.
    #[test]
    fn both_axes_page_forward_alike() {
        assert_eq!(
            pull_from_page_one(
                DisplayMode::SinglePage,
                ScrollDirection::Vertical,
                egui::vec2(0.0, -500.0)
            ),
            2,
            "vertical, pulled past the bottom"
        );
        assert_eq!(
            pull_from_page_one(
                DisplayMode::SinglePage,
                ScrollDirection::Horizontal,
                egui::vec2(-400.0, 0.0)
            ),
            2,
            "horizontal, pulled past the right"
        );
    }

    #[test]
    fn both_axes_page_back_alike() {
        assert_eq!(
            pull_from_page_one(
                DisplayMode::SinglePage,
                ScrollDirection::Vertical,
                egui::vec2(0.0, 200.0)
            ),
            0,
            "vertical, pulled past the top"
        );
        assert_eq!(
            pull_from_page_one(
                DisplayMode::SinglePage,
                ScrollDirection::Horizontal,
                egui::vec2(400.0, 0.0)
            ),
            0,
            "horizontal, pulled past the left"
        );
    }

    /// A spread steps over both of its pages, not one — 1 and 2 are shown together, so
    /// forward from them is 3 and back from them is 0.
    #[test]
    fn a_spread_steps_over_both_of_its_pages() {
        assert_eq!(
            pull_from_page_one(
                DisplayMode::TwoPageSingle,
                ScrollDirection::Vertical,
                egui::vec2(0.0, -700.0)
            ),
            3,
            "forward off the spread [1, 2]"
        );
        assert_eq!(
            pull_from_page_one(
                DisplayMode::TwoPageSingle,
                ScrollDirection::Vertical,
                egui::vec2(0.0, 300.0)
            ),
            0,
            "back off the spread [1, 2]"
        );
    }

    /// Right-to-left binding reverses which way a horizontal pull pages, and nothing else.
    #[test]
    fn right_to_left_binding_reverses_the_horizontal_axis_only() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.scroll_direction = ScrollDirection::Horizontal;
        view.binding_direction = BindingDirection::RightToLeft;
        view.active_page = 1;
        view.pan = egui::vec2(400.0, 0.0);
        view.clamp_pan(
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0)),
            &pages(),
        );
        assert_eq!(view.active_page, 2, "pulled past the left, bound right-to-left");
    }

    /// A pull that stops short of the threshold pages nothing.
    #[test]
    fn a_pull_under_the_threshold_stays_put() {
        assert_eq!(
            pull_from_page_one(
                DisplayMode::SinglePage,
                ScrollDirection::Vertical,
                egui::vec2(0.0, 60.0)
            ),
            1,
            "60 is under the 80 the overscroll asks for"
        );
    }
}

#[cfg(test)]
mod spread_pairing {
    use super::PDFView;

    /// **This is the one place that decides which pages are shown together.**
    ///
    /// The two fit buttons each carried their own copy of the arithmetic, so the rule
    /// stood in three places; the copies agreed with this one for every page index of
    /// every non-empty document, but a rule in three places is a rule that drifts and only
    /// this one was reachable from a test. The buttons are gone and so are the copies.
    #[test]
    fn a_cover_stands_alone_and_the_rest_pair_off_after_it() {
        let mut view = PDFView::new();
        view.cover_page_alone = true;
        let spreads: Vec<Vec<usize>> = (0..6).map(|p| view.get_spread_indices(p, 6)).collect();
        assert_eq!(
            spreads,
            vec![vec![0], vec![1, 2], vec![1, 2], vec![3, 4], vec![3, 4], vec![5],],
            "cover alone, six pages"
        );
    }

    #[test]
    fn without_a_cover_the_pairing_starts_at_the_first_page() {
        let mut view = PDFView::new();
        view.cover_page_alone = false;
        let spreads: Vec<Vec<usize>> = (0..5).map(|p| view.get_spread_indices(p, 5)).collect();
        assert_eq!(
            spreads,
            vec![vec![0, 1], vec![0, 1], vec![2, 3], vec![2, 3], vec![4]],
            "no cover, five pages"
        );
    }

    /// An empty document has no spread. The copies answered `[current_page]`, naming a
    /// page that is not there; both callers then found no layout for it and did nothing,
    /// which is what this says instead.
    #[test]
    fn an_empty_document_has_no_spread() {
        let view = PDFView::new();
        assert!(view.get_spread_indices(0, 0).is_empty());
        assert!(view.get_spread_indices(3, 0).is_empty());
    }
}

#[cfg(test)]
mod visible_pages {
    use super::{DisplayMode, PDFView, PageLayout};

    /// Six pages, each 100 tall, spaced 120 apart, in a viewport tall enough for all six.
    fn pages() -> Vec<PageLayout> {
        (0..6)
            .map(|i| PageLayout {
                index: i,
                rect: egui::Rect::from_min_max(
                    egui::pos2(-50.0, i as f32 * 120.0),
                    egui::pos2(50.0, (i as f32).mul_add(120.0, 100.0)),
                ),
            })
            .collect()
    }

    fn shown(mode: DisplayMode, active: usize) -> Vec<usize> {
        let mut view = PDFView::new();
        view.display_mode = mode;
        view.active_page = active;
        let layouts = pages();
        view.visible_page_rects(
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 2000.0)),
            &layouts,
        )
        .into_iter()
        .map(|(layout, _)| layout.index)
        .collect()
    }

    /// **The display mode decides which pages are drawn.** `draw_pages` and
    /// `draw_page_backings` each decided it separately; a page one drew and the other did
    /// not would have been a backing with no page on it, or a page with no backing.
    #[test]
    fn a_single_page_mode_shows_the_active_page_and_nothing_else() {
        assert_eq!(shown(DisplayMode::SinglePage, 2), vec![2]);
        assert_eq!(shown(DisplayMode::SinglePage, 0), vec![0]);
    }

    #[test]
    fn a_single_spread_shows_both_of_its_pages_and_nothing_else() {
        assert_eq!(shown(DisplayMode::TwoPageSingle, 2), vec![1, 2]);
        assert_eq!(shown(DisplayMode::TwoPageSingle, 0), vec![0], "the cover is alone");
    }

    #[test]
    fn the_scrolling_modes_show_everything_the_viewport_reaches() {
        assert_eq!(shown(DisplayMode::Continuous, 2), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(shown(DisplayMode::TwoPageSpread, 2), vec![0, 1, 2, 3, 4, 5]);
    }

    /// A page the viewport does not reach is not drawn, whatever the mode says.
    #[test]
    fn a_page_off_the_viewport_is_not_shown() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::Continuous;
        let layouts = pages();
        let shown: Vec<usize> = view
            .visible_page_rects(
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 300.0)),
                &layouts,
            )
            .into_iter()
            .map(|(layout, _)| layout.index)
            .collect();
        assert_eq!(shown, vec![0, 1, 2], "the fourth page starts at 380, past a 300-tall viewport");
    }
}
