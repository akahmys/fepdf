//! Which objects a page, an outline or the document reaches, for the linearised layout.

#![allow(clippy::too_many_arguments, clippy::collapsible_if, clippy::type_complexity)]

use super::{BEAD_CHAIN_KEYS, PdfWriter};
use crate::{Handle, Object, PdfError, PdfName, PdfResult};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

impl<'a, W: Write> PdfWriter<'a, W> {
    pub(super) fn write_object_to_bytes(&mut self, h: Handle<Object>) -> PdfResult<Vec<u8>> {
        let start = self.buffer.len();
        let obj = self.arena.get_object(h).ok_or_else(|| PdfError::internal("Object missing"))?;
        self.write_object(&obj)?;
        let bytes = self.buffer[start..].to_vec();
        self.buffer.truncate(start);
        Ok(bytes)
    }

    pub(super) fn trace_page_reachables(
        &self,
        original_pages: &[Handle<Object>],
        page_objects_set: &BTreeSet<Handle<Object>>,
    ) -> Vec<BTreeSet<Handle<Object>>> {
        let mut page_reachables = Vec::with_capacity(original_pages.len());
        for (i, &ph) in original_pages.iter().enumerate() {
            let mut p_reachable = BTreeSet::new();
            let mut p_exclude = page_objects_set.clone();
            p_exclude.remove(&ph);
            self.trace_reachable_no_parent(ph, &mut p_reachable, &BTreeSet::new(), &p_exclude);
            log::debug!("DEBUG: Page {} reachable count: {}", i, p_reachable.len());
            page_reachables.push(p_reachable);
        }
        page_reachables
    }

    pub(super) fn trace_doc_reachable_selective(
        &self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        page_objects_set: &BTreeSet<Handle<Object>>,
    ) -> BTreeSet<Handle<Object>> {
        let mut doc_reachable = BTreeSet::new();
        if let Some(obj) = self.arena.get_object(root) {
            if let Some(dh) = obj.as_dict_handle() {
                if let Some(dict) = self.arena.get_dict(dh) {
                    for (k, v) in dict {
                        let k_str = self.arena.get_name_str(k).unwrap_or_default();
                        if k_str != "Pages" {
                            let mut stack = Vec::new();
                            self.trace_reachable_inline(
                                &v,
                                &mut doc_reachable,
                                &BTreeSet::new(),
                                &mut stack,
                                &["Parent"],
                                page_objects_set,
                            );
                            while let Some(curr) = stack.pop() {
                                self.trace_reachable_selective(
                                    curr,
                                    &mut doc_reachable,
                                    &BTreeSet::new(),
                                    &["Parent"],
                                    page_objects_set,
                                );
                            }
                        }
                    }
                }
            }
        }
        if let Some(ih) = info {
            self.trace_reachable_no_parent(
                ih,
                &mut doc_reachable,
                &BTreeSet::new(),
                page_objects_set,
            );
        }
        doc_reachable
    }

    pub(super) fn trace_outline_objects(
        &self,
        root: Handle<Object>,
        page_objects_set: &BTreeSet<Handle<Object>>,
    ) -> (BTreeSet<Handle<Object>>, Option<Handle<Object>>) {
        let mut outline_objs = BTreeSet::new();
        let mut outlines_root_h = None;
        if let Some(obj) = self.arena.get_object(root) {
            if let Some(dh) = obj.as_dict_handle() {
                if let Some(dict) = self.arena.get_dict(dh) {
                    if let Some(Object::Reference(oh)) = dict.get(&self.arena.name("Outlines")) {
                        outlines_root_h = Some(*oh);
                        let mut p_reachable = BTreeSet::new();
                        self.trace_reachable_selective(
                            *oh,
                            &mut p_reachable,
                            &BTreeSet::new(),
                            &["Parent"],
                            page_objects_set,
                        );
                        outline_objs = p_reachable;
                    }
                }
            }
        }
        (outline_objs, outlines_root_h)
    }

    pub(super) fn identify_shared_objects(
        &self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        original_pages: &[Handle<Object>],
        page_reachables: &[BTreeSet<Handle<Object>>],
    ) -> BTreeSet<Handle<Object>> {
        let mut page_ref_count = BTreeMap::new();
        for p_reach in page_reachables {
            for &h in p_reach {
                *page_ref_count.entry(h).or_insert(0) += 1;
            }
        }

        let mut shared_objs = BTreeSet::new();
        for (&h, &count) in &page_ref_count {
            if count > 1 {
                shared_objs.insert(h);
            }
        }
        shared_objs.remove(&root);
        if let Some(ih) = info {
            shared_objs.remove(&ih);
        }
        for &ph in original_pages {
            shared_objs.remove(&ph);
        }
        shared_objs
    }

