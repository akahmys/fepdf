//! Document lifecycle and page operations for `FepdfApp`.

use super::FepdfApp;
use crate::interaction::PendingTagRequest;
use crate::sidebar::USTNode;
use crate::worker::WorkerRequest;
use std::collections::BTreeSet;
use std::path::PathBuf;

impl FepdfApp {
    pub(crate) fn inject_tag_to_tree(&mut self, tag: &str, req: &PendingTagRequest) {
        let new_node = USTNode {
            id: self.ust_registry.next_node_id,
            tag: tag.to_string(),
            title: if req.text.len() > 30 {
                format!("{}...", &req.text[..30])
            } else {
                req.text.clone()
            },
            alt_text: if tag == "Figure" { Some(req.text.clone()) } else { None },
            rect: Some([
                req.combined_rect.min.x,
                req.combined_rect.min.y,
                req.combined_rect.max.x,
                req.combined_rect.max.y,
            ]),
            page_index: Some(req.page_index),
            handle_index: None,
            // A tag drawn in the GUI stands for a selection, not for marked content the
            // file already carries: it has a rectangle of its own and claims no `/MCID`.
            mcids: Vec::new(),
            lang: None,
            role: None,
            children: Vec::new(),
        };
        self.ust_registry.next_node_id += 1;

        if let Some(ref mut root) = self.ust_registry.root {
            root.children.push(new_node);
        }

        self.notice = Some(super::Notice::done("notice_tag_created").about(tag));
    }

