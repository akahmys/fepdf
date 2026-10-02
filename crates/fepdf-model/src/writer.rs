//! PDF Physical Writer (Arena Bridge)
//!
//! This module serializes the refined PdfArena back into a physical PDF byte stream.

#![allow(clippy::too_many_arguments, clippy::collapsible_if, clippy::type_complexity)]

// `PdfWriter`'s methods are in five files (ROADMAP Y-7): building blocks and `finish`
// here, and one file each for the file written straight through, the linearised layout,
// what it traces, and the linearised cross-reference.
/// The linearised layout: the parts, and what they share.
mod linearized;
/// The linearised file's numbering, its cross-reference, and its hint stream.
mod linearized_xref;
/// Which objects a page, an outline or the document reaches.
mod reachability;
/// A file written straight through: objects, object streams, cross-reference.
mod standard;

use crate::{Handle, Object, PdfArena, PdfError, PdfName, PdfResult};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

/// Supported text string encodings for PDF output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StringEncoding {
    /// Maximum compatibility using UTF-16BE with BOM (FE FF).
    #[default]
    Utf16BE,
    /// PDF 2.0 native UTF-8 with BOM (EF BB BF).
    Utf8,
}

/// Room left over the length a signature is measured at.
///
/// For the RSA keys [`SigningIdentity`] takes today the measurement is exact — the
/// signature is the width of the modulus and every other field is fixed — and a test in
/// `fepdf-syntax` holds that. It is a property of the algorithm rather than of CMS: an
/// ECDSA signature encodes two integers, whose DER length varies by a byte or two with
/// the values. The margin costs a few bytes in the file and keeps the reservation from
/// depending on which key was used.
const SIGNATURE_SLACK: usize = 32;

/// How deep a `/Kids` tree is followed before it is taken to be looping.
///
/// The same 64 the reader's page walk and the field-tree walks use. A depth and not a
/// visited set, for the reason [ADR-0060] gives: a page may legitimately be reached by
/// more than one path in a malformed-but-readable tree, and a set would drop it.
///
/// [ADR-0060]: ../../../docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md
const MAX_PAGE_TREE_DEPTH: usize = 64;

/// The width reserved for the four `/ByteRange` numbers, which are not known until the
/// file is complete: `0 ` and three ten-digit offsets. Ten digits is every file this
/// engine could write and several it could not.
const BYTE_RANGE_WIDTH: usize = 34;

/// The entries of an article bead (12.4.3) that lead away from its page: its thread, and
/// the next and previous beads. A linearised file places a bead with the page whose `/B`
/// names it (Annex F), and following these from every page reached every bead in the
/// document from each one: `samples/intel_sdm.pdf`, 5,057 pages, reached a median of
/// 4,979 objects per page and did not finish linearising in 240 s.
const BEAD_CHAIN_KEYS: [&str; 3] = ["T", "N", "V"];

/// What the writer needs to sign: which object carries the signature, and whose it is.
struct Signature<'a> {
    handle: Handle<Object>,
    identity: &'a crate::cms::SigningIdentity,
    reserved: usize,
}

/// How many objects go in one `/ObjStm`.
///
/// A container is decompressed whole to reach any one object in it, so the number
/// trades file size against the cost of reaching a single object. 100 is what
/// `samples/intel_sdm.pdf` uses across all 8,044 of its containers, which is as good a
/// reason as any to match it: the file this feature exists for was built that way.
const OBJECTS_PER_STREAM: usize = 100;

/// Where an object ended up, which is what a cross-reference stream records.
enum Location {
    /// Written at a byte offset in the file: a type 1 entry.
    InFile(usize),
    /// Held in an object stream, by that stream's object number and the index within
    /// it: a type 2 entry, which a classic cross-reference table cannot express. That
    /// is the whole reason 7.5.7 requires 7.5.8 alongside it.
    InStream { container: u32, index: usize },
}

/// Where the two fields that cannot be written until the file is finished ended up.
///
/// Both are recorded as they are written rather than searched for afterwards. Searching
/// the object for `/Contents <` would find one inside a `/Reason` string just as
/// happily, and a signature patched into the wrong place is a signature over the wrong
/// bytes.
struct SignatureHole {
    byte_range: std::ops::Range<usize>,
    contents: std::ops::Range<usize>,
}

