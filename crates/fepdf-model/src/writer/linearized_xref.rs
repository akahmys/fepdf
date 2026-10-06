//! The linearised file's numbering, its cross-reference, and the hint stream that
//! indexes it.

#![allow(clippy::too_many_arguments, clippy::collapsible_if, clippy::type_complexity)]

use super::{LinState, PdfWriter, SharedGroup};
use crate::{Handle, Object, PdfError, PdfName, PdfResult};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

impl<'a, W: Write> PdfWriter<'a, W> {
    pub(super) fn assign_lin_ids(
        // RR-15 Limit: Dispatcher - Assigns physical IDs to linearized PDF components
        &mut self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        section2: &[Handle<Object>],
        section6: &[Handle<Object>],
        others: &[Handle<Object>],
        shared_objs: &BTreeSet<Handle<Object>>,
        outline_exclusive: &[Handle<Object>],
        page1: Handle<Object>,
        first_page_reachables: &BTreeSet<Handle<Object>>,
        (pages, page_counts): (&[Handle<Object>], &[u32]),
    ) -> (u32, u32, u32, u32, u32) {
        // (total_count, o_id, hint_stream_id, first_page_shared_count, first_page_shared_start)
        // 1. Partition others into shared and private exactly matching finish_linearized order
        let mut others_shared = Vec::new();
        let mut others_private = Vec::new();
        for &h in others {
            if shared_objs.contains(&h) {
                others_shared.push(h);
            } else {
                others_private.push(h);
            }
        }

        // Identify shared objects that are referenced on the first page
        let mut first_page_shared_set = BTreeSet::new();
        for &h in first_page_reachables {
            if shared_objs.contains(&h) {
                first_page_shared_set.insert(h);
            }
        }

        // 2. Part 7, page by page from the second (F.3.7): each page's objects numbered
        // contiguously from 1, the page object first, since a reader finds a page's first
        // number by adding up the counts before it. Where the save packs, what 7.5.7 and
        // Annex F let be compressed — not a stream, not a page object — goes into object
        // streams numbered in the page's range and written in its section; what they hold
        // is numbered with part 9's, last (ROADMAP Y-0b).
        let mut next_id = 1;
        let part7_packed = self.number_part7(section6, pages, page_counts, &mut next_id);

        // Section 9 (other private objects), and the object streams it is packed into,
        // numbered in this group so that the first-page cross-reference never names them
        // (ROADMAP Y-F22). **What the streams hold is numbered last of all**, after part
        // 8: a cross-reference stream may not list an uncompressed object after a
        // compressed one, which qpdf reads a linearised file by.
        self.lin_direct = outline_exclusive.iter().copied().collect();
        self.lin_containers.clear();
        let (packed, direct): (Vec<Handle<Object>>, Vec<Handle<Object>>) = if self.pack_objects {
            others_private.iter().partition(|h| self.packs_in_part9(**h))
        } else {
            (Vec::new(), others_private.clone())
        };
        for &h in &direct {
            self.id_map.insert(h, next_id);
            next_id += 1;
        }
        // The main cross-reference stream is itself uncompressed, so it is numbered here
        // too rather than last.
        self.lin_xref_id = (!packed.is_empty() || !part7_packed.is_empty()).then(|| {
            next_id += 1;
            next_id - 1
        });
        for _ in 0..packed.len().div_ceil(super::OBJECTS_PER_STREAM) {
            self.lin_containers.push(next_id);
            next_id += 1;
        }

        let second_group_count = next_id - 1;

        // 3. First group (Section 2) starts at O = second_group_count + 1
        let o_id = second_group_count + 1;
        let mut next_first_group_id = o_id + 1;

        // 1) Catalog (root)
        self.id_map.insert(root, next_first_group_id);
        next_first_group_id += 1;

        // 2) Info (if present)
        if let Some(ih) = info {
            self.id_map.insert(ih, next_first_group_id);
            next_first_group_id += 1;
        }

        // 3) doc_private (Part 4)
        let mut doc_private = Vec::new();
        for &h in section2 {
            if h == page1 {
                break;
            }
            if h != root && Some(h) != info {
                doc_private.push(h);
            }
        }
        for &h in &doc_private {
            self.id_map.insert(h, next_first_group_id);
            next_first_group_id += 1;
        }

        // 4) Hint stream (physically inserted here in write_lin_objects_to_stream)
        let hint_stream_id = next_first_group_id;
        next_first_group_id += 1;

        // 5) Page 1
        self.id_map.insert(page1, next_first_group_id);
        next_first_group_id += 1;

        // Sequentially assign IDs to non-shared objects in Section 2 (excluding root, page1, info, doc_private, and shared)
        for &h in section2 {
            if h != root
                && h != page1
                && Some(h) != info
                && !first_page_shared_set.contains(&h)
                && !self.id_map.contains_key(&h)
            {
                self.id_map.insert(h, next_first_group_id);
                next_first_group_id += 1;
            }
        }

        // Now assign IDs to ALL Shared Objects (Section 8: first-page shared and remaining other shared objects)
        // starting immediately after the first-group non-shared objects!
        // This ensures the First Xref Table is 100% contiguous from o_id up to the very last shared object ID!
        let first_page_shared_start = next_first_group_id;
        let mut fp_shared: Vec<Handle<Object>> = first_page_shared_set.iter().copied().collect();
        fp_shared.sort();
        for h in fp_shared {
            self.id_map.insert(h, next_first_group_id);
            next_first_group_id += 1;
        }

        // Remaining shared objects (not in first page)
        let mut remaining_shared = Vec::new();
        for &h in &others_shared {
            if !first_page_shared_set.contains(&h) {
                remaining_shared.push(h);
            }
        }
        remaining_shared.sort();
        for h in remaining_shared {
            self.id_map.insert(h, next_first_group_id);
            next_first_group_id += 1;
        }

        for &h in packed.iter().chain(&part7_packed) {
            self.id_map.insert(h, next_first_group_id);
            next_first_group_id += 1;
        }

        let first_page_shared_count = first_page_shared_set.len() as u32;
        let total_count = next_first_group_id;

        (total_count, o_id, hint_stream_id, first_page_shared_count, first_page_shared_start)
    }

