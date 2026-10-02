//! What the hand does: zoom and scroll gestures, paging, and the pan held inside the pages.

use super::{
    BindingDirection, DisplayMode, EASE_LET_GO, EASE_SLIDE, Edge, GESTURE_GAP, GESTURE_TAIL,
    PAGE_TURN_PULL, PDFView, PageLayout, ScrollDirection, page_hold,
};

impl PDFView {
    pub(super) fn handle_zoom_gestures(
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

    /// Pans by the wheel, and says whether it moved anything.
    pub(super) fn handle_scroll_panning(&mut self, ui: &egui::Ui) -> egui::Vec2 {
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
                return scroll_delta;
            }
            egui::Vec2::ZERO
        })
    }

    pub(super) fn handle_input(
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
        let moved_by_wheel = if is_hovered {
            self.handle_zoom_gestures(ui, viewport_rect, layouts);
            self.handle_scroll_panning(ui)
        } else {
            egui::Vec2::ZERO
        };
        let shift_down = ui.input(|i| i.modifiers.shift);
        let mut moved = moved_by_wheel.length();
        if response.dragged() && (!shift_down || self.is_page_view()) {
            self.pan += response.drag_delta();
            moved += response.drag_delta().length();
        }
        if response.double_clicked() {
            let pos = ui
                .input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()))
                .filter(|p| viewport_rect.contains(*p))
                .unwrap_or_else(|| viewport_rect.center());
            self.double_click_on_the_bench(pos, viewport_rect, layouts);
        }
        // A drag or the wheel: either is the reader moving the view, and the page comes
        // away from its edge for both.
        self.gesture(response.dragged(), moved);
    }

    /// Takes what the reader did to the view this frame: a pointer held, and a distance.
    ///
    /// **A held pointer keeps the page off its edge even while it is still** — a reader
    /// holding a page half-turned has not let go of it. A trackpad that is merely finishing
    /// does not: below [`GESTURE_TAIL`] there is nothing deliberate left in a flick, and a
    /// page nudged off its place by momentum that is nearly spent, only to spring back, is
    /// the restlessness this leaves out — but it takes [`GESTURE_GAP`] such frames in a row
    /// to say so, because a swipe that is still going has gaps of its own. Its own function
    /// so that a test can say what a reader did in the same words a frame does.
    pub(super) fn gesture(&mut self, held: bool, moved: f32) {
        if moved >= GESTURE_TAIL {
            self.quiet = 0;
        } else {
            self.quiet = self.quiet.saturating_add(1);
        }
        self.pulling = held || self.quiet < GESTURE_GAP;
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
        self.open_page(self.current_page());
        let target_zoom = if self.is_page_view() { Self::TILE_STEP } else { 1.0 };
        self.zoom_at(target_zoom, pos, viewport, layouts);
    }

    /// Moves to the page or spread after the current one, and says whether there was one.
    ///
    /// **A spread steps over both of its pages**: forward from `[1, 2]` is 3, not 2. That
    /// rule, and its mirror in [`Self::page_back`], each stood in two places — once for the
    /// vertical axis and once for the horizontal — inside a single function, which is what
    /// its `RR-15 Limit: GUI` was paying for. Two axes deciding the same thing separately
    /// is the shape [`CODING.md`'s Rule D](../../../../CODING.md) names for frontends, arrived
    /// at inside one.
    pub(super) fn page_forward(&mut self, layouts: &[PageLayout]) -> bool {
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
        self.step_to(next, layouts)
    }

    /// Moves to the page or spread before the current one, and says whether there was one.
    pub(super) fn page_back(&mut self, layouts: &[PageLayout]) -> bool {
        let prev = if self.display_mode == DisplayMode::TwoPageSingle {
            self.get_spread_indices(self.active_page, layouts.len())
                .first()
                .copied()
                .filter(|&first| first > 0)
                .map(|first| first - 1)
        } else {
            (self.active_page > 0).then_some(self.active_page.saturating_sub(1))
        };
        self.step_to(prev, layouts)
    }

    /// Goes to `target`, and stops the pull that got there from carrying on into it.
    ///
    /// **The drag is disowned, not the distance.** The pull is measured from where the
    /// page sits, and the page has just moved — so without this the same held pointer
    /// would be over the next page's edge by the same amount and turn it too, a page per
    /// frame for as long as the reader kept hold. Disowning it for one frame was not
    /// enough for the wheel, whose tail arrives over the frames after: [`Self::arriving`]
    /// holds until the page it turned to has got there.
    pub(super) fn step_to(&mut self, target: Option<usize>, layouts: &[PageLayout]) -> bool {
        let Some(target) = target else {
            return false;
        };
        self.scroll_to_page(target, layouts);
        self.pulling = false;
        true
    }

    /// Goes to `page` and brings it in the way a page turned to comes in.
    ///
    /// **The buttons and the keys turn pages the same way the reader's hand does.** Going
    /// to a page used to place it and nothing else, so the four page buttons and the arrow
    /// keys swapped the contents of the window while a pull slid the next page in — two
    /// answers to "show me the next page", one of which was the one nobody had written a
    /// turn for. A page asked for by name arrives at its head whichever side of it the
    /// reader was on, and comes in from the side they are travelling.
    pub fn turn_to(&mut self, page: usize, viewport: egui::Rect, layouts: &[PageLayout]) {
        let onwards = page >= self.active_page;
        if self.step_to(Some(page), layouts) && self.is_page_view() {
            self.land(viewport, onwards, Edge::Head, layouts);
        }
    }

    /// What is on screen, in layout units: the page, the spread, or the whole grid.
    ///
    /// **Asked again after a page turns**, which is why it is its own function: the page
    /// arrived at is not the page the clamp was computed for, and landing on its head
    /// needs its own height.
    pub(super) fn shown_extent(&self, layouts: &[PageLayout]) -> ((f32, f32), (f32, f32)) {
        let mut shown: Vec<&PageLayout> = if self.is_page_view() {
            match self.display_mode {
                DisplayMode::SinglePage => layouts.get(self.active_page).into_iter().collect(),
                DisplayMode::TwoPageSingle => self
                    .get_spread_indices(self.active_page, layouts.len())
                    .iter()
                    .filter_map(|&idx| layouts.get(idx))
                    .collect(),
            }
        } else {
            layouts.iter().collect()
        };
        // **A page the layout does not have is not an empty extent.** `active_page` outlives
        // the pages themselves for a frame whenever a document loses some — an extraction
        // that moves its pages out, a deletion — and `f32::MAX`..`f32::MIN` travels from
        // here into `page_hold` as a page of negative size, which places the view nowhere
        // anybody asked for. What is on screen then is whatever there is.
        if shown.is_empty() {
            shown = layouts.iter().collect();
        }
        let mut across = (f32::MAX, f32::MIN);
        let mut up = (f32::MAX, f32::MIN);
        for layout in &shown {
            across = (across.0.min(layout.rect.min.x), across.1.max(layout.rect.max.x));
            up = (up.0.min(layout.rect.min.y), up.1.max(layout.rect.max.y));
        }
        (across, up)
    }

    pub fn clamp_pan(&mut self, viewport_rect: egui::Rect, layouts: &[PageLayout]) {
        // RR-15 Limit: GUI
        if layouts.is_empty() {
            return;
        }

        let ((min_x, max_x), (min_y, max_y)) = self.shown_extent(layouts);

        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        let min_overlap = 50.0f32;

        // **The tiles do not float sideways.** The grid hangs from the binding edge, so
        // the only horizontal freedom it has is scrolling towards its far end when it is
        // wider than the window. Without this the anchor carried across a change of
        // arrangement can leave it centred on one column, with the first tile — the one
        // a reader looks for first — pushed off the side they read from.
        if self.arranged_as_tiles {
            // Zooming out to the grid while a page was still coming in leaves the offset
            // it was carrying to be added to a pan that no longer means the same thing.
            self.nothing_in_flight();
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

        if self.is_page_view() {
            return self.hold_the_page(viewport_rect, (min_x, max_x), (min_y, max_y), layouts);
        }

        self.pan.x = clamped_x;
        self.pan.y = clamped_y;
    }

    /// Moves the held-off distance towards `want`, and says where it now is.
    ///
    /// **Out with the gesture at once, back on its own time.** Going out the page is under
    /// the reader's finger, and anything but following it exactly reads as lag; coming
    /// back nothing is holding it, and the single-frame jump it used to make from eighty
    /// points to nothing was the jerk a reader saw every time they let go short of a turn.
    pub(super) fn ease_home(&mut self, want: f32) -> f32 {
        self.adrift = if want.abs() >= self.adrift.abs() {
            // Following the reader out: the way back from here is the brisk one.
            self.homing = EASE_LET_GO;
            want
        } else {
            (want - self.adrift).mul_add(self.homing, self.adrift)
        };
        // Below half a point there is nothing left to see, and stopping keeps the view
        // from asking for a repaint for ever.
        if self.adrift.abs() < 0.5 {
            self.adrift = 0.0;
            self.arriving = false;
        }
        self.adrift
    }

    /// Starts the page just turned to on its way in, stopping it at `at`.
    ///
    /// **A page too big for the window arrives at one of its edges, not its middle.**
    /// Landing in the middle hides the lines a page starts with, and from there the foot is
    /// half a page away — so a reader turning pages saw only bottom halves, and each turn
    /// took a pull far shorter than the last had. `along.1` is the head on either axis —
    /// the top, or the binding side — and `along.0` the foot; a page that fits has only the
    /// one place to be and [`page_hold`] returns it for both.
    ///
    /// `onwards` is the direction of travel, which is the side the page comes in from; it
    /// is not the same question as which edge it stops at. A pull that reaches the foot of
    /// one page carries on to the head of the next, and a pull the other way stops at the
    /// foot of the one before — but a reader who asks for page 40 wants its head whichever
    /// side of 40 they were on.
    pub(super) fn land(
        &mut self,
        viewport: egui::Rect,
        onwards: bool,
        at: Edge,
        layouts: &[PageLayout],
    ) {
        let horizontal = self.scroll_direction == ScrollDirection::Horizontal;
        let (across, up) = self.shown_extent(layouts);
        let origin = self.get_origin_no_pan(viewport);
        let sideways = page_hold(across, (viewport.min.x, viewport.max.x), self.zoom, origin.x);
        let upright = page_hold(up, (viewport.min.y, viewport.max.y), self.zoom, origin.y);
        let (along, cross) = if horizontal { (sideways, upright) } else { (upright, sideways) };
        let read_from = match at {
            Edge::Head => along.1,
            Edge::Foot => along.0,
        };
        self.landing = read_from;
        // **It comes in from the side it comes from**, a window's width away and travelling,
        // rather than being where the last page was between one frame and the next. Reading
        // on, the next page is below, or past the binding edge — and a document bound on the
        // right has that edge on the other side, so the same page arrives from the other way.
        let behind = if horizontal && self.binding_direction == BindingDirection::RightToLeft {
            !onwards
        } else {
            onwards
        };
        let window = if horizontal { viewport.width() } else { viewport.height() };
        self.adrift = if behind { window } else { -window };
        self.homing = EASE_SLIDE;
        self.arriving = true;
        // Across the page, wherever the reader had it, held inside the new page's own room.
        self.pan = if horizontal {
            egui::vec2(read_from + self.adrift, self.pan.y.clamp(cross.0, cross.1))
        } else {
            egui::vec2(self.pan.x.clamp(cross.0, cross.1), read_from + self.adrift)
        };
    }

    /// Carries the arriving page the rest of the way to where it stops.
    ///
    /// **Nothing else moves it while it travels.** The pull is measured from where a page
    /// belongs and an arriving one is a window away from that, so a reader still scrolling
    /// would be read as hauling it backwards — which is a page flapping between two. Their
    /// scrolling reaches the page once it is home.
    pub(super) fn glide(&mut self, viewport: egui::Rect, across: (f32, f32), up: (f32, f32)) {
        let origin = self.get_origin_no_pan(viewport);
        let sideways = page_hold(across, (viewport.min.x, viewport.max.x), self.zoom, origin.x);
        let upright = page_hold(up, (viewport.min.y, viewport.max.y), self.zoom, origin.y);
        let horizontal = self.scroll_direction == ScrollDirection::Horizontal;
        let (along, cross) = if horizontal { (sideways, upright) } else { (upright, sideways) };
        // Held inside the page's own range, so that a zoom or a re-layout part way through
        // cannot land it somewhere the page is not.
        let stops_at = self.landing.clamp(along.0, along.1);
        let travelling = self.ease_home(0.0);
        if horizontal {
            self.pan = egui::vec2(stops_at + travelling, self.pan.y.clamp(cross.0, cross.1));
        } else {
            self.pan = egui::vec2(self.pan.x.clamp(cross.0, cross.1), stops_at + travelling);
        }
    }

    /// Holds the page against the window, and turns to the next one when it is pulled far
    /// enough off it.
    ///
    /// **The page moves while it is being pulled.** It used to be pinned to its bound on
    /// every frame while a hidden accumulator counted the drag, so nothing happened and
    /// then the page changed; what the reader sees now is the gap opening between the
    /// page's edge and the window's, and the turn comes when that gap reaches
    /// [`PAGE_TURN_PULL`]. Letting go without reaching it puts the page back, because the
    /// pull is only allowed while a drag is in hand.
    pub(super) fn hold_the_page(
        &mut self,
        viewport: egui::Rect,
        across: (f32, f32),
        up: (f32, f32),
        layouts: &[PageLayout],
    ) {
        let origin = self.get_origin_no_pan(viewport);
        let horizontal = self.scroll_direction == ScrollDirection::Horizontal;
        if self.arriving {
            return self.glide(viewport, across, up);
        }
        let (hold, along) = if horizontal {
            (page_hold(across, (viewport.min.x, viewport.max.x), self.zoom, origin.x), self.pan.x)
        } else {
            (page_hold(up, (viewport.min.y, viewport.max.y), self.zoom, origin.y), self.pan.y)
        };

        let over = if along > hold.1 {
            along - hold.1
        } else if along < hold.0 {
            along - hold.0
        } else {
            0.0
        };

        // **Only a drag turns a page.** Anything that leaves `pan` far from where the
        // page sits — a document opening, a zoom, a change of mode — is a distance and not
        // a pull, and turning on it would page the document for reasons the reader never
        // asked about. The first version of this turned a page while settling the very
        // first frame.
        if self.pulling && !self.arriving && over.abs() >= PAGE_TURN_PULL {
            // Pulled the page up past its bottom, or leftwards past its right edge: on to
            // the next one. A right-bound document reads the other way across.
            let onwards = if horizontal {
                (over < 0.0) != (self.binding_direction == BindingDirection::RightToLeft)
            } else {
                over < 0.0
            };
            let turned = if onwards { self.page_forward(layouts) } else { self.page_back(layouts) };
            if turned {
                let at = if onwards { Edge::Head } else { Edge::Foot };
                self.land(viewport, onwards, at, layouts);
                return;
            }
        }

        // The pull is the reader's to hold, and only theirs: a page nobody is moving sits
        // where it belongs — and a page still on its way in is left to get there rather
        // than being leant on while it travels.
        let want = if self.pulling && !self.arriving {
            over.clamp(-PAGE_TURN_PULL, PAGE_TURN_PULL)
        } else {
            0.0
        };
        let allowed = self.ease_home(want);
        // **Both axes are held, and only the one that pages carries the pull.** The cross
        // axis used to be set to its lower bound outright, which pinned a zoomed-in page
        // against one side and made scrolling across it do nothing at all.
        let sideways = page_hold(across, (viewport.min.x, viewport.max.x), self.zoom, origin.x);
        let upright = page_hold(up, (viewport.min.y, viewport.max.y), self.zoom, origin.y);
        self.pan.x =
            self.pan.x.clamp(sideways.0, sideways.1) + if horizontal { allowed } else { 0.0 };
        self.pan.y =
            self.pan.y.clamp(upright.0, upright.1) + if horizontal { 0.0 } else { allowed };
    }
}
