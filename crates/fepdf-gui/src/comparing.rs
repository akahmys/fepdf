//! The compare drawer: this document against another, page by page (ROADMAP W-18).
//!
//! **The comparison is the engine's and runs on the worker**, which holds this document;
//! the other is read from its file there. What comes back is listed here and outlined on
//! the pages, and a page in the list is a button that goes to it.

use fepdf::compare::Comparison;

/// What the drawer holds between frames.
#[derive(Default)]
pub struct CompareState {
    /// The other document's file name, once one is chosen.
    pub other: Option<String>,
    /// Whether the worker is comparing.
    pub waiting: bool,
    /// What it found.
    pub result: Option<Comparison>,
}

/// What the drawer asked for.
pub enum Asked {
    /// Nothing.
    Nothing,
    /// To compare with a file.
    With(std::path::PathBuf),
    /// To go to a page, counting from zero.
    GoTo(usize),
}

impl CompareState {
    /// The regions page `page` looks different in, in its points.
    pub fn regions_on(&self, page: usize) -> &[[f64; 4]] {
        self.result
            .as_ref()
            .and_then(|r| r.differences.iter().find(|d| d.page == page))
            .map_or(&[], |d| d.regions.as_slice())
    }
}

/// The drawer.
pub fn show(state: &CompareState, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) -> Asked {
    use crate::app::theme::space;
    let mut asked = Asked::Nothing;
    ui.label(tr("compare_how"));
    ui.add_space(space::ITEM);
    if ui.add_enabled(!state.waiting, egui::Button::new(tr("compare_choose"))).clicked()
        && let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
    {
        asked = Asked::With(path);
    }
    if let Some(name) = &state.other {
        ui.label(tr("compare_against").replacen("{}", name, 1));
    }
    let Some(result) = &state.result else { return asked };
    ui.add_space(space::ITEM);
    let pages = tr("compare_pages").replacen("{}", &result.pages.0.to_string(), 1).replacen(
        "{}",
        &result.pages.1.to_string(),
        1,
    );
    ui.label(pages);
    if result.differences.is_empty() {
        ui.label(tr("compare_same"));
        return asked;
    }
    ui.label(tr("compare_differ").replacen("{}", &result.differences.len().to_string(), 1));
    list(result, ui, tr).unwrap_or(asked)
}

/// Each page that differs: a link to it, and what differs on it.
fn list(result: &Comparison, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) -> Option<Asked> {
    let mut asked = None;
    for difference in &result.differences {
        ui.separator();
        let head = tr("compare_page").replacen("{}", &(difference.page + 1).to_string(), 1);
        if ui.link(head).clicked() {
            asked = Some(Asked::GoTo(difference.page));
        }
        if difference.only_in_one {
            ui.label(egui::RichText::new(tr("compare_only_in_one")).weak());
        }
        for line in &difference.removed {
            ui.label(
                egui::RichText::new(format!("− {line}"))
                    .color(crate::app::theme::colors::note::FAIL),
            );
        }
        for line in &difference.added {
            ui.label(
                egui::RichText::new(format!("+ {line}"))
                    .color(crate::app::theme::colors::note::PASS),
            );
        }
        if !difference.regions.is_empty() {
            let regions =
                tr("compare_regions").replacen("{}", &difference.regions.len().to_string(), 1);
            ui.label(egui::RichText::new(regions).weak());
        }
    }
    asked
}

/// Outlines the regions page `page` looks different in, over the page on screen.
pub fn outline(
    state: &CompareState,
    ui: &egui::Ui,
    page: usize,
    page_rect: egui::Rect,
    frame: crate::interaction::PageFrame,
    zoom: f32,
) {
    let regions = state.regions_on(page);
    if regions.is_empty() {
        return;
    }
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("compare_regions"));
    let painter = ui.ctx().layer_painter(layer).with_clip_rect(page_rect);
    let stroke = egui::Stroke::new(2.0_f32, crate::app::theme::colors::rust::ACCENT);
    let to = |x: f64, y: f64| {
        #[allow(clippy::cast_possible_truncation)] // a point on a page, drawn in f32
        let point = egui::pos2(x as f32, y as f32);
        crate::interaction::SelectionManager::pdf_to_screen(page_rect, zoom, frame, point)
    };
    for [x0, y0, x1, y1] in regions {
        let rect = egui::Rect::from_two_pos(to(*x0, *y0), to(*x1, *y1));
        painter.rect_stroke(
            rect,
            crate::app::theme::radius::FLAT,
            stroke,
            egui::StrokeKind::Outside,
        );
    }
}
