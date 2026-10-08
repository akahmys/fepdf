//! The page tree: moving, removing and rebuilding pages, and walking the tree to find
//! them.

use super::{Document, entries};
use crate::error::PdfError;
use crate::{Handle, Object, PdfName, PdfResult};
use std::collections::{BTreeMap, BTreeSet};

impl Document {
    /// Page reorder operation (moves page from `from` index to `to` index with immediate page tree reconstruction)
    pub fn reorder_page(&mut self, from: usize, to: usize) -> PdfResult<()> {
        if from >= self.pages.len() || to >= self.pages.len() {
            return Err(PdfError::no_page(
                if from >= self.pages.len() { from } else { to },
                self.pages.len(),
            ));
        }
        let page = self.pages.remove(from);
        self.pages.insert(to, page);
        self.rebuild_page_tree_in_arena()?;
        Ok(())
    }

    /// Batch page reorder operation (moves multiple pages specified by `source_indices` to `target_insert_pos`).
    pub fn reorder_pages_batch(
        &mut self,
        source_indices: &[usize],
        target_insert_pos: usize,
    ) -> PdfResult<std::ops::Range<usize>> {
        if source_indices.is_empty() {
            return Ok(0..0);
        }
        let total = self.pages.len();
        if target_insert_pos > total {
            return Err(PdfError::no_page(target_insert_pos, total));
        }
        for &idx in source_indices {
            if idx >= total {
                return Err(PdfError::no_page(idx, total));
            }
        }

        let selected_set: BTreeSet<usize> = source_indices.iter().copied().collect();
        let selected_before_target =
            source_indices.iter().filter(|&&idx| idx < target_insert_pos).count();
        let insert_idx_in_remaining = target_insert_pos.saturating_sub(selected_before_target);

        let mut remaining_pages = Vec::with_capacity(total - selected_set.len());
        let mut moving_pages = Vec::with_capacity(selected_set.len());

        for (i, page) in self.pages.drain(..).enumerate() {
            if selected_set.contains(&i) {
                moving_pages.push((i, page));
            } else {
                remaining_pages.push(page);
            }
        }

        moving_pages.sort_by_key(|(orig_idx, _)| *orig_idx);
        let count = moving_pages.len();
        let clamped_insert_idx = insert_idx_in_remaining.min(remaining_pages.len());

        let mut new_pages = Vec::with_capacity(total);
        new_pages.extend(remaining_pages.drain(..clamped_insert_idx));
        for (_, page) in moving_pages {
            new_pages.push(page);
        }
        new_pages.extend(remaining_pages);

        self.pages = new_pages;
        self.rebuild_page_tree_in_arena()?;

        Ok(clamped_insert_idx..(clamped_insert_idx + count))
    }

    /// Page removal operation (O(1) logical removal with immediate B-tree arena synchronization)
    pub fn remove_page(&mut self, index: usize) -> PdfResult<()> {
        if index >= self.pages.len() {
            return Err(PdfError::no_page(index, self.pages.len()));
        }
        self.pages.remove(index);
        self.rebuild_page_tree_in_arena()?;
        Ok(())
    }

    pub(super) fn create_empty_page_tree(&self) -> PdfResult<()> {
        let pages_root_key = self.arena.name("Pages");
        let type_key = self.arena.name("Type");
        let count_key = self.arena.name("Count");
        let kids_key = self.arena.name("Kids");

        let mut root_dict = BTreeMap::new();
        root_dict.insert(type_key, Object::Name(pages_root_key));
        root_dict.insert(count_key, Object::Integer(0));
        root_dict.insert(kids_key, Object::Array(self.arena.alloc_array(Vec::new())));

        let root_dh = self.arena.alloc_dict(root_dict);
        let root_h = self.arena.alloc_object(Object::Dictionary(root_dh));

        // Update Catalog
        let catalog_dh = self.resolve_to_dict(self.root)?;
        let mut catalog_dict = self.arena.get_dict(catalog_dh).unwrap_or_default();
        catalog_dict.insert(pages_root_key, Object::Reference(root_h));
        self.arena.set_dict(catalog_dh, catalog_dict);
        Ok(())
    }

