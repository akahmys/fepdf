//! Theme, typography, and visual styling for `fepdf-gui`.

#[allow(dead_code)]
pub mod colors {
    use egui::Color32;

    // --- 1. Base / White & Off-white Surfaces ---
    pub const CANVAS_BG: Color32 = Color32::from_rgb(244, 245, 247);
    pub const PANEL_BG: Color32 = Color32::from_rgb(255, 255, 255);
    pub const CARD_BG: Color32 = Color32::from_rgb(255, 255, 255);
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(241, 245, 249);
    pub const SURFACE_ACTIVE: Color32 = Color32::from_rgb(226, 232, 240);

    // --- 2. Steel / Slate Structure & Typography ---
    pub const STEEL_PRIMARY: Color32 = Color32::from_rgb(30, 41, 59);
    pub const STEEL_SECONDARY: Color32 = Color32::from_rgb(71, 85, 105);
    pub const STEEL_MUTED: Color32 = Color32::from_rgb(148, 163, 184);
    pub const STEEL_BORDER: Color32 = Color32::from_rgb(203, 213, 225);
    pub const STEEL_BORDER_SUBTLE: Color32 = Color32::from_rgb(226, 232, 240);

    // --- 3. Deep Rust / Terracotta Accents ---
    pub const RUST_PRIMARY: Color32 = Color32::from_rgb(148, 56, 32);
    pub const RUST_HOVER: Color32 = Color32::from_rgb(176, 71, 43);
    pub const RUST_DEEP: Color32 = Color32::from_rgb(110, 38, 19);
    pub const RUST_SELECTION_BG: Color32 = Color32::from_rgba_premultiplied(21, 8, 5, 36);
    pub const RUST_BADGE_BG: Color32 = Color32::from_rgb(248, 239, 234);
    pub const RUST_BADGE_TEXT: Color32 = Color32::from_rgb(125, 45, 24);
    pub const RUST_BADGE_BORDER: Color32 = Color32::from_rgb(235, 214, 207);

    // --- 4. Semantic Status Badges ---
    pub const STATUS_WARN_BG: Color32 = Color32::from_rgb(254, 243, 199);
    pub const STATUS_WARN_TEXT: Color32 = Color32::from_rgb(146, 64, 14);
    pub const STATUS_PASS_BG: Color32 = Color32::from_rgb(236, 253, 245);
    pub const STATUS_PASS_TEXT: Color32 = Color32::from_rgb(21, 128, 61);
    pub const STATUS_INFO_BG: Color32 = Color32::from_rgb(240, 249, 255);
    pub const STATUS_INFO_TEXT: Color32 = Color32::from_rgb(3, 105, 161);
    pub const STATUS_DANGER_BG: Color32 = Color32::from_rgb(254, 242, 242);
    pub const STATUS_DANGER_TEXT: Color32 = Color32::from_rgb(153, 27, 27);
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

    // Load Lucide icon font
    let lucide_data = include_bytes!("../../assets/lucide.ttf");
    fonts.font_data.insert("lucide".to_owned(), egui::FontData::from_static(lucide_data).into());
    if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        families.push("lucide".to_owned());
    }
    if let Some(families) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        families.push("lucide".to_owned());
    }

    load_system_cjk_font(&mut fonts);

    ctx.set_fonts(fonts);
    apply_global_styles(ctx);
}

pub fn apply_global_styles(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = colors::PANEL_BG;
    visuals.panel_fill = colors::PANEL_BG;
    visuals.extreme_bg_color = colors::CANVAS_BG;
    visuals.faint_bg_color = colors::SURFACE_HOVER;

    visuals.selection.stroke = egui::Stroke::new(1.0_f32, colors::RUST_PRIMARY);
    visuals.selection.bg_fill = colors::RUST_SELECTION_BG;
    visuals.hyperlink_color = colors::RUST_PRIMARY;

    visuals.widgets.noninteractive.bg_fill = colors::PANEL_BG;
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0_f32, colors::STEEL_BORDER_SUBTLE);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, colors::STEEL_SECONDARY);

    visuals.widgets.inactive.bg_fill = colors::PANEL_BG;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, colors::STEEL_SECONDARY);

    visuals.widgets.hovered.bg_fill = colors::SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, colors::STEEL_BORDER_SUBTLE);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, colors::STEEL_PRIMARY);

    visuals.widgets.active.bg_fill = colors::SURFACE_ACTIVE;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, colors::RUST_PRIMARY);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, colors::RUST_PRIMARY);

    visuals.widgets.open.bg_fill = colors::SURFACE_HOVER;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, colors::STEEL_BORDER);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, colors::STEEL_PRIMARY);

    ctx.set_visuals(visuals);
}