    /// Numbers part 7 page by page from `next_id`, as [`Self::assign_lin_ids`] describes,
    /// and answers what the pages' object streams hold, to be numbered last.
    fn number_part7(
        &mut self,
        section6: &[Handle<Object>],
        pages: &[Handle<Object>],
        page_counts: &[u32],
        next_id: &mut u32,
    ) -> Vec<Handle<Object>> {
        let page_set: BTreeSet<Handle<Object>> = pages.iter().copied().collect();
        self.lin_part7.clear();
        let mut all_packed = Vec::new();
        let mut at = 0;
        for &count in page_counts.iter().skip(1) {
            let end = (at + count as usize).min(section6.len());
            let objects = &section6[at..end];
            at = end;
            let (packed, direct): (Vec<Handle<Object>>, Vec<Handle<Object>>) = objects
                .iter()
                .partition(|h| self.pack_objects && !page_set.contains(*h) && self.may_pack(**h));
            for &h in &direct {
                self.id_map.insert(h, *next_id);
                *next_id += 1;
            }
            let mut streams = Vec::new();
            for batch in packed.chunks(super::OBJECTS_PER_STREAM) {
                streams.push((*next_id, batch.to_vec()));
                *next_id += 1;
            }
            all_packed.extend(packed);
            self.lin_part7.push((direct, streams));
        }
        all_packed
    }

    pub(super) fn reserve_lin_headers(
        &mut self,
        primary_count: u32,
        first_page_end: u32,
    ) -> (usize, usize) {
        let dict_pos = self.current_offset();
        self.xref.insert(primary_count, dict_pos); // REGISTER ID primary_count (O)
        self.buffer.extend(vec![b' '; 512]); // Shrink to 512 bytes to strictly comply with 1024-byte limit
        let p_xref_pos = self.current_offset();
        // Each entry is exactly 20 bytes. Allocate (entries * 20) + 256 safety margin for header/trailer.
        // **The first page's entries, not every object numbered after it**: what part 9
        // packs is numbered last (ROADMAP Y-F22), and sized from the total this reserved
        // 5.7 MB of spaces in `intel_sdm.pdf`.
        let entries = (first_page_end as usize).saturating_sub(primary_count as usize) + 2;
        let reserve = (entries * 20) + 256;
        self.buffer.extend(vec![b' '; reserve]);
        (dict_pos, p_xref_pos)
    }