    pub fn open_file(&mut self, path: PathBuf, ctx: &egui::Context) {
        if let Ok(bytes) = std::fs::read(&path) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            self.open_file_bytes(bytes::Bytes::from(bytes), name, ctx);
        }
    }

    pub fn open_file_bytes(
        &mut self,
        data: bytes::Bytes,
        name: Option<String>,
        ctx: &egui::Context,
    ) {
        self.notice = None;
        self.total_pages = 0;
        self.page_layouts.clear();
        self.scenes.clear();
        self.request_queue.clear();
        self.selection_manager.clear();
        self.page_spans.clear();
        self.ust_registry.clear();
        self.selected_pages.clear();
        self.last_selected_page = None;
        self.clear_thumbnails_pending = true;
        self.is_loading = true;
        self.loading_message = "1/4: Decrypting and normalizing document...".to_string();
        self.doc_metadata = None;
        self.doc_file_size = None;
        self.doc_version = None;
        self.doc_security_method = None;
        self.doc_permissions = None;
        self.doc_page_sizes.clear();
        self.doc_fonts.clear();
        // The last document's answers are not this one's.
        self.survey = crate::sidebar::what_it_does::Survey::default();
        self.reset_view();
        let _ = self.tx_worker.send(WorkerRequest::Open { data, name, password: None });
        ctx.request_repaint();
    }

    pub fn reorder_pages_batch(&mut self, source_indices: &[usize], target_insert_pos: usize) {
        if source_indices.is_empty() || target_insert_pos > self.total_pages {
            return;
        }

        let selected_set: BTreeSet<usize> = source_indices.iter().copied().collect();
        let selected_before_target =
            source_indices.iter().filter(|&&idx| idx < target_insert_pos).count();
        let insert_idx_in_remaining = target_insert_pos.saturating_sub(selected_before_target);

        let mut remaining_sizes =
            Vec::with_capacity(self.doc_page_sizes.len().saturating_sub(selected_set.len()));
        let mut moving_sizes = Vec::with_capacity(selected_set.len());

        for (i, size) in self.doc_page_sizes.drain(..).enumerate() {
            if selected_set.contains(&i) {
                moving_sizes.push((i, size));
            } else {
                remaining_sizes.push(size);
            }
        }

        moving_sizes.sort_by_key(|(orig_idx, _)| *orig_idx);
        let count = moving_sizes.len();
        let clamped_insert_idx = insert_idx_in_remaining.min(remaining_sizes.len());

        let mut new_sizes = Vec::with_capacity(self.total_pages);
        new_sizes.extend(remaining_sizes.drain(..clamped_insert_idx));
        for (_, size) in moving_sizes {
            new_sizes.push(size);
        }
        new_sizes.extend(remaining_sizes);
        self.doc_page_sizes = new_sizes;

        self.scenes.clear();
        self.raw_texts.clear();
        self.page_spans.clear();
        self.clear_thumbnails_pending = true;

        self.compute_layouts();

        let new_range = clamped_insert_idx..(clamped_insert_idx + count);
        self.selected_pages = new_range.collect();
        self.last_selected_page = Some(clamped_insert_idx);
        self.view.active_page = clamped_insert_idx;

        let _ = self.tx_worker.send(WorkerRequest::ReorderPagesBatch {
            source_indices: source_indices.to_vec(),
            target_insert_pos,
        });
    }

    pub fn duplicate_page(&mut self, index: usize) {
        if index >= self.total_pages {
            return;
        }

        let page_size = self.doc_page_sizes.get(index).copied().unwrap_or((595.0, 842.0));
        self.doc_page_sizes.insert(index + 1, page_size);
        self.total_pages += 1;

        self.scenes.clear();
        self.raw_texts.clear();
        self.page_spans.clear();
        self.clear_thumbnails_pending = true;

        self.compute_layouts();

        self.selected_pages.clear();
        self.selected_pages.insert(index + 1);
        self.last_selected_page = Some(index + 1);

        let _ = self.tx_worker.send(WorkerRequest::DuplicatePage { index });
    }

    pub fn remove_selected_pages(&mut self) {
        if self.selected_pages.is_empty() || self.total_pages <= 1 {
            return;
        }
        if self.selected_pages.len() >= self.total_pages {
            return;
        }

        let mut indices: Vec<usize> = self.selected_pages.iter().copied().collect();
        indices.sort_unstable_by(|a, b| b.cmp(a));

        for &idx in &indices {
            if idx < self.doc_page_sizes.len() {
                self.doc_page_sizes.remove(idx);
            }
        }

        self.total_pages -= indices.len();
        self.scenes.clear();
        self.raw_texts.clear();
        self.page_spans.clear();
        self.clear_thumbnails_pending = true;

        self.compute_layouts();

        self.selected_pages.clear();
        self.last_selected_page = None;

        if self.view.active_page >= self.total_pages {
            self.view.active_page = self.total_pages.saturating_sub(1);
        }

        let _ = self.tx_worker.send(WorkerRequest::RemovePages { indices });
    }

    /// Puts every page of another document in at `at`, which is a page position in the
    /// current numbering.
    ///
    /// **The bytes are read here and the document is opened in the worker.** An
    /// operation is a value that has to serialise — `fepdf-mcp` reaches the same one
    /// through JSON — so `InsertFrom` carries the source file rather than a handle to
    /// one already open.
    ///
    /// The page count is not adjusted here. Unlike a removal, this window does not know
    /// how many pages are coming until the worker has opened the file, so the count and
    /// the layout come back with the reload rather than being guessed at.
    pub fn insert_document_at(&mut self, at: usize) {
        let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() else {
            return;
        };
        self.insert_document_bytes(&path, at);
    }

    /// Everything [`Self::insert_document_at`] does once a file has been named.
    ///
    /// Split out because a capture plan cannot answer a file dialog, and a second copy of
    /// the read and the send would be a second thing to keep true (UI-12).
    pub fn insert_document_bytes(&mut self, path: &std::path::Path, at: usize) {
        let source = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(why) => {
                self.notice =
                    Some(super::Notice::check("notice_open_failed").about(why.to_string()));
                return;
            }
        };
        let _ = self.tx_worker.send(WorkerRequest::Apply {
            operation: Box::new(fepdf::Operation::InsertFrom { source, at }),
            done: self.tr("menu_insert_done"),
        });
    }

    /// Takes the selected pages out into a document of their own.
    ///
    /// **The result opens in a window, not a save dialog.** A window holds one document,
    /// so the extracted pages arrive as a second window — where the reader can look at
    /// what they got and then export it with every option the wizard has. Asking for a
    /// path first would be asking before they have seen what they are saving.
    ///
    /// `remove` decides whether the pages also leave this document. That half is an
    /// operation and is recorded, so it can be undone; the extraction itself changes
    /// nothing here and is not.
    pub fn extract_selected_pages(&mut self, remove: bool) {
        let mut indices: Vec<usize> = self.selected_pages.iter().copied().collect();
        indices.sort_unstable();
        if indices.is_empty() {
            return;
        }
        // Every page is leaving, so there will be no document left to show. The window
        // closes once the new one is up; nothing is applied here, so the file it was
        // opened from is exactly as it was.
        self.close_after_extract = remove && indices.len() >= self.total_pages;
        let name = self.extracted_file_name();
        let _ = self.tx_worker.send(WorkerRequest::ExtractPages { indices, remove, name });
    }

    /// What the extracted document is called, in the reader's language.
    ///
    /// **Named here and not in the worker**, which holds the document and not the
    /// language — the same division as `WorkerResponse::Busy` carrying a key rather than
    /// a sentence. The name reaches the reader twice: as the file in the temporary
    /// directory, and as the title of the window it opens in.
    fn extracted_file_name(&self) -> String {
        // The local is `stem` and not `source`: clippy reads `"{source}"` in a string
        // beside a binding of that name as a formatting argument someone forgot to put
        // in a `format!`, and it is right to — the placeholder's name is the locale
        // file's business and matching it here is a coincidence waiting to mislead.
        let stem = self.pdf_name.as_deref().unwrap_or_default();
        let stem = stem.trim_end_matches(".pdf").trim_end_matches(".PDF");
        let named = self.tr("extracted_name").replace("{source}", stem);
        // A name is a path component, and a document called `a/b.pdf` would otherwise
        // ask for a directory that is not there.
        named.replace(['/', '\\'], "-")
    }

    /// Prints where the current page sits against the viewport, for a capture plan.
    ///
    /// **A screenshot cannot answer this.** The shots are scaled before they are read and
    /// the window's own pixels are not the coordinates the placement is computed in, so
    /// measuring a picture measures the picture.
    pub(crate) fn probe_placement(&mut self, label: &str) {
        let viewport = self.last_viewport_rect.unwrap_or(egui::Rect::NOTHING);
        let page = self.view.current_page(viewport, &self.page_layouts);
        let Some(layout) = self.page_layouts.get(page) else { return };
        let origin = self.view.get_origin(viewport);
        let zoom = self.view.zoom();
        let on_screen = egui::Rect::from_min_size(
            origin + layout.rect.min.to_vec2() * zoom,
            layout.rect.size() * zoom,
        );
        println!(
            "PROBE {label}: page {} viewport=({:.0},{:.0})..({:.0},{:.0}) \
             page=({:.0},{:.0})..({:.0},{:.0}) dx={:.0} dy={:.0} fits_y={}",
            page + 1,
            viewport.min.x,
            viewport.min.y,
            viewport.max.x,
            viewport.max.y,
            on_screen.min.x,
            on_screen.min.y,
            on_screen.max.x,
            on_screen.max.y,
            on_screen.center().x - viewport.center().x,
            on_screen.center().y - viewport.center().y,
            on_screen.height() <= viewport.height(),
        );
    }

    pub fn rotate_pages(&mut self, indices: Vec<usize>, delta: fepdf::Quarter) {
        if indices.is_empty() || self.total_pages == 0 {
            return;
        }

        let is_90_or_270 = delta.to_degrees() % 180 != 0;
        for &idx in &indices {
            if idx < self.doc_page_sizes.len() {
                if is_90_or_270 {
                    let (w, h) = self.doc_page_sizes[idx];
                    self.doc_page_sizes[idx] = (h, w);
                }
                self.scenes.remove(&idx);
                self.raw_texts.remove(&idx);
                self.page_spans.remove(&idx);
            }
        }

        self.clear_thumbnails_pending = true;
        self.compute_layouts();

        let _ = self.tx_worker.send(WorkerRequest::RotatePages { indices, delta });
    }

    /// Turns what the reader can see they have chosen.
    ///
    /// **The selection only counts where it is shown.** It is made in the tiles and kept
    /// when the view zooms into the pages — deliberately, so that zooming in does not
    /// throw it away — but there it is neither drawn nor changeable, so acting on it is
    /// acting on something invisible: a reader who had picked three tiles, zoomed in to
    /// read page 12 and pressed rotate turned those three and not the page in front of
    /// them. In the page view the target is the page they are on.
    pub fn rotate_selected_pages(&mut self, delta: fepdf::Quarter) {
        let viewport = self.last_viewport_rect.unwrap_or(egui::Rect::NOTHING);
        let current =
            (self.total_pages > 0).then(|| self.view.current_page(viewport, &self.page_layouts));
        let targets = pages_to_turn(self.view.selects_pages(), &self.selected_pages, current);
        self.rotate_pages(targets, delta);
    }

    pub fn rotate_page_action(&mut self, clicked_idx: usize, delta: fepdf::Quarter) {
        let targets = if self.selected_pages.contains(&clicked_idx) {
            self.selected_pages.iter().copied().collect()
        } else {
            vec![clicked_idx]
        };
        self.rotate_pages(targets, delta);
    }
}

