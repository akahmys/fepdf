//! A file laid out for a reader that shows the first page first (Annex F): the parts,
//! and what they share.

#![allow(clippy::too_many_arguments, clippy::collapsible_if, clippy::type_complexity)]

use super::{LinState, PdfWriter};
use crate::{Handle, Object, PdfError, PdfResult};
use std::collections::BTreeSet;
use std::io::Write;

impl<'a, W: Write> PdfWriter<'a, W> {
    pub(super) fn partition_and_collect_shared(
        &self,
        others: &[Handle<Object>],
        shared_objs: &BTreeSet<Handle<Object>>,
        page_reachables_0: &BTreeSet<Handle<Object>>,
    ) -> (Vec<Handle<Object>>, Vec<Handle<Object>>, Vec<u32>) {
        let mut others_shared = Vec::new();
        let mut others_private = Vec::new();
        for &h in others {
            if shared_objs.contains(&h) {
                others_shared.push(h);
            } else {
                others_private.push(h);
            }
        }

        let mut first_page_shared_set = BTreeSet::new();
        for &h in page_reachables_0 {
            if shared_objs.contains(&h) {
                first_page_shared_set.insert(h);
            }
        }

        let mut shared_ids: Vec<u32> = Vec::new();
        for &h in &others_shared {
            if !first_page_shared_set.contains(&h) {
                shared_ids.push(self.id_map[&h]);
            }
        }
        shared_ids.sort_unstable();
        shared_ids.dedup();

        (others_shared, others_private, shared_ids)
    }

    pub(super) fn calculate_worst_case_hint_size(
        &self,
        num_pages: usize,
        dummy_groups_len: usize,
        shared_ids_len: usize,
    ) -> usize {
        let total_shared = dummy_groups_len + shared_ids_len;
        let max_shared_per_page = total_shared;
        let bits_idx = if max_shared_per_page > 0 {
            32 - (max_shared_per_page as u32).leading_zeros()
        } else {
            0
        };
        let worst_page_bits =
            num_pages * (16 + 32 + 16 + (bits_idx as usize + 16) * max_shared_per_page);
        let worst_shared_bits = total_shared * (32 + 1 + 16);
        let worst_header_bits = 13 * 32 + 7 * 32;
        let worst_bits = worst_header_bits + worst_page_bits + worst_shared_bits;
        worst_bits.div_ceil(8)
    }