    pub(super) fn reserve_hint_stream(
        &mut self,
        hint_stream_id: u32,
        hint_size: usize,
    ) -> (usize, usize, usize, usize) {
        let pos = self.current_offset();
        self.xref.insert(hint_stream_id, pos); // REGISTER ID hint_stream_id

        // Match the 128-byte header dict exactly in the dummy pass
        let dummy_h_dict = format!(
            "{hint_stream_id} 0 obj\r\n<< /Length {hint_size} /S 00000 /O 00000 >>\r\nstream\r\n"
        );
        let pad_len = 128_usize.saturating_sub(dummy_h_dict.len());
        let full_dummy_dict = format!(
            "{hint_stream_id} 0 obj\r\n<< /Length {hint_size} /S 00000 /O 00000{} >>\r\nstream\r\n",
            " ".repeat(pad_len)
        );
        self.write_all(full_dummy_dict.as_bytes())
            .expect("Write of full dummy dict to in-memory buffer should succeed"); // RR-15 Safe: Writing to in-memory buffer does not fail

        let stream_start = self.current_offset();
        self.buffer.extend(vec![b' '; hint_size]);

        // Match the 21-byte footer exactly in the dummy pass
        let dummy_footer = "\r\nendstream\r\nendobj\r\n";
        self.write_all(dummy_footer.as_bytes())
            .expect("Write of dummy footer to in-memory buffer should succeed"); // RR-15 Safe: Writing to in-memory buffer does not fail

        (pos, stream_start, 0, 0)
    }

    pub(super) fn write_lin_main_xref(
        &mut self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        total_size: u32,
        primary_count: u32,
        pxref_pos: usize,
        obj_stm_id: Option<u32>,
        first_shared_id: u32,
        first_page_shared_count: u32,
    ) -> PdfResult<usize> {
        let info_id = info.map(|ih| self.id_map[&ih]);
        if obj_stm_id.is_some() {
            self.write_lin_main_xref_stream(
                root,
                info,
                total_size,
                primary_count,
                0,
                pxref_pos,
                first_shared_id,
                first_page_shared_count,
            )
        } else {
            self.write_lin_main_xref_standard(root, info_id, total_size, primary_count, pxref_pos)
        }
    }

    pub(super) fn generate_file_id(&mut self, info: Option<Handle<Object>>) -> Vec<u8> {
        if let Some(cached) = &self.cached_file_id {
            return cached.clone();
        }
        let mut hasher = md5::Context::new();
        if let Some(h) = info {
            if let Some(Object::Dictionary(dh)) = self.arena.get_object(h) {
                if let Some(dict) = self.arena.get_dict(dh) {
                    for (k, v) in dict {
                        hasher.consume(self.arena.get_name_str(k).unwrap_or_default().as_bytes());
                        hasher.consume(format!("{v:?}").as_bytes());
                    }
                }
            }
        }
        // Salt with a fixed but unique-ish string if metadata is empty
        hasher.consume(b"fepdf-sdk-v2.2.1");
        let id_bytes = hasher.finalize().0.to_vec();
        self.cached_file_id = Some(id_bytes.clone());
        id_bytes
    }

    pub(super) fn write_lin_main_xref_standard(
        &mut self,
        root: Handle<Object>,
        info_id: Option<u32>,
        total_size: u32,
        primary_count: u32,
        pxref_pos: usize,
    ) -> PdfResult<usize> {
        let off = self.current_offset();
        self.write_all(format!("xref\r\n0 {total_size}\r\n0000000000 65535 f\r\n").as_bytes())?;
        for id in 1..total_size {
            if id < primary_count {
                let o = self.xref.get(&id).copied().unwrap_or(0);
                if o == 0 {
                    self.write_all(b"0000000000 65535 f\r\n")?;
                } else {
                    self.write_all(format!("{o:010} 00000 n\r\n").as_bytes())?;
                }
            } else {
                self.write_all(b"0000000000 65535 f\r\n")?;
            }
        }
        let id_bytes = self.generate_file_id(None);
        let id_hex = hex::encode(&id_bytes).to_uppercase();
        let root_id = self.id_map.get(&root).copied().unwrap_or(2);
        let info_str =
            if let Some(ih) = info_id { format!(" /Info {ih} 0 R") } else { String::new() };
        self.write_all(format!("trailer\r\n<< /Size {total_size} /Root {root_id} 0 R{info_str} /ID [<{id_hex}> <{id_hex}>] >>\r\nstartxref\r\n{pxref_pos}\r\n%%EOF\r\n").as_bytes())?;
        Ok(off)
    }

