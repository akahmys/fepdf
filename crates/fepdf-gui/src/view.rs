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
/// What the page view shows at once.
///
/// **Two, and both reachable.** There were four, which were a two-by-two: one page or
/// two, scrolling or a page at a time. The scrolling half is gone — the tile view is
/// what this window offers for moving through a document, and a column of pages was a
/// second answer to the same question that carried its own layout, its own clamp and its
/// own idea of which page you were on.
///
/// `TwoPageSingle` was the other half of the pair and had no button: eighteen sites of
/// logic reachable from nothing. It is the spread now.
pub enum DisplayMode {
    /// One page.
    SinglePage,
    /// Two pages side by side, a spread at a time.
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
    /// Whether the reader is moving the view this frame, by a drag or by the wheel.
    ///
    /// **`clamp_pan` runs every frame and takes no input**, so without this it cannot tell
    /// a reader holding a page off the window from the frame after they let go — and the
    /// page would stay where it was pulled to rather than settling back.
    ///
    /// **The wheel counts.** It was a drag alone, so a wheel could never turn a page: on a
    /// page that fits the window there is nothing to scroll and nothing happened at all,
    /// which is a scroll wheel that does nothing on the commonest page there is.
    pulling: bool,
    /// How far the page is from where it belongs, along the axis that pages, in points.
    ///
    /// **A position of its own, not a reading of `pan`.** Two things put a page off its
    /// place and both come home the same way: a reader pulling it off its edge, and a page
    /// that has just been turned to, which starts a window's width out and slides in. Each
    /// used to arrive in a single frame, which is a teleport and reads as a snap.
    adrift: f32,
    /// How much of the remaining distance the page closes each frame, this journey.
    ///
    /// **The two journeys are not the same errand.** A page let go short of a turn is going
    /// back to where it already was, and the reader wants that out of the way; a page
    /// turned to is arriving, and that is the movement they are meant to see. One rate for
    /// both made the second as brisk as the first.
    homing: f32,
    /// Whether the page turned to is still coming in.
    ///
    /// **A page on its way in cannot be pulled, and nothing turns while it travels.** A
    /// held drag never stops sending anything and a trackpad keeps sending for most of a
    /// second after the fingers leave it: without this the same gesture re-earns the pull
    /// on the frame after a turn and pages through the document, and worse, the distance
    /// the arriving page has left to travel reads as a pull the other way and turns it
    /// back — which is a page flapping between two.
    ///
    /// **It is the arrival that holds it, not the gesture.** Waiting for the gesture to
    /// end froze the view instead: a reader scrolling steadily never stops sending, so one
    /// page turned and nothing more happened until they took their fingers off.
    arriving: bool,
    /// Where the arriving page stops, as a `pan` along the axis that pages.
    ///
    /// **Remembered rather than clamped to.** The travelling offset used to be added to a
    /// `pan` that had just been held inside the page's own range, which works only while
    /// the page comes in from the side its landing edge is on: a page turned back to from
    /// a later one arrives at its head and slides down from above, and the hold ate the
    /// offset every frame and settled it on its foot instead.
    landing: f32,
    /// Frames since the reader last moved the view by anything worth counting.
    ///
    /// **A frame with nothing in it is not the end of a scroll.** The wheel arrives in
    /// gaps — a slow swipe reaches egui as a few points, then nothing, then a few more —
    /// and reading each gap as "let go" had the page spring nearly half the way back and
    /// be hauled out again on the next frame: a page shaking in place, and a swipe that
    /// could never turn anything either, because every gap ate what it had gained.
    quiet: u8,
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

/// Something a reader can ask for that only one of the two views answers.
///
/// **The split was spelled four ways and declared nowhere.** `is_page_view`,
/// `selects_pages`, `selects_text` and a bare `zoom < TILE_ZOOM` all said the same thing at
/// different call sites, so which view could do what was something you found out by reading
/// the whole window — and two of the four spellings had drifted: a content tool could be
/// switched on where its clicks were thrown away, and the arrow keys built a selection
/// nothing showed.
///
/// **Selecting text and selecting pages are opposites**, which is the invariant that
/// matters most here: one `Response` covers a page and both of them read it. While both
/// were live, a drag meant to select text also selected the page, and `Delete` then removed
/// the page the reader had merely clicked in. One act — rotating a page — belongs to both
/// views, and it says so here rather than at the six places that would each have to
/// remember.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Selecting words on a page, and the brushes that work over a selection.
    SelectText,
    /// Drawing on a page: a redaction box, a caliper measurement, a signature.
    DrawOnPage,
    /// Going from one page to the next, by pulling the page or by the arrow keys.
    TurnPages,
    /// Choosing whole pages: a click, a marquee, select-all.
    SelectPages,
    /// Changing which pages there are and in what order: dragging one to a new place,
    /// duplicating, deleting, inserting another document, extracting a selection.
    ///
    /// **The grid is where a document is arranged**, because every one of these is about a
    /// page's place among the others and the page view shows a page with no others around
    /// it. Rotation is not one of them: it changes the page rather than the document.
    ArrangePages,
    /// Opening one page from the grid, by double-clicking it.
    OpenPage,
    /// Turning a page a quarter at a time.
    ///
    /// **One of the two acts both views answer.** A reader who is reading a page that came
    /// in sideways wants it upright there and then, and a reader looking at the grid wants
    /// the same for the pages they have picked out.
    RotatePages,
    /// Taking pages out of the document.
    ///
    /// **The other act both answer, and for the same reason**: a reader who has read a
    /// page and does not want it should not have to go and find it in the grid first. What
    /// goes is what the view has in hand — the page being read, or the pages picked out —
    /// which is the same rule [`Self::RotatePages`] follows and is why the two are not one
    /// act: rotating changes a page and this removes it, and a menu that offered "delete"
    /// where it meant something else is how a reader loses a page they were looking at.
    DeletePages,
}