    pub(super) fn trace_dict_keys(
        &self,
        dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
        reachable: &mut BTreeSet<Handle<Object>>,
        assigned: &BTreeSet<Handle<Object>>,
        stack: &mut Vec<Handle<Object>>,
        exclude_keys: &[&str],
        exclude_objects: &BTreeSet<Handle<Object>>,
    ) {
        if let Some(d) = self.arena.get_dict(dh) {
            let bead = self.is_bead(&d);
            for (k, v) in d {
                let k_str = self.arena.get_name_str(k).unwrap_or_default();
                if exclude_keys.contains(&k_str.as_str()) {
                    continue;
                }
                if bead && BEAD_CHAIN_KEYS.contains(&k_str.as_str()) {
                    continue;
                }
                self.trace_reachable_inline(
                    &v,
                    reachable,
                    assigned,
                    stack,
                    exclude_keys,
                    exclude_objects,
                );
            }
        }
    }

    /// Whether a dictionary is an article bead (12.4.3, Table 160). `/Type` is optional
    /// there, so a dictionary carrying all four of the entries a bead requires is one too.
    pub(super) fn is_bead(&self, d: &BTreeMap<Handle<PdfName>, Object>) -> bool {
        let has = |key: &str| d.contains_key(&self.arena.name(key));
        let typed = d
            .get(&self.arena.name("Type"))
            .and_then(Object::as_name)
            .and_then(|n| self.arena.get_name_str(n))
            .is_some_and(|n| n == "Bead");
        typed || ["N", "V", "P", "R"].iter().all(|key| has(key))
    }

    pub(super) fn trace_reachable_selective(
        // RR-15 Limit: Dispatcher - trace reachable object tree selectively
        &self,
        h: Handle<Object>,
        reachable: &mut BTreeSet<Handle<Object>>,
        assigned: &BTreeSet<Handle<Object>>,
        exclude_keys: &[&str],
        exclude_objects: &BTreeSet<Handle<Object>>,
    ) {
        if assigned.contains(&h) || exclude_objects.contains(&h) {
            return;
        }
        reachable.insert(h);

        let mut stack = vec![h];
        while let Some(curr_h) = stack.pop() {
            let Some(obj) = self.arena.get_object(curr_h) else {
                continue;
            };

            // For indirect references, we check if they are already assigned or seen
            match obj {
                Object::Reference(rh) => {
                    if !assigned.contains(&rh)
                        && !exclude_objects.contains(&rh)
                        && reachable.insert(rh)
                    {
                        stack.push(rh);
                    }
                }
                Object::Array(ah) => {
                    if let Some(a) = self.arena.get_array(ah) {
                        for item in a {
                            self.trace_reachable_inline(
                                &item,
                                reachable,
                                assigned,
                                &mut stack,
                                exclude_keys,
                                exclude_objects,
                            );
                        }
                    }
                }
                Object::Dictionary(dh) | Object::Stream(dh, _) => {
                    self.trace_dict_keys(
                        dh,
                        reachable,
                        assigned,
                        &mut stack,
                        exclude_keys,
                        exclude_objects,
                    );
                }
                _ => {}
            }
        }
    }

    pub(super) fn trace_reachable_inline(
        // RR-15 Limit: Dispatcher - trace reachable inline objects recursively
        &self,
        obj: &Object,
        reachable: &mut BTreeSet<Handle<Object>>,
        assigned: &BTreeSet<Handle<Object>>,
        stack: &mut Vec<Handle<Object>>,
        exclude_keys: &[&str],
        exclude_objects: &BTreeSet<Handle<Object>>,
    ) {
        match obj {
            Object::Reference(rh) => {
                if !assigned.contains(rh) && !exclude_objects.contains(rh) && reachable.insert(*rh)
                {
                    stack.push(*rh);
                }
            }
            Object::Array(ah) => {
                if let Some(a) = self.arena.get_array(*ah) {
                    for item in a {
                        self.trace_reachable_inline(
                            &item,
                            reachable,
                            assigned,
                            stack,
                            exclude_keys,
                            exclude_objects,
                        );
                    }
                }
            }
            Object::Dictionary(dh) | Object::Stream(dh, _) => {
                self.trace_dict_keys(
                    *dh,
                    reachable,
                    assigned,
                    stack,
                    exclude_keys,
                    exclude_objects,
                );
            }
            _ => {}
        }
    }

    pub(super) fn trace_reachable_no_parent(
        &self,
        h: Handle<Object>,
        reachable: &mut BTreeSet<Handle<Object>>,
        assigned: &BTreeSet<Handle<Object>>,
        exclude_objects: &BTreeSet<Handle<Object>>,
    ) {
        self.trace_reachable_selective(
            h,
            reachable,
            assigned,
            &["Parent", "Pages", "Root", "Catalog", "Info"],
            exclude_objects,
        );
    }
}
