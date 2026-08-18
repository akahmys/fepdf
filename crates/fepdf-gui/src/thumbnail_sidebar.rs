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

                if ui.input(|ins| ins.pointer.any_released()) {
                    egui::DragAndDrop::clear_payload(ui.ctx());
                }

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

                if ui.input(|ins| ins.pointer.any_released()) {
                    egui::DragAndDrop::clear_payload(ui.ctx());
                }

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
                let indicator_color = egui::Color32::from_rgb(0, 120, 215);
                let line_min_x = rect.min.x + 2.0;
                let line_max_x = rect.max.x - 2.0;
                ui.painter().line_segment(
                    [egui::pos2(line_min_x, indicator_y), egui::pos2(line_max_x, indicator_y)],
                    egui::Stroke::new(3.0_f32, indicator_color),
                );
                ui.painter().circle_filled(
                    egui::pos2(line_min_x, indicator_y),
                    4.0,
                    indicator_color,
                );
                ui.painter().circle_filled(
                    egui::pos2(line_max_x, indicator_y),
                    4.0,
                    indicator_color,
                );
            }

            if response.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), i);
            }

            let dragged_from = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|p| *p);
            let is_drag_source = dragged_from == Some(i)
                || (dragged_from.is_some()
                    && app.selected_pages.contains(&dragged_from.unwrap_or(usize::MAX))
                    && app.selected_pages.contains(&i));
            let reorder_target = Self::handle_page_reorder_drag(app, ui, rect, i, dragged_from);

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

            let menu_action = Self::render_thumbnail_context_menu(app, &response, i);
            if let Some(action) = menu_action {
                Self::handle_thumbnail_menu_action(app, action, i);
            } else if let Some((sources, target_insert_pos)) = reorder_target {
                app.reorder_pages_batch(&sources, target_insert_pos);
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
                is_drag_source,
            );
        });
        hovered_target
    }

    fn handle_page_reorder_drag(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        i: usize,
        dragged_from: Option<usize>,
    ) -> Option<(Vec<usize>, usize)> {
        let from_idx = dragged_from?;
        let pointer_pos = ui.input(|ins| ins.pointer.interact_pos().or(ins.pointer.latest_pos()));
        let pos = pointer_pos?;

        if !rect.contains(pos) {
            return None;
        }

        let sources: Vec<usize> = if app.selected_pages.contains(&from_idx) {
            app.selected_pages.iter().copied().collect()
        } else {
            vec![from_idx]
        };

        let is_bottom = pos.y > rect.center().y;
        let (indicator_y, target_insert_pos) =
            if !is_bottom { (rect.min.y, i) } else { (rect.max.y, i + 1) };

        let selected_before = sources.iter().filter(|&&idx| idx < target_insert_pos).count();
        let insert_idx_in_remaining = target_insert_pos.saturating_sub(selected_before);
        let is_identity = sources
            .iter()
            .enumerate()
            .all(|(offset, &orig)| orig == insert_idx_in_remaining + offset);

        if !is_identity {
            let indicator_color = egui::Color32::from_rgb(0, 120, 215);
            let line_min_x = rect.min.x + 2.0;
            let line_max_x = rect.max.x - 2.0;
            ui.painter().line_segment(
                [egui::pos2(line_min_x, indicator_y), egui::pos2(line_max_x, indicator_y)],
                egui::Stroke::new(3.5_f32, indicator_color),
            );
            ui.painter().circle_filled(egui::pos2(line_min_x, indicator_y), 4.0, indicator_color);
            ui.painter().circle_filled(egui::pos2(line_max_x, indicator_y), 4.0, indicator_color);

            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

            if ui.input(|ins| ins.pointer.any_released()) {
                return Some((sources, target_insert_pos));
            }
        }

        None
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
        is_drag_source: bool,
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

        if is_drag_source {
            ui.painter().rect_filled(
                mini_page_rect,
                2.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 150),
            );
            ui.painter().rect_stroke(
                mini_page_rect,
                2.0,
                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 120, 215)),
                egui::StrokeKind::Inside,
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
                let indicator_color = egui::Color32::from_rgb(0, 120, 215);
                let line_min_y = rect.min.y + 2.0;
                let line_max_y = rect.max.y - 2.0;
                ui.painter().line_segment(
                    [egui::pos2(indicator_x, line_min_y), egui::pos2(indicator_x, line_max_y)],
                    egui::Stroke::new(3.0_f32, indicator_color),
                );
                ui.painter().circle_filled(
                    egui::pos2(indicator_x, line_min_y),
                    4.0,
                    indicator_color,
                );
                ui.painter().circle_filled(
                    egui::pos2(indicator_x, line_max_y),
                    4.0,
                    indicator_color,
                );
            }

            if response.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), i);
            }

            let dragged_from = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|p| *p);
            let is_drag_source = dragged_from == Some(i)
                || (dragged_from.is_some()
                    && app.selected_pages.contains(&dragged_from.unwrap_or(usize::MAX))
                    && app.selected_pages.contains(&i));
            let reorder_target =
                Self::handle_page_reorder_drag_horizontal(app, ui, rect, i, dragged_from);

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

            let menu_action = Self::render_thumbnail_context_menu(app, &response, i);
            if let Some(action) = menu_action {
                Self::handle_thumbnail_menu_action(app, action, i);
            } else if let Some((sources, target_insert_pos)) = reorder_target {
                app.reorder_pages_batch(&sources, target_insert_pos);
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
                is_drag_source,
            );
        });
        hovered_target
    }

    fn handle_page_reorder_drag_horizontal(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        i: usize,
        dragged_from: Option<usize>,
    ) -> Option<(Vec<usize>, usize)> {
        let from_idx = dragged_from?;
        let pointer_pos = ui.input(|ins| ins.pointer.interact_pos().or(ins.pointer.latest_pos()));
        let pos = pointer_pos?;

        if !rect.contains(pos) {
            return None;
        }

        let sources: Vec<usize> = if app.selected_pages.contains(&from_idx) {
            app.selected_pages.iter().copied().collect()
        } else {
            vec![from_idx]
        };

        let is_right = pos.x > rect.center().x;
        let (indicator_x, target_insert_pos) =
            if !is_right { (rect.min.x, i) } else { (rect.max.x, i + 1) };

        let selected_before = sources.iter().filter(|&&idx| idx < target_insert_pos).count();
        let insert_idx_in_remaining = target_insert_pos.saturating_sub(selected_before);
        let is_identity = sources
            .iter()
            .enumerate()
            .all(|(offset, &orig)| orig == insert_idx_in_remaining + offset);

        if !is_identity {
            let indicator_color = egui::Color32::from_rgb(0, 120, 215);
            let line_min_y = rect.min.y + 2.0;
            let line_max_y = rect.max.y - 2.0;
            ui.painter().line_segment(
                [egui::pos2(indicator_x, line_min_y), egui::pos2(indicator_x, line_max_y)],
                egui::Stroke::new(3.5_f32, indicator_color),
            );
            ui.painter().circle_filled(egui::pos2(indicator_x, line_min_y), 4.0, indicator_color);
            ui.painter().circle_filled(egui::pos2(indicator_x, line_max_y), 4.0, indicator_color);

            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);

            if ui.input(|ins| ins.pointer.any_released()) {
                return Some((sources, target_insert_pos));
            }
        }

        None
    }

    fn render_thumbnail_context_menu(
        // RR-15 Limit: GUI - Render thumbnail context menu for page reordering, selection, and editing
        app: &crate::app::FepdfApp,
        response: &egui::Response,
        i: usize,
    ) -> Option<ThumbnailMenuAction> {
        let mut action = None;

        response.context_menu(|ui| {
            Self::render_select_submenu(app, ui, &mut action);
            Self::render_edit_submenu(app, ui, &mut action, i);
        });

        action
    }

    fn render_select_submenu(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        action: &mut Option<ThumbnailMenuAction>,
    ) {
        ui.menu_button(app.locale_mgr.tr(&app.active_language, "menu_select"), |ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_select_all")).clicked() {
                *action = Some(ThumbnailMenuAction::Select(ThumbnailSelectAction::All));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_select_odd")).clicked() {
                *action = Some(ThumbnailMenuAction::Select(ThumbnailSelectAction::Odd));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_select_even")).clicked() {
                *action = Some(ThumbnailMenuAction::Select(ThumbnailSelectAction::Even));
                ui.close_kind(egui::UiKind::Menu);
            }
            ui.separator();
            let has_selection = !app.selected_pages.is_empty();
            if ui
                .add_enabled(
                    has_selection,
                    egui::Button::new(app.locale_mgr.tr(&app.active_language, "menu_select_clear")),
                )
                .clicked()
            {
                *action = Some(ThumbnailMenuAction::Select(ThumbnailSelectAction::Clear));
                ui.close_kind(egui::UiKind::Menu);
            }
        });
    }

    fn is_contiguous_selection(selected: &std::collections::BTreeSet<usize>) -> bool {
        let (Some(&min), Some(&max)) = (selected.iter().min(), selected.iter().max()) else {
            return true;
        };
        max.saturating_sub(min) + 1 == selected.len()
    }

    fn render_edit_submenu(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        action: &mut Option<ThumbnailMenuAction>,
        i: usize,
    ) {
        let has_selection = !app.selected_pages.is_empty();
        ui.add_enabled_ui(has_selection, |ui| {
            ui.menu_button(app.locale_mgr.tr(&app.active_language, "menu_edit"), |ui| {
                Self::render_rotate_submenu(app, ui, action);
                if ui.button(app.locale_mgr.tr(&app.active_language, "menu_edit_scale")).clicked() {
                    *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Scale));
                    ui.close_kind(egui::UiKind::Menu);
                }
                Self::render_move_submenu(app, ui, action, i);
                Self::render_insert_submenu(app, ui, action);
                if ui.button(app.locale_mgr.tr(&app.active_language, "menu_edit_extract")).clicked()
                {
                    *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Extract));
                    ui.close_kind(egui::UiKind::Menu);
                }
                let can_replace = Self::is_contiguous_selection(&app.selected_pages);
                if ui
                    .add_enabled(
                        can_replace,
                        egui::Button::new(
                            app.locale_mgr.tr(&app.active_language, "menu_edit_replace"),
                        ),
                    )
                    .clicked()
                {
                    *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Replace));
                    ui.close_kind(egui::UiKind::Menu);
                }
                if ui
                    .button(app.locale_mgr.tr(&app.active_language, "menu_edit_duplicate"))
                    .clicked()
                {
                    *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Duplicate));
                    ui.close_kind(egui::UiKind::Menu);
                }
                ui.separator();
                if ui.button(app.locale_mgr.tr(&app.active_language, "menu_edit_delete")).clicked()
                {
                    *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Delete));
                    ui.close_kind(egui::UiKind::Menu);
                }
            });
        });
    }

    fn render_rotate_submenu(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        action: &mut Option<ThumbnailMenuAction>,
    ) {
        ui.menu_button(app.locale_mgr.tr(&app.active_language, "menu_edit_rotate"), |ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_edit_rotate_cw")).clicked() {
                *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Rotate(
                    fepdf::Quarter::Q90,
                )));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_edit_rotate_ccw")).clicked()
            {
                *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Rotate(
                    fepdf::Quarter::Q270,
                )));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_edit_rotate_180")).clicked()
            {
                *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::Rotate(
                    fepdf::Quarter::Q180,
                )));
                ui.close_kind(egui::UiKind::Menu);
            }
        });
    }

    fn render_insert_submenu(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        action: &mut Option<ThumbnailMenuAction>,
    ) {
        ui.menu_button(app.locale_mgr.tr(&app.active_language, "menu_edit_insert"), |ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "page_insert_file")).clicked() {
                *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::InsertFile));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "page_insert_blank")).clicked() {
                *action = Some(ThumbnailMenuAction::Edit(ThumbnailEditAction::InsertBlank));
                ui.close_kind(egui::UiKind::Menu);
            }
        });
    }

    fn render_move_submenu(
        app: &crate::app::FepdfApp,
        ui: &mut egui::Ui,
        action: &mut Option<ThumbnailMenuAction>,
        _i: usize,
    ) {
        ui.menu_button(app.locale_mgr.tr(&app.active_language, "menu_move"), |ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_move_first")).clicked() {
                *action = Some(ThumbnailMenuAction::Move(ThumbnailMoveAction::First));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_move_prev")).clicked() {
                *action = Some(ThumbnailMenuAction::Move(ThumbnailMoveAction::Prev));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_move_next")).clicked() {
                *action = Some(ThumbnailMenuAction::Move(ThumbnailMoveAction::Next));
                ui.close_kind(egui::UiKind::Menu);
            }
            if ui.button(app.locale_mgr.tr(&app.active_language, "menu_move_last")).clicked() {
                *action = Some(ThumbnailMenuAction::Move(ThumbnailMoveAction::Last));
                ui.close_kind(egui::UiKind::Menu);
            }
        });
    }

    fn handle_selection_menu_action(app: &mut crate::app::FepdfApp, action: ThumbnailSelectAction) {
        match action {
            ThumbnailSelectAction::All => {
                app.selected_pages = (0..app.total_pages).collect();
            }
            ThumbnailSelectAction::Even => {
                app.selected_pages =
                    (0..app.total_pages).filter(|&idx| (idx + 1) % 2 == 0).collect();
            }
            ThumbnailSelectAction::Odd => {
                app.selected_pages =
                    (0..app.total_pages).filter(|&idx| (idx + 1) % 2 != 0).collect();
            }
            ThumbnailSelectAction::Clear => {
                app.selected_pages.clear();
            }
        }
    }

    fn handle_edit_menu_action(
        app: &mut crate::app::FepdfApp,
        action: ThumbnailEditAction,
        i: usize,
    ) {
        match action {
            ThumbnailEditAction::Rotate(delta) => {
                app.rotate_page_action(i, delta);
            }
            ThumbnailEditAction::Scale | ThumbnailEditAction::InsertBlank => {}
            ThumbnailEditAction::Replace => {
                let (start_idx, count) = if app.selected_pages.is_empty() {
                    (i, 1)
                } else {
                    let min = *app.selected_pages.iter().min().unwrap_or(&i);
                    (min, app.selected_pages.len())
                };
                Self::replace_document_from_file(app, start_idx, count);
            }
            ThumbnailEditAction::Duplicate => {
                app.duplicate_page(i);
            }
            ThumbnailEditAction::InsertFile => {
                Self::insert_document_from_file(app, i + 1);
            }
            ThumbnailEditAction::Extract => {
                let indices = if app.selected_pages.contains(&i) {
                    app.selected_pages.iter().copied().collect()
                } else {
                    vec![i]
                };
                let _ = app.tx_worker.send(crate::worker::WorkerRequest::ExtractPages { indices });
            }
            ThumbnailEditAction::Delete => {
                if !app.selected_pages.contains(&i) {
                    app.selected_pages.clear();
                    app.selected_pages.insert(i);
                }
                app.remove_selected_pages();
            }
        }
    }

    fn replace_document_from_file(app: &mut crate::app::FepdfApp, at_index: usize, count: usize) {
        if let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
            && let Ok(bytes) = std::fs::read(&path)
        {
            let _ = app.tx_worker.send(crate::worker::WorkerRequest::ReplaceDocument {
                data: bytes::Bytes::from(bytes),
                at_index,
                count,
            });
        }
    }

    fn handle_move_menu_action(
        app: &mut crate::app::FepdfApp,
        action: ThumbnailMoveAction,
        i: usize,
    ) {
        let sources: Vec<usize> = if app.selected_pages.contains(&i) {
            app.selected_pages.iter().copied().collect()
        } else {
            vec![i]
        };
        if sources.is_empty() {
            return;
        }
        let total = app.total_pages;
        let min_selected = *sources.iter().min().unwrap_or(&0);
        let max_selected = *sources.iter().max().unwrap_or(&0);

        match action {
            ThumbnailMoveAction::First => {
                app.reorder_pages_batch(&sources, 0);
            }
            ThumbnailMoveAction::Last => {
                app.reorder_pages_batch(&sources, total);
            }
            ThumbnailMoveAction::Prev => {
                if min_selected > 0 {
                    let prev_target = (0..min_selected)
                        .rev()
                        .find(|idx| !app.selected_pages.contains(idx))
                        .unwrap_or(0);
                    app.reorder_pages_batch(&sources, prev_target);
                }
            }
            ThumbnailMoveAction::Next => {
                if max_selected < total.saturating_sub(1) {
                    let next_target = (max_selected + 1..total)
                        .find(|idx| !app.selected_pages.contains(idx))
                        .map_or(total, |idx| idx + 1);
                    app.reorder_pages_batch(&sources, next_target);
                }
            }
        }
    }

    fn insert_document_from_file(app: &mut crate::app::FepdfApp, at_index: usize) {
        if let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
            && let Ok(bytes) = std::fs::read(&path)
        {
            let _ = app.tx_worker.send(crate::worker::WorkerRequest::InsertDocument {
                data: bytes::Bytes::from(bytes),
                at_index,
            });
        }
    }

    fn handle_thumbnail_menu_action(
        app: &mut crate::app::FepdfApp,
        action: ThumbnailMenuAction,
        i: usize,
    ) {
        match action {
            ThumbnailMenuAction::Select(select_action) => {
                Self::handle_selection_menu_action(app, select_action);
            }
            ThumbnailMenuAction::Edit(edit_action) => {
                Self::handle_edit_menu_action(app, edit_action, i);
            }
            ThumbnailMenuAction::Move(move_action) => {
                Self::handle_move_menu_action(app, move_action, i);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailSelectAction {
    All,
    Even,
    Odd,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailEditAction {
    Rotate(fepdf::Quarter),
    Scale,
    Duplicate,
    InsertFile,
    InsertBlank,
    Replace,
    Extract,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailMoveAction {
    First,
    Prev,
    Next,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailMenuAction {
    Select(ThumbnailSelectAction),
    Edit(ThumbnailEditAction),
    Move(ThumbnailMoveAction),
}
