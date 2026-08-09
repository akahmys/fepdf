//! PDF Physical Writer (Arena Bridge)
//!
//! This module serializes the refined PdfArena back into a physical PDF byte stream.

use fepdf_core::{Handle, Object, PdfArena, PdfError, PdfName, PdfResult};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

/// A physical PDF writer that serializes objects resolved from an arena.
pub struct PdfWriter<'a, W: Write> {
    inner: W,
    arena: &'a PdfArena,
    buffer: Vec<u8>,
    xref: BTreeMap<u32, usize>,
    compression_level: Option<u32>,
    id_map: BTreeMap<Handle<Object>, u32>,
    linearize: bool,
    sig_handle: Option<Handle<Object>>,
    string_encoding: crate::StringEncoding,
    security_handler: Option<fepdf_core::security::SecurityHandler>,
    current_obj_id: u32,
    current_obj_gen: u16,
    recursion_depth: u32,
    cached_file_id: Option<Vec<u8>>,
    obj_sizes: BTreeMap<u32, usize>,
}

impl<'a, W: Write> PdfWriter<'a, W> {
    /// Creates a new PdfWriter with arena access.
    pub fn new(inner: W, arena: &'a PdfArena) -> Self {
        Self {
            inner,
            arena,
            buffer: Vec::new(),
            xref: BTreeMap::new(),
            compression_level: None,
            id_map: BTreeMap::new(),
            linearize: false,
            sig_handle: None,
            string_encoding: crate::StringEncoding::default(),
            security_handler: None,
            current_obj_id: 0,
            current_obj_gen: 0,
            recursion_depth: 0,
            cached_file_id: None,
            obj_sizes: BTreeMap::new(),
        }
    }

    /// Sets the security handler for encryption.
    pub fn set_security_handler(&mut self, handler: fepdf_core::security::SecurityHandler) {
        self.security_handler = Some(handler);
    }

    /// Sets the encoding for string literals (Standard or Unicode).
    pub fn set_string_encoding(&mut self, encoding: crate::StringEncoding) {
        self.string_encoding = encoding;
    }

    /// Adds a handle to be targeted for digital signature patching.
    pub fn add_signature_target(&mut self, handle: Handle<Object>) {
        self.sig_handle = Some(handle);
    }

    /// Writes the PDF header with the specified version string.
    pub fn write_header(&mut self, version: &str) -> PdfResult<()> {
        self.write_all(format!("%PDF-{version}\r\n").as_bytes())?;
        self.write_all(b"%\xE2\xE3\xCF\xD3\r\n")?;
        Ok(())
    }

    /// Enables or disables PDF linearization (Fast Web View).
    pub fn set_linearize(&mut self, linearize: bool) {
        self.linearize = linearize;
    }

    /// Sets the Zlib compression level for streams (0-9).
    pub fn set_compression(&mut self, level: u32) {
        self.compression_level = Some(level.min(9));
    }

    /// Triggers a vacuum operation to remove unreferenced objects (implicit in linearization).
    pub fn set_vacuum(&mut self, _vacuum: bool) {
        // Implementation detail: vacuum is inherent in our ID remapping logic
    }

    /// Returns the current byte offset in the output buffer.
    pub fn current_offset(&self) -> usize {
        self.buffer.len()
    }

    fn encrypt_data(&self, data: &[u8]) -> PdfResult<Vec<u8>> {
        if let Some(sh) = &self.security_handler {
            sh.encrypt_stream(data, self.current_obj_id, self.current_obj_gen)
        } else {
            Ok(data.to_vec())
        }
    }

