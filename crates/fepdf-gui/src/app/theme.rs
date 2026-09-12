//! The vocabularies the interface is built from: colour, dimension, layout, and the
//! fonts that carry them.
//!
//! **Three vocabularies, one home each.** A value that is not in one of them is not
//! available to a widget — see `CODING.md` UI-9, UI-11 and UI-13. The engine already
//! works this way on the other side of the page edge: [`crate::app::FepdfApp::PAGE_GAP`]
//! and [`crate::app::FepdfApp::TILE_COLUMNS`] arrange pages on a 72pt grid, with a
//! build-time assertion holding their order. This module is the same discipline for the
//! side of the window the reader clicks on.

/// 紙・鋼・錆 — the whole palette, and the only place a colour is written down.
///
/// **No `#[allow(dead_code)]`, and no two names for one value.** A colour nobody paints
/// with is a leftover, and a second name for a colour that already has one is how
/// `CARD_BG` came to exist. The pair this replaced held one: `SURFACE_ACTIVE` and
/// `STEEL_BORDER_SUBTLE` were both `(226, 232, 240)`, and rustc was happy because each
/// was used.
pub mod colors {
    use egui::Color32;

    /// 紙 — the surfaces. Every light plane in the window is paper of some grade.
    pub mod paper {
        use egui::Color32;

        /// The page, and the panels. One white, because they are the same material.
        ///
        /// **The sheet is told apart from the canvas by [`super::steel::EDGE`], not by
        /// its fill.** White paper on a `CANVAS` ground measures 1.09:1, which is not a
        /// boundary; a one-pixel edge measures 3.74:1, which is (WCAG 1.4.11).
        pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);
        /// The bench the sheets are laid on.
        pub const CANVAS: Color32 = Color32::from_rgb(244, 245, 247);
        /// A control under the pointer.
        pub const HOVER: Color32 = Color32::from_rgb(241, 245, 249);
        /// A control being pressed, and the fill of one that is on.
        pub const PRESSED: Color32 = Color32::from_rgb(226, 232, 240);
    }

    /// 鋼 — the lines and the letters.
    ///
    /// Four grades, each with one job. `EDGE` and `RULE` are the pair that used to be
    /// one constant doing both: a boundary that carries meaning needs 3:1 and a
    /// decorative separator does not, so they cannot be the same value.
    pub mod steel {
        use egui::Color32;

        /// Body and heading text. 14.63:1 on [`super::paper::WHITE`].
        pub const TEXT: Color32 = Color32::from_rgb(30, 41, 59);
        /// Secondary text and resting icons. 7.58:1.
        pub const MUTED: Color32 = Color32::from_rgb(71, 85, 105);
        /// **Any boundary that carries meaning**: the edge of a sheet, the outline of a
        /// tag, a control that is present but disabled.
        ///
        /// **3:1 against every surface it can meet, not only against white.** It was
        /// picked at 3.20:1 on `paper::WHITE` and left at that, which is the one ground
        /// a sheet's edge does *not* sit on — the edge divides the sheet from
        /// `paper::CANVAS` (2.93:1) and a disabled control can sit on `paper::PRESSED`
        /// (2.59:1). The darkest of the four decides it: 3.03:1 there, 3.74:1 on white.
        ///
        /// A disabled control drawn below this stops being a disabled control and
        /// becomes an absent one, which the interface may not do (principle P3).
        pub const EDGE: Color32 = Color32::from_rgb(120, 133, 154);
        /// Decorative separators only. 1.48:1, and deliberately below the boundary
        /// threshold — nothing may depend on seeing it.
        pub const RULE: Color32 = Color32::from_rgb(203, 213, 225);
    }

    /// 錆 — the one accent.
    ///
    /// **Rust marks what the reader is touching**: a selection, an active tool, work in
    /// progress. Nothing else is rust. That sentence is the whole rule, and it is what
    /// retired a second terracotta `(226, 135, 67)` and a gold `(255, 215, 0)` that had
    /// accumulated beside it.
    pub mod rust {
        use egui::Color32;

        /// The accent itself. 7.38:1 on [`super::paper::WHITE`].
        pub const ACCENT: Color32 = Color32::from_rgb(148, 56, 32);

        /// The accent as a wash, for the fill behind selected text.
        pub fn wash() -> Color32 {
            super::tint(ACCENT, 36)
        }
    }

    /// What the reader should do about it — for a document's [`fepdf::Severity`] and for
    /// the application's own notices alike, because the question is the same one.
    ///
    /// **Every one of these is at least 26° of hue from [`rust::ACCENT`].** Rust sits at
    /// 12°, so the amber this replaced (23°) and the danger red it replaced (0°) were
    /// both close enough to read as the accent on a surface they share — and the status
    /// bar is such a surface.
    pub mod note {
        use egui::Color32;

        /// Conforming, complete, done. 6.31:1.
        pub const PASS: Color32 = Color32::from_rgb(24, 110, 58);
        /// Worth knowing. `Severity::Ambiguity`. 7.02:1, and in the slate hue family.
        pub const INFO: Color32 = Color32::from_rgb(29, 92, 145);
        /// Worth checking. `Severity::Repaired` — the engine changed the input to make
        /// it work, which is a thing to look at rather than a success. 6.51:1.
        pub const WARN: Color32 = Color32::from_rgb(122, 88, 0);
        /// Something was dropped. `Severity::Violation`. 8.07:1.
        pub const FAIL: Color32 = Color32::from_rgb(158, 20, 52);
    }

    /// The same colour, for the rasteriser.
    ///
    /// **This window paints in two colour types and the palette governs both.** Vello
    /// takes `peniko::Color`, and the workbench was `Color::from_rgb8(235, 237, 240)`
    /// written into `vello_egui.rs` — a value `paper::CANVAS` was supposed to be, three
    /// shades away from it, and invisible to a checker that reads `Color32`.
    /// The same colour for vello, alpha included and un-premultiplied.
    ///
    /// **Two conversions, not one, and both were invisible while every colour crossing
    /// here was opaque.** It used to call `from_rgb8` and drop the alpha outright. And
    /// `Color32` stores its channels premultiplied — `from_rgba_unmultiplied` multiplies
    /// on the way in, so `c.r()` afterwards is the multiplied byte — while
    /// `peniko::Color` does not, so handing the components across unchanged darkens
    /// anything translucent towards black in proportion to how translucent it is. The
    /// bench's grid is `RULE` at 40 of 255: it came out `(211, 212, 213)` against the
    /// `(238, 240, 243)` egui draws for the same colour, which is a grey line where a
    /// faint one was asked for.
    ///
    /// An opaque colour passes through both steps unchanged, which is why nothing had
    /// noticed either.
    pub const fn to_peniko(c: Color32) -> vello::peniko::Color {
        let alpha = c.a();
        if alpha == 0 {
            return vello::peniko::Color::TRANSPARENT;
        }
        vello::peniko::Color::from_rgba8(
            straighten(c.r(), alpha),
            straighten(c.g(), alpha),
            straighten(c.b(), alpha),
            alpha,
        )
    }

    /// One premultiplied channel, divided back out. `alpha` is never zero here.
    #[allow(clippy::cast_possible_truncation)]
    const fn straighten(channel: u8, alpha: u8) -> u8 {
        (channel as u16 * 255 / alpha as u16) as u8
    }

    /// The same colour at `alpha`, for a badge ground or an overlay fill.
    ///
    /// A tint is derived rather than declared so that a badge cannot drift away from the
    /// text it sits behind — the three `STATUS_*_BG` constants this replaced were each
    /// picked by hand.
    pub fn tint(c: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
    }
}