/// Which end of a page the view stops at when it gets there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// The top of it, or the binding side: where the reading starts.
    Head,
    /// The other end, which is where a reader going backwards was last looking.
    Foot,
}

/// How far a page may be pulled past its edge before the view turns to the next one.
///
/// **It is a distance the reader can see, not a total they cannot.** The pull used to be
/// summed into an accumulator while `pan` was held at its bound every frame, so the page
/// never moved: a reader dragged, nothing happened, and then the page changed. This is the
/// gap that opens between the page's edge and the window's, which is on screen the whole
/// time it is growing.
///
/// **Half what it was**, because until the turn comes the scrolling is going nowhere and
/// the reader feels it catch. Eighty points is two lines of body text of travel spent on
/// nothing; forty is still far enough that a page cannot turn by a twitch.
const PAGE_TURN_PULL: f32 = 40.0;

/// How much of the way to its place an out-of-place page travels each frame.
///
/// **Only the way home is eased.** Going out, the page is under the reader's finger and
/// anything but following it exactly reads as lag; coming home, nothing is holding it, and
/// arriving in a single frame is the snap this eases out of.
///
/// At 0.45 a full pull is back inside a tenth of a second — enough frames to read as a
/// movement rather than a jump, and not so many that the page is still travelling when the
/// reader's next gesture arrives.
const EASE_LET_GO: f32 = 0.45;

/// The same, for a page arriving from a turn: about a third of a second.
///
/// **Slower than a page going back, because this one is worth watching.** A page put back
/// where it already was is an undoing and wants to be over; a page coming in is the turn
/// itself, and at the rate of an undoing it is over before the eye has followed it.
const EASE_SLIDE: f32 = 0.28;

/// The per-frame movement below which nothing the reader did is left in it.
///
/// **The tail of a flick is not the reader.** A trackpad's momentum dies away rather than
/// stopping, and what it has left to give below a point and a half a frame — about twenty
/// points, at the rate momentum decays — is enough to lift a settled page off its place and
/// have it spring back, which is a page fidgeting after every turn and nothing the reader
/// asked for. Slower than any deliberate scroll, and well under a turn either way.
const GESTURE_TAIL: f32 = 1.5;

/// How many frames of nothing end a scroll.
///
/// **The gaps inside one are longer than a frame.** Six of them is a tenth of a second —
/// longer than any gap in a swipe that is still going, shorter than a reader would call a
/// pause — and until they are up the page stays where the reader put it rather than
/// springing back between one delivery and the next.
const GESTURE_GAP: u8 = 6;