    /// Writes raw bytes directly to the output buffer.
    pub fn write_all(&mut self, data: &[u8]) -> PdfResult<()> {
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    /// Recursively serializes a high-level Object into the output buffer.
    pub fn write_object(&mut self, obj: &Object) -> PdfResult<()> {
        self.recursion_depth += 1;
        let res = match obj {
            Object::Boolean(b) => self.write_all(if *b { b"true" } else { b"false" }),
            Object::Integer(i) => self.write_all(i.to_string().as_bytes()),
            Object::Real(f) => {
                let s = format!("{f:.4}");
                let trimmed = s.trim_end_matches('0').trim_end_matches('.');
                self.write_all(trimmed.as_bytes())
            }
            Object::String(s) => self.write_string_obj(s),
            Object::Hex(s) => self.write_hex_obj(s),
            Object::Text(s) => self.write_text_obj(s),
            Object::Name(n) => self.write_name(n),
            Object::Array(h) => self.write_array_obj(*h),
            Object::Dictionary(h) => self.write_dictionary_obj(*h),
            Object::Stream(dh, data) => self.write_stream_obj(*dh, data),
            Object::Null => self.write_all(b"null"),
            Object::Reference(h) => self.write_reference_obj(*h),
        };
        self.recursion_depth -= 1;
        res
    }

    fn write_string_obj(&mut self, s: &[u8]) -> PdfResult<()> {
        let data = self.encrypt_data(s)?;
        self.write_string_literal(&data)
    }

    fn write_hex_obj(&mut self, s: &[u8]) -> PdfResult<()> {
        let data = self.encrypt_data(s)?;
        self.write_string_hex(&data)
    }

    fn write_text_obj(&mut self, s: &str) -> PdfResult<()> {
        let encoding_str = match self.string_encoding {
            crate::StringEncoding::Utf8 => "utf8",
            crate::StringEncoding::Utf16BE => "utf16be",
        };
        let encoded = fepdf_core::refine::text::encode_string(s, encoding_str);
        let data = self.encrypt_data(&encoded)?;
        self.write_string_literal(&data)
    }

    fn write_array_obj(&mut self, h: Handle<Vec<Object>>) -> PdfResult<()> {
        let a = self.arena.get_array(h).ok_or_else(|| PdfError::Other("Array not found".into()))?;
        self.write_all(b"[")?;
        for (i, item) in a.iter().enumerate() {
            if i > 0 {
                self.write_all(b" ")?;
            }
            self.write_object(item)?;
        }
        self.write_all(b"]")
    }

    fn write_dictionary_obj(
        &mut self,
        h: Handle<BTreeMap<Handle<PdfName>, Object>>,
    ) -> PdfResult<()> {
        let d =
            self.arena.get_dict(h).ok_or_else(|| PdfError::Other("Dictionary not found".into()))?;
        self.write_dict(&d)
    }

    fn write_reference_obj(&mut self, h: Handle<Object>) -> PdfResult<()> {
        let id = *self.id_map.get(&h).ok_or_else(|| {
            PdfError::Other(format!("Object {h:?} not in id_map during writing").into())
        })?;
        self.write_all(format!("{id} 0 R").as_bytes())
    }

    fn check_dct_filter(
        &self,
        d: &BTreeMap<Handle<PdfName>, Object>,
        filter_key: Handle<PdfName>,
    ) -> bool {
        d.get(&filter_key).is_some_and(|v| {
            let resolved = v.resolve(self.arena);
            if let Some(n) = resolved.as_name() {
                let s = self.arena.get_name_str(n).unwrap_or_default();
                s == "DCTDecode" || s == "DCT"
            } else if let Some(ah) = resolved.as_array() {
                self.arena.get_array(ah).unwrap_or_default().iter().any(|o| {
                    let s = o
                        .resolve(self.arena)
                        .as_name()
                        .and_then(|n| self.arena.get_name_str(n))
                        .unwrap_or_default();
                    s == "DCTDecode" || s == "DCT"
                })
            } else {
                false
            }
        })
    }

    fn resolve_stream_bytes(
        &self,
        d: &BTreeMap<Handle<PdfName>, Object>,
        filter_key: Handle<PdfName>,
        is_sublimated: bool,
        stream_bytes: &bytes::Bytes,
    ) -> (bytes::Bytes, bool) {
        let has_dct = self.check_dct_filter(d, filter_key);
        if has_dct {
            (stream_bytes.clone(), true)
        } else if is_sublimated {
            (stream_bytes.clone(), false)
        } else {
            let is_flate = d
                .get(&filter_key)
                .and_then(|o| o.resolve(self.arena).as_name())
                .and_then(|nh| self.arena.get_name_str(nh))
                .is_some_and(|s| s == "FlateDecode" || s == "Fl");

            if is_flate && self.compression_level.is_some() {
                (stream_bytes.clone(), true)
            } else {
                self.prepare_stream_data(stream_bytes, d, Some(filter_key))
            }
        }
    }

    fn write_stream_dictionary_keys(
        &mut self,
        d: &BTreeMap<Handle<PdfName>, Object>,
        length_key: Handle<PdfName>,
        filter_key: Handle<PdfName>,
        applied_new_compression: bool,
        already_filtered: bool,
    ) -> PdfResult<()> {
        for (k, v) in d {
            if *k == length_key
                || (*k == filter_key && (applied_new_compression || !already_filtered))
            {
                continue;
            }
            self.write_all(b"\r\n")?;
            self.write_name(k)?;
            self.write_all(b" ")?;
            self.write_object(v)?;
        }
        Ok(())
    }

    fn finalize_stream_data(
        &self,
        d: &BTreeMap<Handle<PdfName>, Object>,
        is_sublimated: bool,
        stream_bytes: &bytes::Bytes,
    ) -> PdfResult<(Vec<u8>, bool, bool)> {
        let filter_key = self.arena.name("Filter");
        let (stream_data, already_filtered) =
            self.resolve_stream_bytes(d, filter_key, is_sublimated, stream_bytes);

        let mut final_data = stream_data.to_vec();
        let applied_new_compression = self.try_compress_stream(&mut final_data, already_filtered);

        let type_key = self.arena.get_name_by_str("Type");
        let is_metadata = type_key
            .and_then(|tk| d.get(&tk))
            .and_then(|o| o.as_name())
            .and_then(|nh| self.arena.get_name_str(nh))
            .as_deref()
            == Some("Metadata");
        if let Some(sh) = &self.security_handler
            && (!is_metadata || sh.should_decrypt_metadata())
        {
            final_data =
                sh.encrypt_stream(&final_data, self.current_obj_id, self.current_obj_gen)?;
        }
        Ok((final_data, applied_new_compression, already_filtered))
    }

    fn write_stream_obj(
        &mut self,
        dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
        data: &std::sync::Arc<fepdf_core::object::SublimatedData>,
    ) -> PdfResult<()> {
        if self.recursion_depth > 1 {
            return Err(PdfError::Other(
                format!(
                    "Attempted to write an inline stream at depth {} in object {} (illegal in PDF)",
                    self.recursion_depth, self.current_obj_id
                )
                .into(),
            ));
        }

        let d = self
            .arena
            .get_dict(dh)
            .ok_or_else(|| PdfError::Other("Dictionary not found".into()))?;

        let is_sublimated = !matches!(**data, fepdf_core::object::SublimatedData::Raw(_));
        let stream_bytes = self.arena.get_stream_bytes(data)?;

        let (final_data, applied_new_compression, already_filtered) =
            self.finalize_stream_data(&d, is_sublimated, &stream_bytes)?;

        let length_key = self.arena.name("Length");
        let filter_key = self.arena.name("Filter");

        self.write_all(b"<<")?;
        self.write_stream_dictionary_keys(
            &d,
            length_key,
            filter_key,
            applied_new_compression,
            already_filtered,
        )?;

        if applied_new_compression {
            self.write_all(b"\r\n/Filter /FlateDecode")?;
        }

        self.write_all(format!("\r\n/Length {}", final_data.len()).as_bytes())?;
        self.write_all(b"\r\n>>\r\nstream\r\n")?;
        self.write_all(&final_data)?;
        self.write_all(b"\r\nendstream")
    }

    fn prepare_stream_data(
        &self,
        data: &bytes::Bytes,
        d: &BTreeMap<Handle<PdfName>, Object>,
        filter_key: Option<Handle<PdfName>>,
    ) -> (bytes::Bytes, bool) {
        if let Some(fk) = filter_key
            && d.contains_key(&fk)
        {
            if let Ok(decompressed) = self.arena.process_filters(data, d) {
                return (decompressed, false);
            }
            return (data.clone(), true);
        }
        (data.clone(), false)
    }

    fn try_compress_stream(&self, data: &mut Vec<u8>, already_filtered: bool) -> bool {
        if !already_filtered && let Some(level) = self.compression_level {
            use flate2::{Compression, write::ZlibEncoder};
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
            if std::io::Write::write_all(&mut encoder, data).is_ok()
                && let Ok(compressed) = encoder.finish()
            {
                *data = compressed;
                return true;
            }
        }
        false
    }

    fn write_dict(&mut self, d: &BTreeMap<Handle<PdfName>, Object>) -> PdfResult<()> {
        self.write_all(b"<<")?;
        for (k, v) in d {
            self.write_all(b"\r\n")?;
            self.write_name(k)?;
            self.write_all(b" ")?;
            self.write_object(v)?;
        }
        self.write_all(b"\r\n>>")
    }

    fn write_name(&mut self, n: &Handle<PdfName>) -> PdfResult<()> {
        let name =
            self.arena.get_name(*n).ok_or_else(|| PdfError::Other("Name not found".into()))?;
        self.write_all(b"/")?;
        for &b in name.as_ref() {
            if b == b'#'
                || b <= 32
                || b >= 127
                || b == b'('
                || b == b')'
                || b == b'<'
                || b == b'>'
                || b == b'['
                || b == b']'
                || b == b'{'
                || b == b'}'
                || b == b'/'
                || b == b'%'
            {
                self.write_all(format!("#{b:02X}").as_bytes())?;
            } else {
                self.write_all(&[b])?;
            }
        }
        Ok(())
    }

    fn write_string_literal(&mut self, s: &[u8]) -> PdfResult<()> {
        self.write_all(b"(")?;
        for &b in s {
            match b {
                b'(' => self.write_all(b"\\(")?,
                b')' => self.write_all(b"\\)")?,
                b'\\' => self.write_all(b"\\\\")?,
                _ => self.write_all(&[b])?,
            }
        }
        self.write_all(b")")
    }

    fn write_string_hex(&mut self, s: &[u8]) -> PdfResult<()> {
        self.write_all(b"<")?;
        for &b in s {
            self.write_all(format!("{b:02X}").as_bytes())?;
        }
        self.write_all(b">")
    }

    fn write_indirect_object(
        &mut self,
        id: u32,
        generation: u16,
        handle: Handle<Object>,
    ) -> PdfResult<()> {
        let start_pos = self.current_offset();
        self.xref.insert(id, start_pos);
        self.current_obj_id = id;
        self.current_obj_gen = generation;
        self.write_all(format!("{id} {generation} obj\r\n").as_bytes())?;
        let obj = self
            .arena
            .get_object(handle)
            .ok_or_else(|| PdfError::Other(format!("Object {id} missing").into()))?;
        self.write_object(&obj)?;
        self.write_all(b"\r\nendobj\r\n")?;
        let end_pos = self.current_offset();
        self.obj_sizes.insert(id, end_pos.saturating_sub(start_pos));
        Ok(())
    }

    /// Finalizes the PDF by writing trailers and cross-reference tables.
    pub fn finish(
        &mut self,
        root_handle: Handle<Object>,
        info_handle: Option<Handle<Object>>,
    ) -> PdfResult<()> {
        if self.linearize {
            self.finish_linearized(root_handle, info_handle)?;
        } else {
            self.finish_standard(root_handle, info_handle)?;
        }
        self.patch_signatures()?;
        self.inner.write_all(&self.buffer).map_err(PdfError::Io)?;
        self.inner.flush().map_err(PdfError::Io)?;
        Ok(())
    }

    /// Appends an incremental update to the output.
    pub fn write_incremental_update(
        &mut self,
        _root_handle: Handle<Object>,
        prev_xref: usize,
        total_objects: u32,
        changed_handles: &[(u32, Handle<Object>)],
    ) -> PdfResult<()> {
        let update_start = self.current_offset();
        for (id, handle) in changed_handles {
            self.write_indirect_object(*id, 0, *handle)?;
        }

        let xref_offset = self.current_offset();
        self.write_all(b"xref\r\n")?;
        for (id, _) in changed_handles {
            self.write_all(format!("{id} 1\r\n").as_bytes())?;
            let off = self.xref.get(id).copied().unwrap_or(0);
            self.write_all(format!("{off:010} 00000 n\r\n").as_bytes())?;
        }

        let id_bytes = self.generate_file_id(None);
        let id_hex = hex::encode(&id_bytes).to_uppercase();
        self.write_all(format!("trailer\r\n<< /Size {total_objects} /Prev {prev_xref} /Root 2 0 R /ID [<{id_hex}> <{id_hex}>] >>\r\nstartxref\r\n{xref_offset}\r\n%%EOF\r\n").as_bytes())?;

        self.inner.write_all(&self.buffer[update_start..]).map_err(PdfError::Io)?;
        self.inner.flush().map_err(PdfError::Io)?;
        Ok(())
    }

    fn patch_signatures(&mut self) -> PdfResult<()> {
        let Some(sig_h) = self.sig_handle else { return Ok(()) };
        let id =
            *self.id_map.get(&sig_h).ok_or_else(|| PdfError::Other("Sig object missing".into()))?;
        let off =
            *self.xref.get(&id).ok_or_else(|| PdfError::Other("Sig offset missing".into()))?;

        let end = self.find_obj_end(off);
        let (c_start, c_end) = self.find_contents_offsets(off, end)?;
        let (br_start, br_end) = self.find_byte_range_offsets(off, end)?;

        let br_str = format!(
            "0 {:010} {:010} {:010}",
            c_start - 1,
            c_end + 1,
            self.buffer.len() - (c_end + 1)
        );
        let br_bytes = br_str.as_bytes();
        if br_bytes.len() > (br_end - br_start) {
            return Err(PdfError::Other("ByteRange overflow".into()));
        }

        for i in br_start..br_end {
            self.buffer[i] = b' ';
        }
        self.buffer[br_start..br_start + br_bytes.len()].copy_from_slice(br_bytes);
        Ok(())
    }

    fn find_obj_end(&self, start: usize) -> usize {
        let mut end = start;
        while end + 6 <= self.buffer.len() {
            if &self.buffer[end..end + 6] == b"endobj" {
                return end + 6;
            }
            end += 1;
        }
        end
    }

    fn find_contents_offsets(&self, start: usize, end: usize) -> PdfResult<(usize, usize)> {
        let key = b"/Contents <";
        let pos = self.buffer[start..end]
            .windows(key.len())
            .position(|w| w == key)
            .ok_or_else(|| PdfError::Other("Missing /Contents".into()))?;
        let c_start = start + pos + 11;
        let c_end_pos = self.buffer[c_start..end]
            .iter()
            .position(|&b| b == b'>')
            .ok_or_else(|| PdfError::Other("Missing end of /Contents".into()))?;
        Ok((c_start, c_start + c_end_pos))
    }

    fn find_byte_range_offsets(&self, start: usize, end: usize) -> PdfResult<(usize, usize)> {
        let key = b"/ByteRange [";
        let pos = self.buffer[start..end]
            .windows(key.len())
            .position(|w| w == key)
            .ok_or_else(|| PdfError::Other("Missing /ByteRange".into()))?;
        let br_start = start + pos + 12;
        let br_end_pos = self.buffer[br_start..end]
            .iter()
            .position(|&b| b == b']')
            .ok_or_else(|| PdfError::Other("Missing end of /ByteRange".into()))?;
        Ok((br_start, br_start + br_end_pos))
    }

    fn finish_standard(
        &mut self,
        root_handle: Handle<Object>,
        info_handle: Option<Handle<Object>>,
    ) -> PdfResult<()> {
        let mut reachable = BTreeSet::<Handle<Object>>::new();
        self.trace_reachable_handle(root_handle, &mut reachable);
        if let Some(ih) = info_handle {
            self.trace_reachable_handle(ih, &mut reachable);
        }

        let mut sorted_handles: Vec<_> = reachable.into_iter().collect();
        sorted_handles.sort_by_key(|h| h.index());

        let mut next_id = 1;
        for &handle in &sorted_handles {
            self.id_map.insert(handle, next_id);
            next_id += 1;
        }

        for (current_id, &handle) in (1..).zip(&sorted_handles) {
            self.write_indirect_object(current_id, 0, handle)?;
        }

        let total_size = next_id;
        let start_xref = self.current_offset();
        self.write_all(format!("xref\r\n0 {total_size}\r\n0000000000 65535 f\r\n").as_bytes())?;
        for id in 1..total_size {
            let offset = self.xref.get(&id).copied().unwrap_or(0);
            self.write_all(format!("{offset:010} 00000 n\r\n").as_bytes())?;
        }

        let id_bytes = self.generate_file_id(info_handle);
        let id_hex = hex::encode(&id_bytes).to_uppercase();
        self.write_all(b"trailer\r\n<<\r\n")?;
        self.write_all(format!("/Size {total_size}\r\n").as_bytes())?;
        self.write_all(format!("/Root {} 0 R\r\n", self.id_map[&root_handle]).as_bytes())?;
        if let Some(ih) = info_handle {
            if let Some(&id) = self.id_map.get(&ih) {
                self.write_all(format!("/Info {id} 0 R\r\n").as_bytes())?;
            }
        }
        self.write_all(format!("/ID [<{id_hex}> <{id_hex}>]\r\n").as_bytes())?;
        self.write_all(b">>\r\nstartxref\r\n")?;
        self.write_all(start_xref.to_string().as_bytes())?;
        self.write_all(b"\r\n%%EOF\r\n")?;
        Ok(())
    }

    fn partition_and_collect_shared(
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

    fn calculate_worst_case_hint_size(
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

    fn pre_populate_obj_sizes(&mut self) {
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
    fn finish_linearized(
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
            *self.xref.get(&page1_id).ok_or_else(|| PdfError::Other("Page 1 missing".into()))?;

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

    fn collect_and_write_first_page_shared(
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

    fn collect_and_write_part8_shared(
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

    fn write_lin_objects_to_stream(
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

    fn collect_lin_objects(
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
            return Err(PdfError::Other(format!("No pages found (Catalog root: {root:?})").into()));
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

    fn write_object_to_bytes(&mut self, h: Handle<Object>) -> PdfResult<Vec<u8>> {
        let start = self.buffer.len();
        let obj =
            self.arena.get_object(h).ok_or_else(|| PdfError::Other("Object missing".into()))?;
        self.write_object(&obj)?;
        let bytes = self.buffer[start..].to_vec();
        self.buffer.truncate(start);
        Ok(bytes)
    }

    fn trace_page_reachables(
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

    fn trace_doc_reachable_selective(
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

    fn trace_outline_objects(
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

    fn identify_shared_objects(
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

    fn trace_dict_keys(
        &self,
        dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
        reachable: &mut BTreeSet<Handle<Object>>,
        assigned: &BTreeSet<Handle<Object>>,
        stack: &mut Vec<Handle<Object>>,
        exclude_keys: &[&str],
        exclude_objects: &BTreeSet<Handle<Object>>,
    ) {
        if let Some(d) = self.arena.get_dict(dh) {
            for (k, v) in d {
                let k_str = self.arena.get_name_str(k).unwrap_or_default();
                if exclude_keys.contains(&k_str.as_str()) {
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

    fn trace_reachable_selective(
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
        let _ = reachable.insert(h);

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

    fn trace_reachable_inline(
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
                if let Some(d) = self.arena.get_dict(*dh) {
                    for (k, v) in d {
                        let k_str = self.arena.get_name_str(k).unwrap_or_default();
                        if exclude_keys.contains(&k_str.as_str()) {
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
            _ => {}
        }
    }

    fn trace_reachable_no_parent(
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

    fn assign_lin_ids(
        // RR-15 Limit: Dispatcher - Assigns physical IDs to linearized PDF components
        &mut self,
        root: Handle<Object>,
        info: Option<Handle<Object>>,
        section2: &[Handle<Object>],
        section6: &[Handle<Object>],
        others: &[Handle<Object>],
        shared_objs: &BTreeSet<Handle<Object>>,
        _outline_exclusive: &[Handle<Object>],
        page1: Handle<Object>,
        first_page_reachables: &BTreeSet<Handle<Object>>,
    ) -> (u32, u32, u32, u32) {
        // (total_count, o_id, hint_stream_id, first_page_shared_count)
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

        // 2. Assign IDs contiguous starting at 1 for Section 6 (Remaining pages exclusive)
        let mut next_id = 1;
        for &h in section6 {
            self.id_map.insert(h, next_id);
            next_id += 1;
        }

        // Section 9 (other private objects)
        for &h in &others_private {
            self.id_map.insert(h, next_id);
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

        let first_page_shared_count = first_page_shared_set.len() as u32;
        let total_count = next_first_group_id;

        (total_count, o_id, hint_stream_id, first_page_shared_count)
    }

    fn reserve_lin_headers(&mut self, primary_count: u32, total_size: u32) -> (usize, usize) {
        let dict_pos = self.current_offset();
        self.xref.insert(primary_count, dict_pos); // REGISTER ID primary_count (O)
        self.buffer.extend(vec![b' '; 512]); // Shrink to 512 bytes to strictly comply with 1024-byte limit
        let p_xref_pos = self.current_offset();
        // Each entry is exactly 20 bytes. Allocate (entries * 20) + 256 safety margin for header/trailer
        let entries = (total_size as usize).saturating_sub(primary_count as usize) + 2;
        let reserve = (entries * 20) + 256;
        self.buffer.extend(vec![b' '; reserve]);
        (dict_pos, p_xref_pos)
    }

    fn reserve_hint_stream(
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

    fn write_lin_main_xref(
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

    fn generate_file_id(&mut self, info: Option<Handle<Object>>) -> Vec<u8> {
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

    fn write_lin_main_xref_standard(
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

    fn build_xref_stream_data(
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
                if let Some(&offset) = self.xref.get(&id) {
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

    fn build_xref_stream_dict(
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

    fn write_lin_main_xref_stream(
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
        let xref_id = total_size - 1;
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
            std::sync::Arc::new(fepdf_core::object::SublimatedData::Raw(stream_data.into())),
        );

        self.write_indirect_object(xref_id, 0, self.arena.alloc_object(stream_obj))?;
        self.write_all(format!("startxref\r\n{pxref_pos}\r\n%%EOF\r\n").as_bytes())?;
        Ok(actual_off)
    }

    fn finalize_lin_headers(
        // RR-15 Limit: Dispatcher - Sequentially finalizes and writes linearized PDF headers, trailers, and file IDs
        &mut self,
        s: LinState,
    ) -> PdfResult<()> {
        println!(
            "DEBUG_FINALIZE: first_shared_id={}, primary_count={}, obj_stm_count={}, shared_ids_len={}, first_page_groups_len={}",
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
            s.obj_stm_id.ok_or_else(|| PdfError::Other("Hint stream ID missing".into()))?;
        let page1_id = self.id_map[&s.pages[0]];

        // 1. Generate Hint Stream
        // Table F.5 Item 1: First object ID of all shared objects.
        let primary_start_id = s.shared_ids.first().copied().unwrap_or(page1_id);

        // Fix B (revised): hint_obj_total_size is exactly hint_size+149 because
        // reserve_hint_stream allocates: 128-byte dict header + hint_size data + 21-byte footer.
        // The previous xref-based lookup was fragile (failed when root is in an obj_stm).
        let hint_obj_total_size = s.hint_size + 149;
        log::debug!(
            "DEBUG_FINALIZE: primary_start_id={primary_start_id}, hint_pos={}, hint_obj_total_size={hint_obj_total_size}",
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

    fn overwrite_with_padding(
        &mut self,
        pos: usize,
        mut data: Vec<u8>,
        reserve_size: usize,
    ) -> PdfResult<()> {
        if data.len() > reserve_size {
            return Err(PdfError::Other(
                format!(
                    "Linearization header overflow: data len {} exceeds reserved {} at pos {}",
                    data.len(),
                    reserve_size,
                    pos
                )
                .into(),
            ));
        } else {
            // Pad with spaces
            data.extend(vec![b' '; reserve_size - data.len()]);
        }
        self.buffer[pos..pos + reserve_size].copy_from_slice(&data);
        Ok(())
    }

    fn build_lin_structures(
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
        let page1 = *pgs.first().ok_or_else(|| PdfError::Other("Page 1 missing".into()))?;

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

#[derive(Clone, Debug)]
struct SharedGroup {
    _is_shared: bool,
    _first_id: u32,
    _offset: usize,
    length: usize,
    count: usize,
}

struct LinState {
    dict_pos: usize,
    pxref_pos: usize,
    pxref_size: usize,
    hint_pos: usize,
    hint_size: usize,
    page1_offset: u32,
    page1_end: usize,
    s7_start: usize,
    s8_start: usize,
    main_xref_offset: usize,
    pages: Vec<Handle<Object>>,
    page_obj_counts: Vec<u32>,
    total_size: u32,
    primary_count: u32,
    obj_stm_id: Option<u32>,
    obj_stm_count: usize,
    info_handle: Option<Handle<Object>>,
    root: Handle<Object>,
    shared_ids: Vec<u32>,
    outline_exclusive: Vec<Handle<Object>>,
    first_page_groups: Vec<SharedGroup>,
    page_shared_refs: Vec<Vec<usize>>,
    first_page_shared_count: u32,
    first_shared_id: u32,
}

impl<W: std::io::Write> PdfWriter<'_, W> {
    #[allow(clippy::cast_possible_truncation)]
    fn generate_hint_tables(
        // RR-15 Limit: Dispatcher - Sequentially synthesizes and formats linearization hint tables
        &self,
        page_handles: &[Handle<Object>],
        shared_ids: &[u32],
        p1_offset: usize,
        page1_end: usize,
        _main_xref_offset: usize,
        s6_start: usize,
        _s8_start: usize,
        obj_counts: &[u32],
        primary_start_id: u32,
        outline_exclusive: &[Handle<Object>],
        first_page_groups: &[SharedGroup],
        page_shared_refs: &[Vec<usize>],
        hint_pos: usize,
        _hint_size: usize,
        hint_obj_total_size: usize, // Fix B: actual hint object total bytes (header+data+footer)
    ) -> (Vec<u8>, usize, Option<usize>) {
        let mut writer = BitWriter::new();

        let max_shared_refs = page_shared_refs.iter().map(|r| r.len()).max().unwrap_or(0);
        let bits_num_shared =
            (if max_shared_refs > 0 { 32 - (max_shared_refs as u32).leading_zeros() } else { 0 }
                as u8)
                .max(1);

        let max_shared_idx =
            page_shared_refs.iter().flat_map(|r| r.iter()).copied().max().unwrap_or(0);
        let bits_greatest_shared_idx =
            (if max_shared_idx > 0 { 32 - (max_shared_idx as u32).leading_zeros() } else { 0 }
                as u8)
                .max(1);

        // Helper to adjust absolute offsets for primary hint stream presence.
        // Fix B: subtract the *actual* hint object total size (not a hardcoded constant)
        // so that offsets reported in the hint table match qpdf's computed positions.
        let adjust_offset = |off: usize| -> u32 {
            if off > hint_pos {
                (off.saturating_sub(hint_obj_total_size)) as u32
            } else {
                off as u32
            }
        };

        // --- Page Offset Hint Table Header (Table F.3) ---
        writer.write_u32(1); // Item 1: Least number of objects in a page
        writer.write_u32(adjust_offset(p1_offset)); // Item 2: Location of first-page object (adjusted)
        writer.write_u16(16); // Item 3: Bits for object count delta
        writer.write_u32(0); // Item 4: Least page length
        writer.write_u16(32); // Item 5: Bits for page length delta
        writer.write_u32(0); // Item 6: Offset of first content stream
        writer.write_u16(0); // Item 7: Bits for content stream offset delta
        writer.write_u32(0); // Item 8: Least content stream length
        writer.write_u16(0); // Item 9: Bits for content stream length delta
        writer.write_u16(u16::from(bits_num_shared)); // Item 10: Bits for number of shared objects
        writer.write_u16(u16::from(bits_greatest_shared_idx)); // Item 11: Bits for greatest shared object index
        writer.write_u16(0); // Item 12: Bits for numerator of fraction (0 bits)
        writer.write_u16(0); // Item 13: Denominator of fraction (0)

        // --- Page Offset Hint Table Entries (Table F.4) - INTERLEAVED ---
        let page_count = page_handles.len();
        let mut lengths = Vec::with_capacity(page_count);
        for i in 0..page_count {
            let h = page_handles[i];
            let start_id = self.id_map[&h];
            let offset = *self.xref.get(&start_id).unwrap_or(&0);
            let next_off = if i + 1 < page_count {
                let next_page_start_id = self.id_map[&page_handles[i + 1]];
                *self.xref.get(&next_page_start_id).unwrap_or(&s6_start)
            } else {
                s6_start
            };

            let length = if i == 0 {
                (adjust_offset(page1_end) as usize).saturating_sub(adjust_offset(offset) as usize)
            } else {
                (adjust_offset(next_off) as usize).saturating_sub(adjust_offset(offset) as usize)
            };
            lengths.push(length);
            log::debug!(
                "DEBUG_HINT_TABLE_PAGE: page={}, start_id={}, offset={}, next_off={}, length={}, obj_count={}",
                i,
                start_id,
                offset,
                next_off,
                length,
                obj_counts.get(i).copied().unwrap_or(0)
            );
        }

        // a) Item 1: Object count delta for all pages
        for i in 0..page_count {
            let count_delta = obj_counts.get(i).copied().unwrap_or(1).saturating_sub(1);
            writer.write_bits(count_delta, 16);
        }
        writer.pad_to_alignment(8);

        // b) Item 2: Page length delta for all pages
        for &len in &lengths {
            writer.write_bits(len as u32, 32);
        }
        writer.pad_to_alignment(8);

        // c) Item 3: Number of shared objects referenced from the page (For the first page, this number shall be 0)
        if bits_num_shared > 0 {
            for (i, refs) in page_shared_refs.iter().enumerate().take(page_count) {
                let ref_count = if i == 0 { 0 } else { refs.len() };
                writer.write_bits(ref_count as u32, bits_num_shared);
            }
        }
        writer.pad_to_alignment(8);

        // d) Item 4: Shared object identifiers (Item 4 starts with the second page)
        if bits_greatest_shared_idx > 0 {
            for refs in page_shared_refs.iter().take(page_count).skip(1) {
                for &idx in refs {
                    writer.write_bits(idx as u32, bits_greatest_shared_idx);
                }
            }
        }
        writer.pad_to_alignment(8);

        // e) Item 5: Numerator of fractional position (0 bits per entry, matching Table F.3 Item 12)
        for refs in page_shared_refs.iter().take(page_count).skip(1) {
            for _ in 0..refs.len() {
                writer.write_bits(0, 0);
            }
        }
        writer.pad_to_alignment(8);

        // f) Item 6: Content stream offset delta for all pages (Table F.3 Item 7 = 0 bits)
        for _ in 0..page_count {
            writer.write_bits(0, 0);
        }
        writer.pad_to_alignment(8);

        // g) Item 7: Content stream length delta for all pages (Table F.3 Item 9 = 0 bits)
        for _ in 0..page_count {
            writer.write_bits(0, 0);
        }
        writer.pad_to_alignment(8);

        writer.pad_to_alignment(32);
        let p_len_bits = writer.total_bits();

        let least_shared_size: u32 = 0;
        let max_delta = first_page_groups
            .iter()
            .map(|g| g.length as u32)
            .chain(shared_ids.iter().map(|&id| *self.obj_sizes.get(&id).unwrap_or(&0) as u32))
            .max()
            .unwrap_or(0);
        let bits_shared_size = (32 - max_delta.leading_zeros() as u8).max(1);

        // 3. Shared Object Hint Table Header (Table F.5)
        let first_page_entry_count = first_page_groups.len() as u32;
        let total_shared_entry_count = first_page_entry_count + shared_ids.len() as u32;
        let bits_group_size: u16 = 0;

        writer.write_u32(primary_start_id); // Item 1: First object ID of all shared objects (Part 8 start ID)
        // Fix 3: Use the actual xref offset of the first Part 8 shared object (shared_ids[0])
        // rather than s6_start (the buffer cursor before Part 8 writing) which may differ.
        let first_part8_offset =
            shared_ids.first().and_then(|id| self.xref.get(id)).copied().unwrap_or(s6_start);
        log::debug!(
            "DEBUG_SHARED_OFFSET: s6_start={s6_start}, first_part8_offset={first_part8_offset}, adjust={}",
            adjust_offset(first_part8_offset)
        );
        writer.write_u32(adjust_offset(first_part8_offset)); // Item 2: Location of first shared object in Part 8 (adjusted)
        writer.write_u32(first_page_entry_count); // Item 3: Number of shared object entries for the first page
        writer.write_u32(total_shared_entry_count); // Item 4: Total number of shared object entries
        writer.write_u16(bits_group_size); // Item 5: Bits for number of objects in a group
        writer.write_u32(least_shared_size); // Item 6: Least length of a shared object group
        writer.write_u16(u16::from(bits_shared_size)); // Item 7: Bits for length difference

        // 4. Shared Object Hint Table Entries (Table F.6)
        // Per ISO 32000-2:2020 §F.4.3, "The order of items in each sequence shall be as follows".
        // The table is stored in COLUMN-MAJOR order per sequence:
        // SEQUENCE 1: first-page groups
        // SEQUENCE 2: Part 8 shared objects

        // --- Shared Object Hint Table Entries (Table F.6) ---
        // Per ISO 32000-2:2020 §F.4.3, "There shall be two sequences of shared object group entries:
        // the ones for objects located in the first page, followed by the ones for objects located
        // in the shared objects section... The order of items in each sequence shall be as follows".
        // The table is stored in COLUMN-MAJOR order per sequence:
        // SEQUENCE 1: first-page groups
        // SEQUENCE 2: Part 8 shared objects

        // --- Sequence 1 and 2 (Lengths) ---
        let mut seq1_deltas = Vec::new();
        for g in first_page_groups {
            let delta = (g.length as u32).saturating_sub(least_shared_size);
            seq1_deltas.push(delta);
            writer.write_bits(delta, bits_shared_size);
        }
        log::debug!("DEBUG_SEQ1_DELTAS: {seq1_deltas:?}");

        let mut seq2_deltas = Vec::new();
        let mut seq2_ids = Vec::new();
        for &id in shared_ids {
            let length = *self.obj_sizes.get(&id).unwrap_or(&0) as u32;
            let delta = length.saturating_sub(least_shared_size);
            seq2_deltas.push(delta);
            seq2_ids.push(id);
            writer.write_bits(delta, bits_shared_size);
        }
        log::debug!("DEBUG_SEQ2_DELTAS: {seq2_deltas:?}");
        log::debug!("DEBUG_SEQ2_IDS: {seq2_ids:?}");
        writer.pad_to_alignment(8);

        // --- Sequence 1 and 2 (MD5 Flags) ---
        for _ in 0..first_page_groups.len() {
            writer.write_bits(0, 1); // MD5 present flags Seq 1
        }
        for _ in 0..shared_ids.len() {
            writer.write_bits(0, 1); // MD5 present flags Seq 2
        }
        writer.pad_to_alignment(8);

        // --- Sequence 1 and 2 (Group Sizes) ---
        if bits_group_size > 0 {
            for g in first_page_groups {
                writer.write_bits((g.count - 1) as u32, bits_group_size as u8);
            }
            for _ in 0..shared_ids.len() {
                writer.write_bits(0, bits_group_size as u8); // 1 object per group Seq 2
            }
            writer.pad_to_alignment(8);
        }

        writer.pad_to_alignment(32);

        // Outline Hint Table (Table F.9)
        let mut outline_offset = None;
        if !outline_exclusive.is_empty() {
            let first_id = self.id_map[&outline_exclusive[0]];
            let first_off = *self.xref.get(&first_id).unwrap_or(&0);
            let last_id =
                outline_exclusive.last().and_then(|k| self.id_map.get(k)).copied().unwrap_or(0);
            let last_off = *self.xref.get(&last_id).unwrap_or(&0);
            let last_size = *self.obj_sizes.get(&last_id).unwrap_or(&0);
            let outlines_len = last_off.saturating_add(last_size).saturating_sub(first_off);

            writer.pad_to_alignment(32);
            let o_off = writer.total_bits() / 8;
            outline_offset = Some(o_off);

            writer.write_bits(first_id, 32);
            writer.write_bits(adjust_offset(first_off), 32);
            writer.write_bits(outline_exclusive.len() as u32, 32);
            writer.write_bits(outlines_len as u32, 32);
            writer.pad_to_alignment(32);
        }

        let data = writer.finish();
        (data, p_len_bits, outline_offset)
    }

    fn trace_reachable_handle(&self, h: Handle<Object>, reachable: &mut BTreeSet<Handle<Object>>) {
        if !reachable.insert(h) {
            return;
        }
        let mut stack = vec![h];
        while let Some(curr_h) = stack.pop() {
            let Some(obj) = self.arena.get_object(curr_h) else {
                continue;
            };
            match obj {
                Object::Reference(rh) => {
                    if reachable.insert(rh) {
                        stack.push(rh);
                    }
                }
                Object::Array(ah) => {
                    if let Some(a) = self.arena.get_array(ah) {
                        for item in a {
                            self.trace_reachable_handle_inline(&item, reachable, &mut stack);
                        }
                    }
                }
                Object::Dictionary(dh) | Object::Stream(dh, _) => {
                    if let Some(d) = self.arena.get_dict(dh) {
                        for v in d.values() {
                            self.trace_reachable_handle_inline(v, reachable, &mut stack);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn trace_reachable_handle_inline(
        &self,
        obj: &Object,
        reachable: &mut BTreeSet<Handle<Object>>,
        stack: &mut Vec<Handle<Object>>,
    ) {
        match obj {
            Object::Reference(rh) => {
                if reachable.insert(*rh) {
                    stack.push(*rh);
                }
            }
            Object::Array(ah) => {
                if let Some(a) = self.arena.get_array(*ah) {
                    for item in a {
                        self.trace_reachable_handle_inline(&item, reachable, stack);
                    }
                }
            }
            Object::Dictionary(dh) | Object::Stream(dh, _) => {
                if let Some(d) = self.arena.get_dict(*dh) {
                    for v in d.values() {
                        self.trace_reachable_handle_inline(v, reachable, stack);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_pages_recursive(
        &self,
        h: Handle<Object>,
        pages: &mut Vec<Handle<Object>>,
    ) -> PdfResult<()> {
        let Some(obj) = self.arena.get_object(h) else {
            return Ok(());
        };
        let Some(dh) = obj.as_dict_handle() else {
            return Ok(());
        };
        let dict = self.arena.get_dict(dh).unwrap_or_default();

        let type_name = dict.get(&self.arena.name("Type")).and_then(|v| v.as_name());
        let type_str = type_name.and_then(|tn| self.arena.get_name_str(tn));

        if type_str.as_deref() == Some("Page") {
            pages.push(h);
        } else {
            // Pages node
            if let Some(kids_h) = dict.get(&self.arena.name("Kids")).and_then(|v| v.as_array()) {
                if let Some(kids) = self.arena.get_array(kids_h) {
                    for kid in kids {
                        if let Some(kid_h) = kid.as_reference() {
                            self.collect_pages_recursive(kid_h, pages)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// --- Utilities ---

struct BitWriter {
    data: Vec<u8>,
    current_byte: u8,
    bits_used: u8,
    total_bits: usize,
}

impl BitWriter {
    fn new() -> Self {
        Self { data: Vec::new(), current_byte: 0, bits_used: 0, total_bits: 0 }
    }
    fn write_bits(&mut self, value: u32, count: u8) {
        for i in (0..count).rev() {
            let bit = (value >> i) & 1;
            self.current_byte = (self.current_byte << 1) | (bit as u8);
            self.bits_used += 1;
            self.total_bits += 1;
            if self.bits_used == 8 {
                self.data.push(self.current_byte);
                self.current_byte = 0;
                self.bits_used = 0;
            }
        }
    }
    fn write_u32(&mut self, val: u32) {
        self.write_bits(val, 32);
    }
    fn write_u16(&mut self, val: u16) {
        self.write_bits(u32::from(val), 16);
    }
    fn pad_to_alignment(&mut self, bit_alignment: usize) {
        let remainder = self.total_bits % bit_alignment;
        if remainder > 0 {
            let pad_count = bit_alignment - remainder;
            self.write_bits(0, pad_count as u8);
        }
    }
    fn total_bits(&self) -> usize {
        self.total_bits
    }
    fn finish(mut self) -> Vec<u8> {
        if self.bits_used > 0 {
            self.current_byte <<= 8 - self.bits_used;
            self.data.push(self.current_byte);
        }
        // Strict PDF 2.0 conformance: Pad the entire hint stream to a 32-bit (4-byte) boundary!
        let remainder = self.data.len() % 4;
        if remainder > 0 {
            let pad_bytes = 4 - remainder;
            self.data.extend(std::iter::repeat_n(0, pad_bytes));
        }
        self.data
    }
}