/// The space between things, in screen points, and what each step means.
///
/// **The step says what the break is.** They were 2, 3, 4, 5, 6, 8, 10 and 12, which is
/// eight values carrying no distinction at all; `layout.rs` had already reached the other
/// half of this conclusion for the page grid, where the row gap is twice the column gap
/// "because a row break is a bigger break than a column one".
pub mod space {
    /// Inside one thing — an icon and its label.
    pub const ITEM: f32 = 4.0;
    /// Between things of a kind — one button and the next.
    pub const GROUP: f32 = 8.0;
    /// Between groups — the zoom controls and the view modes.
    pub const SECTION: f32 = 12.0;
    /// Between the parts of a panel — a heading and what it introduces.
    pub const PANE: f32 = 24.0;
}

/// Four type sizes. There were eight, in a window that holds no document text.
pub mod text {
    /// Captions, units, secondary figures.
    pub const SMALL: f32 = 11.0;
    /// Everything by default.
    pub const BODY: f32 = 13.0;
    /// A panel or drawer heading.
    pub const HEAD: f32 = 15.0;
    /// A window title.
    pub const TITLE: f32 = 18.0;
}

/// Two radii: a plane is flat, a control is rounded.
pub mod radius {
    /// Panels, sheets, fills that are not pressable.
    pub const FLAT: f32 = 0.0;
    /// Anything that can be clicked.
    pub const CONTROL: f32 = 4.0;
}

