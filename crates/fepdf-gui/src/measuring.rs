//! What the caliper measures besides a distance, and the scale it measures in
//! (ROADMAP W-16).
//!
//! **A drawing's own scale, when it declares one.** A length in points is a length on the
//! sheet; the reader of a 1:100 plan wants metres, and the plan may say so in a viewport
//! (12.9). What the page declares is read by the worker and kept per page; a measurement
//! is given in the scale that holds at its first point, as 12.9.1 says.
//!
//! **A unit of user space is not always a point.** A page's `/UserUnit` (Table 31) makes it
//! that many 1/72 inch, and large drawings are where it is used. A declared scale already
//! converts user space units (`/X`), so it needs nothing; the lengths shown on the sheet,
//! and a scale the reader sets, are multiplied by it. Both were read as points until
//! 2026-09-29, so on a page of `/UserUnit 10` they came out a tenth of the truth.

use crate::cad_canvas::CaliperTool;
use fepdf::MeasurementScale;
use fepdf::measure::{Scale, holding};

/// What the caliper takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaliperMode {
    /// A drag from one point to another.
    #[default]
    Distance,
    /// A click at each corner of a shape, for its perimeter and area.
    Polygon,
}

/// The units a scale can be set in, and how many metres each is.
pub const UNITS: [(&str, f64); 5] =
    [("mm", 0.001), ("cm", 0.01), ("m", 1.0), ("in", 0.0254), ("ft", 0.3048)];

/// How many of a unit `unit_metres` long one point stands for on a 1:`denominator` sheet.
///
/// A point is 1/72 inch on the sheet, and the drawing is `denominator` times that.
#[must_use]
pub fn per_point(denominator: f64, unit_metres: f64) -> f64 {
    denominator * 0.0254 / 72.0 / unit_metres
}

/// The length round the polygon through `points`, closed back to the first.
#[must_use]
pub fn perimeter(points: &[(f64, f64)]) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| (b.0 - a.0).hypot(b.1 - a.1))
        .sum()
}

/// The scale a form asks to set: 1 to `denominator`, in `UNITS[unit]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleForm {
    /// The `N` of 1:N.
    pub denominator: f64,
    /// Which of [`UNITS`].
    pub unit: usize,
}

impl Default for ScaleForm {
    fn default() -> Self {
        Self { denominator: 100.0, unit: 2 }
    }
}

impl ScaleForm {
    /// The scale this form describes, for `page`, whose user space unit is `user_unit`
    /// points: `/X` converts user space units, so the ratio is per one of those.
    #[must_use]
    pub fn scale(&self, page: usize, user_unit: f64) -> Option<MeasurementScale> {
        let (label, metres) = UNITS.get(self.unit)?;
        #[allow(clippy::cast_possible_truncation)] // a ratio, carried as `f32` by the vocabulary
        let scale_ratio = (per_point(self.denominator, *metres) * user_unit) as f32;
        Some(MeasurementScale { page, scale_ratio, unit_label: (*label).to_owned() })
    }
}

/// The scale a measurement on `page` starting at `first` is given in, if the page
/// declares one there.
fn scale_for(tool: &CaliperTool, page: usize, first: egui::Pos2) -> Option<&Scale> {
    holding(tool.scales.get(&page)?, (f64::from(first.x), f64::from(first.y)))
}

/// The caliper's drawer: what it takes, what it measured, and the scale to set.
///
/// Answers the scale to set on the page, when the reader asked for one.
pub fn show(
    tool: &mut CaliperTool,
    ui: &mut egui::Ui,
    tr: &dyn Fn(&str) -> String,
    page: usize,
) -> Option<MeasurementScale> {
    use crate::app::theme::space;
    ui.horizontal(|ui| {
        for (mode, key) in [
            (CaliperMode::Distance, "caliper_mode_distance"),
            (CaliperMode::Polygon, "caliper_mode_polygon"),
        ] {
            if ui.selectable_label(tool.mode == mode, tr(key)).clicked() && tool.mode != mode {
                tool.clear();
                tool.mode = mode;
            }
        }
    });
    ui.add_space(space::ITEM);
    ui.label(tr(if tool.mode == CaliperMode::Polygon {
        "caliper_polygon_help"
    } else {
        "caliper_help"
    }));
    ui.add_space(space::GROUP);
    readout(tool, ui, tr);
    ui.add_space(space::GROUP);
    if ui.button(tr("caliper_clear")).clicked() {
        tool.clear();
    }
    ui.add_space(space::GROUP);
    ui.separator();
    scale_form(tool, ui, tr, page)
}

/// The points measured, in PDF space: a distance's two ends, or a shape's corners.
fn measured_points(tool: &CaliperTool) -> Vec<(f64, f64)> {
    let point = |p: egui::Pos2| (f64::from(p.x), f64::from(p.y));
    match tool.mode {
        CaliperMode::Distance => {
            tool.caliper_line.map(|(a, b)| vec![point(a), point(b)]).unwrap_or_default()
        }
        CaliperMode::Polygon => tool.polygon.iter().map(|p| point(*p)).collect(),
    }
}

