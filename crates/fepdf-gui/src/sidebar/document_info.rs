use crate::locale::LocaleManager;

#[allow(clippy::too_many_arguments)]
pub fn show_document_info(
    // RR-15 Limit: GUI - document info properties rendering
    ui: &mut egui::Ui,
    pdf_name: &Option<String>,
    total_pages: usize,
    metadata: &Option<fepdf::MetadataInfo>,
    file_size: Option<usize>,
    pdf_version: &Option<String>,
    security_method: &Option<String>,
    permissions: Option<i32>,
    page_sizes: &[(f64, f64)],
    fonts: &[fepdf::FontSummary],
    decisions: &[fepdf::Decision],
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    fn format_pdf_date(date_str: &str) -> String {
        if let Some(clean) = date_str.strip_prefix("D:")
            && clean.len() >= 14
        {
            let year = &clean[0..4];
            let month = &clean[4..6];
            let day = &clean[6..8];
            let hour = &clean[8..10];
            let min = &clean[10..12];
            let sec = &clean[12..14];
            return format!("{year}/{month}/{day} {hour}:{min}:{sec}");
        }
        date_str.to_string()
    }

    let format_file_size = |bytes: usize| -> String {
        fn format_num(n: usize) -> String {
            let s = n.to_string();
            let mut result = String::new();
            let len = s.len();
            for (i, c) in s.chars().enumerate() {
                result.push(c);
                if (len - i - 1).is_multiple_of(3) && i != len - 1 {
                    result.push(',');
                }
            }
            result
        }

        if bytes >= 1_048_576 {
            let mb = format!("{:.2}", bytes as f64 / 1_048_576.0);
            if active_lang == "ja" {
                format!("{} MB ({} バイト)", mb, format_num(bytes))
            } else {
                format!("{} MB ({} bytes)", mb, format_num(bytes))
            }
        } else if bytes >= 1024 {
            let kb = format!("{:.2}", bytes as f64 / 1024.0);
            if active_lang == "ja" {
                format!("{} KB ({} バイト)", kb, format_num(bytes))
            } else {
                format!("{} KB ({} bytes)", kb, format_num(bytes))
            }
        } else if active_lang == "ja" {
            format!("{} バイト", format_num(bytes))
        } else {
            format!("{} bytes", format_num(bytes))
        }
    };

    let format_page_size = |w: f64, h: f64| -> String {
        let w_mm = w * 25.4 / 72.0;
        let h_mm = h * 25.4 / 72.0;

        let is_a4 = (w_mm - 210.0).abs() < 2.0 && (h_mm - 297.0).abs() < 2.0;
        let is_a4_landscape = (w_mm - 297.0).abs() < 2.0 && (h_mm - 210.0).abs() < 2.0;
        let is_letter = (w_mm - 215.9).abs() < 2.0 && (h_mm - 279.4).abs() < 2.0;
        let is_letter_landscape = (w_mm - 279.4).abs() < 2.0 && (h_mm - 215.9).abs() < 2.0;

        let format_name = if is_a4 {
            " (A4)"
        } else if is_a4_landscape {
            if active_lang == "ja" { " (A4 横)" } else { " (A4 Landscape)" }
        } else if is_letter {
            " (Letter)"
        } else if is_letter_landscape {
            if active_lang == "ja" { " (Letter 横)" } else { " (Letter Landscape)" }
        } else {
            ""
        };

        format!("{w_mm:.1} x {h_mm:.1} mm{format_name}")
    };

    ui.vertical(|ui| {
        ui.label(egui::RichText::new(locale_mgr.tr(active_lang, "info_title")).strong().size(16.0));
        egui::ScrollArea::vertical().id_salt("doc_info_scroll").show(ui, |ui| {
            let render_row = |ui: &mut egui::Ui, key: &str, val: &str, is_val_strong: bool| {
                ui.label(egui::RichText::new(key).weak());
                let text = egui::RichText::new(val);
                let text = if is_val_strong { text.strong() } else { text };
                ui.add(egui::Label::new(text).truncate());
                ui.add_space(4.0);
            };

            // 1. 概要 (Description)
            egui::CollapsingHeader::new(
                egui::RichText::new(locale_mgr.tr(active_lang, "info_summary")).strong().size(13.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let filename_key = locale_mgr.tr(active_lang, "info_filename");
                    let filename_val_fallback;
                    let filename_val = match pdf_name.as_deref() {
                        Some(name) => name,
                        None => {
                            filename_val_fallback = "-".to_string();
                            &filename_val_fallback
                        }
                    };
                    render_row(ui, &filename_key, filename_val, true);

                    // `render_row` takes `&str`, so these borrow from the metadata
                    // rather than copying it out field by field.
                    const EMPTY: &str = "-";
                    let field = |pick: fn(&fepdf::MetadataInfo) -> Option<&String>| {
                        metadata.as_ref().and_then(pick).map_or(EMPTY, String::as_str)
                    };
                    let title = field(|m| m.title.as_ref());
                    let author = field(|m| m.author.as_ref());
                    let subject = field(|m| m.subject.as_ref());
                    let keywords = field(|m| m.keywords.as_ref());
                    let creator = field(|m| m.creator.as_ref());
                    let producer = field(|m| m.producer.as_ref());
                    // Dates are reformatted, so these do own their string.
                    let created = metadata
                        .as_ref()
                        .and_then(|m| m.creation_date.as_ref().map(|d| format_pdf_date(d)));
                    let created = created.as_deref().unwrap_or(EMPTY);
                    let modified = metadata
                        .as_ref()
                        .and_then(|m| m.mod_date.as_ref().map(|d| format_pdf_date(d)));
                    let modified = modified.as_deref().unwrap_or(EMPTY);

                    render_row(ui, &locale_mgr.tr(active_lang, "info_doc_title"), title, true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_author"), author, true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_subject"), subject, true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_keywords"), keywords, true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_created"), created, true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_modified"), modified, true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_application"), creator, true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_producer"), producer, true);
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 2. ファイル仕様 (File Specification)
            egui::CollapsingHeader::new(
                egui::RichText::new(locale_mgr.tr(active_lang, "info_file_spec"))
                    .strong()
                    .size(13.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let ver = if pdf_name.is_none() {
                        "-"
                    } else {
                        pdf_version.as_deref().unwrap_or("1.7")
                    };
                    render_row(ui, &locale_mgr.tr(active_lang, "info_pdf_version"), ver, true);

                    let size_str =
                        file_size.map_or_else(|| "-".to_string(), |s| format_file_size(s));
                    render_row(ui, &locale_mgr.tr(active_lang, "info_file_size"), &size_str, true);

                    let count_str =
                        if pdf_name.is_none() { "-".to_string() } else { total_pages.to_string() };
                    render_row(
                        ui,
                        &locale_mgr.tr(active_lang, "info_page_count"),
                        &count_str,
                        true,
                    );

                    let page_size_str = if let Some(first_size) = page_sizes.first() {
                        let formatted = format_page_size(first_size.0, first_size.1);
                        if page_sizes.len() > 1 {
                            format!(
                                "{} ({})",
                                formatted,
                                locale_mgr.tr(active_lang, "info_other_sizes")
                            )
                        } else {
                            formatted
                        }
                    } else {
                        "-".to_string()
                    };
                    render_row(
                        ui,
                        &locale_mgr.tr(active_lang, "info_page_size"),
                        &page_size_str,
                        true,
                    );
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 3. セキュリティと制限事項 (Security & Restrictions)
            egui::CollapsingHeader::new(
                egui::RichText::new(locale_mgr.tr(active_lang, "info_security"))
                    .strong()
                    .size(13.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let method_fallback;
                    let method = match security_method.as_deref() {
                        Some(m) => m,
                        None => {
                            if pdf_name.is_none() {
                                "-"
                            } else {
                                method_fallback = locale_mgr.tr(active_lang, "info_sec_none");
                                &method_fallback
                            }
                        }
                    };
                    render_row(
                        ui,
                        &locale_mgr.tr(active_lang, "info_security_method"),
                        method,
                        true,
                    );

                    // Permissions bits helper
                    let has_perm = |bit: i32| -> String {
                        if pdf_name.is_none() {
                            "-".to_string()
                        } else if let Some(p) = permissions {
                            if (p & bit) != 0 {
                                locale_mgr.tr(active_lang, "info_perm_allowed")
                            } else {
                                locale_mgr.tr(active_lang, "info_perm_not_allowed")
                            }
                        } else {
                            locale_mgr.tr(active_lang, "info_perm_allowed")
                        }
                    };

                    render_row(ui, &locale_mgr.tr(active_lang, "info_print"), &has_perm(4), true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_modify"), &has_perm(8), true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_copy"), &has_perm(16), true);
                    render_row(
                        ui,
                        &locale_mgr.tr(active_lang, "info_accessibility_copy"),
                        &has_perm(512),
                        true,
                    );
                    render_row(
                        ui,
                        &locale_mgr.tr(active_lang, "info_extract"),
                        &has_perm(16),
                        true,
                    );
                    render_row(
                        ui,
                        &locale_mgr.tr(active_lang, "info_annotation"),
                        &has_perm(32),
                        true,
                    );
                    render_row(ui, &locale_mgr.tr(active_lang, "info_form"), &has_perm(256), true);
                    render_row(ui, &locale_mgr.tr(active_lang, "info_sign"), &has_perm(256), true);
                    render_row(
                        ui,
                        &locale_mgr.tr(active_lang, "info_assembly"),
                        &has_perm(1024),
                        true,
                    );
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 4. フォント情報 (Fonts)
            let fonts_title = locale_mgr
                .tr(active_lang, "info_fonts_in_use")
                .replace("{}", &fonts.len().to_string());
            egui::CollapsingHeader::new(egui::RichText::new(fonts_title).strong().size(13.0))
                .default_open(false)
                .show(ui, |ui| {
                    if fonts.is_empty() {
                        ui.label(
                            egui::RichText::new(locale_mgr.tr(active_lang, "info_no_fonts")).weak(),
                        );
                    } else {
                        for font in fonts {
                            ui.vertical(|ui| {
                                let embed_status = if font.is_embedded {
                                    if font.is_subset {
                                        locale_mgr.tr(active_lang, "info_font_embedded_subset")
                                    } else {
                                        locale_mgr.tr(active_lang, "info_font_embedded")
                                    }
                                } else {
                                    String::new()
                                };
                                let label_text = format!("🔠 {}{}", font.name, embed_status);
                                ui.add(
                                    egui::Label::new(egui::RichText::new(label_text).strong())
                                        .truncate(),
                                );
                                ui.indent("font_details", |ui| {
                                    let type_text = locale_mgr
                                        .tr(active_lang, "info_font_type")
                                        .replace("{}", &font.font_type);
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(type_text).weak())
                                            .truncate(),
                                    );
                                    let enc_text = locale_mgr
                                        .tr(active_lang, "info_font_encoding")
                                        .replace("{}", &font.encoding);
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(enc_text).weak())
                                            .truncate(),
                                    );
                                });
                                ui.add_space(4.0);
                            });
                        }
                    }
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 5. 規格適合・判定ログ (Conformance & Reading Decisions - ISO 32000-2 6.3.2.3)
            render_decisions_section(ui, decisions, locale_mgr, active_lang);
        });
    });
}

fn render_decisions_section(
    ui: &mut egui::Ui,
    decisions: &[fepdf::Decision],
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    let decisions_title =
        locale_mgr.tr(active_lang, "info_decisions").replace("{}", &decisions.len().to_string());
    egui::CollapsingHeader::new(egui::RichText::new(decisions_title).strong().size(13.0))
        .default_open(!decisions.is_empty())
        .show(ui, |ui| {
            if decisions.is_empty() {
                ui.label(
                    egui::RichText::new(locale_mgr.tr(active_lang, "info_no_decisions")).weak(),
                );
            } else {
                for decision in decisions {
                    ui.vertical(|ui| {
                        let color = match decision.severity {
                            fepdf::Severity::Ambiguity => egui::Color32::from_rgb(180, 180, 60),
                            fepdf::Severity::Repaired => egui::Color32::from_rgb(60, 160, 220),
                            fepdf::Severity::Violation => egui::Color32::from_rgb(220, 90, 60),
                        };
                        ui.horizontal(|ui| {
                            let tag = if decision.clause.is_empty() {
                                "[Reader]".to_string()
                            } else {
                                format!("[§{}]", decision.clause)
                            };
                            ui.colored_label(color, tag);
                            ui.add(
                                egui::Label::new(egui::RichText::new(&decision.found).strong())
                                    .truncate(),
                            );
                        });
                        ui.indent("decision_action", |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("→ {}", decision.action)).weak(),
                                )
                                .truncate(),
                            );
                        });
                        ui.add_space(4.0);
                    });
                }
            }
        });
}