/// Drawing on the page, where the colours belong to the document.
///
/// **An overlay may not rely on its hue.** The engine goes to some length to put the
/// document's own colours on the raster — `/ICCBased` through its profile, `/Separation`
/// through its tint transform — and a drawing printed in orange is as likely as one
/// printed in black. So an overlay is told apart by *shape*, by the word written on it,
/// or by being drawn outside the sheet; the accent says only that the reader is touching
/// it.
///
/// What is left is legibility, and that is what the halo is for: a stroke or a letter
/// with a paper-coloured outline reads on any ground. Maps, CAD and subtitles all do
/// this, for the same reason.
pub mod canvas {
    use super::{colors, size};

    /// How far the halo extends, in points.
    pub const HALO: f32 = 1.0;

    /// The bench's grid lines over a viewport of `size`, in points from its top-left.
    ///
    /// **Handed back rather than drawn, because two painters draw this bench.** egui
    /// paints it in the thumbnail path; in the viewport path an opaque vello texture
    /// covers the whole viewport a step later, so anything egui puts under it is painted
    /// and hidden — which is what happened to this grid for as long as it existed. Vello
    /// draws it now, from these same lines.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn grid_lines(viewport: egui::Vec2, pan: egui::Vec2, zoom: f32) -> Vec<[egui::Pos2; 2]> {
        let step = size::GRID * zoom;
        if step <= 0.1 {
            return Vec::new();
        }
        let mut lines = Vec::new();
        let down = ((viewport.x - pan.x % step) / step).ceil().max(0.0) as usize;
        for i in 0..down {
            let x = (i as f32).mul_add(step, pan.x % step);
            lines.push([egui::pos2(x, 0.0), egui::pos2(x, viewport.y)]);
        }
        let across = ((viewport.y - pan.y % step) / step).ceil().max(0.0) as usize;
        for i in 0..across {
            let y = (i as f32).mul_add(step, pan.y % step);
            lines.push([egui::pos2(0.0, y), egui::pos2(viewport.x, y)]);
        }
        lines
    }

    /// The grid's colour: the rule, faint enough to sit under a page without competing.
    pub fn grid_colour() -> egui::Color32 {
        colors::tint(colors::steel::RULE, 40)
    }

    /// Draws `text` with a paper-coloured halo, so it reads on any page.
    pub fn haloed_text(
        painter: &egui::Painter,
        pos: egui::Pos2,
        anchor: egui::Align2,
        text: &str,
        font: egui::FontId,
        colour: egui::Color32,
    ) {
        for dx in [-HALO, HALO] {
            for dy in [-HALO, HALO] {
                painter.text(
                    pos + egui::vec2(dx, dy),
                    anchor,
                    text,
                    font.clone(),
                    colors::paper::WHITE,
                );
            }
        }
        painter.text(pos, anchor, text, font, colour);
    }

    /// Outlines `rect` with a paper-coloured halo outside the stroke it then draws.
    pub fn haloed_rect(
        painter: &egui::Painter,
        rect: egui::Rect,
        radius: f32,
        stroke: egui::Stroke,
    ) {
        painter.rect_stroke(
            rect.expand(stroke.width),
            radius,
            egui::Stroke::new(HALO, colors::paper::WHITE),
            egui::StrokeKind::Outside,
        );
        painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Outside);
    }
}

/// The sizes the layout is built from, on a 4pt grid.
///
/// **Derived where one follows from another**, so that changing the grid moves
/// everything that stands on it rather than leaving a residue behind.
pub mod size {
    use super::space;

    /// Every icon button, everywhere. There were seven click-target sizes; the smallest
    /// were 20×20 and 22×20, which is under the size a pointer reliably hits.
    pub const ICON: f32 = 32.0;
    /// A row that carries a label; the width comes from the content.
    pub const ROW: f32 = 24.0;
    /// The glyph inside an icon button — half the button, so the two move together.
    pub const GLYPH: f32 = ICON / 2.0;
    /// The left icon rail: one icon with a group's margin on each side.
    pub const RAIL: f32 = ICON + space::GROUP * 2.0;
    /// The status bar: one row with an item's margin above and below.
    pub const STATUS: f32 = ROW + space::ITEM * 2.0;
    /// A window holding a single column of fields.
    pub const FORM_W: f32 = 360.0;
    /// A window holding a table or a list.
    pub const TABLE_W: f32 = 560.0;
    /// The utility drawer, and the range it may be dragged through.
    pub const DRAWER_W: f32 = 320.0;
    /// The narrowest the drawer may become.
    pub const DRAWER_MIN: f32 = 260.0;
    /// The widest the drawer may become.
    pub const DRAWER_MAX: f32 = 600.0;
    /// The bench's grid, at zoom 1.
    pub const GRID: f32 = 32.0;
    /// The label column of a two-column property grid.
    ///
    /// Wide enough for the longest label the drawer carries — `代替テキスト (Alt Text):`,
    /// which is the name in the reader's language and the name of the entry in the file.
    /// Without a floor the column takes whatever is left and breaks the label between two
    /// characters of a word: `境界ボ / ックス / (BBox) / :` down four lines.
    pub const LABEL_W: f32 = 144.0;
}