    pub(super) fn build_xref_stream_data(
        &self,
        total_size: u32,
        xref_id: u32,
        actual_off: usize,
        primary_count: u32,
        first_shared_id: u32,
        first_page_shared_count: u32,
    ) -> Vec<u8> {
        let mut stream_data = Vec::with_capacity(total_size as usize * 7);
        for id in 0..total_size {
            let mut b = [0u8; 7];
            if id == 0 {
                b[0] = 0;
                b[1..5].copy_from_slice(&0u32.to_be_bytes());
                b[5..7].copy_from_slice(&65535u16.to_be_bytes());
            } else if id == xref_id {
                b[0] = 1;
                b[1..5].copy_from_slice(&(actual_off as u32).to_be_bytes());
                b[5..7].copy_from_slice(&0u16.to_be_bytes());
            } else if id >= primary_count && id < first_shared_id + first_page_shared_count {
                b[0] = 0;
                b[1..5].copy_from_slice(&0u32.to_be_bytes());
                b[5..7].copy_from_slice(&0u16.to_be_bytes());
            } else {
                b[5..7].copy_from_slice(&0u16.to_be_bytes());
                if let Some(super::Location::InStream { container, index }) = self.located.get(&id)
                {
                    // A type 2 entry: the object stream holding it, and where (7.5.8.3).
                    b[0] = 2;
                    b[1..5].copy_from_slice(&container.to_be_bytes());
                    b[5..7]
                        .copy_from_slice(&u16::try_from(*index).unwrap_or(u16::MAX).to_be_bytes());
                } else if let Some(&offset) = self.xref.get(&id) {
                    b[0] = 1;
                    b[1..5].copy_from_slice(&(offset as u32).to_be_bytes());
                } else {
                    b[0] = 0;
                    b[1..5].copy_from_slice(&0u32.to_be_bytes());
                }
            }
            stream_data.extend_from_slice(&b);
        }
        stream_data
    }

    pub(super) fn build_xref_stream_dict(
        &mut self,
        total_size: u32,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
    ) -> BTreeMap<Handle<PdfName>, Object> {
        let mut dict = BTreeMap::new();
        dict.insert(self.arena.name("Type"), Object::Name(self.arena.name("XRef")));
        dict.insert(self.arena.name("Size"), Object::Integer(i64::from(total_size)));
        dict.insert(
            self.arena.name("Index"),
            Object::Array(
                self.arena
                    .alloc_array(vec![Object::Integer(0), Object::Integer(i64::from(total_size))]),
            ),
        );
        dict.insert(
            self.arena.name("W"),
            Object::Array(self.arena.alloc_array(vec![
                Object::Integer(1),
                Object::Integer(4),
                Object::Integer(2),
            ])),
        );
        dict.insert(self.arena.name("Root"), Object::Reference(root));
        if let Some(ih) = info {
            dict.insert(self.arena.name("Info"), Object::Reference(ih));
        }
        let id_bytes = self.generate_file_id(info);
        dict.insert(
            self.arena.name("ID"),
            Object::Array(self.arena.alloc_array(vec![
                Object::Hex(id_bytes.clone().into()),
                Object::Hex(id_bytes.into()),
            ])),
        );
        dict
    }

