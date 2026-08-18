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
            children: Vec::new(),
        };
        self.ust_registry.next_node_id += 1;

        if let Some(ref mut root) = self.ust_registry.root {
            root.children.push(new_node);
        }

        self.error = Some(format!("Successfully created <{tag}> tag"));
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
        self.error = None;
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
        self.reset_view();
        let _ = self.tx_worker.send(WorkerRequest::Open { data, name });
        ctx.request_repaint();
    }

    #[allow(dead_code)]
    pub fn reorder_page(&mut self, from: usize, to: usize) {
        if from >= self.total_pages || to >= self.total_pages || from == to {
            return;
        }
        let target_pos = if to > from { to + 1 } else { to };
        self.reorder_pages_batch(&[from], target_pos);
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

    #[allow(dead_code)]
    pub fn rotate_selected_pages(&mut self, delta: fepdf::Quarter) {
        let targets = if !self.selected_pages.is_empty() {
            self.selected_pages.iter().copied().collect()
        } else if self.total_pages > 0 && self.view.active_page < self.total_pages {
            vec![self.view.active_page]
        } else {
            Vec::new()
        };
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
