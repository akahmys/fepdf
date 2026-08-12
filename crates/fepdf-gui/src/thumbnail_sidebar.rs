// RR-15 Limit: GUI - Thumbnail Sidebar panel definition and interaction
pub struct ThumbnailSidebar;

impl ThumbnailSidebar {
    pub fn show(
        // RR-15 Limit: GUI - Thumbnail Sidebar panel layout and page actions
        app: &mut crate::app::FepdfApp,
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
                let panel_rect = ui.max_rect();
                if ui.input(|ins| {
                    ins.key_pressed(egui::Key::Delete) || ins.key_pressed(egui::Key::Backspace)
                }) && !app.selected_pages.is_empty()
                    && app.total_pages > 1
                {
                    app.remove_selected_pages();
                }

                let mut hovered_item_target = None;
                egui::ScrollArea::vertical().id_salt("thumbnail_scroll_area").hscroll(false).show(
                    ui,
                    |ui| {
                        if app.total_pages > 0 {
                            for i in 0..app.total_pages {
                                if let Some(target) = Self::show_thumbnail_item(app, ui, frame, i) {
                                    hovered_item_target = Some(target);
                                }
                            }
                        }
                        ui.add_space(16.0);
                    },
                );

                let target_index = hovered_item_target.unwrap_or(app.total_pages);
                Self::handle_external_drop(app, ui, panel_rect, target_index);
                Self::render_external_hover(app, ui, panel_rect);
            });
    }

    pub fn show_horizontal(
        app: &mut crate::app::FepdfApp,
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
                let panel_rect = ui.max_rect();
                if ui.input(|ins| {
                    ins.key_pressed(egui::Key::Delete) || ins.key_pressed(egui::Key::Backspace)
                }) && !app.selected_pages.is_empty()
                    && app.total_pages > 1
                {
                    app.remove_selected_pages();
                }

                let mut hovered_item_target = None;
                egui::ScrollArea::horizontal()
                    .id_salt("thumbnail_horizontal_scroll")
                    .vscroll(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if app.total_pages > 0 {
                                for i in 0..app.total_pages {
                                    if let Some(target) =
                                        Self::show_thumbnail_item_horizontal(app, ui, frame, i)
                                    {
                                        hovered_item_target = Some(target);
                                    }
                                }
                            }
                            ui.add_space(16.0);
                        });
                    });

                let target_index = hovered_item_target.unwrap_or(app.total_pages);
                Self::handle_external_drop(app, ui, panel_rect, target_index);
                Self::render_external_hover(app, ui, panel_rect);
            });
    }

    fn handle_external_drop(
        app: &mut crate::app::FepdfApp,
        ui: &mut egui::Ui,
        panel_rect: egui::Rect,
        target_index: usize,
    ) {
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let is_hovered =
            ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| panel_rect.contains(p));
        if is_hovered {
            for file in dropped {
                let bytes_opt = if let Some(ref path) = file.path {
                    std::fs::read(path).ok()
                } else {
                    file.bytes.as_ref().map(|b| b.to_vec())
                };
                if let Some(bytes) = bytes_opt {
                    let _ = app.tx_worker.send(crate::worker::WorkerRequest::InsertDocument {
                        data: bytes::Bytes::from(bytes),
                        at_index: target_index,
                    });
                }
            }
        }
    }

    fn render_external_hover(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        panel_rect: egui::Rect,
    ) {
        let hovered = ui.input(|i| i.raw.hovered_files.clone());
        if hovered.is_empty() {
            return;
        }
        let is_hovered =
            ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| panel_rect.contains(p));
        if is_hovered {
            ui.painter().rect_filled(
                panel_rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(0, 120, 215, 30),
            );
            ui.painter().rect_stroke(
                panel_rect.shrink(2.0),
                2.0_f32,
                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 120, 215)),
                egui::StrokeKind::Inside,
            );
            let msg = if app.total_pages == 0 {
                app.locale_mgr.tr(&app.active_language, "drop_to_open_pdf")
            } else {
                app.locale_mgr.tr(&app.active_language, "drop_to_insert_pages")
            };
            ui.painter().text(
                panel_rect.center(),
                egui::Align2::CENTER_CENTER,
                msg,
                egui::FontId::proportional(13.0),
                egui::Color32::from_rgb(0, 80, 180),
            );
        }
    }

    fn show_thumbnail_item(
        // RR-15 Limit: GUI - Render individual page thumbnail item and handle click interaction
        app: &mut crate::app::FepdfApp,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        i: usize,
    ) -> Option<usize> {
        let (size, layout_rect) = {
            let layout = app.page_layouts.get(i)?;
            (layout.rect.size(), layout.rect)
        };
        let aspect_ratio = size.y / size.x;
        let is_visible = app.view.visible_pages.contains(&i);
        let is_selected = app.selected_pages.contains(&i);
        let mut hovered_target = None;

        ui.vertical_centered(|ui| {
            ui.add_space(1.0);

            let sidebar_width = ui.available_width();
            let mini_page_width = (sidebar_width - 50.0).clamp(110.0, 250.0);
            let mini_page_height = mini_page_width * aspect_ratio;

            let (rect, response) = ui.allocate_at_least(
                egui::vec2(sidebar_width - 20.0, mini_page_height + 26.0),
                egui::Sense::click_and_drag(),
            );

            let hovered_ext = !ui.input(|ins| ins.raw.hovered_files.is_empty());
            if response.hovered() {
                let mouse_pos = ui.input(|ins| ins.pointer.hover_pos());
                if let Some(pos) = mouse_pos {
                    hovered_target = if pos.y > rect.center().y { Some(i + 1) } else { Some(i) };
                } else {
                    hovered_target = Some(i);
                }
            }

            if hovered_ext && response.hovered() {
                let indicator_y =
                    if hovered_target == Some(i + 1) { rect.max.y } else { rect.min.y };
                ui.painter().line_segment(
                    [egui::pos2(rect.min.x, indicator_y), egui::pos2(rect.max.x, indicator_y)],
                    egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(0, 120, 215)),
                );
            }

            if response.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), i);
            }

            let dragged_from = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|p| *p);
            let mut reorder_target = None;

            if let Some(from_idx) = dragged_from
                && from_idx != i
                && response.hovered()
            {
                let indicator_y = if from_idx < i { rect.max.y } else { rect.min.y };
                ui.painter().line_segment(
                    [egui::pos2(rect.min.x, indicator_y), egui::pos2(rect.max.x, indicator_y)],
                    egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(0, 120, 215)),
                );

                if ui.input(|ins| ins.pointer.any_released()) {
                    reorder_target = Some((from_idx, i));
                }
            }

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

            let (action_rotate, action_move, action_duplicate, action_extract, action_delete) =
                Self::render_thumbnail_context_menu(app, &response, i);

            if let Some(delta) = action_rotate {
                app.rotate_page_action(i, delta);
            } else if let Some((from_idx, target_idx)) = reorder_target {
                app.reorder_page(from_idx, target_idx);
            } else if let Some((from_idx, target_idx)) = action_move {
                app.reorder_page(from_idx, target_idx);
            } else if action_duplicate {
                app.duplicate_page(i);
            } else if action_extract {
                let indices = if app.selected_pages.contains(&i) {
                    app.selected_pages.iter().copied().collect()
                } else {
                    vec![i]
                };
                let _ = app.tx_worker.send(crate::worker::WorkerRequest::ExtractPages { indices });
            } else if action_delete {
                app.selected_pages.clear();
                app.selected_pages.insert(i);
                app.remove_selected_pages();
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

            let visible_mask_rect = if is_visible {
                Self::compute_visible_mask_rect(app, layout_rect, mini_page_rect)
            } else {
                None
            };

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
        hovered_target
    }

    /// Maps the portion of the page currently inside the viewport onto the thumbnail,
    /// producing the shaded "you are here" overlay rectangle.
    fn compute_visible_mask_rect(
        app: &crate::app::FepdfApp,
        layout_rect: egui::Rect,
        mini_page_rect: egui::Rect,
    ) -> Option<egui::Rect> {
        let viewport_rect = app.last_viewport_rect?;
        let origin = app.view.get_origin(viewport_rect);
        let page_rect = egui::Rect::from_min_size(
            origin + layout_rect.min.to_vec2() * app.view.zoom,
            layout_rect.size() * app.view.zoom,
        );
        let intersection = viewport_rect.intersect(page_rect);
        if !intersection.is_positive() {
            return None;
        }

        let x_min = ((intersection.min.x - page_rect.min.x) / page_rect.width()).clamp(0.0, 1.0);
        let x_max = ((intersection.max.x - page_rect.min.x) / page_rect.width()).clamp(0.0, 1.0);
        let y_min = ((intersection.min.y - page_rect.min.y) / page_rect.height()).clamp(0.0, 1.0);
        let y_max = ((intersection.max.y - page_rect.min.y) / page_rect.height()).clamp(0.0, 1.0);

        let mask_min = egui::pos2(
            x_min.mul_add(mini_page_rect.width(), mini_page_rect.min.x),
            y_min.mul_add(mini_page_rect.height(), mini_page_rect.min.y),
        );
        let mask_max = egui::pos2(
            x_max.mul_add(mini_page_rect.width(), mini_page_rect.min.x),
            y_max.mul_add(mini_page_rect.height(), mini_page_rect.min.y),
        );
        Some(egui::Rect::from_min_max(mask_min, mask_max))
    }

    fn render_thumbnail_graphics(
        // RR-15 Limit: GUI - Render actual thumbnail image or loader on sidebar
        app: &mut crate::app::FepdfApp,
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
        let rendered_thumb = if let (Some(r), Some(rs)) =
            (&mut app.vello_renderer, frame.wgpu_render_state())
            && let Some(scene) = app.scenes.get(&i)
            && let Some(tex_id) = r.render_thumbnail(rs, i, scene, size, 256)
        {
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
            ui.painter().rect_stroke(mini_page_rect, 2.0, page_stroke, egui::StrokeKind::Inside);
            true
        } else {
            false
        };

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

    // RR-15 Limit: GUI
    fn show_thumbnail_item_horizontal(
        // RR-15 Limit: GUI
        app: &mut crate::app::FepdfApp,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        i: usize,
    ) -> Option<usize> {
        let (size, layout_rect) = {
            let layout = app.page_layouts.get(i)?;
            (layout.rect.size(), layout.rect)
        };
        let aspect_ratio = size.y / size.x;
        let is_visible = app.view.visible_pages.contains(&i);
        let is_selected = app.selected_pages.contains(&i);
        let mut hovered_target = None;

        ui.vertical(|ui| {
            ui.add_space(1.0);

            let sidebar_height = ui.available_height();
            let mini_page_height = (sidebar_height - 30.0).clamp(50.0, 120.0);
            let mini_page_width = mini_page_height / aspect_ratio;

            let (rect, response) = ui.allocate_at_least(
                egui::vec2(mini_page_width + 16.0, sidebar_height - 10.0),
                egui::Sense::click_and_drag(),
            );

            let hovered_ext = !ui.input(|ins| ins.raw.hovered_files.is_empty());
            if response.hovered() {
                let mouse_pos = ui.input(|ins| ins.pointer.hover_pos());
                if let Some(pos) = mouse_pos {
                    hovered_target = if pos.x > rect.center().x { Some(i + 1) } else { Some(i) };
                } else {
                    hovered_target = Some(i);
                }
            }

            if hovered_ext && response.hovered() {
                let indicator_x =
                    if hovered_target == Some(i + 1) { rect.max.x } else { rect.min.x };
                ui.painter().line_segment(
                    [egui::pos2(indicator_x, rect.min.y), egui::pos2(indicator_x, rect.max.y)],
                    egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(0, 120, 215)),
                );
            }

            if response.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), i);
            }

            let dragged_from = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|p| *p);
            let mut reorder_target = None;

            if let Some(from_idx) = dragged_from
                && from_idx != i
                && response.hovered()
            {
                let indicator_x = if from_idx < i { rect.max.x } else { rect.min.x };
                ui.painter().line_segment(
                    [egui::pos2(indicator_x, rect.min.y), egui::pos2(indicator_x, rect.max.y)],
                    egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(0, 120, 215)),
                );

                if ui.input(|ins| ins.pointer.any_released()) {
                    reorder_target = Some((from_idx, i));
                }
            }

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

            let (action_rotate, action_move, action_duplicate, action_extract, action_delete) =
                Self::render_thumbnail_context_menu(app, &response, i);

            if let Some(delta) = action_rotate {
                app.rotate_page_action(i, delta);
            } else if let Some((from_idx, target_idx)) = reorder_target {
                app.reorder_page(from_idx, target_idx);
            } else if let Some((from_idx, target_idx)) = action_move {
                app.reorder_page(from_idx, target_idx);
            } else if action_duplicate {
                app.duplicate_page(i);
            } else if action_extract {
                let indices = if app.selected_pages.contains(&i) {
                    app.selected_pages.iter().copied().collect()
                } else {
                    vec![i]
                };
                let _ = app.tx_worker.send(crate::worker::WorkerRequest::ExtractPages { indices });
            } else if action_delete {
                app.selected_pages.clear();
                app.selected_pages.insert(i);
                app.remove_selected_pages();
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

            let visible_mask_rect = if is_visible {
                Self::compute_visible_mask_rect(app, layout_rect, mini_page_rect)
            } else {
                None
            };

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
        hovered_target
    }

    fn render_thumbnail_context_menu(
        // RR-15 Limit: GUI - Render thumbnail context menu for page reordering and rotation
        app: &crate::app::FepdfApp,
        response: &egui::Response,
        i: usize,
    ) -> (Option<fepdf_sdk::Quarter>, Option<(usize, usize)>, bool, bool, bool) {
        let mut action_rotate = None;
        let mut action_move = None;
        let mut action_duplicate = false;
        let mut action_extract = false;
        let mut action_delete = false;

        response.context_menu(|ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "rotate_right_90")).clicked() {
                action_rotate = Some(fepdf_sdk::Quarter::Q90);
                ui.close_kind(egui::UiKind::Menu);
            }
            ui.separator();
            if ui.button(app.locale_mgr.tr(&app.active_language, "page_setup_menu")).clicked() {
                // Future modal open for scale setup
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "page_duplicate")).clicked() {
                action_duplicate = true;
                ui.close_kind(egui::UiKind::Menu);
            }
            ui.menu_button(app.locale_mgr.tr(&app.active_language, "page_insert_menu"), |ui| {
                if ui.button(app.locale_mgr.tr(&app.active_language, "page_insert_file")).clicked()
                {
                    ui.close_kind(egui::UiKind::Menu);
                }
                if ui.button(app.locale_mgr.tr(&app.active_language, "page_insert_blank")).clicked()
                {
                    ui.close_kind(egui::UiKind::Menu);
                }
            });
            if ui.button(app.locale_mgr.tr(&app.active_language, "page_replace_menu")).clicked() {
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "page_extract_menu")).clicked() {
                action_extract = true;
                ui.close_kind(egui::UiKind::Menu);
            }
            ui.separator();

            // 4 Move Options: Move to Top, Move Previous, Move Next, Move to Bottom
            let can_move_top = i > 0;
            let can_move_prev = i > 0;
            let can_move_next = i < app.total_pages.saturating_sub(1);
            let can_move_bottom = i < app.total_pages.saturating_sub(1);

            let top_btn = ui.add_enabled(
                can_move_top,
                egui::Button::new(app.locale_mgr.tr(&app.active_language, "reorder_move_top")),
            );
            if top_btn.clicked() {
                action_move = Some((i, 0));
                ui.close_kind(egui::UiKind::Menu);
            }

            let prev_btn = ui.add_enabled(
                can_move_prev,
                egui::Button::new(app.locale_mgr.tr(&app.active_language, "reorder_move_up")),
            );
            if prev_btn.clicked() {
                action_move = Some((i, i - 1));
                ui.close_kind(egui::UiKind::Menu);
            }

            let next_btn = ui.add_enabled(
                can_move_next,
                egui::Button::new(app.locale_mgr.tr(&app.active_language, "reorder_move_down")),
            );
            if next_btn.clicked() {
                action_move = Some((i, i + 1));
                ui.close_kind(egui::UiKind::Menu);
            }

            let bottom_btn = ui.add_enabled(
                can_move_bottom,
                egui::Button::new(app.locale_mgr.tr(&app.active_language, "reorder_move_bottom")),
            );
            if bottom_btn.clicked() {
                action_move = Some((i, app.total_pages.saturating_sub(1)));
                ui.close_kind(egui::UiKind::Menu);
            }

            if app.total_pages > 1 {
                ui.separator();
                if ui
                    .button(app.locale_mgr.tr(&app.active_language, "reorder_delete_page"))
                    .clicked()
                {
                    action_delete = true;
                    ui.close_kind(egui::UiKind::Menu);
                }
            }
        });

        (action_rotate, action_move, action_duplicate, action_extract, action_delete)
    }
}
