//! Document lifecycle and page operations for `FepdfApp`.

use super::FepdfApp;
use crate::interaction::PendingTagRequest;
use crate::sidebar::USTNode;
use crate::view::Act;

/// Which pages a bulk selection takes, as the reader counts them.
///
/// **Counted from one, stored from zero.** Page 1 is odd and sits at index 0, which is the
/// one place this can go wrong and therefore the one place it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// Every page in the document.
    Every,
    /// Pages 1, 3, 5 — the right-hand pages of a left-bound book.
    Odd,
    /// Pages 2, 4, 6.
    Even,
}

impl Run {
    /// Every run, in the order the menu offers them.
    pub const ALL: [Self; 3] = [Self::Every, Self::Odd, Self::Even];

    /// Whether the page at `index`, counted from zero, is one of them.
    pub const fn takes(self, index: usize) -> bool {
        match self {
            Self::Every => true,
            Self::Odd => index.is_multiple_of(2),
            Self::Even => !index.is_multiple_of(2),
        }
    }

    /// The locale key naming it. No wildcard arm, so a fourth run needs a name (Rule 5).
    pub const fn key(self) -> &'static str {
        match self {
            Self::Every => "menu_select_every",
            Self::Odd => "menu_select_odd",
            Self::Even => "menu_select_even",
        }
    }
}
use crate::worker::WorkerRequest;
use std::collections::BTreeSet;
use std::path::PathBuf;