/// What was measured, on the sheet and in the drawing's scale.
fn readout(tool: &CaliperTool, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) {
    let accent = crate::app::theme::colors::rust::ACCENT;
    let row = |ui: &mut egui::Ui, key: &str, value: String| {
        ui.horizontal(|ui| {
            ui.label(tr(key));
            ui.label(egui::RichText::new(value).strong().color(accent));
        });
    };
    let Some(page) = tool.page else {
        ui.label(egui::RichText::new(tr("caliper_none")).weak());
        return;
    };
    let points = measured_points(tool);
    let Some(&first) = points.first() else {
        ui.label(egui::RichText::new(tr("caliper_none")).weak());
        return;
    };
    #[allow(clippy::cast_possible_truncation)] // a point on the page, which came in as `f32`
    let scale = scale_for(tool, page, egui::pos2(first.0 as f32, first.1 as f32));
    let mm = 25.4 / 72.0;
    // Points on the sheet: a unit of this page's user space is `unit` of them.
    let unit = tool.user_unit(page);
    if tool.mode == CaliperMode::Distance {
        let length = perimeter(&points) / 2.0 * unit;
        row(ui, "caliper_distance", format!("{length:.2} pt  ({:.2} mm)", length * mm));
        if let (Some(scale), [a, b]) = (scale, points.as_slice()) {
            row(ui, "caliper_in_scale", scale.distance(*a, *b));
        }
    } else if points.len() >= 3 {
        let length = perimeter(&points) * unit;
        let area = fepdf::measure::polygon_area(&points) * unit * unit;
        row(ui, "caliper_perimeter", format!("{length:.2} pt  ({:.2} mm)", length * mm));
        row(ui, "caliper_area", format!("{area:.2} pt²  ({:.2} mm²)", area * mm * mm));
        if let Some(scale) = scale {
            let mut closed = points.clone();
            closed.push(first);
            row(ui, "caliper_perimeter_in_scale", scale.path_length(&closed));
            row(ui, "caliper_area_in_scale", scale.area(&points));
        }
    } else {
        ui.label(egui::RichText::new(tr("caliper_polygon_more")).weak());
    }
    if let Some(scale) = scale {
        ui.label(tr("caliper_scale_declared").replacen("{}", &scale.ratio, 1));
    } else {
        ui.label(egui::RichText::new(tr("caliper_no_scale")).weak());
    }
}

/// Setting the page's scale: 1 to N, in a unit.
fn scale_form(
    tool: &mut CaliperTool,
    ui: &mut egui::Ui,
    tr: &dyn Fn(&str) -> String,
    page: usize,
) -> Option<MeasurementScale> {
    ui.label(tr("caliper_set_scale_title"));
    ui.horizontal(|ui| {
        ui.label(tr("caliper_one_to"));
        ui.add(egui::DragValue::new(&mut tool.form.denominator).range(1.0..=1_000_000.0));
        for (index, (unit, _)) in UNITS.iter().enumerate() {
            ui.selectable_value(&mut tool.form.unit, index, *unit);
        }
    });
    if ui.button(tr("caliper_set_scale")).clicked() {
        tool.form.scale(page, tool.user_unit(page))
    } else {
        None
    }
}

/// The arithmetic the drawer shows.
#[cfg(test)]
mod arithmetic {
    use super::{ScaleForm, per_point, perimeter};

    /// **A 1:100 plan in metres**: an inch on the sheet is 2.54 metres.
    #[test]
    fn a_one_to_a_hundred_inch_is_two_and_a_half_metres() {
        assert!((per_point(100.0, 1.0) * 72.0 - 2.54).abs() < 1e-9);
        let scale = ScaleForm::default().scale(3, 1.0).expect("a scale");
        assert_eq!((scale.page, scale.unit_label.as_str()), (3, "m"));
        assert!((f64::from(scale.scale_ratio) * 72.0 - 2.54).abs() < 1e-6);
    }

    /// **On a page of `/UserUnit 10` a unit of user space is ten points**, and `/X`
    /// converts user space units, so a 1:100 plan's ratio is ten times a point's.
    #[test]
    fn a_user_unit_multiplies_the_ratio_the_form_sets() {
        let scale = ScaleForm::default().scale(0, 10.0).expect("a scale");
        assert!((f64::from(scale.scale_ratio) * 72.0 - 25.4).abs() < 1e-5);
    }

    /// The way round a square, closed back to where it began.
    #[test]
    fn a_perimeter_closes() {
        let square = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!((perimeter(&square) - 40.0).abs() < 1e-9);
        assert!((perimeter(&[(0.0, 0.0)])).abs() < 1e-9);
    }
}