    pub(super) fn build_page_tree_layer(
        &self,
        layer: &[Object],
        max_kids: usize,
    ) -> PdfResult<Vec<Object>> {
        let mut next_layer = Vec::new();
        for chunk in layer.chunks(max_kids) {
            let mut total_count = 0;
            let mut kids_refs = Vec::new();

            for kid_obj in chunk {
                kids_refs.push(kid_obj.clone());
                if let Some(kh) = kid_obj.as_reference() {
                    let kid_dh = self.resolve_to_dict(kh)?;
                    let kid_dict = self.arena.get_dict(kid_dh).unwrap_or_default();
                    total_count += self.get_node_count(&kid_dict);
                }
            }

            let pages_root_key = self.arena.name("Pages");
            let type_key = self.arena.name("Type");
            let count_key = self.arena.name("Count");
            let kids_key = self.arena.name("Kids");

            let mut pages_dict = BTreeMap::new();
            pages_dict.insert(type_key, Object::Name(pages_root_key));
            pages_dict.insert(count_key, Object::Integer(total_count as i64));
            pages_dict.insert(kids_key, Object::Array(self.arena.alloc_array(kids_refs)));

            let pages_dh = self.arena.alloc_dict(pages_dict);
            let pages_h = self.arena.alloc_object(Object::Dictionary(pages_dh));

            for kid_obj in chunk {
                if let Some(kh) = kid_obj.as_reference() {
                    let kid_dh = self.resolve_to_dict(kh)?;
                    let mut kid_dict = self.arena.get_dict(kid_dh).unwrap_or_default();
                    kid_dict.insert(self.arena.name("Parent"), Object::Reference(pages_h));
                    self.arena.set_dict(kid_dh, kid_dict);
                }
            }

            next_layer.push(Object::Reference(pages_h));
        }
        Ok(next_layer)
    }

    /// Dynamically rebuilds a clean, balanced B-Tree (max_kids = 50) in the arena.
    pub fn rebuild_page_tree_in_arena(&mut self) -> PdfResult<()> {
        let max_kids = 50;
        let mut current_layer: Vec<Object> =
            self.pages.iter().map(|&h| Object::Reference(h)).collect();

        if current_layer.is_empty() {
            return self.create_empty_page_tree();
        }

        // Build the first layer of Pages nodes
        current_layer = self.build_page_tree_layer(&current_layer, max_kids)?;

        // Loop until we have a single root node in the subsequent layers
        while current_layer.len() > 1 {
            current_layer = self.build_page_tree_layer(&current_layer, max_kids)?;
        }

        // Now current_layer has exactly one node (the root)
        if let Some(root_obj) = current_layer.first()
            && let Some(new_root_h) = root_obj.as_reference()
        {
            // Update Catalog /Pages reference
            let catalog_dh = self.resolve_to_dict(self.root)?;
            let mut catalog_dict = self.arena.get_dict(catalog_dh).unwrap_or_default();
            catalog_dict.insert(self.arena.name("Pages"), Object::Reference(new_root_h));
            self.arena.set_dict(catalog_dh, catalog_dict);

            // Root node in the page tree MUST NOT have a Parent key
            let root_dh = self.resolve_to_dict(new_root_h)?;
            let mut root_dict = self.arena.get_dict(root_dh).unwrap_or_default();
            root_dict.remove(&self.arena.name("Parent"));
            self.arena.set_dict(root_dh, root_dict);
        }

        Ok(())
    }