impl FepdfApp {
    pub(crate) fn inject_tag_to_tree(&mut self, tag: &str, req: &PendingTagRequest) {
        let new_node = USTNode {
            id: self.ust_registry.next_node_id,
            tag: tag.to_string(),
            // Thirty characters, not thirty bytes: a byte slice panics when the thirtieth
            // byte falls inside a character, which mixed Latin and kana text makes likely.
            title: if req.text.chars().count() > 30 {
                format!("{}...", req.text.chars().take(30).collect::<String>())
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
            mark_pages: Vec::new(),
            lang: None,
            role: None,
            actual_text: None,
            expansion: None,
            phoneme: None,
            phonetic_alphabet: "ipa".to_owned(),
            order: Vec::new(),
            children: Vec::new(),
        };
        self.ust_registry.next_node_id += 1;

        if let Some(ref mut root) = self.ust_registry.root {
            root.children.push(new_node);
        }

        // **`check`, not `done`, and it says where the tag actually goes.** A tag drawn
        // here has no `handle_index`, which is to say it is in this window and in no PDF:
        // the vocabulary has no operation that creates a structure element, so nothing
        // writes it and nothing ever did. The export wizard carried a checkbox — "Compile
        // & Inject USTRegistry Tags" — reading a flag nobody consulted, so the reader was
        // told twice that something had been saved that had not been. What does preserve
        // it is the UST draft JSON, which is in the same wizard and works.
        self.notice = Some(super::Notice::check("notice_tag_window_only").about(tag));
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
        self.forget_the_document();
        let _ = self.tx_worker.send(WorkerRequest::Open { data, name, password: None });
        ctx.request_repaint();
    }

    /// Clears everything the window knows about the document it had, before another
    /// arrives in its place: opened, or recovered after a crash.
    ///
    /// **One place for both**, because recovery began with a copy that left out the
    /// render queue. The window asks for page 0 before any document is open, the worker
    /// has nothing to draw it from, and the request stayed queued, so the recovered
    /// document's first page was never asked for and said "rendering" for good.
    pub(crate) fn forget_the_document(&mut self) {
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
        // **A key, and no "1/4".** It was English in every language, and it counted a
        // stage out of four when the other three come from the engine as they happen —
        // `WorkerResponse::LoadingProgress` — and are not four.
        self.loading_message = self.tr("busy_opening");
        self.doc_metadata = None;
        self.doc_file_size = None;
        self.doc_version = None;
        self.doc_security_method = None;
        self.doc_permissions = None;
        self.doc_page_frames.clear();
        self.doc_fonts.clear();
        // The last document's answers are not this one's.
        self.survey = crate::sidebar::what_it_does::Survey::default();
        self.reset_view();
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
            Vec::with_capacity(self.doc_page_frames.len().saturating_sub(selected_set.len()));
        let mut moving_sizes = Vec::with_capacity(selected_set.len());

        for (i, size) in self.doc_page_frames.drain(..).enumerate() {
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
        self.doc_page_frames = new_sizes;

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

        let frame = self.doc_page_frames.get(index).copied().unwrap_or_default();
        self.doc_page_frames.insert(index + 1, frame);
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

    /// Picks out a run of pages by their number, and forgets any anchor.
    ///
    /// **One home for "pick out several"** (UI-12): `Cmd+A` reached into `selected_pages`
    /// itself, so select-all was a shortcut with no visible door and nothing for the two
    /// runs beside it to be written next to.
    pub fn select_run(&mut self, run: Run) {
        self.selected_pages.clear();
        self.selected_pages.extend((0..self.total_pages).filter(|&index| run.takes(index)));
        // Shift extends from the page last clicked, and a run was not clicked: extending
        // from one of its pages would mean whichever the loop above happened to reach last.
        self.last_selected_page = None;
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
            if idx < self.doc_page_frames.len() {
                self.doc_page_frames.remove(idx);
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

        self.view.keep_page_inside(self.total_pages);

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
    ///
    /// **Pictures go in through the same door** (ROADMAP AA-5): a JPEG, PNG or TIFF chosen
    /// here becomes a page, on a sheet the size of the page it goes before.
    pub fn insert_document_at(&mut self, at: usize) {
        let Some(paths) = opening_dialog().pick_files() else { return };
        self.insert_files(&paths, at);
    }

    /// Everything [`Self::insert_document_at`] does once files have been named.
    ///
    /// Pictures alone go in as one act. Otherwise each file goes in at `at` in turn, last
    /// first, so they come out in the order chosen.
    pub fn insert_files(&mut self, paths: &[std::path::PathBuf], at: usize) {
        let mut read = Vec::with_capacity(paths.len());
        for path in paths {
            if is_text_file(&path.to_string_lossy()) {
                let text = std::fs::read(path).map_err(|e| e.to_string()).and_then(|bytes| {
                    fepdf::PdfDocument::plain_text(&bytes).map_err(|e| e.to_string())
                });
                match text {
                    Ok(text) => self.insert_text(text, at),
                    Err(why) => {
                        self.notice = Some(super::Notice::check("notice_open_failed").about(why));
                    }
                }
                continue;
            }
            match std::fs::read(path) {
                Ok(bytes) => read.push(bytes),
                Err(why) => {
                    self.notice =
                        Some(super::Notice::check("notice_open_failed").about(why.to_string()));
                    return;
                }
            }
        }
        let operations: Vec<fepdf::Operation> =
            // Not when nothing but text was chosen: "all pictures" is true of no files, and
            // asked for a page of each of none.
            if !read.is_empty() && read.iter().all(|b| fepdf::PdfDocument::is_picture(b)) {
                vec![fepdf::Operation::InsertImages { images: read, at, sheet: self.sheet_at(at) }]
            } else {
                read.into_iter()
                    .rev()
                    .map(|bytes| {
                        if fepdf::PdfDocument::is_picture(&bytes) {
                            fepdf::Operation::InsertImages {
                                images: vec![bytes],
                                at,
                                sheet: self.sheet_at(at),
                            }
                        } else {
                            fepdf::Operation::InsertFrom { source: bytes, at }
                        }
                    })
                    .collect()
            };
        for operation in operations {
            let _ = self.tx_worker.send(WorkerRequest::Apply {
                operation: Box::new(operation),
                done: self.tr("menu_insert_done"),
            });
        }
    }

    /// Text set on pages the size of the page it goes before (ROADMAP AA-6).
    fn insert_text(&mut self, text: String, at: usize) {
        let mut setting = fepdf::TextSetting::default();
        if let Some(sheet) = self.sheet_at(at) {
            setting.sheet = sheet;
        }
        let _ = self.tx_worker.send(WorkerRequest::Apply {
            operation: Box::new(fepdf::Operation::InsertText { text, at, setting }),
            done: self.tr("menu_insert_done"),
        });
    }

    /// The sheet a picture put in at `at` is fitted to: the page it goes before, or the
    /// last page, as it is shown — turned where the page is turned a quarter.
    fn sheet_at(&self, at: usize) -> Option<[f32; 2]> {
        let beside = at.min(self.total_pages.checked_sub(1)?);
        let frame = self.page_layouts.iter().find(|l| l.index == beside)?.frame;
        let (w, h) = (frame.rect.x2 - frame.rect.x1, frame.rect.y2 - frame.rect.y1);
        let (w, h) = if frame.rotation.rem_euclid(180) == 90 { (h, w) } else { (w, h) };
        #[allow(clippy::cast_possible_truncation)] // a page's size, well inside f32
        let sheet = [w.abs() as f32, h.abs() as f32];
        Some(sheet)
    }

    /// Everything [`Self::insert_document_at`] does once a file has been named.
    ///
    /// Split out because a capture plan cannot answer a file dialog, and a second copy of
    /// the read and the send would be a second thing to keep true (UI-12).
    pub fn insert_document_bytes(&mut self, path: &std::path::Path, at: usize) {
        self.insert_files(&[path.to_path_buf()], at);
    }

    /// Writes `pages` as PNG files into a folder the reader picks.
    ///
    /// **A folder rather than a file**, because a selection is usually more than one page
    /// and a save dialog names one file. Each is called after the document and numbered as
    /// the reader counts pages.
    pub fn export_pages_as_images(&mut self, pages: &BTreeSet<usize>) {
        let Some(folder) = rfd::FileDialog::new().pick_folder() else { return };
        self.export_pages_into(pages, folder);
    }

    /// Everything [`Self::export_pages_as_images`] does once a folder has been named.
    ///
    /// Split out because a capture plan cannot answer a folder dialog (UI-12).
    pub fn export_pages_into(&mut self, pages: &BTreeSet<usize>, folder: PathBuf) {
        if pages.is_empty() {
            return;
        }
        let stem = self.pdf_name.as_deref().unwrap_or("page");
        let stem = stem.trim_end_matches(".pdf").trim_end_matches(".PDF").replace(['/', '\\'], "-");
        let pages = pages.iter().copied().collect();
        let _ = self.tx_worker.send(WorkerRequest::ExportImages { pages, folder, stem });
    }

    /// Puts every page of another document where `pages` are, as one act.
    ///
    /// **One act, so one undo.** The engine has no operation for it and does not need one:
    /// it is `InsertFrom` and `RemovePages`, and the worker records the two as a single
    /// entry in the history, so taking it back restores the old pages and removes the new.
    pub fn replace_pages(&mut self, pages: &BTreeSet<usize>) {
        let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() else {
            return;
        };
        self.replace_pages_from(pages, &path);
    }

    /// Everything [`Self::replace_pages`] does once a file has been named (UI-12).
    pub fn replace_pages_from(&mut self, pages: &BTreeSet<usize>, path: &std::path::Path) {
        if pages.is_empty() {
            return;
        }
        let source = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(why) => {
                self.notice =
                    Some(super::Notice::check("notice_open_failed").about(why.to_string()));
                return;
            }
        };
        let _ = self.tx_worker.send(WorkerRequest::ReplacePages {
            pages: pages.iter().copied().collect(),
            source,
            done: self.tr("menu_replace_done"),
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

    /// Fills the header-and-footer form and presses its apply, for a capture plan.
    ///
    /// `place` is `top-left` through `bottom-right`; one this does not know leaves the
    /// position as the form has it, which a plan's screenshot then shows.
    pub(crate) fn drive_decoration(&mut self, place: &str, text: String) {
        use fepdf::DecorationPosition as At;
        let position = match place {
            "top-left" => Some(At::TopLeft),
            "top-centre" => Some(At::TopCenter),
            "top-right" => Some(At::TopRight),
            "bottom-left" => Some(At::BottomLeft),
            "bottom-centre" => Some(At::BottomCenter),
            "bottom-right" => Some(At::BottomRight),
            _ => None,
        };
        if let Some(position) = position {
            self.tools.decoration_position = position;
        }
        self.tools.decoration_text = text;
        crate::document_tools::send_decoration(self);
    }

    /// Fills the resize form and presses its apply, for a capture plan.
    ///
    /// **Everything the form does except the pointer.** A plan cannot click a radio
    /// button, so this sets the same fields the pickers set and calls the same
    /// `send_resize` the button calls.
    pub(crate) fn drive_resize(&mut self, sheet: &str, fit: &str) {
        self.fill_resize_form(sheet, fit);
        crate::document_tools::send_resize(self);
    }

    /// Fills the resize form without pressing anything, so a plan can photograph what it
    /// says before it is applied.
    pub(crate) fn fill_resize_form(&mut self, sheet: &str, fit: &str) {
        self.tools.offset = (0.0, 0.0);
        if let Some(size) = fepdf::PageResize::sheet(sheet) {
            self.tools.sheet = Some(sheet.to_string().leak());
            self.tools.sheet_size = size;
        } else if let Some((w, h)) = sheet.split_once('x')
            && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
        {
            self.tools.sheet = None;
            self.tools.sheet_size = (w, h);
        }
        self.tools.change_sheet = sheet != "keep";
        self.tools.fit = match fit.split_once(':') {
            Some(("scale", by)) => {
                // **The field as well as the fit.** Dragging the field is how a reader
                // chooses this one, so the two cannot disagree through the window — and a
                // plan that set only the fit put the form in a state the window cannot
                // reach, showing `1.00×` beside a factor of 0.8.
                self.tools.scale = by.parse().unwrap_or(1.0);
                fepdf::ContentScale::By(self.tools.scale)
            }
            _ => match fit {
                "keep" => fepdf::ContentScale::Keep,
                "fill" => fepdf::ContentScale::Fill,
                _ => fepdf::ContentScale::Fit,
            },
        };
    }

    /// Prints where the current page sits against the viewport, for a capture plan.
    ///
    /// **A screenshot cannot answer this.** The shots are scaled before they are read and
    /// the window's own pixels are not the coordinates the placement is computed in, so
    /// measuring a picture measures the picture.
    pub(crate) fn probe_placement(&mut self, label: &str) {
        let viewport = self.last_viewport_rect.unwrap_or(egui::Rect::NOTHING);
        let page = self.view.current_page();
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

        for &idx in &indices {
            if let Some(frame) = self.doc_page_frames.get_mut(idx) {
                // The turn itself, not only the swapped size: the view maps points
                // through it, and a page turned in the window is drawn turned.
                *frame = frame.turned_by(delta.to_degrees());
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
        let current = (self.total_pages > 0).then(|| self.view.current_page());
        let targets =
            pages_to_turn(self.view.does(Act::SelectPages), &self.selected_pages, current);
        self.rotate_pages(targets, delta);
    }

    /// Turns what a menu entry reached from `clicked_idx` acts on. See `acting_on`.
    ///
    /// **It asked the selection alone**, which in the page view is one carried in from the
    /// grid and invisible: a reader who had picked pages out, zoomed in on one of them and
    /// turned it from the menu turned all of them, under an entry that said "this page".
    pub fn rotate_page_action(&mut self, clicked_idx: usize, delta: fepdf::Quarter) {
        let targets = self.acting_on(clicked_idx).into_iter().collect();
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
mod runs {
    use super::Run;

    /// **A reader counts from one and the document from zero**, which is the whole of what
    /// this type is for: page 1 is odd and its index is 0, so an odd run takes the even
    /// indices. Written the other way round it takes every page the reader did not ask for.
    #[test]
    fn odd_takes_the_pages_a_reader_calls_odd() {
        let numbers =
            |run: Run| (0..8).filter(|&i| run.takes(i)).map(|i| i + 1).collect::<Vec<usize>>();
        assert_eq!(numbers(Run::Odd), vec![1, 3, 5, 7]);
        assert_eq!(numbers(Run::Even), vec![2, 4, 6, 8]);
        assert_eq!(numbers(Run::Every), (1..=8).collect::<Vec<usize>>());
    }

    /// The two halves are a document: neither takes a page twice, together they take all.
    #[test]
    fn odd_and_even_divide_the_document() {
        for index in 0..100 {
            assert_ne!(Run::Odd.takes(index), Run::Even.takes(index), "page {}", index + 1);
            assert!(Run::Every.takes(index));
        }
    }

    /// Every run is offered, and each one is named. `ALL` is what the menu walks.
    #[test]
    fn every_run_is_offered_and_named() {
        assert_eq!(Run::ALL.len(), 3);
        let keys: Vec<&str> = Run::ALL.iter().map(|run| run.key()).collect();
        assert_eq!(keys.len(), keys.iter().collect::<std::collections::BTreeSet<_>>().len());
    }
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

/// What the window opens and inserts: PDF, and the pictures it makes pages of (ROADMAP
/// AA-5). One list for every dialog that opens or inserts, so that none of them is the
/// one that forgot pictures (UI-12). The names are file types, the same in every
/// language, so the filter carries no locale key.
pub fn opening_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter(
        "PDF / JPEG / PNG / TIFF / Text",
        &["pdf", "jpg", "jpeg", "png", "tif", "tiff", "txt"],
    )
}

/// Whether a file is plain text, by its name: text has no signature in its bytes to be
/// known by, as a PDF or a picture has (ROADMAP AA-6).
pub fn is_text_file(name: &str) -> bool {
    std::path::Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("txt"))
}