    pub(super) fn write_lin_main_xref_stream(
        &mut self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        total_size: u32,
        primary_count: u32,
        _prev_xref: usize,
        pxref_pos: usize,
        first_shared_id: u32,
        first_page_shared_count: u32,
    ) -> PdfResult<usize> {
        let xref_id = self.lin_xref_id.unwrap_or(total_size - 1);
        let actual_off = self.current_offset();

        let stream_data = self.build_xref_stream_data(
            total_size,
            xref_id,
            actual_off,
            primary_count,
            first_shared_id,
            first_page_shared_count,
        );

        let dict = self.build_xref_stream_dict(total_size, root, info);
        let dict_h = self.arena.alloc_dict(dict);
        let stream_obj = Object::Stream(
            dict_h,
            std::sync::Arc::new(crate::object::SublimatedData::Raw(stream_data.into())),
        );

        self.write_indirect_object(xref_id, 0, self.arena.alloc_object(stream_obj))?;
        self.write_all(format!("startxref\r\n{pxref_pos}\r\n%%EOF\r\n").as_bytes())?;
        Ok(actual_off)
    }

    pub(super) fn finalize_lin_headers(
        // RR-15 Limit: Dispatcher - Sequentially finalizes and writes linearized PDF headers, trailers, and file IDs
        &mut self,
        s: LinState,
    ) -> PdfResult<()> {
        // `println!`, so `publish upgrade --linearize` printed linearisation internals
        // to the stdout its own output shares. `log::debug!` like the sibling below.
        log::debug!(
            "linearisation: first_shared_id={}, primary_count={}, obj_stm_count={}, shared_ids_len={}, first_page_groups_len={}",
            s.shared_ids.first().copied().unwrap_or(s.primary_count + s.obj_stm_count as u32),
            s.primary_count,
            s.obj_stm_count,
            s.shared_ids.len(),
            s.first_page_groups.len()
        );
        let id_bytes = self.generate_file_id(s.info_handle);
        let id_hex = hex::encode(&id_bytes).to_uppercase();

        let dict_id = s.primary_count;
        let hint_stream_id =
            s.obj_stm_id.ok_or_else(|| PdfError::internal("Hint stream ID missing"))?;
        let page1_id = self.id_map[&s.pages[0]];

        // 1. Generate Hint Stream
        // Table F.5 Item 1: First object ID of all shared objects.
        let primary_start_id = s.shared_ids.first().copied().unwrap_or(page1_id);

        // Fix B (revised): hint_obj_total_size is exactly hint_size+149 because
        // reserve_hint_stream allocates: 128-byte dict header + hint_size data + 21-byte footer.
        // The previous xref-based lookup was fragile (failed when root is in an obj_stm).
        let hint_obj_total_size = s.hint_size + 149;
        log::debug!(
            "linearisation: primary_start_id={primary_start_id}, hint_pos={}, hint_obj_total_size={hint_obj_total_size}",
            s.hint_pos
        );

        let (h_data, p_len_bits, outline_offset) = self.generate_hint_tables(
            &s.pages,
            &s.shared_ids,
            s.page1_offset as usize,
            s.page1_end,
            s.main_xref_offset,
            s.s7_start,
            s.s8_start,
            &s.page_obj_counts,
            primary_start_id, // Fix A: correct first shared object ID (Group 0 start)
            &s.outline_exclusive,
            &s.first_page_groups,
            &s.page_shared_refs,
            s.hint_pos,
            s.hint_size,
            hint_obj_total_size, // Fix B: real hint object byte size for adjust_offset
        );
        let p_len = p_len_bits.div_ceil(8); // Convert bits to bytes

        // 🚀 CRITICAL: Fully pad h_data to match exactly s.hint_size bytes!
        // This makes stream /Length, actual written size, and /H [offset size] 100% consistent!
        let mut full_h_data = h_data;
        if full_h_data.len() < s.hint_size {
            let diff = s.hint_size - full_h_data.len();
            full_h_data.extend(vec![0; diff]);
        }
        let data_len = s.hint_size; // Must write the reserved stream size including space padding!

        let h_dict = if let Some(o_off) = outline_offset {
            let base = format!(
                "{hint_stream_id} 0 obj\r\n<< /Length {data_len} /S {p_len} /O {o_off} >>\r\nstream\r\n"
            );
            let pad_len = 128_usize.saturating_sub(base.len());
            format!(
                "{hint_stream_id} 0 obj\r\n<< /Length {data_len} /S {p_len} /O {o_off}{} >>\r\nstream\r\n",
                " ".repeat(pad_len)
            )
        } else {
            let base = format!(
                "{hint_stream_id} 0 obj\r\n<< /Length {data_len} /S {p_len} >>\r\nstream\r\n"
            );
            let pad_len = 128_usize.saturating_sub(base.len());
            format!(
                "{hint_stream_id} 0 obj\r\n<< /Length {data_len} /S {p_len}{} >>\r\nstream\r\n",
                " ".repeat(pad_len)
            )
        };
        let h_footer = "\r\nendstream\r\nendobj\r\n";
        let mut full_h = Vec::new();
        full_h.extend_from_slice(h_dict.as_bytes());
        full_h.extend_from_slice(&full_h_data);
        full_h.extend_from_slice(h_footer.as_bytes());

        // 2. Generate Linearization Dictionary
        let t_val = if s.obj_stm_id.is_none() {
            // Standard XRef table. /T represents the offset of the first entry (object 0).
            // qpdf computes /T mathematically as main_xref_offset + 8 + total_size_digits.
            let total_size_digits = s.total_size.to_string().len();
            s.main_xref_offset + 8 + total_size_digits
        } else {
            // XRef stream. /T represents the offset of the stream object itself.
            s.main_xref_offset
        };

        let d_str = format!(
            "{} 0 obj\r\n<< /Linearized 1 /L {} /P 0 /O {} /E {} /N {} /T {} /H [{} {}] >>\r\nendobj\r\n",
            dict_id,
            self.buffer.len(),
            page1_id,
            s.page1_end,
            s.pages.len(),
            t_val,
            s.hint_pos,
            s.hint_size + 149 // Must report stream object length including overhead
        );
        self.overwrite_with_padding(s.dict_pos, d_str.into_bytes(), 512)?;

        // Overwrite hint stream (moves full_h)
        self.overwrite_with_padding(s.hint_pos, full_h, s.hint_size + 149)?;

        // Variables needed for first-page xref contiguous subsection below.
        let first_shared_id = s.first_shared_id;
        let non_shared_total = first_shared_id.saturating_sub(s.primary_count + 1);

        // 3. Generate First Xref Table (excludes the main XRef stream, and excludes Part 8 shared objects!)
        // ISO 32000-2 / F.3.4: "It shall consist of a single cross-reference subsection that has no free entries."
        // We combine first-page non-shared and first-page shared objects into a single contiguous subsection.
        let first_page_shared_count = s.first_page_shared_count;

        use std::fmt::Write as _;
        let mut px = String::new();

        let total_first_page_entries = non_shared_total + 1 + first_page_shared_count;
        let _ = write!(px, "xref\r\n{} {}\r\n", s.primary_count, total_first_page_entries);
        for id in s.primary_count..(s.primary_count + total_first_page_entries) {
            let offset = self.xref.get(&id).copied().unwrap_or(0);
            let _ = write!(px, "{offset:010} 00000 n\r\n");
        }
        // ISO 32000-2: The first-page trailer MUST contain a /Prev entry pointing to the main Xref.
        let root_id = self.id_map.get(&s.root).copied().unwrap_or(2);
        let info_str = if let Some(ih) = s.info_handle {
            let inf_id = self.id_map[&ih];
            format!(" /Info {inf_id} 0 R")
        } else {
            String::new()
        };
        let _ = write!(
            px,
            "trailer\r\n<< /Size {} /Prev {} /Root {} 0 R{} /ID [<{id_hex}> <{id_hex}>] >>\r\nstartxref\r\n0\r\n%%EOF\r\n",
            s.total_size, s.main_xref_offset, root_id, info_str
        );
        self.overwrite_with_padding(s.pxref_pos, px.into_bytes(), s.pxref_size)?;
        Ok(())
    }