/// The range `pan` may take on one axis for the page to fill the window, or the single
/// value that centres it when it is smaller than the window.
///
/// **A page view holds its page.** The bound here used to be the general one — keep fifty
/// points of the document on screen — which let a single page be dragged until almost all
/// of it was off the window, and made "past the edge" mean nothing in particular. A page
/// taller than the window scrolls within itself; one that fits does not move.
///
/// `lo <= hi` always: when the page fits, both are the value that centres it.
fn page_hold(page: (f32, f32), window: (f32, f32), zoom: f32, origin: f32) -> (f32, f32) {
    let covered = (page.1 - page.0) * zoom;
    let window_size = window.1 - window.0;
    if covered <= window_size {
        let centred = f32::midpoint(page.0, page.1)
            .mul_add(-zoom, f32::midpoint(window.0, window.1) - origin);
        return (centred, centred);
    }
    (page.1.mul_add(-zoom, window.1 - origin), page.0.mul_add(-zoom, window.0 - origin))
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
            display_mode: DisplayMode::SinglePage,
            active_page: 0,
            scroll_direction: ScrollDirection::Vertical,
            binding_direction: BindingDirection::LeftToRight,
            cover_page_alone: true,
            pulling: false,
            adrift: 0.0,
            homing: EASE_LET_GO,
            arriving: false,
            landing: 0.0,
            quiet: GESTURE_GAP,
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
        if !self.arranged_as_tiles {
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

    /// Whether this view is the one that answers `act`. See [`Act`].
    #[must_use]
    pub fn does(&self, act: Act) -> bool {
        match act {
            Act::SelectText | Act::DrawOnPage | Act::TurnPages => self.is_page_view(),
            Act::SelectPages | Act::ArrangePages | Act::OpenPage => !self.is_page_view(),
            Act::RotatePages | Act::DeletePages => true,
        }
    }

    /// Whether the view is showing pages rather than tiles. See [`Self::TILE_ZOOM`].
    ///
    /// **What is drawn, not what can be done.** The layout, the renderer and the status
    /// bar each read this to decide what to put on screen; anything deciding whether the
    /// reader may *do* something asks [`Self::does`], which names the thing being asked
    /// about rather than the surface it happens on.
    #[must_use]
    pub fn is_page_view(&self) -> bool {
        Self::page_zoom(self.zoom)
    }

    /// Whether `zoom` is a page-view zoom, for a caller that has one but not a view.
    ///
    /// **The one comparison against [`Self::TILE_ZOOM`] in the window.** A page number is
    /// drawn from a free function that is handed a zoom, and it read the boundary itself;
    /// four such readings is how the split came to mean slightly different things in
    /// different places (UI-14).
    #[must_use]
    pub fn page_zoom(zoom: f32) -> bool {
        zoom >= Self::TILE_ZOOM
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
    /// Only pages that are actually on screen are candidates: zooming in the page view must
    /// not anchor to a page that is not shown, and a spread anchors within its own pair.
    /// **In the tiles every page on screen is a candidate** — the mode says `SinglePage`
    /// there and means nothing by it, and a zoom anchored on the one active tile moved the
    /// grid out from under the reader's cursor.
    fn layout_under<'a>(
        &self,
        center_pos: egui::Pos2,
        current_origin: egui::Pos2,
        old_zoom: f32,
        layouts: &'a [PageLayout],
    ) -> Option<&'a PageLayout> {
        let (mut closest, mut min_dist_sq) = (None, f32::MAX);
        for layout in layouts {
            if self.is_page_view() && !self.shows(layout.index, layouts.len()) {
                continue;
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
    fn nothing_in_flight(&mut self) {
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
    fn centre_on(&mut self, page: usize, viewport: egui::Rect, layouts: &[PageLayout]) -> bool {
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
    fn shows(&self, index: usize, total: usize) -> bool {
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

    /// Pans by the wheel, and says whether it moved anything.
    fn handle_scroll_panning(&mut self, ui: &egui::Ui) -> egui::Vec2 {
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
    fn gesture(&mut self, held: bool, moved: f32) {
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
    /// is the shape [`CODING.md`'s Rule D](../../../CODING.md) names for frontends, arrived
    /// at inside one.
    fn page_forward(&mut self, layouts: &[PageLayout]) -> bool {
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
    fn page_back(&mut self, layouts: &[PageLayout]) -> bool {
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
    fn step_to(&mut self, target: Option<usize>, layouts: &[PageLayout]) -> bool {
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
    fn shown_extent(&self, layouts: &[PageLayout]) -> ((f32, f32), (f32, f32)) {
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
    fn ease_home(&mut self, want: f32) -> f32 {
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
    fn land(&mut self, viewport: egui::Rect, onwards: bool, at: Edge, layouts: &[PageLayout]) {
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
    fn glide(&mut self, viewport: egui::Rect, across: (f32, f32), up: (f32, f32)) {
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
    fn hold_the_page(
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

    /// And the page view it is laid out as above it: every page on the origin, one drawn.
    ///
    /// **They are all the same rectangle**, which is what `compute_layouts` writes now
    /// that there is no column. Which one is on screen is `active_page`'s business.
    fn column(pages: usize) -> Vec<PageLayout> {
        (0..pages)
            .map(|i| PageLayout {
                index: i,
                rect: egui::Rect::from_min_size(egui::pos2(-306.0, 0.0), egui::vec2(612.0, 792.0)),
            })
            .collect()
    }

    /// Where `at` sits on the page being shown, in page units.
    ///
    /// Not `layout_under`: the page view stacks every page on one rectangle, so what is
    /// under a point is all of them and the one that matters is the one being drawn.
    fn on_the_page(view: &PDFView, at: egui::Pos2, layouts: &[PageLayout]) -> egui::Vec2 {
        let layout = &layouts[view.active_page];
        let page_min = view.get_origin(WINDOW) + layout.rect.min.to_vec2() * view.zoom();
        (at - page_min) / view.zoom()
    }

    /// **The point under the cursor does not move.** One sentence for every zoom,
    /// including the one that changes the arrangement — which is what a reader expects of
    /// a zoom and took five attempts to arrive at. The column and the grid put the same
    /// page in quite different places, so the view moves by whatever difference that
    /// makes.
    #[test]
    fn the_point_under_the_cursor_does_not_move() {
        let mut view = PDFView::new();
        let tiles = grid(25);
        view.set_zoom(0.2);
        view.restore_anchor(None, WINDOW, &tiles);

        let cursor = egui::pos2(700.0, 300.0);
        view.zoom_at(0.33, cursor, WINDOW, &tiles);
        assert!(view.arrangement_is_changing(), "the crossing went unnoticed");

        // **What the anchor holds, not what a hit-test guesses.** `take_anchor` clamps the
        // point onto the page it names, so a cursor over a gap between tiles anchors to
        // the nearest page's edge — and asserting against an unclamped hit-test made the
        // test depend on the cursor happening to be over a tile rather than on the rule.
        let carried = view.take_anchor(WINDOW, &tiles).expect("a page under the cursor");
        let (page, local) = (carried.page, carried.local);

        let pages = column(25);
        view.restore_anchor(Some(carried), WINDOW, &pages);

        assert_eq!(view.active_page, page, "a different page was carried across");
        let after = on_the_page(&view, cursor, &pages);
        assert!(
            (after - local).length() < 1.0,
            "the cursor came out at {after:?} of the page, not {local:?}"
        );
    }

    /// **The tiles are the tiles whatever the mode is.**
    ///
    /// The grid used to be laid out inside `Continuous`'s branch, so zooming out of the
    /// single-page or the spread view left every page stacked at the origin with one of
    /// them drawn — a chooser showing one thing to choose from. `is_page_view` is the zoom
    /// and nothing else, so the zoom decides the arrangement and the mode decides what the
    /// page view will be when the zoom comes back.
    ///
    /// The count is compared against `Continuous` rather than against 25: the viewport
    /// culls what is off-screen, so the number is the fixture's business and the claim
    /// here is only that the mode does not change it.
    #[test]
    fn the_mode_does_not_change_which_tiles_are_shown() {
        let tiles = grid(25);
        let shown = |mode| {
            let mut view = PDFView::new();
            view.display_mode = mode;
            view.set_zoom(0.2);
            assert!(!view.is_page_view(), "0.2 is not a tile zoom");
            view.restore_anchor(None, WINDOW, &tiles);
            view.visible_page_rects(WINDOW, &tiles)
                .into_iter()
                .map(|(layout, _)| layout.index)
                .collect::<Vec<_>>()
        };
        let all = shown(DisplayMode::SinglePage);
        assert!(all.len() > 1, "the fixture shows one tile, so this proves nothing");
        assert_eq!(shown(DisplayMode::TwoPageSingle), all, "the spread shows other tiles");
    }

    /// **A double-click on a tile opens that page, in the middle of the window**, by the
    /// same route as the one on the bench. It used to rebuild the layout in the caller and
    /// scroll into it — the one way across the tile boundary that did not go through the
    /// anchor, and so the one whose landing had to be reasoned about separately.
    #[test]
    fn a_double_click_on_a_tile_opens_that_page_in_the_middle() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
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
        view.display_mode = DisplayMode::SinglePage;
        let pages = column(25);
        view.set_zoom(1.0);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(10, &pages);

        // The gesture: the page being read is remembered, then the zoom crosses.
        view.centre_next = Some(view.current_page());
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

    /// **Going to a page does not re-frame the window — in the tiles.**
    ///
    /// A reader a third of the way down a tile presses *next* and the grid moves by the
    /// pitch between the two, leaving their eye where it was. This was the column's rule
    /// and the column is gone; the page view stacks its pages on the origin and centres
    /// the one it is going to, because there is nowhere else to put it.
    #[test]
    fn going_to_a_page_in_the_tiles_leaves_the_view_where_it_was_over_it() {
        let mut view = PDFView::new();
        let tiles = grid(25);
        view.set_zoom(0.2);
        view.restore_anchor(None, WINDOW, &tiles);
        view.scroll_to_page(8, &tiles);

        // A little way down tile 8, near enough that it is still the tile in hand.
        let into = 40.0_f32;
        view.pan.y = into.mul_add(-view.zoom(), view.pan.y);
        let before = (tiles[8].rect.min.y + into).mul_add(view.zoom(), view.get_origin(WINDOW).y);

        view.scroll_to_page(18, &tiles);
        let after = (tiles[18].rect.min.y + into).mul_add(view.zoom(), view.get_origin(WINDOW).y);
        assert!(
            (after - before).abs() < 0.01,
            "the same point of the next tile came out at {after}, not {before}"
        );
    }

    /// **In the page view, going to a page puts it in the middle.**
    ///
    /// There is nowhere else for it: every page is on the origin, so "leave the view where
    /// it was" and "put the page in front of the reader" are the same placement.
    #[test]
    fn going_to_a_page_in_the_page_view_centres_it() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        let pages = column(25);
        view.set_zoom(0.5);
        view.restore_anchor(None, WINDOW, &pages);
        // Somewhere other than the middle, so that arriving at it means something.
        view.pan.y += 200.0;

        view.scroll_to_page(11, &pages);
        assert_eq!(view.active_page, 11);
        let middle = view.get_origin(WINDOW) + pages[11].rect.center().to_vec2() * view.zoom();
        assert!(
            (middle - WINDOW.center()).length() < 1.0,
            "page 11 came out centred on {middle:?}, not {:?}",
            WINDOW.center()
        );
    }

    /// **The same button works in both arrangements, and has something to do in each.**
    /// The grid hangs from its binding edge and the reader scrolls away from the page
    /// they were on; the column leaves it wherever the scroll left it.
    #[test]
    fn centring_brings_the_current_page_to_the_middle_of_the_window() {
        for (zoom, layouts) in [(0.2_f32, grid(25)), (1.0, column(25))] {
            let mut view = PDFView::new();
            view.display_mode = DisplayMode::SinglePage;
            view.set_zoom(zoom);
            view.restore_anchor(None, WINDOW, &layouts);
            view.active_page = 12;
            view.pan = egui::vec2(-300.0, -4000.0); // scrolled somewhere else entirely

            let page = view.current_page();
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
        view.display_mode = DisplayMode::SinglePage;
        let pages = column(25);
        view.set_zoom(1.0);
        view.restore_anchor(None, WINDOW, &pages);
        view.scroll_to_page(8, &pages);
        assert_eq!(view.current_page(), 8, "the page view reads its middle");

        // In the tiles, scrolling past a page is not being on it.
        let tiles = grid(25);
        view.set_zoom(0.2);
        view.restore_anchor(None, WINDOW, &tiles);
        view.active_page = 8;
        view.pan = egui::vec2(0.0, -2000.0);
        assert_eq!(view.current_page(), 8, "the grid reads where the reader is");
    }

    /// A zoom that stays on one side of the boundary rearranges nothing, so nothing is
    /// carried and the view is not touched: the reader is zooming, not navigating.
    #[test]
    fn a_zoom_that_crosses_nothing_rearranges_nothing() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
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
    use super::{Act, PDFView};

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
                view.does(Act::SelectPages),
                view.does(Act::SelectText),
                "at {zoom} pages={} text={}",
                view.does(Act::SelectPages),
                view.does(Act::SelectText)
            );
        }
    }

    /// **Every act belongs to the page view, to the tiles, or to both — and to the same
    /// one wherever it is asked about.** The split used to be spelled four ways at the
    /// call sites and declared nowhere; this is the table those call sites now read.
    #[test]
    fn each_act_has_the_view_it_belongs_to() {
        let mut tiles = PDFView::new();
        tiles.set_zoom(PDFView::TILE_STEP);
        let mut pages = PDFView::new();
        pages.set_zoom(1.0);

        for act in [Act::SelectText, Act::DrawOnPage, Act::TurnPages] {
            assert!(pages.does(act) && !tiles.does(act), "{act:?} is the page view's");
        }
        for act in [Act::SelectPages, Act::ArrangePages, Act::OpenPage] {
            assert!(tiles.does(act) && !pages.does(act), "{act:?} is the tiles'");
        }
        for act in [Act::RotatePages, Act::DeletePages] {
            assert!(
                pages.does(act) && tiles.does(act),
                "{act:?} is answered wherever the page it acts on is being looked at"
            );
        }
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

    /// Runs the frames a page needs to reach its place, with nobody touching the view.
    fn settle(view: &mut PDFView, window: egui::Rect, layouts: &[PageLayout]) {
        for _ in 0..60 {
            view.gesture(false, 0.0);
            view.clamp_pan(window, layouts);
        }
    }

    /// Pulls the view `by` past where the page sits, and says where it landed.
    ///
    /// **From rest, not from the origin.** These used to set `pan` outright, which meant
    /// something when the bound was the general "keep fifty points on screen" one; against
    /// a page held on its window it means whatever the page's layout happens to make it.
    /// Settling first and pulling from there is the gesture being described.
    fn pull_from_page_one(mode: DisplayMode, dir: ScrollDirection, by: egui::Vec2) -> usize {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = mode;
        view.scroll_direction = dir;
        view.active_page = 1;
        // Settle where the page sits, then pull from there with a drag in hand — which is
        // the only thing that turns a page.
        view.clamp_pan(window, &layouts);
        view.pulling = true;
        view.pan += by;
        view.clamp_pan(window, &layouts);
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
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        view.clamp_pan(window, &pages());
        view.pulling = true;
        view.pan += egui::vec2(400.0, 0.0);
        view.clamp_pan(window, &pages());
        assert_eq!(view.active_page, 2, "pulled past the left, bound right-to-left");
    }

    /// **A page that fills the window still answers the wheel**, by turning.
    ///
    /// This is the commonest page there is — one that fits — and there is nothing on it to
    /// scroll to, so a wheel that only scrolled did nothing at all. The pull was a drag
    /// and the wheel is not one, which is how "scrolling stopped working" came to be a
    /// true report of a page view that looked finished.
    #[test]
    fn the_wheel_turns_a_page_that_has_nowhere_to_scroll() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 1;
        view.clamp_pan(window, &layouts);
        let (lo, hi) = super::page_hold(
            (layouts[1].rect.min.y, layouts[1].rect.max.y),
            (window.min.y, window.max.y),
            view.zoom(),
            view.get_origin_no_pan(window).y,
        );
        assert!((lo - hi).abs() < 1e-6, "the fixture's page does not fit, so it would scroll");

        // What the wheel does: move the pan, and say it moved it.
        view.pulling = true;
        view.pan.y -= super::PAGE_TURN_PULL + 1.0;
        view.clamp_pan(window, &layouts);
        assert_eq!(view.active_page, 2, "the wheel did nothing on a page that fits");
    }

    /// **A page larger than the window still scrolls, on both axes.**
    #[test]
    fn a_page_larger_than_the_window_scrolls_within_itself() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(200.0, 200.0));
        // One page, 100 by 100 in layout units, at four times the size of the window.
        let layouts = vec![PageLayout {
            index: 0,
            rect: egui::Rect::from_min_max(egui::pos2(-50.0, 0.0), egui::pos2(50.0, 100.0)),
        }];
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.set_zoom(4.0);
        view.clamp_pan(window, &layouts);
        let rest = view.pan;

        for step in [egui::vec2(0.0, -30.0), egui::vec2(-30.0, 0.0)] {
            let mut scrolled = PDFView::new();
            scrolled.display_mode = DisplayMode::SinglePage;
            scrolled.set_zoom(4.0);
            scrolled.pan = rest + step;
            scrolled.clamp_pan(window, &layouts);
            assert!(
                (scrolled.pan - (rest + step)).length() < 0.01,
                "scrolling by {step:?} was undone: {:?} instead of {:?}",
                scrolled.pan,
                rest + step
            );
        }
    }

    /// **The page moves while it is being pulled, and settles back when it is let go.**
    ///
    /// This is what the pull is for: the reader sees the gap opening between the page's
    /// edge and the window's and knows how far there is to go. The old rule held `pan` at
    /// its bound on every frame and summed the drag into an accumulator nobody could see,
    /// so the page did not move and then it changed.
    #[test]
    fn a_page_being_pulled_comes_away_from_the_edge_and_goes_back() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 1;
        view.clamp_pan(window, &layouts);
        let resting = view.pan.y;

        let short_of_a_turn = super::PAGE_TURN_PULL * 0.6;
        view.pulling = true;
        view.pan.y -= short_of_a_turn;
        view.clamp_pan(window, &layouts);
        assert_eq!(view.active_page, 1, "the pull stopped short of what a turn asks for");
        assert!(
            (resting - view.pan.y - short_of_a_turn).abs() < 0.01,
            "the page did not come away: it sits at {} and rested at {resting}",
            view.pan.y
        );

        // Let go. It comes back over several frames — a single frame of it is a jump,
        // which is what the page turning used to look like.
        view.pulling = false;
        view.clamp_pan(window, &layouts);
        assert!(
            (view.pan.y - resting).abs() > 0.01,
            "it went back in one frame, which is the jerk this eases"
        );
        for _ in 0..60 {
            view.clamp_pan(window, &layouts);
        }
        assert!((view.pan.y - resting).abs() < 0.01, "it stayed pulled after being let go");
    }

    /// **A page too big to fit shows its head when it is turned to, not its middle.**
    ///
    /// Landing in the middle of such a page hides the lines it starts with, and the pull
    /// that reaches its foot from there is short enough that a reader turning pages sees
    /// only their bottom halves — which is the page turn reading as jerky.
    #[test]
    fn a_page_that_does_not_fit_is_turned_to_at_its_head() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(200.0, 200.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.set_zoom(4.0); // a 100-unit page is 400 points tall against a 200-point window
        view.active_page = 1;
        view.clamp_pan(window, &layouts);

        // Forward: the top edge of page 2 arrives on the top of the window.
        view.pulling = true;
        view.pan.y -= 1000.0;
        view.clamp_pan(window, &layouts);
        assert_eq!(view.active_page, 2, "the pull did not turn the page");
        settle(&mut view, window, &layouts);
        let head = layouts[2].rect.min.y.mul_add(view.zoom(), view.get_origin(window).y);
        assert!(
            (head - window.min.y).abs() < 0.01,
            "page 2 came in with its top at {head}, not at the window's {}",
            window.min.y
        );

        // Back: the bottom edge of page 1 arrives on the bottom of the window, because
        // that is where a reader going back was last looking.
        view.gesture(true, 1000.0);
        view.pan.y += 1000.0;
        view.clamp_pan(window, &layouts);
        assert_eq!(view.active_page, 1, "the pull did not turn back");
        settle(&mut view, window, &layouts);
        let foot = layouts[1].rect.max.y.mul_add(view.zoom(), view.get_origin(window).y);
        assert!(
            (foot - window.max.y).abs() < 0.01,
            "page 1 came back with its bottom at {foot}, not at the window's {}",
            window.max.y
        );
    }

    /// **The page turned to slides in from the side it comes from.** Arriving where it
    /// belongs between one frame and the next is a snap; this is the same landing, reached
    /// over a fifth of a second, and it starts a window away so that what a reader sees is
    /// a page coming in rather than a page appearing.
    #[test]
    fn the_page_turned_to_comes_in_from_its_own_side() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 1;
        view.clamp_pan(window, &layouts);

        // Forward: the next page is below, so it starts below and travels up.
        view.pulling = true;
        view.pan.y -= 1000.0;
        view.clamp_pan(window, &layouts);
        assert_eq!(view.active_page, 2, "the pull did not turn the page");
        let arriving = view.pan.y;

        // It closes the distance every frame, and every frame of it is on screen.
        let mut travelled = 0;
        for _ in 0..60 {
            let before = view.pan.y;
            view.gesture(false, 0.0);
            view.clamp_pan(window, &layouts);
            if (view.pan.y - before).abs() > 0.01 {
                travelled += 1;
            }
        }
        let home = view.pan.y;
        assert!(
            arriving - home >= window.height() - 0.01,
            "page 2 came in from {arriving} to {home}, less than the window it starts beyond"
        );
        assert!(
            (10..=45).contains(&travelled),
            "it came in over {travelled} frames, which is a snap at one end or a crawl at \
             the other"
        );

        // And where it stops is where the page belongs — the same place a page that fits
        // sits when nothing has been touched.
        let mut untouched = PDFView::new();
        untouched.display_mode = DisplayMode::SinglePage;
        untouched.active_page = 2;
        untouched.clamp_pan(window, &layouts);
        assert!(
            (home - untouched.pan.y).abs() < 0.01,
            "it stopped at {home} rather than where page 2 belongs, {}",
            untouched.pan.y
        );
    }

    /// **Nothing turns while the page turned to is still on its way in.** A held drag
    /// never stops sending and the wheel keeps delivering for most of a second, so without
    /// this the same gesture re-earned the pull on the frame after a turn and paged through
    /// the document several pages at a time.
    #[test]
    fn a_page_still_arriving_cannot_be_turned_past() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 0;
        view.clamp_pan(window, &layouts);

        // A drag that keeps going, hard, for as long as the page takes to come in.
        let mut frames = 0;
        loop {
            view.gesture(true, 100.0);
            view.pan.y -= 100.0;
            view.clamp_pan(window, &layouts);
            frames += 1;
            assert_eq!(view.active_page, 1, "it turned twice in {frames} frames");
            if !view.arriving {
                break;
            }
            assert!(frames < 60, "the page never finished arriving");
        }
        assert!(frames > 5, "it was home in {frames} frames, which is not an arrival");

        // Home, and the same drag turns the next one.
        view.gesture(true, 100.0);
        view.pan.y -= 100.0;
        view.clamp_pan(window, &layouts);
        assert_eq!(view.active_page, 2, "the drag could not turn the page it had come to");
    }

    /// A pull is capped at the distance that turns the page, so it cannot be dragged into
    /// open space.
    #[test]
    fn a_pull_goes_no_further_than_the_turn() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 0;
        view.clamp_pan(window, &layouts);
        let resting = view.pan.y;

        // Backwards off page 0, where there is no previous page to turn to.
        view.pulling = true;
        view.pan.y += 400.0;
        view.clamp_pan(window, &layouts);
        assert_eq!(view.active_page, 0, "there is no page before the first");
        assert!(
            (view.pan.y - resting) <= super::PAGE_TURN_PULL + 0.01,
            "it was pulled {} past its rest",
            view.pan.y - resting
        );
    }

    /// **A flick turns one page and stays there.** The page that was still coming in used
    /// to be pulled back out by the same flick's tail, and the distance it had left to
    /// travel read as a pull the other way: the view turned back to the page it had just
    /// left, slid in from that side, and did it again — a page flapping between two.
    ///
    /// A weak flick is the one that showed it, because its tail dies away while the page
    /// is still arriving.
    #[test]
    fn a_flick_does_not_flap_between_two_pages() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 1;
        view.clamp_pan(window, &layouts);

        // A trackpad's flick: a few frames of the fingers moving, then a tail that dies
        // away rather than stopping.
        let mut delta = 10.0_f32;
        let mut visited = vec![view.active_page];
        for _ in 0..120 {
            view.gesture(false, delta);
            view.pan.y -= delta;
            view.clamp_pan(window, &layouts);
            if view.active_page != *visited.last().unwrap_or(&0) {
                visited.push(view.active_page);
            }
            delta *= 0.9;
        }
        assert_eq!(visited, vec![1, 2], "one flick, and it went {visited:?}");
    }

    /// **A reader scrolling steadily keeps turning pages**, one for each that finishes
    /// arriving. Holding the turn until the gesture itself ended froze the view instead:
    /// a trackpad under a moving finger never stops sending, so one page turned and then
    /// nothing happened at all until they took their hand off.
    #[test]
    fn a_steady_scroll_keeps_turning_pages() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 0;
        view.clamp_pan(window, &layouts);

        for _ in 0..90 {
            view.gesture(false, 40.0);
            view.pan.y -= 40.0;
            view.clamp_pan(window, &layouts);
        }
        assert_eq!(view.active_page, 3, "a steady scroll stopped paging: {}", view.active_page);
    }

    /// **A page sent for does not arrive carrying the last one's journey.** The offset a
    /// page is coming in with is measured from where *that* page belonged: a reader who
    /// presses *next page* while one is still sliding in used to get the page they asked
    /// for displaced by most of a window, easing in from nowhere in particular.
    #[test]
    fn a_page_gone_to_while_another_arrives_is_where_it_belongs() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 1;
        view.clamp_pan(window, &layouts);

        view.pulling = true;
        view.pan.y -= 1000.0;
        view.clamp_pan(window, &layouts);
        assert!(view.arriving, "the turn did not start a page on its way in");

        // The reader presses *next page* while it is still travelling.
        view.scroll_to_page(3, &layouts);
        view.gesture(false, 0.0);
        view.clamp_pan(window, &layouts);

        let mut untouched = PDFView::new();
        untouched.display_mode = DisplayMode::SinglePage;
        untouched.active_page = 3;
        untouched.clamp_pan(window, &layouts);
        assert!(
            (view.pan.y - untouched.pan.y).abs() < 0.01,
            "page 3 came in at {} rather than where it belongs, {}",
            view.pan.y,
            untouched.pan.y
        );
    }

    /// **A page the layout no longer has does not send the view nowhere.** `active_page`
    /// outlives the pages for a frame whenever a document loses some — pages extracted out
    /// of it, pages deleted — and an extent taken over nothing is `f32::MAX`..`f32::MIN`,
    /// which reaches the hold as a page of negative size.
    #[test]
    fn a_view_on_a_page_that_is_gone_still_lands_on_the_document() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        for mode in [DisplayMode::SinglePage, DisplayMode::TwoPageSingle] {
            let mut view = PDFView::new();
            view.display_mode = mode;
            view.active_page = layouts.len() + 7;
            view.clamp_pan(window, &layouts);
            assert!(view.pan.is_finite(), "{mode:?} put the view at {:?}", view.pan);

            let across = layouts.iter().fold(f32::MIN, |far, l| far.max(l.rect.max.x));
            let up = layouts.iter().fold(f32::MIN, |far, l| far.max(l.rect.max.y));
            let origin = view.get_origin(window);
            assert!(
                origin.x <= across.mul_add(view.zoom(), window.max.x)
                    && origin.y <= up.mul_add(view.zoom(), window.max.y),
                "{mode:?} left the document off the window at {origin:?}"
            );
        }
    }

    /// A slow swipe, as egui hands one over: a few points, a gap, a few more.
    fn slow_swipe() -> impl Iterator<Item = f32> {
        [3.0, 0.0, 2.5, 0.0, 0.0, 3.0, 1.0, 0.0, 2.0, 0.0, 3.0, 0.0, 0.0, 2.5, 0.0]
            .into_iter()
            .cycle()
    }

    /// **A slow swipe moves the page one way, and turns it.** Every gap in it used to read
    /// as the reader letting go: the page sprang nearly half the way back to its place and
    /// was hauled out again on the next delivery, which is a page shaking in place — and
    /// shaking was all it could do, because each gap ate what the swipe had gained, so no
    /// amount of slow swiping ever reached the distance that turns a page.
    #[test]
    fn a_slow_swipe_moves_the_page_one_way_and_turns_it() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 1;
        view.clamp_pan(window, &layouts);
        let resting = view.pan.y;

        let mut previous = view.pan.y;
        for (frame, delta) in slow_swipe().take(40).enumerate() {
            view.gesture(false, delta);
            view.pan.y -= delta;
            view.clamp_pan(window, &layouts);
            if view.active_page != 1 {
                assert!(frame > 10, "it turned on frame {frame}, faster than the swipe moved");
                return;
            }
            assert!(
                view.pan.y <= previous + 0.01,
                "on frame {frame} the page went back up, from {previous} to {}",
                view.pan.y
            );
            previous = view.pan.y;
        }
        panic!("forty frames of swiping turned nothing; it got {} from {resting}", view.pan.y);
    }

    /// **A swipe that stops lets the page home.** The gaps inside a swipe are held through,
    /// so the end of one has to be told apart from them: enough frames of nothing in a row.
    #[test]
    fn a_swipe_that_stops_lets_the_page_go_home() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 400.0));
        let layouts = pages();
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        view.active_page = 1;
        view.clamp_pan(window, &layouts);
        let resting = view.pan.y;

        for delta in slow_swipe().take(12) {
            view.gesture(false, delta);
            view.pan.y -= delta;
            view.clamp_pan(window, &layouts);
        }
        assert!((view.pan.y - resting).abs() > 5.0, "the swipe moved nothing to let go of");

        for _ in 0..40 {
            view.gesture(false, 0.0);
            view.clamp_pan(window, &layouts);
        }
        assert!(
            (view.pan.y - resting).abs() < 0.01,
            "it stayed at {} after the swipe stopped, rather than {resting}",
            view.pan.y
        );
    }

    /// **A page asked for by name comes in like a page turned to, and stops at its head.**
    /// The four page buttons, the bookmarks and the arrow keys placed it and nothing else,
    /// so half the ways to reach a page slid it in and half swapped it for the last one —
    /// and the swap kept the pan of the page it replaced, which at any zoom where a page
    /// does not fit put the reader half way down a page they had not started.
    #[test]
    fn a_page_asked_for_by_name_is_turned_to() {
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(200.0, 200.0));
        let layouts = pages();
        for (from, to) in [(1_usize, 3_usize), (3, 1)] {
            let mut view = PDFView::new();
            view.display_mode = DisplayMode::SinglePage;
            view.set_zoom(4.0); // a 100-unit page is 400 points tall in a 200-point window
            view.active_page = from;
            view.clamp_pan(window, &layouts);

            view.turn_to(to, window, &layouts);
            assert_eq!(view.active_page, to);
            assert!(view.arriving, "page {to} was placed rather than turned to");

            settle(&mut view, window, &layouts);
            let head = layouts[to].rect.min.y.mul_add(view.zoom(), view.get_origin(window).y);
            assert!(
                (head - window.min.y).abs() < 0.01,
                "going {from} to {to} left its top at {head}, not at the window's {}",
                window.min.y
            );
        }
    }

    /// A pull that stops short of the threshold pages nothing.
    #[test]
    fn a_pull_under_the_threshold_stays_put() {
        assert_eq!(
            pull_from_page_one(
                DisplayMode::SinglePage,
                ScrollDirection::Vertical,
                egui::vec2(0.0, super::PAGE_TURN_PULL * 0.6)
            ),
            1,
            "a pull of three fifths of the threshold turned the page"
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

    /// **In the tiles every page on screen is shown, whatever the mode says.** The mode is
    /// `SinglePage` there and means nothing by it — the grid belongs to the zoom — and this
    /// one answer is what the drawing, the renderer's work list and the click targets all
    /// read. Three copies of it had drifted: the tiles were drawn with no pixels asked for,
    /// and a click or a right-click on any tile but one reached nothing at all.
    #[test]
    fn the_tiles_show_every_page_on_screen() {
        let layouts = pages();
        let window = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 2000.0));
        for mode in [DisplayMode::SinglePage, DisplayMode::TwoPageSingle] {
            let mut view = PDFView::new();
            view.display_mode = mode;
            view.active_page = 2;
            view.set_zoom(PDFView::TILE_STEP);
            let shown: Vec<usize> = view
                .visible_page_rects(window, &layouts)
                .into_iter()
                .map(|(layout, _)| layout.index)
                .collect();
            assert_eq!(shown, (0..layouts.len()).collect::<Vec<usize>>(), "in {mode:?}");
        }
    }

    /// A page the viewport does not reach is not drawn.
    ///
    /// **Asked of the tiles**, which are what shows more than one page now — and where it
    /// earns its keep: `intel_sdm.pdf` is 5,057 of them. It used to be asked of the
    /// continuous mode, in a column laid out the same way.
    #[test]
    fn a_page_off_the_viewport_is_not_shown() {
        let mut view = PDFView::new();
        view.display_mode = DisplayMode::SinglePage;
        let layouts = pages();
        // A page view at 1.0 draws one page, so this is the tile zoom — the arrangement
        // that draws many and therefore the one with something to leave out.
        view.set_zoom(0.2);
        let tall = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 2000.0));
        let short = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(400.0, 40.0));
        let shown = |viewport| {
            view.visible_page_rects(viewport, &layouts)
                .into_iter()
                .map(|(layout, _)| layout.index)
                .collect::<Vec<_>>()
        };
        let all = shown(tall);
        assert_eq!(all.len(), 6, "the fixture does not fit in the tall viewport: {all:?}");
        let some = shown(short);
        assert!(some.len() < all.len(), "a 40-tall viewport reached all six: {some:?}");
        assert!(!some.is_empty(), "it reached none of them, so this proves nothing");
    }
}