/// A physical PDF writer that serializes objects resolved from an arena.
pub struct PdfWriter<'a, W: Write> {
    inner: W,
    arena: &'a PdfArena,
    buffer: Vec<u8>,
    xref: BTreeMap<u32, usize>,
    compression_level: Option<u32>,
    id_map: BTreeMap<Handle<Object>, u32>,
    linearize: bool,
    signature: Option<Signature<'a>>,
    hole: Option<SignatureHole>,
    string_encoding: StringEncoding,
    security_handler: Option<crate::security::SecurityHandler>,
    artifacts: Option<crate::security::EncryptionArtifacts>,
    /// The `/Recipients` entries, when the document is encrypted to certificates.
    recipients: Option<Vec<Vec<u8>>>,
    pack_objects: bool,
    /// Where each object went, for the cross-reference stream to record.
    located: BTreeMap<u32, Location>,
    /// Set while serialising into an object stream, where 7.6.2 says strings are not
    /// encrypted individually because the container is encrypted around them.
    inside_object_stream: bool,
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
            signature: None,
            hole: None,
            string_encoding: StringEncoding::default(),
            security_handler: None,
            artifacts: None,
            recipients: None,
            pack_objects: false,
            located: BTreeMap::new(),
            inside_object_stream: false,
            current_obj_id: 0,
            current_obj_gen: 0,
            recursion_depth: 0,
            cached_file_id: None,
            obj_sizes: BTreeMap::new(),
        }
    }

    /// Encrypts the output, writing the `/Encrypt` dictionary that opens it again.
    ///
    /// The handler and the artifacts arrive together because they are two halves of one
    /// thing: the handler holds the file key, which is never written, and the artifacts
    /// are what a reader needs to recover it. Setting one without the other produces
    /// either a file that cannot be opened or a file that is not encrypted, and both
    /// used to be reachable — `set_security_handler` took the handler alone, had no
    /// caller, and would have written ciphertext with no `/Encrypt` to describe it.
    pub fn encrypt_with(
        &mut self,
        handler: crate::security::SecurityHandler,
        artifacts: crate::security::EncryptionArtifacts,
    ) {
        self.security_handler = Some(handler);
        self.artifacts = Some(artifacts);
    }

    /// Encrypts the output to certificates (7.6.5), writing the `/Recipients` that open
    /// it again.
    ///
    /// The same pairing as [`Self::encrypt_with`] and for the same reason: the handler
    /// holds the key, the entries are what recovers it, and either alone produces a file
    /// that is unopenable or unencrypted.
    pub fn encrypt_to(
        &mut self,
        handler: crate::security::SecurityHandler,
        recipients: Vec<Vec<u8>>,
    ) {
        self.security_handler = Some(handler);
        self.recipients = Some(recipients);
    }

    /// Sets the encoding for string literals (Standard or Unicode).
    pub fn set_string_encoding(&mut self, encoding: StringEncoding) {
        self.string_encoding = encoding;
    }

    /// Signs the file with `identity`, putting the signature in `handle`.
    ///
    /// `handle` must be a dictionary, and must **not** carry `/ByteRange` or
    /// `/Contents`: a signature covers the file except itself, so neither value exists
    /// until the file is complete, and the writer supplies both. A caller that could
    /// state a byte range could state a wrong one, which is how the removed
    /// implementation came to write four constants.
    ///
    /// # Errors
    /// If the identity cannot produce a signature — reported here, before a hole has
    /// been reserved for one that will not arrive.
    pub fn sign_with(
        &mut self,
        handle: Handle<Object>,
        identity: &'a crate::cms::SigningIdentity,
    ) -> PdfResult<()> {
        let reserved = identity.signature_len()? + SIGNATURE_SLACK;
        self.signature = Some(Signature { handle, identity, reserved });
        Ok(())
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

    /// Packs objects into `/ObjStm` containers, and writes a cross-reference stream.
    ///
    /// The two are one switch because 7.5.7 makes them one: a classic cross-reference
    /// table has no entry type that can say where a packed object lives, so a file with
    /// object streams and a classic table is unreadable.
    pub fn set_pack_objects(&mut self, pack: bool) {
        self.pack_objects = pack;
    }

    /// Sets the Zlib compression level for streams (0-9).
    pub fn set_compression(&mut self, level: u32) {
        self.compression_level = Some(level.min(9));
    }

    /// Returns the current byte offset in the output buffer.
    pub fn current_offset(&self) -> usize {
        self.buffer.len()
    }

    fn encrypt_data(&self, data: &[u8]) -> PdfResult<Vec<u8>> {
        // 7.6.2: a string in an object stream is not encrypted on its own — the
        // container is encrypted around it. Encrypting here as well would encrypt it
        // twice, and no reader would undo the second layer.
        if self.inside_object_stream {
            return Ok(data.to_vec());
        }
        if let Some(sh) = &self.security_handler {
            Ok(sh.encrypt_stream(data, self.current_obj_id, self.current_obj_gen)?)
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
                // Six places, as the content serialiser uses: the two had diverged, so
                // a number in a dictionary was rounded harder than the same number in a
                // content stream. Four places turned the source's 18.157801 into
                // 18.1578, which is 378 of fy05.pdf's 4,574 objects differing for no
                // reason anyone chose.
                let s = format!("{f:.6}");
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
            StringEncoding::Utf8 => "utf8",
            StringEncoding::Utf16BE => "utf16be",
        };
        let encoded = crate::refine::text::encode_string(s, encoding_str);
        let data = self.encrypt_data(&encoded)?;
        self.write_string_literal(&data)
    }

    fn write_array_obj(&mut self, h: Handle<Vec<Object>>) -> PdfResult<()> {
        let a = self.arena.get_array(h).ok_or_else(|| PdfError::internal("Array not found"))?;
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
        let d = self.arena.get_dict(h).ok_or_else(|| PdfError::internal("Dictionary not found"))?;
        self.write_dict(&d)
    }

    fn write_reference_obj(&mut self, h: Handle<Object>) -> PdfResult<()> {
        let id = *self.id_map.get(&h).ok_or_else(|| {
            PdfError::internal(format!("Object {h:?} not in id_map during writing"))
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
        data: &std::sync::Arc<crate::object::SublimatedData>,
    ) -> PdfResult<()> {
        if self.recursion_depth > 1 {
            return Err(PdfError::internal(format!(
                "Attempted to write an inline stream at depth {} in object {} (illegal in PDF)",
                self.recursion_depth, self.current_obj_id
            )));
        }

        let d =
            self.arena.get_dict(dh).ok_or_else(|| PdfError::internal("Dictionary not found"))?;

        let is_sublimated = !matches!(**data, crate::object::SublimatedData::Raw(_));
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
        let name = self.arena.get_name(*n).ok_or_else(|| PdfError::internal("Name not found"))?;
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

    /// Writes a literal string, escaping every byte that would not survive being read
    /// back (7.3.4.2).
    ///
    /// The carriage return is the one that is easy to miss. 7.3.4.2 says an end-of-line
    /// marker inside a literal string, unescaped, "shall be treated as a byte value of
    /// (0Ah)" — so a raw `\r` written here comes back as `\n`, and a raw `\r\n` comes
    /// back as one byte instead of two. This escaped only the parentheses and the
    /// backslash, and the resulting corruption was invisible for as long as the strings
    /// were text: `fepdf`'s own lexer returns a raw `\r` unchanged, so the two mistakes
    /// cancelled and every round trip through this engine was clean. Encrypting made
    /// every string a random byte sequence, one in 256 of which is a carriage return,
    /// and PDFKit read the difference.
    fn write_string_literal(&mut self, s: &[u8]) -> PdfResult<()> {
        self.write_all(b"(")?;
        for &b in s {
            match b {
                b'(' => self.write_all(b"\\(")?,
                b')' => self.write_all(b"\\)")?,
                b'\\' => self.write_all(b"\\\\")?,
                b'\r' => self.write_all(b"\\r")?,
                b'\n' => self.write_all(b"\\n")?,
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
        self.located.insert(id, Location::InFile(start_pos));
        self.current_obj_id = id;
        self.current_obj_gen = generation;
        self.write_all(format!("{id} {generation} obj\r\n").as_bytes())?;
        let obj = self
            .arena
            .get_object(handle)
            .ok_or_else(|| PdfError::internal(format!("Object {id} missing")))?;
        if self.signature.as_ref().is_some_and(|s| s.handle == handle) {
            self.write_signature_dict(&obj)?;
        } else {
            self.write_object(&obj)?;
        }
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
            // The hint tables state object sizes worked out ahead of the write, and the
            // signature dictionary is the one object whose written size does not match
            // what serialising it predicts. Rather than teach the estimate about the
            // reservation, refuse: nothing asks for both, `save_signed` and
            // `save_linearized` being separate paths.
            if self.signature.is_some() {
                return Err(PdfError::refused(
                    "linearize",
                    "a signed file cannot also be linearized",
                ));
            }
            // The linearized path builds its own trailer, and nothing there writes an
            // `/Encrypt` or names one. Encrypting the objects anyway would produce a
            // file of ciphertext that declares itself plaintext — unopenable, and
            // unopenable in the way that looks like corruption rather than like a
            // password prompt.
            if self.artifacts.is_some() {
                return Err(PdfError::refused(
                    "linearize",
                    "an encrypted file cannot also be linearized",
                ));
            }
            self.finish_linearized(root_handle, info_handle)?;
        } else {
            self.finish_standard(root_handle, info_handle)?;
        }
        self.patch_signature()?;
        self.inner.write_all(&self.buffer).map_err(PdfError::Io)?;
        self.inner.flush().map_err(PdfError::Io)?;
        Ok(())
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

    /// Collects the page objects under `h`, in `/Kids` order.
    ///
    /// **Bounded since 2026-09-05**, on RR-15 Rule 6's terms rather than on a crash seen
    /// from a file. A `/Kids` naming an ancestor made this follow it forever, and the
    /// only reason no document reached it is that `Document::open` gets there first and
    /// *expands* the cycle instead: a two-node loop around one page arrives here as a
    /// flat list of sixteen references to that page. Depending on that is depending on a
    /// defect, so the bound does not.
    fn collect_pages_recursive(
        &self,
        h: Handle<Object>,
        pages: &mut Vec<Handle<Object>>,
    ) -> PdfResult<()> {
        self.collect_pages_at(h, pages, 0)
    }

    fn collect_pages_at(
        &self,
        h: Handle<Object>,
        pages: &mut Vec<Handle<Object>>,
        depth: usize,
    ) -> PdfResult<()> {
        if depth >= MAX_PAGE_TREE_DEPTH {
            return Ok(());
        }
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
                            self.collect_pages_at(kid_h, pages, depth + 1)?;
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

#[cfg(test)]
mod page_tree {
    use super::{MAX_PAGE_TREE_DEPTH, PdfWriter};
    use crate::arena::PdfArena;
    use crate::handle::Handle;
    use crate::object::Object;
    use std::collections::BTreeMap;

    /// A `/Kids` that names an ancestor is not followed forever.
    ///
    /// **This is the one walk in the sweep of 2026-09-05 that no file can reach**, and
    /// the reason is not reassuring: `Document::open` meets a cyclic page tree first and
    /// expands it rather than refusing it, so the writer is handed a flat list and never
    /// sees the loop. The test therefore builds the arena directly, which is the only way
    /// to put the writer in front of the thing it was not bounded against.
    ///
    /// Verified by removing the bound: `fatal runtime error: stack overflow`.
    #[test]
    fn a_kids_that_names_an_ancestor_is_not_followed_forever() {
        let arena = PdfArena::new();
        let kids = arena.name("Kids");
        let type_key = arena.name("Type");
        let pages = arena.name("Pages");

        let top = arena.alloc_object(Object::Null);
        let below = arena.alloc_object(Object::Null);

        let node = |kid: Handle<Object>| {
            let mut d = BTreeMap::new();
            d.insert(type_key, Object::Name(pages));
            d.insert(kids, Object::Array(arena.alloc_array(vec![Object::Reference(kid)])));
            Object::Dictionary(arena.alloc_dict(d))
        };
        arena.set_object(top, node(below));
        arena.set_object(below, node(top));

        let writer = PdfWriter::new(Vec::new(), &arena);
        let mut found = Vec::new();
        writer.collect_pages_recursive(top, &mut found).expect("the walk returns");

        assert!(found.is_empty(), "neither node is a /Page, so nothing is collected");
    }

    /// A page tree as deep as the bound allows still yields its page.
    ///
    /// Without this, tightening the bound to nothing passes the test above.
    #[test]
    fn a_page_within_the_bound_is_still_collected() {
        let arena = PdfArena::new();
        let kids = arena.name("Kids");
        let type_key = arena.name("Type");
        let pages = arena.name("Pages");
        let page = arena.name("Page");

        let mut leaf = BTreeMap::new();
        leaf.insert(type_key, Object::Name(page));
        let mut current = arena.alloc_object(Object::Dictionary(arena.alloc_dict(leaf)));

        for _ in 0..(MAX_PAGE_TREE_DEPTH - 2) {
            let mut d = BTreeMap::new();
            d.insert(type_key, Object::Name(pages));
            d.insert(kids, Object::Array(arena.alloc_array(vec![Object::Reference(current)])));
            current = arena.alloc_object(Object::Dictionary(arena.alloc_dict(d)));
        }

        let writer = PdfWriter::new(Vec::new(), &arena);
        let mut found = Vec::new();
        writer.collect_pages_recursive(current, &mut found).expect("the walk returns");

        assert_eq!(found.len(), 1, "the page at the bottom of a legal tree is reached");
    }
}

#[cfg(test)]
mod beads {
    use super::PdfWriter;
    use crate::arena::PdfArena;
    use crate::handle::Handle;
    use crate::object::Object;
    use std::collections::{BTreeMap, BTreeSet};

    /// One article thread through `count` pages, a bead on each, the way 12.4.3 lays it
    /// out: `/N` and `/V` close the chain into a ring, every bead names the thread in
    /// `/T` (Table 160 requires it only of the first), and each page names its bead in `/B`.
    fn threaded(arena: &PdfArena, count: usize) -> (Vec<Handle<Object>>, Vec<Handle<Object>>) {
        let key = |k: &str| arena.name(k);
        let pages: Vec<_> = (0..count).map(|_| arena.alloc_object(Object::Null)).collect();
        let beads: Vec<_> = (0..count).map(|_| arena.alloc_object(Object::Null)).collect();
        let thread = arena.alloc_object(Object::Null);

        let mut t = BTreeMap::new();
        t.insert(key("F"), Object::Reference(beads[0]));
        arena.set_object(thread, Object::Dictionary(arena.alloc_dict(t)));
        for i in 0..count {
            let mut b = BTreeMap::new();
            b.insert(key("T"), Object::Reference(thread));
            b.insert(key("N"), Object::Reference(beads[(i + 1) % count]));
            b.insert(key("V"), Object::Reference(beads[(i + count - 1) % count]));
            b.insert(key("P"), Object::Reference(pages[i]));
            let rect = vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(9),
                Object::Integer(9),
            ];
            b.insert(key("R"), Object::Array(arena.alloc_array(rect)));
            arena.set_object(beads[i], Object::Dictionary(arena.alloc_dict(b)));

            let mut p = BTreeMap::new();
            p.insert(key("Type"), Object::Name(key("Page")));
            let list = vec![Object::Reference(beads[i])];
            p.insert(key("B"), Object::Array(arena.alloc_array(list)));
            arena.set_object(pages[i], Object::Dictionary(arena.alloc_dict(p)));
        }
        (pages, beads)
    }

    /// A page reaches itself and its own bead, and no other page's.
    ///
    /// Verified by removing the `BEAD_CHAIN_KEYS` check: every page then reaches all
    /// four beads and the thread.
    #[test]
    fn a_page_reaches_its_own_bead_and_not_the_chain() {
        let arena = PdfArena::new();
        let (pages, beads) = threaded(&arena, 4);
        let set: BTreeSet<_> = pages.iter().copied().collect();

        let writer = PdfWriter::new(Vec::new(), &arena);
        let reached = writer.trace_page_reachables(&pages, &set);

        for (i, reach) in reached.iter().enumerate() {
            let own = BTreeSet::from([pages[i], beads[i]]);
            assert_eq!(reach, &own, "page {i} reaches itself and its bead");
        }
    }
}