/// The steps are a scale, and a build says so.
///
/// **At module scope, because an associated constant nobody reads is never evaluated** —
/// the same reason `layout.rs` puts `_ROW_BREAK_IS_BIGGER` here rather than in an `impl`.
const _SPACE_IS_A_SCALE: () = assert!(
    space::ITEM < space::GROUP && space::GROUP < space::SECTION && space::SECTION < space::PANE,
    "each step must be a bigger break than the one before it"
);

/// Every size stands on the 4pt grid.
const _SIZES_ARE_ON_THE_GRID: () = assert!(
    (size::ICON as u32).is_multiple_of(4)
        && (size::ROW as u32).is_multiple_of(4)
        && (size::GLYPH as u32).is_multiple_of(4)
        && (size::RAIL as u32).is_multiple_of(4)
        && (size::STATUS as u32).is_multiple_of(4)
        && (size::FORM_W as u32).is_multiple_of(4)
        && (size::TABLE_W as u32).is_multiple_of(4)
        && (size::DRAWER_W as u32).is_multiple_of(4)
        && (size::LABEL_W as u32).is_multiple_of(4)
        && (size::GRID as u32).is_multiple_of(4),
    "chrome dimensions are multiples of the 4pt grid"
);

/// The family the icon font is reached through.
///
/// **A family of its own, rather than a fallback at the end of the proportional list.**
/// Appended to `Proportional`, the icon font was consulted *last*, so any earlier font
/// claiming the same private-use codepoint won: `U+E0FF` is the Ubuntu logo in
/// egui's own `Ubuntu-Light`, and the continuous-scroll button drew it — which is to say
/// drew nothing. A named family cannot be shadowed, because nothing else is in it.
pub fn icon_family() -> egui::FontFamily {
    egui::FontFamily::Name("lucide".into())
}

fn load_system_cjk_font(fonts: &mut egui::FontDefinitions) {
    let mut paths = vec![
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf".to_owned(),
        "/System/Library/Fonts/Hiragino Sans GB.ttc".to_owned(),
        "/Library/Fonts/Arial Unicode.ttf".to_owned(),
    ];

    if let Ok(win_dir) = std::env::var("windir") {
        paths.push(format!(r"{win_dir}\Fonts\msgothic.ttc"));
        paths.push(format!(r"{win_dir}\Fonts\yugothm.ttc"));
        paths.push(format!(r"{win_dir}\Fonts\meiryo.ttc"));
    } else {
        paths.push(r"C:\Windows\Fonts\msgothic.ttc".to_owned());
        paths.push(r"C:\Windows\Fonts\yugothm.ttc".to_owned());
        paths.push(r"C:\Windows\Fonts\meiryo.ttc".to_owned());
    }

    paths.push("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf".to_owned());
    paths.push("/usr/share/fonts/opentype/ipafont-gothic/ipag.otf".to_owned());
    paths.push("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc".to_owned());
    paths.push("/usr/share/fonts/TTF/NotoSansCJK-Regular.ttc".to_owned());

    for path in &paths {
        if let Ok(font_data) = std::fs::read(path) {
            log::info!("Successfully loaded CJK font from {path}");
            fonts.font_data.insert("cjk".to_owned(), egui::FontData::from_owned(font_data).into());
            if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                families.insert(0, "cjk".to_owned());
            }
            if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                families.insert(0, "cjk".to_owned());
            }
            break;
        }
    }
}

pub fn configure_fonts_and_styles(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let lucide_data = include_bytes!("../../assets/lucide.ttf");
    fonts.font_data.insert("lucide".to_owned(), egui::FontData::from_static(lucide_data).into());
    // Its own family, holding it alone: see `icon_family`.
    fonts.families.insert(icon_family(), vec!["lucide".to_owned()]);

    load_system_cjk_font(&mut fonts);

    ctx.set_fonts(fonts);
    apply_global_styles(ctx);
}

