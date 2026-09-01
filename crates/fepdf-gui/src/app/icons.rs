//! Dedicated icon definitions and vector rendering widgets for `fepdf-gui`.

use crate::app::theme::colors;

/// Vector icon kinds rendered natively using `egui::Painter`.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum VectorIcon {
    /// Lucide Download rotated 90° counter-clockwise (Import into container).
    Import,
    /// Lucide Upload rotated 90° clockwise (Export from container).
    Export,
}

/// Renders a vector icon into the given bounding rect with crisp strokes.
pub fn render_vector_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: VectorIcon,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.6_f32, color);
    let size = 16.0_f32;
    let center = rect.center();
    let min = center - egui::vec2(size / 2.0, size / 2.0);

    let scale = size / 24.0;
    let pt = |x: f32, y: f32| -> egui::Pos2 {
        egui::pos2(x.mul_add(scale, min.x), y.mul_add(scale, min.y))
    };

    match icon {
        VectorIcon::Import => {
            // Lucide download rotated 90° left:
            // Tray on right: (14,4) -> (18.5,4) -> (20,5.5) -> (20,18.5) -> (18.5,20) -> (14,20)
            let tray = [
                pt(14.0, 4.0),
                pt(18.5, 4.0),
                pt(20.0, 5.5),
                pt(20.0, 18.5),
                pt(18.5, 20.0),
                pt(14.0, 20.0),
            ];
            for w in tray.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            // Arrow pointing right into tray:
            painter.line_segment([pt(4.0, 12.0), pt(14.5, 12.0)], stroke);
            painter.line_segment([pt(9.5, 7.5), pt(14.5, 12.0)], stroke);
            painter.line_segment([pt(14.5, 12.0), pt(9.5, 16.5)], stroke);
        }
        VectorIcon::Export => {
            // Lucide upload rotated 90° right:
            // Tray on left: (10,4) -> (5.5,4) -> (4,5.5) -> (4,18.5) -> (5.5,20) -> (10,20)
            let tray = [
                pt(10.0, 4.0),
                pt(5.5, 4.0),
                pt(4.0, 5.5),
                pt(4.0, 18.5),
                pt(5.5, 20.0),
                pt(10.0, 20.0),
            ];
            for w in tray.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
            // Arrow pointing right out of tray:
            painter.line_segment([pt(9.5, 12.0), pt(20.0, 12.0)], stroke);
            painter.line_segment([pt(15.0, 7.5), pt(20.0, 12.0)], stroke);
            painter.line_segment([pt(20.0, 12.0), pt(15.0, 16.5)], stroke);
        }
    }
}

/// Creates a 32x32 button displaying a custom vector icon.
pub fn vector_icon_bar_btn(
    ui: &mut egui::Ui,
    icon: VectorIcon,
    is_active: bool,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, is_active);
        if response.hovered() && enabled {
            ui.painter().rect_filled(rect, 4.0, visuals.bg_fill);
        }
        let color = if !enabled {
            colors::STEEL_BORDER_SUBTLE
        } else if is_active {
            colors::RUST_PRIMARY
        } else if response.hovered() {
            colors::STEEL_PRIMARY
        } else {
            colors::STEEL_SECONDARY
        };
        render_vector_icon(ui.painter(), rect, icon, color);
    }
    response
}

/// Creates a standard 32x32 icon button using Lucide font codepoint.
pub fn icon_bar_btn(icon: &'static str, is_active: bool) -> egui::Button<'static> {
    let rich_text = if is_active {
        egui::RichText::new(icon).size(15.0).color(colors::RUST_PRIMARY)
    } else {
        egui::RichText::new(icon).size(15.0).color(colors::STEEL_SECONDARY)
    };
    egui::Button::new(rich_text).min_size(egui::vec2(32.0, 32.0)).selected(is_active)
}