    /// Retrieves the parent Pages node chain from a leaf Page node up to the root.
    pub fn get_parent_chain(&self, page_h: Handle<Object>) -> Vec<Handle<Object>> {
        let mut chain = Vec::new();
        let mut current = page_h;
        // A `/Parent` that returns to a node already in the chain is a cycle, and the
        // chain grew until memory ran out (ROADMAP Z-1, beside the same walk in ingestion).
        let mut seen = std::collections::BTreeSet::from([page_h.index()]);
        while let Ok(dict_h) = self.resolve_to_dict(current) {
            let Some(dict) = self.arena.get_dict(dict_h) else { break };
            let parent_key = self.arena.name("Parent");
            if let Some(parent_obj) = dict.get(&parent_key)
                && let Some(parent_h) = parent_obj.resolve(&self.arena).as_reference()
                && seen.insert(parent_h.index())
            {
                chain.push(parent_h);
                current = parent_h;
            } else {
                break;
            }
        }
        chain.reverse();
        chain
    }

    /// Returns a list of all page object handles in the document.
    ///
    /// **A page tree that will not walk is recorded, not swallowed.** Both failures here
    /// were `if let Ok(..)` and `let _ =`, which is the shape that cost this engine a
    /// catalogue and eleven objects once already (Phase G). It cost pages too:
    /// `UnknownFilter-xrefstm.pdf` names `/Pages 5 0 R`, object 5 was indexed only by a
    /// cross-reference stream written with `/XXXDecode`, and the recovery scan does not
    /// find it — so the walk failed, the failure was dropped, and `inspect info` reported
    /// **"Pages: 0"** about a file that has one. Reported as a `Violation`: something was
    /// lost, and 7.7.3.2 requires a page tree with at least one leaf, so this cannot fire
    /// on a conforming document.
    pub fn find_all_pages(&self) -> Vec<Handle<Object>> {
        let mut pages = Vec::new();
        match self.get_pages_root() {
            Ok(root) => {
                let mut seen = std::collections::BTreeSet::new();
                if let Err(why) = self.walk_pages_recursive(root, &mut pages, 0, &mut seen) {
                    self.record(crate::interpretation::Decision::violation(
                        "7.7.3.2",
                        format!("the page tree could not be walked: {why}"),
                        format!(
                            "kept the {} pages reached before it stopped; the rest of the \
                             tree is not in this document",
                            pages.len()
                        ),
                    ));
                }
            }
            Err(why) => self.record(crate::interpretation::Decision::violation(
                "7.7.3.2",
                format!("the catalogue's page tree could not be reached: {why}"),
                "reported the document as having no pages, because none can be found",
            )),
        }
        pages
    }

    /// Collects the leaves under `node_h`, refusing to walk the same node twice.
    ///
    /// **Two guards, because they answer different questions.** `seen` is what makes the
    /// count right: 7.7.3.2 requires a page tree, every node of which has one `/Parent`,
    /// so a node reached twice is not conforming and expanding it invents pages. `depth`
    /// is what keeps the stack safe on a tree that is deep and legitimate — those are not
    /// bounded by the object count, because each level is a separate object and the
    /// parser's nesting limit does not apply across them.
    ///
    /// **Before 2026-09-05 there was only `depth`, and it produced a wrong answer
    /// quietly.** A two-node loop around a single page read as **sixteen pages** — one
    /// pushed every two levels until the limit — and was written back out as
    /// `/Kids [6 0 R x16]` with `DECISIONS TAKEN READING` saying "none".
    pub(super) fn walk_pages_recursive(
        &self,
        node_h: Handle<Object>,
        out: &mut Vec<Handle<Object>>,
        depth: usize,
        seen: &mut std::collections::BTreeSet<Handle<Object>>,
    ) -> PdfResult<()> {
        if depth > 32 {
            return Err(PdfError::DepthLimitExceeded(32));
        }
        if !seen.insert(node_h) {
            self.record_second_visit(node_h);
            return Ok(());
        }

        let dict_h = self.resolve_to_dict(node_h)?;
        let dict = self
            .arena
            .get_dict(dict_h)
            .ok_or_else(|| PdfError::internal("Invalid node in page tree"))?;

        let type_key = self.arena.name("Type");
        let node_type = dict
            .get(&type_key)
            .and_then(|o| o.resolve(&self.arena).as_name())
            .and_then(|h| self.arena.get_name(h));

        if let Some(name) = node_type
            && name.as_str() == "Page"
        {
            out.push(node_h);
            return Ok(());
        }

        self.walk_kids(&dict, out, depth, seen)
    }

