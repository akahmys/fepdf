//! The menus a page offers: rotating, sheets, tab order, files, and the pages they act on.

use super::{FepdfApp, pages_in_hand};
use crate::app::page_ops::Run;
use crate::view::Act;
use std::collections::BTreeSet;

impl FepdfApp {
    /// What a right-click on a page offers, which is not the same in the two views.
    ///
    /// **One line per act, and the ways of doing it under it.** Rotating is three lines of
    /// one act, picking out a run is three of another, and inserting and extracting are two
    /// each: eleven entries to read past, of which a reader wanted one. The submenus are
    /// named for the act, so the list is what can be done here and the second level is how.
    ///
    /// **Turning a page upright is the only act both views answer.** Duplicating, deleting,
    /// inserting and extracting are all about a page's place among the others —
    /// [`Act::ArrangePages`] — and the page view shows one page with no others around it,
    /// so the menu that offered them there was offering to rearrange a document the reader
    /// could not see. Deleting is the exception it looks like: it is answered in both, and
    /// says what it would take.
    pub(super) fn render_page_context_menu(&mut self, response: &egui::Response, page_idx: usize) {
        response.context_menu(|ui| {
            // **No heading.** The menu opened with the number of the page it was on, which
            // is the one thing a reader right-clicking a page already knows — and in the
            // grid it named the page under the pointer while the entries below acted on
            // the selection, so the heading and the menu disagreed.
            // **Picking out several pages is the grid's**, and it is offered where the
            // pages are rather than only behind `Cmd+A`, which a reader who does not
            // already know it would never find (UI-4).
            if self.view.does(Act::SelectPages) {
                self.render_select_menu(ui);
            }
            if self.view.does(Act::RotatePages) {
                self.render_rotate_menu(ui, page_idx);
                self.render_tab_order_menu(ui, page_idx);
                self.render_sheet_menu(ui, page_idx);
            }

            // The copy that stays here, which is the one edit with no variants to hang
            // under a name of its own.
            let arranges = self.view.does(Act::ArrangePages);
            if arranges {
                ui.separator();
                if ui.button(self.tr("menu_duplicate_page")).clicked() {
                    self.duplicate_page(page_idx);
                    ui.close();
                }
            }

            // Documents in, and documents out: the last group is the one that changes which
            // document a page belongs to.
            let split = self.split_in_hand(page_idx);
            if arranges || split.is_some() {
                ui.separator();
            }
            if arranges {
                self.render_page_file_menu(ui, page_idx);
            }
            if let Some((label, taking)) = split
                && ui.button(label).clicked()
            {
                self.selected_pages = taking;
                self.extract_selected_pages(true);
                ui.close();
            }
        });
    }