    pub(super) fn pre_populate_obj_sizes(&mut self) {
        for (&h, &id) in &self.id_map.clone() {
            if let Ok(bytes) = self.write_object_to_bytes(h) {
                let header_len = format!("{id} 0 obj\r\n").len();
                let footer_len = 10;
                let sz = bytes.len() + header_len + footer_len;
                self.obj_sizes.insert(id, sz);
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn finish_linearized(
        // RR-15 Limit: Dispatcher - Sequential PDF linearization generator routing and sorting object tables, hint table, and headers
        &mut self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
    ) -> PdfResult<()> {
        self.buffer.clear();
        self.xref.clear();
        self.id_map.clear();

        // 1. Header
        self.write_all(b"%PDF-2.0\r\n%\xe2\xe3\xcf\xd3\r\n")?;

        let (s2, s6, others, pgs, counts, outline_exclusive, shared_objs, page_reachables) =
            self.collect_lin_objects(root, info)?;
        log::debug!("DEBUG: Total pages collected: {}", pgs.len());
        log::debug!("DEBUG: Section 2 objects: {}", s2.len());
        let page1 = pgs[0];
        let mut doc_private = Vec::new();
        for &h in &s2 {
            if h == page1 {
                break;
            }
            if h != root && Some(h) != info {
                doc_private.push(h);
            }
        }
        let last_doc_level_handle = if let Some(&last_h) = doc_private.last() {
            last_h
        } else if let Some(inf) = info {
            inf
        } else {
            root
        };
        let (mut total_size, primary_count, hint_stream_id, first_page_shared_count) = self
            .assign_lin_ids(
                root,
                info,
                &s2,
                &s6,
                &others,
                &shared_objs,
                &outline_exclusive,
                pgs[0],
                &page_reachables[0],
            );
        total_size += 1; // ACCOUNT FOR MAIN XREF STREAM

        // Pre-populate obj_sizes for all objects using the assigned IDs in id_map
        self.pre_populate_obj_sizes();

        // Partition others into shared and private, collect shared IDs
        let (others_shared, others_private, shared_ids) =
            self.partition_and_collect_shared(&others, &shared_objs, &page_reachables[0]);

        // Build dummy structures to determine exact hint table size
        let _outline_count = outline_exclusive.len() as u32;

        let (dummy_groups, dummy_refs, _dummy_outline_params) = self.build_lin_structures(
            root,
            info,
            &s2,
            &pgs,
            &outline_exclusive,
            &shared_objs,
            &page_reachables,
            &[],
            &[],
            &[],
            &shared_ids,
            &counts,
            0,
            None,
            true,
        )?;

        let page1_id = self.id_map[&pgs[0]];
        let dummy_first_shared_id = shared_ids.first().copied().unwrap_or(page1_id);
        let (_, _, _) = self.generate_hint_tables(
            &pgs,
            &shared_ids,
            0,
            0,
            0,
            0,
            0,
            &counts,
            dummy_first_shared_id,
            &outline_exclusive,
            &dummy_groups,
            &dummy_refs,
            0,
            0,
            0,
        );

        let exact_hint_size =
            self.calculate_worst_case_hint_size(pgs.len(), dummy_groups.len(), shared_ids.len());

        // 2. Section 1: Linearization Dictionary and First Xref (Reserved)
        let (dict_pos, p_xref_pos) = self.reserve_lin_headers(primary_count, total_size);

        // 3. Section 2 & 6: Write objects
        let p0_non_shared_count = counts[0] - first_page_shared_count;
        let doc_private_len = (page1_id - (primary_count + 1))
            .saturating_sub(u32::from(info.is_some()))
            .saturating_sub(1);
        let non_shared_total =
            2 + u32::from(info.is_some()) + doc_private_len + p0_non_shared_count;
        let first_page_shared_start_id = primary_count + 1 + non_shared_total;

        let (hint_pos, s2_end, s7_start, s8_start) = self.write_lin_objects_to_stream(
            root,
            info,
            &s2,
            &s6,
            &others_shared,
            &others_private,
            pgs.len(),
            exact_hint_size,
            primary_count,
            hint_stream_id,
            first_page_shared_count,
            first_page_shared_start_id,
            last_doc_level_handle,
        )?;

        // 4. Main Xref and Trailer
        let main_xref_off = self.write_lin_main_xref(
            root,
            info,
            total_size,
            primary_count,
            p_xref_pos,
            Some(hint_stream_id),
            first_page_shared_start_id,
            first_page_shared_count,
        )?;

        // Resolve Page 1 object offset for the hint table
        let page1_id = self.id_map[&pgs[0]];
        let p1_off =
            *self.xref.get(&page1_id).ok_or_else(|| PdfError::internal("Page 1 missing"))?;

        let (first_page_groups, page_shared_refs, _outline_params) = self.build_lin_structures(
            root,
            info,
            &s2,
            &pgs,
            &outline_exclusive,
            &shared_objs,
            &page_reachables,
            &[],
            &[],
            &[],
            &shared_ids,
            &counts,
            s2_end,
            None,
            false,
        )?;

        let state = LinState {
            dict_pos,
            pxref_pos: p_xref_pos,
            pxref_size: ((total_size as usize).saturating_sub(primary_count as usize) + 2) * 20
                + 256,
            hint_pos,
            hint_size: exact_hint_size,
            page1_offset: p1_off as u32,
            page1_end: s2_end,
            s7_start,
            s8_start,
            main_xref_offset: main_xref_off,
            pages: pgs,
            page_obj_counts: counts,
            total_size,
            primary_count,
            obj_stm_id: Some(hint_stream_id),
            obj_stm_count: 1,
            info_handle: info,
            root,
            shared_ids,
            outline_exclusive: outline_exclusive.clone(),
            first_page_groups,
            page_shared_refs,
            first_page_shared_count,
            first_shared_id: first_page_shared_start_id,
        };
        let max_id_map = self.id_map.values().copied().max().unwrap_or(0);
        let max_xref = self.xref.keys().copied().max().unwrap_or(0);
        log::debug!(
            "DEBUG_TOTAL_SIZE_INFO: total_size={total_size}, max_id_map={max_id_map}, max_xref={max_xref}"
        );
        self.finalize_lin_headers(state)?;
        Ok(())
    }

    pub(super) fn collect_and_write_first_page_shared(
        &mut self,
        others_shared: &[Handle<Object>],
        first_page_shared_start_id: u32,
        first_page_shared_count: u32,
    ) -> PdfResult<()> {
        let mut first_page_shared: Vec<(u32, Handle<Object>)> = Vec::new();
        let first_page_shared_end_id = first_page_shared_start_id + first_page_shared_count;

        for &h in others_shared {
            let id = self.id_map[&h];
            if id >= first_page_shared_start_id && id < first_page_shared_end_id {
                first_page_shared.push((id, h));
            }
        }
        first_page_shared.sort_by_key(|&(id, _)| id);

        for &(id, h) in &first_page_shared {
            self.write_indirect_object(id, 0, h)?;
        }
        Ok(())
    }

    pub(super) fn collect_and_write_part8_shared(
        &mut self,
        others_shared: &[Handle<Object>],
        s2: &[Handle<Object>],
        first_page_shared_end_id: u32,
    ) -> PdfResult<()> {
        let s2_set: std::collections::BTreeSet<Handle<Object>> = s2.iter().copied().collect();
        let mut part8_objects: Vec<(u32, Handle<Object>)> = Vec::new();
        for &h in others_shared {
            if !s2_set.contains(&h) {
                let id = self.id_map[&h];
                if id >= first_page_shared_end_id {
                    part8_objects.push((id, h));
                }
            }
        }
        part8_objects.sort_by_key(|&(id, _)| id);

        for &(id, h) in &part8_objects {
            self.write_indirect_object(id, 0, h)?;
        }
        Ok(())
    }

    pub(super) fn write_lin_objects_to_stream(
        &mut self,
        _root: Handle<Object>,
        _info: Option<Handle<Object>>,
        s2: &[Handle<Object>],
        s6: &[Handle<Object>],
        others_shared: &[Handle<Object>],
        others_private: &[Handle<Object>],
        _page_count: usize,
        hint_size: usize,
        _primary_count: u32,
        hint_stream_id: u32,
        first_page_shared_count: u32,
        first_page_shared_start_id: u32,
        last_doc_level_handle: Handle<Object>,
    ) -> PdfResult<(usize, usize, usize, usize)> {
        // Write Section 2 objects exactly in physical s2 order (Golden Layout)
        let mut hint_pos = 0;
        let mut hint_inserted = false;

        for &h in s2 {
            let id = self.id_map[&h];
            self.write_indirect_object(id, 0, h)?;

            // Insert Primary Hint Stream physically after Part 4 document-level objects (Catalog, Info, and doc_private)
            let is_last_part4 = h == last_doc_level_handle;
            if is_last_part4 && !hint_inserted {
                let (h_pos, ..) = self.reserve_hint_stream(hint_stream_id, hint_size);
                hint_pos = h_pos;
                hint_inserted = true;
            }
        }

        // --- NEW: Write Part 6 (First-page shared objects) immediately after Section 2 non-shared objects ---
        self.collect_and_write_first_page_shared(
            others_shared,
            first_page_shared_start_id,
            first_page_shared_count,
        )?;

        let s2_end = self.current_offset();

        // 6. Write Section 6: Other pages exclusive objects (Pages 2..N / Part 7)
        for &h in s6 {
            let id = self.id_map[&h];
            self.write_indirect_object(id, 0, h)?;
        }

        let s7_start = self.current_offset(); // This is where the shared objects (Part 8) start!

        // 7. Write Part 8: Shared Objects
        let first_page_shared_end_id = first_page_shared_start_id + first_page_shared_count;
        self.collect_and_write_part8_shared(others_shared, s2, first_page_shared_end_id)?;

        let s8_start = self.current_offset(); // This is where the other private objects (Part 9) start!

        // 8. Write Part 9: Other Objects
        for &h in others_private {
            let id = self.id_map[&h];
            self.write_indirect_object(id, 0, h)?;
        }

        Ok((hint_pos, s2_end, s7_start, s8_start))
    }

    pub(super) fn collect_lin_objects(
        // RR-15 Limit: Dispatcher - Sequential PDF linearization generator routing and sorting object tables, hint table, and headers
        &self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
    ) -> PdfResult<(
        Vec<Handle<Object>>,
        Vec<Handle<Object>>,
        Vec<Handle<Object>>,
        Vec<Handle<Object>>,
        Vec<u32>,
        Vec<Handle<Object>>,
        BTreeSet<Handle<Object>>,
        Vec<BTreeSet<Handle<Object>>>,
    )> {
        let mut all = BTreeSet::<Handle<Object>>::new();
        self.trace_reachable_handle(root, &mut all);
        if let Some(ih) = info {
            self.trace_reachable_handle(ih, &mut all);
        }

        let mut original_pages = Vec::new();
        let mut doc_reachable = BTreeSet::new();
        if let Some(dh) = self.arena.get_object(root).and_then(|o| o.as_dict_handle())
            && let Some(dict) = self.arena.get_dict(dh)
            && let Some(Object::Reference(ph)) = dict.get(&self.arena.name("Pages"))
        {
            self.collect_pages_recursive(*ph, &mut original_pages)?;
            doc_reachable.insert(*ph);
        }

        if original_pages.is_empty() {
            return Err(PdfError::refused(
                "linearize",
                format!("No pages found (Catalog root: {root:?})"),
            ));
        }

        let page_objects_set: BTreeSet<Handle<Object>> = original_pages.iter().copied().collect();
        let page_reachables = self.trace_page_reachables(&original_pages, &page_objects_set);
        let doc_reachable_set = self.trace_doc_reachable_selective(root, info, &page_objects_set);
        doc_reachable.extend(doc_reachable_set);

        let (outline_objs, outlines_root_h) = self.trace_outline_objects(root, &page_objects_set);
        let shared_objs =
            self.identify_shared_objects(root, info, &original_pages, &page_reachables);

        let mut assigned = BTreeSet::new();
        let mut page_obj_counts = Vec::new();
        let mut section6 = Vec::new();

        assigned.insert(root);
        if let Some(ih) = info {
            assigned.insert(ih);
        }
        let page1 = original_pages[0];
        assigned.insert(page1);

        let mut outline_exclusive = Vec::new();
        if let Some(root_h) = outlines_root_h {
            outline_exclusive.push(root_h);
        }
        for &h in &doc_reachable {
            if !shared_objs.contains(&h) && outline_objs.contains(&h) {
                if Some(h) != outlines_root_h {
                    outline_exclusive.push(h);
                }
            }
        }

        let mut p0_exclusive = Vec::new();
        for &h in &page_reachables[0] {
            if !shared_objs.contains(&h) {
                if assigned.insert(h) {
                    p0_exclusive.push(h);
                }
            }
        }
        p0_exclusive.sort();

        let mut first_page_shared = Vec::new();
        for &h in &page_reachables[0] {
            if shared_objs.contains(&h) {
                if assigned.insert(h) {
                    first_page_shared.push(h);
                }
            }
        }
        first_page_shared.sort();

        let mut doc_private = Vec::new();
        for &h in &doc_reachable {
            if !outline_objs.contains(&h) && h != root && Some(h) != info {
                if assigned.insert(h) {
                    doc_private.push(h);
                }
            }
        }
        doc_private.sort();

        for &h in &outline_exclusive {
            assigned.insert(h);
        }

        let mut section2_final = vec![root];
        if let Some(ih) = info {
            section2_final.push(ih);
        }
        section2_final.extend(doc_private.clone());
        section2_final.push(page1);
        section2_final.extend(p0_exclusive.clone());
        section2_final.extend(outline_exclusive.clone());
        section2_final.extend(first_page_shared.clone());

        let p0_count = section2_final
            .len()
            .saturating_sub(1)
            .saturating_sub(usize::from(info.is_some()))
            .saturating_sub(doc_private.len());
        page_obj_counts.push(p0_count as u32);

        for i in 1..original_pages.len() {
            let ph = original_pages[i];
            let mut page_exclusive = Vec::new();
            if assigned.insert(ph) {
                page_exclusive.push(ph);
            }
            for &h in &page_reachables[i] {
                if !shared_objs.contains(&h) {
                    if assigned.insert(h) {
                        page_exclusive.push(h);
                    }
                }
            }
            page_obj_counts.push(page_exclusive.len() as u32);
            section6.extend(page_exclusive);
        }

        let mut others: Vec<_> = all.into_iter().filter(|h| !assigned.contains(h)).collect();
        others.sort();

        let mut registered = BTreeSet::new();
        for &h in &first_page_shared {
            registered.insert(h);
        }
        for &h in &others {
            if shared_objs.contains(&h) {
                registered.insert(h);
            }
        }
        for &h in &shared_objs {
            if !registered.contains(&h) {
                log::debug!(
                    "DEBUG_MISSING_SHARED: object={:?}, id={}",
                    h,
                    self.id_map.get(&h).copied().unwrap_or(0)
                );
            }
        }

        Ok((
            section2_final,
            section6,
            others,
            original_pages,
            page_obj_counts,
            outline_exclusive,
            shared_objs,
            page_reachables,
        ))
    }
}