#[cfg(test)]
mod page_hold_tests {
    use super::{PAGE_TURN_PULL, page_hold};

    /// A window 800 tall, and a page laid from 0 to 1000 in layout units.
    const WINDOW: (f32, f32) = (0.0, 800.0);
    const PAGE: (f32, f32) = (0.0, 1000.0);

    /// **A page taller than the window scrolls within itself, and no further.**
    ///
    /// At either end of the range the page's edge is exactly on the window's: there is no
    /// position inside the bound that shows anything but page.
    #[test]
    fn a_tall_page_scrolls_from_one_edge_to_the_other() {
        let (lo, hi) = page_hold(PAGE, WINDOW, 1.0, 0.0);
        assert!(lo < hi, "a 1000-tall page in an 800-tall window cannot move: {lo}..{hi}");
        // At `hi` the top of the page is on the top of the window.
        assert!((PAGE.0.mul_add(1.0, hi) - WINDOW.0).abs() < 1e-3, "top: {hi}");
        // At `lo` the bottom of the page is on the bottom of the window.
        assert!((PAGE.1.mul_add(1.0, lo) - WINDOW.1).abs() < 1e-3, "bottom: {lo}");
        assert!((hi - lo - 200.0).abs() < 1e-3, "the slack is the 200 it overhangs by");
    }