    /// Records that the page tree came back to a node it had already walked.
    ///
    /// Not an error, which is why the caller records and returns `Ok`: reaching a node
    /// twice is not a failure of the subtree below it, it is a node already counted.
    /// Routed through [`Self::walk_kids`]'s error path it read "the page tree does not
    /// walk below object 2", which says the wrong thing about it.
    pub(super) fn record_second_visit(&self, node_h: Handle<Object>) {
        self.record(crate::interpretation::Decision::violation(
            "7.7.3.2",
            format!(
                "the page tree reaches object {} a second time, so it is not a tree",
                node_h.index()
            ),
            "did not walk it again, so the pages under it are counted once",
        ));
    }

    /// Walks each `/Kids` entry of one node, recording the branches that will not walk.
    ///
    /// **Recorded and not swallowed.** This loop read
    /// `let _ = self.walk_pages_recursive(..)` until 2026-09-05, one scope deeper than
    /// the two that [`Self::find_all_pages`]'s own doc comment describes removing — so
    /// the depth limit's error went nowhere and a looping tree was expanded in silence.
    /// A failing branch still does not take the rest of the document with it, which is
    /// why this records rather than propagates.
    pub(super) fn walk_kids(
        &self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        out: &mut Vec<Handle<Object>>,
        depth: usize,
        seen: &mut std::collections::BTreeSet<Handle<Object>>,
    ) -> PdfResult<()> {
        let Some(kids_obj) = dict.get(&self.arena.name("Kids")) else {
            return Ok(());
        };
        let ah = kids_obj
            .resolve(&self.arena)
            .as_array()
            .ok_or_else(|| PdfError::violation("7.7.3.2", "Invalid Kids array"))?;
        let Some(kids) = self.arena.get_array(ah) else {
            return Ok(());
        };
        for kid in kids {
            if let Some(h) = kid.as_reference()
                && let Err(why) = self.walk_pages_recursive(h, out, depth + 1, seen)
            {
                self.record(crate::interpretation::Decision::violation(
                    "7.7.3.2",
                    format!("the page tree does not walk below object {}: {why}", h.index()),
                    "stopped there and kept the pages reached by the other branches",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn get_pages_root(&self) -> PdfResult<Handle<Object>> {
        let catalog_obj = self
            .arena
            .get_object(self.root)
            .ok_or_else(|| PdfError::violation("7.7.2", "Missing document catalog"))?;
        // The one entry, not the whole catalogue: a document's pages must not become
        // unreachable because some other entry of Table 29 will not parse.
        entries::entry::<entries::Located<entries::PageTreeRoot>>(
            &self.arena,
            &catalog_obj,
            "Pages",
        )?
        .and_then(|p| p.reference)
        .ok_or_else(|| PdfError::violation("7.7.2", "The catalogue names no page tree (7.7.2)"))
    }

    pub(super) fn get_node_count(&self, dict: &BTreeMap<Handle<PdfName>, Object>) -> usize {
        let count_key = self.arena.name("Count");
        if let Some(count) = dict.get(&count_key).and_then(|o| o.resolve(&self.arena).as_integer())
        {
            return usize::try_from(count).unwrap_or(0);
        }
        // Leaf Page nodes usually lack /Count, they count as 1
        let type_key = self.arena.name("Type");
        if let Some(t) = dict.get(&type_key).and_then(|o| o.resolve(&self.arena).as_name())
            && let Some(name) = self.arena.get_name(t)
            && name.as_str() == "Page"
        {
            return 1;
        }
        0
    }
}
