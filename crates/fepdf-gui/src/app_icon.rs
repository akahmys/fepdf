//! The application's icon: [`assets/icon/fepdf.svg`](../assets/icon/fepdf.svg), the
//! element iron on a page.
//!
//! **Rendered once, on a Mac, and carried as a PNG.** Its words are set in the system
//! face (`-apple-system`), which a renderer at run time would replace with whatever face
//! the machine has. So the picture is made where that face is, and the window carries the
//! picture. It was rendered by QuickLook from a copy declared 1024 points square — at the
//! master's own 256, QuickLook scales it past its canvas and crops it:
//!
//! ```text
//! sed 's/width="256" height="256"/width="1024" height="1024"/' fepdf.svg > big.svg
//! qlmanage -t -s 1024 -o . big.svg
//! ```
//!
//! **QuickLook paints onto white**, so what lies outside the frame's rounded outline —
//! the outer edge of its stroke, radius 50 of 256 — was cleared with Pillow, through a
//! mask drawn four times larger and reduced so that its edge is smooth. The result was
//! reduced to 512 with Lanczos. `the_icon_decodes_with_clear_corners` failed on the white
//! corners before that.

/// The icon, as the PNG the build carries.
const PNG: &[u8] = include_bytes!("../assets/icon/fepdf-512.png");

/// The icon's pixels for the window and the dock, or none where the PNG is not the eight
/// bit RGBA it was written as.
pub fn icon() -> Option<egui::IconData> {
    let decoder = png::Decoder::new(std::io::Cursor::new(PNG));
    let mut reader = decoder.read_info().ok()?;
    if reader.output_color_type() != (png::ColorType::Rgba, png::BitDepth::Eight) {
        return None;
    }
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(frame.buffer_size());
    Some(egui::IconData { rgba, width: frame.width, height: frame.height })
}

/// `window` with the icon, where it decodes.
pub fn with_icon(window: egui::ViewportBuilder) -> egui::ViewportBuilder {
    match icon() {
        Some(icon) => window.with_icon(icon),
        None => window,
    }
}

#[cfg(test)]
mod tests {
    /// **The icon the build carries decodes, square, with clear corners**: a corner
    /// outside the rounded frame is transparent, and the middle of the plate is not.
    #[test]
    fn the_icon_decodes_with_clear_corners() {
        let icon = super::icon().expect("the PNG decodes as RGBA");
        assert_eq!((icon.width, icon.height), (512, 512));
        let alpha = |x: usize, y: usize| icon.rgba.get((y * 512 + x) * 4 + 3).copied();
        assert_eq!(alpha(0, 0), Some(0), "the corner is clear");
        assert_eq!(alpha(256, 256), Some(255), "the plate is opaque");
    }
}
