//! Dragging a rectangle on the page and copying what is inside it.
//!
//! **The thing Acrobat calls スナップショット.** A reader drags, and what was inside the
//! rectangle lands on the clipboard as a picture.
//!
//! The drag is the one the redaction brush makes and the arithmetic is
//! `SelectionManager`'s; what is new here is only what happens when it stops.

use crate::interaction::SelectionManager;

/// The resolutions a snapshot can be asked for, as multiples of 96 DPI.
///
/// **A snapshot taken at whatever the screen happens to be showing is one nobody can ask
/// for twice**, so the resolution is a choice rather than the zoom. The three are what a
/// reader means by "as it looks", "for print" and "to read the small type".
pub const RESOLUTIONS: [(&str, f64); 3] =
    [("snapshot_96", 4.0 / 3.0), ("snapshot_192", 8.0 / 3.0), ("snapshot_384", 16.0 / 3.0)];

/// What the snapshot tool is holding between frames.
#[derive(Default)]
pub struct SnapshotTool {
    /// Whether the tool is on.
    pub is_active: bool,
    /// Where the drag began, in PDF user space.
    pub drag_start: Option<egui::Pos2>,
    /// Where it is now, in the same space.
    pub drag_current: Option<egui::Pos2>,
    /// Which multiple of 96 DPI to take it at, as an index into [`RESOLUTIONS`].
    pub resolution: usize,
}

/// A rectangle a reader has finished dragging, on the page it was dragged on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Taken {
    /// Which page.
    pub page: usize,
    /// What to copy, in the page's own space: left, bottom, right, top.
    pub keep: (f64, f64, f64, f64),
    /// What to render it at.
    pub scale: f64,
}

impl SnapshotTool {
    /// Follows the drag, and answers the rectangle when the reader lets go.
    ///
    /// **A drag that went nowhere is not a snapshot.** A click on the page with the tool
    /// on would otherwise copy a rectangle of no size, which is a picture of nothing put
    /// on the clipboard over whatever was there.
    pub fn interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        zoom: f32,
    ) -> Option<Taken> {
        if !self.is_active {
            return None;
        }
        let response = ui.allocate_rect(page_rect, egui::Sense::drag());
        let at = |pos| SelectionManager::screen_to_pdf(page_rect, zoom, page_unscaled_h, pos);
        let pointer = ui.input(|i| i.pointer.hover_pos());

        if response.drag_started()
            && let Some(pos) = pointer
        {
            self.drag_start = Some(at(pos));
        }
        if response.dragged()
            && let Some(pos) = pointer
        {
            self.drag_current = Some(at(pos));
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if !response.drag_stopped() {
            return None;
        }
        let taken = self.taken(page_index);
        self.drag_start = None;
        self.drag_current = None;
        taken
    }

    /// The rectangle the drag covers, when it covers one.
    fn taken(&self, page: usize) -> Option<Taken> {
        let (start, current) = (self.drag_start?, self.drag_current?);
        let rect = egui::Rect::from_two_pos(start, current);
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return None;
        }
        let (_, scale) = RESOLUTIONS.get(self.resolution).copied()?;
        Some(Taken {
            page,
            keep: (
                f64::from(rect.min.x),
                f64::from(rect.min.y),
                f64::from(rect.max.x),
                f64::from(rect.max.y),
            ),
            scale,
        })
    }

    /// The rectangle being dragged, on screen, for the page to draw.
    pub fn dragging(
        &self,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        zoom: f32,
    ) -> Option<egui::Rect> {
        let (start, current) = (self.drag_start?, self.drag_current?);
        let to = |pos| SelectionManager::pdf_to_screen(page_rect, zoom, page_unscaled_h, pos);
        Some(egui::Rect::from_two_pos(to(start), to(current)))
    }
}

/// The rectangle a drag covers, and what it is not.
#[cfg(test)]
mod taken {
    use super::{RESOLUTIONS, SnapshotTool};

    fn dragged(from: (f32, f32), to: (f32, f32)) -> SnapshotTool {
        SnapshotTool {
            is_active: true,
            drag_start: Some(egui::pos2(from.0, from.1)),
            drag_current: Some(egui::pos2(to.0, to.1)),
            resolution: 0,
        }
    }

    #[test]
    fn a_drag_covers_the_rectangle_between_its_ends() {
        let taken = dragged((100.0, 400.0), (300.0, 500.0)).taken(2).expect("it covers one");
        assert_eq!(taken.page, 2, "the snapshot names another page");
        assert_eq!(taken.keep, (100.0, 400.0, 300.0, 500.0), "the rectangle is not the drag");
    }

    /// **Dragged backwards is the same rectangle.** A reader who starts at the bottom
    /// right is dragging the same box as one who starts at the top left.
    #[test]
    fn a_drag_the_other_way_covers_the_same_rectangle() {
        let taken = dragged((300.0, 500.0), (100.0, 400.0)).taken(0).expect("it covers one");
        assert_eq!(taken.keep, (100.0, 400.0, 300.0, 500.0), "a backwards drag came out inverted");
    }

    /// A click is not a drag, and a picture of nothing should not land on the clipboard
    /// over whatever was there.
    #[test]
    fn a_drag_that_went_nowhere_takes_nothing() {
        assert!(
            dragged((100.0, 400.0), (100.2, 400.2)).taken(0).is_none(),
            "a fifth of a point was taken as a snapshot"
        );
    }

    /// The resolution is the one the reader chose, not the one the screen is at.
    #[test]
    fn the_resolution_is_the_one_that_was_chosen() {
        let mut tool = dragged((100.0, 400.0), (300.0, 500.0));
        for (index, (_, wanted)) in RESOLUTIONS.iter().enumerate() {
            tool.resolution = index;
            let taken = tool.taken(0).expect("it covers one");
            assert!(
                (taken.scale - wanted).abs() < 1e-9,
                "resolution {index} came out at {} rather than {wanted}",
                taken.scale
            );
        }
    }
}