pub fn apply_global_styles(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = colors::paper::WHITE;
    visuals.panel_fill = colors::paper::WHITE;
    visuals.extreme_bg_color = colors::paper::CANVAS;
    visuals.faint_bg_color = colors::paper::HOVER;

    visuals.selection.stroke = egui::Stroke::new(1.0_f32, colors::rust::ACCENT);
    visuals.selection.bg_fill = colors::rust::wash();
    visuals.hyperlink_color = colors::rust::ACCENT;

    visuals.widgets.noninteractive.bg_fill = colors::paper::WHITE;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, colors::steel::RULE);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, colors::steel::MUTED);

    visuals.widgets.inactive.bg_fill = colors::paper::WHITE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, colors::steel::MUTED);

    visuals.widgets.hovered.bg_fill = colors::paper::HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, colors::steel::TEXT);

    // **`active` is not the accent, because egui reads strong text out of it.**
    // `Visuals::strong_text_color` returns `widgets.active.text_color()`, so an accent
    // here paints every `RichText::strong()` in the application: the document
    // properties, the About window's own name, the heading of every drawer. Rust marks
    // what the reader is touching (UI-10), and bold text is not that. A pressed control
    // takes the darkest steel, which is what bold should mean in this palette; the accent
    // reaches a *selection* through `visuals.selection` above.
    visuals.widgets.active.bg_fill = colors::paper::PRESSED;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, colors::steel::TEXT);

    visuals.widgets.open.bg_fill = colors::paper::HOVER;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, colors::steel::EDGE);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, colors::steel::TEXT);

    ctx.set_visuals(visuals);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(space::GROUP, space::ITEM);
    style.spacing.button_padding = egui::vec2(space::GROUP, space::ITEM);
    for font in style.text_styles.values_mut() {
        font.size = text::BODY;
    }
    style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(text::SMALL));
    style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(text::HEAD));
    ctx.set_global_style(style);
}

#[cfg(test)]
mod tests {
    use super::{canvas, colors};

    /// What the compositor shows for `c` drawn over `under`, as egui composites it.
    fn blended(c: egui::Color32, under: egui::Color32) -> [u8; 3] {
        let a = f32::from(c.a()) / 255.0;
        let mix = |fg: u8, bg: u8| {
            let straight = f32::from(fg) / a.max(f32::EPSILON);
            straight.mul_add(a, f32::from(bg) * (1.0 - a)).round() as u8
        };
        [mix(c.r(), under.r()), mix(c.g(), under.g()), mix(c.b(), under.b())]
    }

    #[test]
    fn a_translucent_colour_means_the_same_thing_to_vello_as_to_egui() {
        // `Color32` is premultiplied and `peniko::Color` is not. Handing the components
        // across unchanged darkens anything translucent towards black in proportion to
        // how translucent it is, and the bench's grid came out `(211, 212, 213)` against
        // the `(238, 240, 243)` this asks for.
        let grid = canvas::grid_colour();
        let peniko = colors::to_peniko(grid);
        let [r, g, b, _] = peniko.to_rgba8().to_u8_array();
        let straight = egui::Color32::from_rgba_unmultiplied(r, g, b, grid.a());
        assert_eq!(blended(grid, colors::paper::CANVAS), blended(straight, colors::paper::CANVAS));
    }

    #[test]
    fn an_opaque_colour_crosses_unchanged() {
        // Which is why neither conversion had ever been noticed: every colour that had
        // crossed here before the grid did was opaque.
        let canvas_colour = colors::paper::CANVAS;
        let peniko = colors::to_peniko(canvas_colour);
        assert_eq!(
            peniko.to_rgba8().to_u8_array(),
            [canvas_colour.r(), canvas_colour.g(), canvas_colour.b(), 255]
        );
    }

    #[test]
    fn the_grid_starts_where_the_pan_left_it_and_covers_the_viewport() {
        // 32pt apart at zoom 1, offset by the pan: three down the viewport and two
        // across it, the downward ones first.
        let lines = canvas::grid_lines(egui::vec2(100.0, 50.0), egui::vec2(8.0, 0.0), 1.0);
        let down = |x: f32| [egui::pos2(x, 0.0), egui::pos2(x, 50.0)];
        let across = |y: f32| [egui::pos2(0.0, y), egui::pos2(100.0, y)];
        assert_eq!(lines, vec![down(8.0), down(40.0), down(72.0), across(0.0), across(32.0)]);
    }

    #[test]
    fn a_zoom_that_would_draw_a_line_per_pixel_draws_none() {
        // The loops are bounded by the viewport over the step, so a step approaching zero
        // is a length approaching the machine's patience.
        assert!(canvas::grid_lines(egui::vec2(4000.0, 4000.0), egui::Vec2::ZERO, 0.001).is_empty());
    }
}