/// Which pages a turn applies to.
///
/// **The selection only counts where it is shown.** Made in the tiles and kept when the
/// view zooms into the pages — deliberately, so zooming in does not throw it away — it is
/// neither drawn nor changeable there, so acting on it is acting on something invisible.
fn pages_to_turn(
    shows_selection: bool,
    selected: &std::collections::BTreeSet<usize>,
    current: Option<usize>,
) -> Vec<usize> {
    if shows_selection && !selected.is_empty() {
        return selected.iter().copied().collect();
    }
    current.into_iter().collect()
}

#[cfg(test)]
mod turning {
    use super::pages_to_turn;
    use std::collections::BTreeSet;

    /// A reader who had picked three tiles, zoomed in to read page 12 and pressed rotate
    /// turned those three and not the page in front of them.
    #[test]
    fn the_page_view_turns_the_page_being_read() {
        let chosen: BTreeSet<usize> = [2, 5, 7].into_iter().collect();
        assert_eq!(pages_to_turn(false, &chosen, Some(11)), vec![11]);
    }

    /// And the tiles turn what is marked there, which is what the reader can see.
    #[test]
    fn the_tiles_turn_what_is_marked() {
        let chosen: BTreeSet<usize> = [2, 5, 7].into_iter().collect();
        assert_eq!(pages_to_turn(true, &chosen, Some(11)), vec![2, 5, 7]);
    }

    /// With nothing marked, the tiles turn the page the reader is on too.
    #[test]
    fn nothing_marked_falls_back_to_the_current_page() {
        assert_eq!(pages_to_turn(true, &BTreeSet::new(), Some(3)), vec![3]);
        assert!(pages_to_turn(true, &BTreeSet::new(), None).is_empty());
    }
}