    pub(super) fn overwrite_with_padding(
        &mut self,
        pos: usize,
        mut data: Vec<u8>,
        reserve_size: usize,
    ) -> PdfResult<()> {
        if data.len() > reserve_size {
            return Err(PdfError::internal(format!(
                "Linearization header overflow: data len {} exceeds reserved {} at pos {}",
                data.len(),
                reserve_size,
                pos
            )));
        } else {
            // Pad with spaces
            data.extend(vec![b' '; reserve_size - data.len()]);
        }
        self.buffer[pos..pos + reserve_size].copy_from_slice(&data);
        Ok(())
    }

    pub(super) fn build_lin_structures(
        // RR-15 Limit: Dispatcher - Assembles and structures physical state maps for PDF linearization
        &self,
        _root: Handle<Object>,
        _info: Option<Handle<Object>>,
        s2: &[Handle<Object>],
        pgs: &[Handle<Object>],
        outline_exclusive: &[Handle<Object>],
        _shared_objs: &BTreeSet<Handle<Object>>,
        page_reachables: &[BTreeSet<Handle<Object>>],
        _others_packable: &[Handle<Object>],
        _others_indirect: &[Handle<Object>],
        _obj_stm_ids: &[u32],
        shared_ids: &[u32],
        _obj_counts: &[u32],
        s2_end: usize,
        outline_offset_override: Option<usize>,
        dummy: bool,
    ) -> PdfResult<(Vec<SharedGroup>, Vec<Vec<usize>>, Option<(u32, usize, u32, usize)>)> {
        let page1 = *pgs.first().ok_or_else(|| PdfError::internal("Page 1 missing"))?;

        // Construct section2_physical matching exactly the physical write order of Part 6 (first-page) objects!
        let mut section2_physical = Vec::new();
        let mut found_page1 = false;
        for &h in s2 {
            if h == page1 {
                found_page1 = true;
            }
            if found_page1 {
                section2_physical.push(h);
            }
        }

        let mut first_page_shared_objs = BTreeSet::new();
        for &h in &page_reachables[0] {
            if _shared_objs.contains(&h) {
                first_page_shared_objs.insert(h);
            }
        }
        let mut first_page_shared_objs: Vec<_> = first_page_shared_objs.into_iter().collect();
        first_page_shared_objs.sort_by_key(|&h| self.id_map[&h]);
        let _first_page_shared_count = first_page_shared_objs.len();

        let mut first_page_groups = Vec::new();
        for &h in &section2_physical {
            let id = self.id_map[&h];
            let len = if dummy { 0 } else { *self.obj_sizes.get(&id).unwrap_or(&0) };
            let is_shared = _shared_objs.contains(&h);
            first_page_groups.push(SharedGroup {
                _first_id: id,
                count: 1,
                _offset: 0,
                length: len,
                _is_shared: is_shared,
            });
        }

        // Map shared first-page object ID to index in Shared Object Hint Table
        let get_shared_index = |id: u32| -> Option<usize> {
            if let Some(pos) = section2_physical.iter().position(|&x| self.id_map[&x] == id) {
                let x = section2_physical[pos];
                if _shared_objs.contains(&x) {
                    return Some(pos);
                } else {
                    return None;
                }
            }
            let seq1_len = first_page_groups.len();
            shared_ids.iter().position(|&x| x == id).map(|pos| seq1_len + pos)
        };

        // Construct page_shared_refs
        let mut page_shared_refs = Vec::new();
        let page_count = pgs.len();
        for p_reach in page_reachables.iter().take(page_count) {
            let mut refs = Vec::new();
            for &h in p_reach {
                let id = self.id_map[&h];
                if let Some(idx) = get_shared_index(id) {
                    refs.push(idx);
                }
            }
            refs.sort_unstable();
            refs.dedup();
            page_shared_refs.push(refs);
        }

        let outline_params = if !outline_exclusive.is_empty() {
            let first_outline_h = outline_exclusive[0];
            let first_outline_id = self.id_map[&first_outline_h];
            let outline_offset = if dummy {
                0
            } else {
                outline_offset_override
                    .unwrap_or_else(|| *self.xref.get(&first_outline_id).unwrap_or(&0))
            };
            let outline_count = outline_exclusive.len() as u32;
            let outline_length = if dummy { 0 } else { s2_end.saturating_sub(outline_offset) };
            Some((first_outline_id, outline_offset, outline_count, outline_length))
        } else {
            None
        };

        log::debug!("DEBUG_REFS: dummy={dummy}, page_shared_refs={page_shared_refs:?}");
        Ok((first_page_groups, page_shared_refs, outline_params))
    }
}
