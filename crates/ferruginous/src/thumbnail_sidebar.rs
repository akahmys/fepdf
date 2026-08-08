// RR-15 Limit: GUI - Thumbnail Sidebar panel definition and interaction
pub struct ThumbnailSidebar;

impl ThumbnailSidebar {
    pub fn show(
        app: &mut crate::app::FerruginousApp,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
    ) {
        let panel_frame =
            egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(235, 237, 240));

        egui::Panel::right("thumbnail_sidebar")
            .resizable(true)
            .show_separator_line(true)
            .default_size(200.0)
            .size_range(160.0..=300.0)
            .frame(panel_frame)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("thumbnail_scroll_area").hscroll(false).show(
                    ui,
                    |ui| {
                        if app.total_pages > 0 {
                            for i in 0..app.total_pages {
                                Self::show_thumbnail_item(app, ui, frame, i);
                            }
                        }
                        ui.add_space(16.0);
                    },
                );
            });
    }

    pub fn show_horizontal(
        app: &mut crate::app::FerruginousApp,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
    ) {
        let panel_frame =
            egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(235, 237, 240));

        egui::Panel::bottom("thumbnail_sidebar_horizontal")
            .resizable(true)
            .show_separator_line(true)
            .default_size(130.0)
            .size_range(100.0..=180.0)
            .frame(panel_frame)
            .show_inside(ui, |ui| {
                egui::ScrollArea::horizontal().id_salt("thumbnail_horizontal_scroll").vscroll(false).show(
                    ui,
                    |ui| {
                        ui.horizontal(|ui| {
                            if app.total_pages > 0 {
                                for i in 0..app.total_pages {
                                    Self::show_thumbnail_item_horizontal(app, ui, frame, i);
                                }
                            }
                            ui.add_space(16.0);
                        });
                    },
                );
            });
    }

    fn show_thumbnail_item( // RR-15 Limit: GUI - Render individual page thumbnail item and handle click interaction
        app: &mut crate::app::FerruginousApp,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        i: usize,
    ) {
        let (size, layout_rect) = {
            let Some(layout) = app.page_layouts.get(i) else {
                return;
            };
            (layout.rect.size(), layout.rect)
        };
        let aspect_ratio = size.y / size.x;
        let is_visible = app.view.visible_pages.contains(&i);
        let is_selected = app.selected_pages.contains(&i);

        ui.vertical_centered(|ui| {
            ui.add_space(1.0);

            let sidebar_width = ui.available_width();
            let mini_page_width = (sidebar_width - 50.0).clamp(110.0, 250.0);
            let mini_page_height = mini_page_width * aspect_ratio;

            let (rect, response) = ui.allocate_at_least(
                egui::vec2(sidebar_width - 20.0, mini_page_height + 26.0),
                egui::Sense::click(),
            );

            if response.clicked() {
                let shift = ui.input(|ins| ins.modifiers.shift);
                let cmd = ui.input(|ins| ins.modifiers.command || ins.modifiers.ctrl);

                if shift {
                    if let Some(start) = app.last_selected_page {
                        app.selected_pages.clear();
                        let min = start.min(i);
                        let max = start.max(i);
                        for page_idx in min..=max {
                            app.selected_pages.insert(page_idx);
                        }
                    } else {
                        app.selected_pages.clear();
                        app.selected_pages.insert(i);
                        app.last_selected_page = Some(i);
                    }
                } else if cmd {
                    if app.selected_pages.contains(&i) {
                        app.selected_pages.remove(&i);
                    } else {
                        app.selected_pages.insert(i);
                    }
                    app.last_selected_page = Some(i);
                } else {
                    app.selected_pages.clear();
                    app.selected_pages.insert(i);
                    app.last_selected_page = Some(i);
                    app.view.scroll_to_page(i, &app.page_layouts);
                }
            }

            let page_stroke = if is_selected {
                egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(80, 90, 105))
            } else {
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(200, 205, 212))
            };

            let mini_page_rect = egui::Rect::from_center_size(
                rect.center() - egui::vec2(0.0, 7.0),
                egui::vec2(mini_page_width, mini_page_height),
            );

            let mut visible_mask_rect = None;
            if is_visible {
                if let Some(viewport_rect) = app.last_viewport_rect {
                    let origin = app.view.get_origin(viewport_rect);
                    let page_rect = egui::Rect::from_min_size(
                        origin + layout_rect.min.to_vec2() * app.view.zoom,
                        layout_rect.size() * app.view.zoom,
                    );
                    let intersection = viewport_rect.intersect(page_rect);
                    if intersection.is_positive() {
                        let x_min = ((intersection.min.x - page_rect.min.x) / page_rect.width())
                            .clamp(0.0, 1.0);
                        let x_max = ((intersection.max.x - page_rect.min.x) / page_rect.width())
                            .clamp(0.0, 1.0);
                        let y_min = ((intersection.min.y - page_rect.min.y) / page_rect.height())
                            .clamp(0.0, 1.0);
                        let y_max = ((intersection.max.y - page_rect.min.y) / page_rect.height())
                            .clamp(0.0, 1.0);

                        let mask_min = egui::pos2(
                            mini_page_rect.min.x + x_min * mini_page_rect.width(),
                            mini_page_rect.min.y + y_min * mini_page_rect.height(),
                        );
                        let mask_max = egui::pos2(
                            mini_page_rect.min.x + x_max * mini_page_rect.width(),
                            mini_page_rect.min.y + y_max * mini_page_rect.height(),
                        );
                        visible_mask_rect = Some(egui::Rect::from_min_max(mask_min, mask_max));
                    }
                }
            }

            Self::render_thumbnail_graphics(
                app,
                ui,
                frame,
                i,
                rect,
                page_stroke,
                mini_page_rect,
                visible_mask_rect,
                size,
                is_selected,
                is_visible,
            );
        });
    }

    fn render_thumbnail_graphics( // RR-15 Limit: GUI - Render actual thumbnail image or loader on sidebar
        app: &mut crate::app::FerruginousApp,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        i: usize,
        rect: egui::Rect,
        page_stroke: egui::Stroke,
        mini_page_rect: egui::Rect,
        visible_mask_rect: Option<egui::Rect>,
        size: egui::Vec2,
        is_selected: bool,
        is_visible: bool,
    ) {
        let mut rendered_thumb = false;
        if let (Some(r), Some(rs)) = (&mut app.vello_renderer, frame.wgpu_render_state()) {
            if let Some(scene) = app.scenes.get(&i) {
                if let Some(tex_id) = r.render_thumbnail(rs, i, scene, size, 256) {
                    ui.painter().image(
                        tex_id,
                        mini_page_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    if let Some(mask) = visible_mask_rect {
                        ui.painter().rect_filled(
                            mask,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(120, 125, 135, 45),
                        );
                    }
                    ui.painter().rect_stroke(
                        mini_page_rect,
                        2.0,
                        page_stroke,
                        egui::StrokeKind::Inside,
                    );
                    rendered_thumb = true;
                }
            }
        }

        if !rendered_thumb {
            ui.painter().rect_filled(mini_page_rect, 2.0, egui::Color32::WHITE);
            if let Some(mask) = visible_mask_rect {
                ui.painter().rect_filled(
                    mask,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(120, 125, 135, 45),
                );
            }
            ui.painter().rect_stroke(mini_page_rect, 2.0, page_stroke, egui::StrokeKind::Inside);
            ui.painter().text(
                mini_page_rect.center(),
                egui::Align2::CENTER_CENTER,
                "⌛",
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(150, 155, 165),
            );
        }

        let font_id = egui::FontId::proportional(11.0);
        let text_color = if is_selected {
            egui::Color32::from_rgb(50, 55, 65)
        } else if is_visible {
            egui::Color32::from_rgb(90, 100, 110)
        } else {
            egui::Color32::from_rgb(140, 145, 155)
        };
        ui.painter().text(
            egui::pos2(rect.center().x, rect.max.y - 8.0),
            egui::Align2::CENTER_CENTER,
            format!("Page {}", i + 1),
            font_id,
            text_color,
        );
    }

    fn show_thumbnail_item_horizontal(
        app: &mut crate::app::FerruginousApp,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        i: usize,
    ) {
        let (size, layout_rect) = {
            let Some(layout) = app.page_layouts.get(i) else {
                return;
            };
            (layout.rect.size(), layout.rect)
        };
        let aspect_ratio = size.y / size.x;
        let is_visible = app.view.visible_pages.contains(&i);
        let is_selected = app.selected_pages.contains(&i);

        ui.vertical(|ui| {
            ui.add_space(1.0);

            let sidebar_height = ui.available_height();
            let mini_page_height = (sidebar_height - 30.0).clamp(50.0, 120.0);
            let mini_page_width = mini_page_height / aspect_ratio;

            let (rect, response) = ui.allocate_at_least(
                egui::vec2(mini_page_width + 16.0, sidebar_height - 10.0),
                egui::Sense::click(),
            );

            if response.clicked() {
                let shift = ui.input(|ins| ins.modifiers.shift);
                let cmd = ui.input(|ins| ins.modifiers.command || ins.modifiers.ctrl);

                if shift {
                    if let Some(start) = app.last_selected_page {
                        app.selected_pages.clear();
                        let min = start.min(i);
                        let max = start.max(i);
                        for page_idx in min..=max {
                            app.selected_pages.insert(page_idx);
                        }
                    } else {
                        app.selected_pages.clear();
                        app.selected_pages.insert(i);
                        app.last_selected_page = Some(i);
                    }
                } else if cmd {
                    if app.selected_pages.contains(&i) {
                        app.selected_pages.remove(&i);
                    } else {
                        app.selected_pages.insert(i);
                    }
                    app.last_selected_page = Some(i);
                } else {
                    app.selected_pages.clear();
                    app.selected_pages.insert(i);
                    app.last_selected_page = Some(i);
                    app.view.scroll_to_page(i, &app.page_layouts);
                }
            }

            let page_stroke = if is_selected {
                egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(80, 90, 105))
            } else {
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(200, 205, 212))
            };

            let vertical_center = rect.min.y + (rect.height() - 20.0) / 2.0;
            let mini_page_rect = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, vertical_center),
                egui::vec2(mini_page_width, mini_page_height),
            );

            let mut visible_mask_rect = None;
            if is_visible {
                if let Some(viewport_rect) = app.last_viewport_rect {
                    let origin = app.view.get_origin(viewport_rect);
                    let page_rect = egui::Rect::from_min_size(
                        origin + layout_rect.min.to_vec2() * app.view.zoom,
                        layout_rect.size() * app.view.zoom,
                    );
                    let intersection = viewport_rect.intersect(page_rect);
                    if intersection.is_positive() {
                        let x_min = ((intersection.min.x - page_rect.min.x) / page_rect.width())
                            .clamp(0.0, 1.0);
                        let x_max = ((intersection.max.x - page_rect.min.x) / page_rect.width())
                            .clamp(0.0, 1.0);
                        let y_min = ((intersection.min.y - page_rect.min.y) / page_rect.height())
                            .clamp(0.0, 1.0);
                        let y_max = ((intersection.max.y - page_rect.min.y) / page_rect.height())
                            .clamp(0.0, 1.0);

                        let mask_min = egui::pos2(
                            mini_page_rect.min.x + x_min * mini_page_rect.width(),
                            mini_page_rect.min.y + y_min * mini_page_rect.height(),
                        );
                        let mask_max = egui::pos2(
                            mini_page_rect.min.x + x_max * mini_page_rect.width(),
                            mini_page_rect.min.y + y_max * mini_page_rect.height(),
                        );
                        visible_mask_rect = Some(egui::Rect::from_min_max(mask_min, mask_max));
                    }
                }
            }

            Self::render_thumbnail_graphics(
                app,
                ui,
                frame,
                i,
                rect,
                page_stroke,
                mini_page_rect,
                visible_mask_rect,
                size,
                is_selected,
                is_visible,
            );
        });
    }
}