    /// The runs of pages the grid can pick out, under one name.
    pub(super) fn render_select_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(self.tr("menu_select"), |ui| {
            for run in Run::ALL {
                if ui.button(self.tr(run.key())).clicked() {
                    self.select_run(run);
                    ui.close();
                }
            }
        });
    }

    /// The three quarters a page can be turned, under the name of what would turn.
    ///
    /// **It says which pages, because the two views turn different ones.** The page view
    /// turns the page being read — a selection carried in from the grid is not visible
    /// there and must not be what a menu on the page acts on — and the grid turns what is
    /// picked out, with the count, the way the entries below it do.
    pub(super) fn render_rotate_menu(&mut self, ui: &mut egui::Ui, page_idx: usize) {
        let turning = self.acting_on(page_idx);
        let name = if self.view.does(Act::SelectPages) {
            format!("{} ({})", self.tr("menu_rotate_selected"), turning.len())
        } else {
            self.tr("menu_rotate_this")
        };
        ui.menu_button(name, |ui| {
            for (key, quarter) in [
                ("menu_rotate_cw", fepdf::Quarter::Q90),
                ("menu_rotate_ccw", fepdf::Quarter::Q270),
                ("menu_rotate_180", fepdf::Quarter::Q180),
            ] {
                if ui.button(self.tr(key)).clicked() {
                    self.rotate_page_action(page_idx, quarter);
                    ui.close();
                }
            }
        });
    }

    /// Cutting the page into several and putting several onto one sheet (W-11, W-12).
    ///
    /// **Splitting is this page's; combining is the pages in hand's**, because a split
    /// makes pages where there was one and a combine makes one where there were several.
    pub(super) fn render_sheet_menu(&mut self, ui: &mut egui::Ui, page_idx: usize) {
        let combining = self.acting_on(page_idx);
        ui.menu_button(self.tr("menu_sheets"), |ui| {
            for (key, columns, rows) in [
                ("menu_split_halves_across", 2, 1),
                ("menu_split_halves_down", 1, 2),
                ("menu_split_quarters", 2, 2),
            ] {
                if ui.button(self.tr(key)).clicked() {
                    self.split_page(page_idx, columns, rows, key);
                    ui.close();
                }
            }
            ui.separator();
            // Two side by side go on the first page's sheet turned, so each keeps its
            // shape; four go two by two on the sheet as it is.
            let first = combining
                .iter()
                .next()
                .and_then(|p| self.doc_page_frames.get(*p))
                .map(|frame| frame.size());
            for (key, columns, rows, sheet) in [
                ("menu_combine_two", 2, 1, first.map(|(w, h)| (h, w))),
                ("menu_combine_four", 2, 2, None),
            ] {
                let label = format!("{} ({})", self.tr(key), combining.len());
                if ui.add_enabled(combining.len() > 1, egui::Button::new(label)).clicked() {
                    let onto = fepdf::PageArrangement { sheet, columns, rows };
                    self.combine_pages(&combining, onto, key);
                    ui.close();
                }
            }
        });
    }

    /// Cuts page `page` into `columns` by `rows` pages.
    pub(crate) fn split_page(&self, page: usize, columns: usize, rows: usize, key: &str) {
        let into = fepdf::PageDivision::Grid { columns, rows };
        self.apply_from_menu(fepdf::Operation::SplitPage { page, into }, key);
    }

    /// Puts `pages` onto sheets as `onto` arranges them.
    pub(crate) fn combine_pages(
        &self,
        pages: &BTreeSet<usize>,
        onto: fepdf::PageArrangement,
        key: &str,
    ) {
        let pages = fepdf::PageSelection::Indices(pages.iter().copied().collect());
        self.apply_from_menu(fepdf::Operation::CombinePages(pages, onto), key);
    }

    /// Sends `operation`, saying `key` when it is done.
    pub(super) fn apply_from_menu(&self, operation: fepdf::Operation, key: &str) {
        let _ = self.tx_worker.send(crate::worker::WorkerRequest::Apply {
            operation: Box::new(operation),
            done: self.tr(key),
        });
    }

    /// The order a reader's Tab key takes through the annotations and fields of the pages
    /// in hand (`/Tabs`).
    ///
    /// **Beside the turn, because both are the page's own setting** rather than something
    /// drawn on it, and both act on the same pages. Structure order comes first: it is the
    /// one PDF/UA asks of a page with annotations.
    pub(super) fn render_tab_order_menu(&mut self, ui: &mut egui::Ui, page_idx: usize) {
        let pages = self.acting_on(page_idx);
        let name = format!("{} ({})", self.tr("menu_tab_order"), pages.len());
        ui.menu_button(name, |ui| {
            for (key, order) in [
                ("menu_tab_structure", fepdf::TabOrder::Structure),
                ("menu_tab_row", fepdf::TabOrder::Row),
                ("menu_tab_column", fepdf::TabOrder::Column),
                ("menu_tab_annotations", fepdf::TabOrder::Annotations),
                ("menu_tab_widgets", fepdf::TabOrder::Widgets),
            ] {
                if ui.button(self.tr(key)).clicked() {
                    let pages = fepdf::PageSelection::Indices(pages.iter().copied().collect());
                    let _ = self.tx_worker.send(crate::worker::WorkerRequest::Apply {
                        operation: Box::new(fepdf::Operation::SetTabOrder { pages, order }),
                        done: self.tr("menu_tab_order"),
                    });
                    ui.close();
                }
            }
        });
    }

    /// The pages an entry reached from `page_idx` acts on.
    ///
    /// **In the page view that is the page being read, and nothing else.** A selection
    /// survives the trip in from the grid — checking a page before deleting it is the
    /// ordinary reason to zoom in — and it is invisible there, so an entry that acted on it
    /// would act on pages the reader cannot see while its own name said "this page".
    pub(crate) fn acting_on(&self, page_idx: usize) -> BTreeSet<usize> {
        if self.view.does(Act::SelectPages) {
            self.pages_in_hand(page_idx)
        } else {
            BTreeSet::from([page_idx])
        }
    }

    /// What the entry that takes pages out of the document would say, and what it would
    /// take — or nothing, where the view does not answer for it.
    ///
    /// **Asked before the separator above it is drawn**, because an entry that is not there
    /// leaves a rule with nothing under it.
    ///
    /// **It takes every page it is given, the whole document included.** Splitting all of
    /// them out is a document moving to a window of its own, which is what the reader asked
    /// for; the delete this replaced could not do it, and refused in silence.
    pub(super) fn split_in_hand(&self, page_idx: usize) -> Option<(String, BTreeSet<usize>)> {
        if !self.view.does(Act::SplitPages) {
            return None;
        }
        let taking = self.acting_on(page_idx);
        if !self.view.does(Act::SelectPages) {
            return Some((self.tr("menu_split_this_page"), taking));
        }
        Some((format!("{} ({})", self.tr("menu_split_selected"), taking.len()), taking))
    }

    /// Bringing another document in, and sending pages of this one out.
    ///
    /// **Its own group under a separator**, because these two are not edits to the page
    /// they are reached from: one adds pages beside it and the other writes a second
    /// file, and neither is the kind of thing the four above it are.
    pub(super) fn render_page_file_menu(&mut self, ui: &mut egui::Ui, page_idx: usize) {
        let taking = self.acting_on(page_idx);
        // **One place, which is where the pages in hand start.** It was before *or* after
        // the page clicked, two entries for a choice a reader makes by clicking one page
        // to the left; where the selection begins is the answer to both.
        if ui.button(self.tr("menu_insert_at_selection")).clicked() {
            let at = taking.iter().next().copied().unwrap_or(page_idx);
            self.insert_document_at(at);
            ui.close();
        }
        // **Extracting leaves the originals.** Taking them out is the split below, which is
        // the same operation with the pages removed here — one act each, rather than one
        // act and a question about what happens to what it took.
        if ui.button(format!("{} ({})", self.tr("menu_extract_selected"), taking.len())).clicked() {
            self.selected_pages.clone_from(&taking);
            self.extract_selected_pages(false);
            ui.close();
        }
        // The two that make a file from pages and put pages from a file in their place:
        // the page as a picture, and the page swapped for a corrected one.
        if ui.button(format!("{} ({})", self.tr("menu_images_selected"), taking.len())).clicked() {
            self.export_pages_as_images(&taking);
            ui.close();
        }
        if ui.button(format!("{} ({})", self.tr("menu_replace_selected"), taking.len())).clicked() {
            self.replace_pages(&taking);
            ui.close();
        }
    }

    /// The pages an entry reached from `page_idx` acts on. See [`pages_in_hand`].
    pub(super) fn pages_in_hand(&self, page_idx: usize) -> std::collections::BTreeSet<usize> {
        pages_in_hand(&self.selected_pages, page_idx)
    }
}