    /// **A page that fits does not move at all**, and the one position it has centres it.
    #[test]
    fn a_page_that_fits_is_pinned_in_the_middle() {
        let (lo, hi) = page_hold(PAGE, WINDOW, 0.5, 0.0);
        assert!((lo - hi).abs() < 1e-6, "a 500-tall page in an 800-tall window slid: {lo}..{hi}");
        let top = PAGE.0.mul_add(0.5, lo);
        let bottom = PAGE.1.mul_add(0.5, lo);
        assert!((top - WINDOW.0 - (WINDOW.1 - bottom)).abs() < 1e-3, "uneven: {top} and {bottom}");
    }

    /// The zoom decides which of the two it is, and the crossing is where they meet.
    #[test]
    fn the_two_answers_meet_where_the_page_exactly_fills_the_window() {
        let exact = 800.0 / 1000.0;
        let (lo, hi) = page_hold(PAGE, WINDOW, exact, 0.0);
        assert!((lo - hi).abs() < 1e-3, "exactly filling is not one position: {lo}..{hi}");
        let (lo2, hi2) = page_hold(PAGE, WINDOW, exact + 0.01, 0.0);
        assert!(hi2 > lo2, "a hair taller does not scroll");
        assert!((hi2 - lo2) < PAGE_TURN_PULL, "and barely, which is what a hair means");
    }

    /// The origin is subtracted, so a window whose origin is not zero holds the same way.
    #[test]
    fn the_origin_shifts_the_range_and_nothing_else() {
        let (lo, hi) = page_hold(PAGE, WINDOW, 1.0, 0.0);
        let (lo2, hi2) = page_hold(PAGE, WINDOW, 1.0, 150.0);
        assert!((lo - lo2 - 150.0).abs() < 1e-3 && (hi - hi2 - 150.0).abs() < 1e-3);
    }
}
