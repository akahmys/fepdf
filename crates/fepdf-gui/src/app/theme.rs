//! Theme, typography, and visual styling for `fepdf-gui`.

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

    ctx.set_fonts(fonts);
    apply_global_styles(ctx);
}

pub fn apply_global_styles(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.global_style_mut(|style| {
        style.visuals.selection.stroke = egui::Stroke::NONE;
        style.visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(120, 125, 135, 45);
        style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        style.visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(210));
    });
}
