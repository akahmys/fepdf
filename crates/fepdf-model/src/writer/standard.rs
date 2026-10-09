//! A file written straight through: the signature patched in, the objects, object
//! streams, the cross-reference and the encryption dictionary.

#![allow(clippy::too_many_arguments, clippy::collapsible_if, clippy::type_complexity)]

use super::{BYTE_RANGE_WIDTH, Location, OBJECTS_PER_STREAM, PdfWriter, SignatureHole};
use crate::{Handle, Object, PdfError, PdfResult};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

impl<'a, W: Write> PdfWriter<'a, W> {
    /// Writes the signature dictionary, reserving the two fields the file has to be
    /// finished before it can state.
    ///
    /// Neither placeholder goes through `write_object`, which is deliberate for
    /// `/Contents`: 7.6.2 exempts it from encryption, and every other string in the file
    /// is encrypted on the way out. Reserving it here means a future write-time security
    /// handler cannot reach it.
    pub(super) fn write_signature_dict(&mut self, obj: &Object) -> PdfResult<()> {
        let Some(dict_handle) = obj.as_dict_handle() else {
            return Err(PdfError::refused("sign", "the signature object is not a dictionary"));
        };
        let dict = self
            .arena
            .get_dict(dict_handle)
            .ok_or_else(|| PdfError::internal("the signature dictionary is missing"))?;
        let reserved = self.signature.as_ref().map_or(0, |s| s.reserved);

        self.write_all(b"<<")?;
        for (k, v) in &dict {
            let name = self.arena.get_name_str(*k).unwrap_or_default();
            if name == "ByteRange" || name == "Contents" {
                return Err(PdfError::refused(
                    "sign",
                    format!("the caller set /{name} on a signature dictionary"),
                ));
            }
            self.write_all(b"\r\n")?;
            self.write_name(k)?;
            self.write_all(b" ")?;
            self.write_object(v)?;
        }

        self.write_all(b"\r\n/ByteRange [")?;
        let byte_range = self.current_offset()..self.current_offset() + BYTE_RANGE_WIDTH;
        self.write_all(&[b'0'; BYTE_RANGE_WIDTH])?;
        self.write_all(b"]\r\n/Contents <")?;
        let contents = self.current_offset()..self.current_offset() + reserved * 2;
        self.write_all(&vec![b'0'; reserved * 2])?;
        self.write_all(b">\r\n>>")?;

        self.hole = Some(SignatureHole { byte_range, contents });
        Ok(())
    }

    /// Fills the hole reserved by [`Self::write_signature_dict`].
    ///
    /// The order is the whole of it. `/ByteRange` sits inside the range it describes, so
    /// it has to be final before anything is hashed; the digest then covers the file
    /// either side of `/Contents`, and the signature goes where the digest was taken
    /// around.
    /// The object number the catalogue was written under. Every reachable object is
    /// numbered before a trailer is written, so a catalogue without one is this writer's
    /// own mistake, said rather than panicked on.
    fn root_number(&self, root_handle: Handle<Object>) -> PdfResult<u32> {
        self.id_map
            .get(&root_handle)
            .copied()
            .ok_or_else(|| PdfError::internal("the catalogue was given no object number"))
    }

    pub(super) fn patch_signature(&mut self) -> PdfResult<()> {
        let Some(signature) = self.signature.take() else { return Ok(()) };
        let hole = self.hole.take().ok_or_else(|| {
            PdfError::refused("sign", "the signature object was never written; is it reachable?")
        })?;

        let gap_start = hole.contents.start - 1;
        let gap_end = hole.contents.end + 1;
        let stated =
            format!("0 {gap_start} {gap_end} {}", self.buffer.len() - gap_end).into_bytes();
        if stated.len() > hole.byte_range.len() {
            return Err(PdfError::refused(
                "sign",
                format!("this file needs a /ByteRange {} bytes wide", stated.len()),
            ));
        }
        let outside = || PdfError::internal("the signature's placeholder is outside the file");
        let field = self.buffer.get_mut(hole.byte_range.clone()).ok_or_else(outside)?;
        field.fill(b' ');
        field.get_mut(..stated.len()).ok_or_else(outside)?.copy_from_slice(&stated);

        let before = self.buffer.get(..gap_start).ok_or_else(outside)?;
        let after = self.buffer.get(gap_end..).ok_or_else(outside)?;
        let taken = crate::cms::digest(&[before, after]);
        let der = crate::cms::sign_detached(&taken, signature.identity)?;
        if der.len() * 2 > hole.contents.len() {
            return Err(PdfError::refused(
                "sign",
                format!(
                    "the signature is {} bytes and {} were reserved",
                    der.len(),
                    hole.contents.len() / 2
                ),
            ));
        }

        // Hex, in place: the field keeps its width, and what the signature does not
        // fill stays the zero padding a reader stops at once the DER is complete.
        let contents = self.buffer.get_mut(hole.contents.clone()).ok_or_else(outside)?;
        for (pair, byte) in contents.as_chunks_mut::<2>().0.iter_mut().zip(&der) {
            pair.copy_from_slice(format!("{byte:02X}").as_bytes());
        }
        Ok(())
    }

    pub(super) fn finish_standard(
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

        next_id = self.write_objects(&sorted_handles, next_id)?;

        // The `/Encrypt` dictionary is written last and is not in the arena, because it
        // is not part of the document: nothing references it, the catalogue cannot reach
        // it, and it describes the file rather than its contents.
        let encrypt_id = (self.artifacts.is_some() || self.recipients.is_some()).then(|| {
            let id = next_id;
            next_id += 1;
            id
        });
        if let Some(id) = encrypt_id {
            self.write_encrypt_dictionary(id)?;
        }

        if self.pack_objects {
            // 7.5.7 requires it: a classic table has no type 2 entry, so it cannot say
            // where a packed object lives.
            self.write_xref_stream(next_id, root_handle, info_handle, encrypt_id)
        } else {
            self.write_xref_and_trailer(next_id, root_handle, info_handle, encrypt_id)
        }
    }

    /// Writes every object, packing the ones 7.5.7 permits, and returns the next free
    /// object number.
    ///
    /// Object numbers are assigned before any of this, so packing changes where an
    /// object lives and never what it is called. A reference into a container is the
    /// same `N 0 R` it would be to a loose object, which is the property that lets the
    /// decision be made here rather than everywhere a reference is written.
    pub(super) fn write_objects(
        &mut self,
        sorted_handles: &[Handle<Object>],
        mut next_id: u32,
    ) -> PdfResult<u32> {
        let packable: Vec<(u32, Handle<Object>)> = if self.pack_objects {
            (1..)
                .zip(sorted_handles)
                .filter(|(_, h)| self.may_pack(**h))
                .map(|(i, h)| (i, *h))
                .collect()
        } else {
            Vec::new()
        };
        let packed: BTreeSet<u32> = packable.iter().map(|(id, _)| *id).collect();

        for (current_id, &handle) in (1..).zip(sorted_handles) {
            if !packed.contains(&current_id) {
                self.write_indirect_object(current_id, 0, handle)?;
            }
        }

        // Containers come after the objects that stayed loose, so that the numbers they
        // need are already assigned. Each is itself an ordinary indirect object.
        for batch in packable.chunks(OBJECTS_PER_STREAM) {
            let container = next_id;
            next_id += 1;
            self.write_object_stream(container, batch)?;
        }
        Ok(next_id)
    }

    /// Whether 7.5.7 allows this object inside an object stream.
    ///
    /// The clause names three exclusions and this engine adds a fourth. Streams cannot
    /// nest. Objects with a generation other than zero are excluded, which costs
    /// nothing here because this writer only ever emits generation zero. The `/Encrypt`
    /// dictionary is excluded, and is not in the arena to begin with. The fourth is the
    /// signature dictionary: its `/Contents` is a hole patched at a byte offset and its
    /// `/ByteRange` names offsets in the file, neither of which exists for an object
    /// that lives inside a compressed container.
    pub(super) fn may_pack(&self, handle: Handle<Object>) -> bool {
        if self.signature.as_ref().is_some_and(|s| s.handle == handle) {
            return false;
        }
        !matches!(self.arena.get_object(handle), Some(Object::Stream(..)) | None)
    }

    /// Writes one `/ObjStm` holding `batch`, and records where each object went.
    ///
    /// The objects are serialised without their `N 0 obj` wrapper — the header is the
    /// pair list at the front of the stream instead — and, per 7.6.2, their strings are
    /// not encrypted here: the container is encrypted around them, and encrypting both
    /// would encrypt them twice.
    pub(super) fn write_object_stream(
        &mut self,
        container: u32,
        batch: &[(u32, Handle<Object>)],
    ) -> PdfResult<()> {
        let was_inside = std::mem::replace(&mut self.inside_object_stream, true);
        let mut pairs = Vec::new();
        let mut body = Vec::new();
        for (index, &(id, handle)) in batch.iter().enumerate() {
            pairs.extend_from_slice(format!("{id} {} ", body.len()).as_bytes());
            body.extend_from_slice(&self.write_object_to_bytes(handle)?);
            body.push(b'\n');
            self.located.insert(id, Location::InStream { container, index });
        }
        self.inside_object_stream = was_inside;

        let first = pairs.len();
        let mut data = pairs;
        data.extend_from_slice(&body);

        let mut dict = BTreeMap::new();
        dict.insert(self.arena.name("Type"), Object::Name(self.arena.name("ObjStm")));
        dict.insert(self.arena.name("N"), Object::Integer(batch.len().cast_signed() as i64));
        dict.insert(self.arena.name("First"), Object::Integer(first.cast_signed() as i64));
        let dict_handle = self.arena.alloc_dict(dict);
        let stream = self.arena.alloc_object(Object::Stream(
            dict_handle,
            std::sync::Arc::new(crate::object::SublimatedData::Raw(data.into())),
        ));
        self.write_indirect_object(container, 0, stream)
    }

    /// The cross-reference stream (7.5.8), which is what object streams require.
    ///
    /// It is an ordinary indirect object that also stands in for the trailer, so it
    /// carries `/Root`, `/Info`, `/ID` and `/Encrypt` itself. 7.6.2 exempts it from
    /// encryption — a reader has to read it before it has a key — which is why it is
    /// assembled here rather than handed to `write_indirect_object`.
    pub(super) fn write_xref_stream(
        &mut self,
        total_size: u32,
        root_handle: Handle<Object>,
        info_handle: Option<Handle<Object>>,
        encrypt_id: Option<u32>,
    ) -> PdfResult<()> {
        let xref_id = total_size;
        let size = total_size + 1;
        let start_xref = self.current_offset();
        self.located.insert(xref_id, Location::InFile(start_xref));

        let rows = self.xref_rows(size);
        let compressed = self.compression_level.and_then(|level| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
            encoder.write_all(&rows).ok()?;
            encoder.finish().ok()
        });
        let flate = compressed.is_some();
        let data = compressed.unwrap_or(rows);

        let id_hex = hex::encode(self.generate_file_id(info_handle)).to_uppercase();
        let mut dictionary = format!(
            "<<\r\n/Type /XRef\r\n/Size {size}\r\n/W [1 4 2]\r\n/Root {} 0 R\r\n",
            self.root_number(root_handle)?
        );
        if let Some(id) = info_handle.and_then(|h| self.id_map.get(&h)) {
            dictionary.push_str(&format!("/Info {id} 0 R\r\n"));
        }
        if let Some(id) = encrypt_id {
            dictionary.push_str(&format!("/Encrypt {id} 0 R\r\n"));
        }
        dictionary.push_str(&format!("/ID [<{id_hex}> <{id_hex}>]\r\n"));
        if flate {
            dictionary.push_str("/Filter /FlateDecode\r\n");
        }
        dictionary.push_str(&format!("/Length {}\r\n>>", data.len()));

        self.xref.insert(xref_id, start_xref);
        self.write_all(format!("{xref_id} 0 obj\r\n{dictionary}\r\nstream\r\n").as_bytes())?;
        self.write_all(&data)?;
        self.write_all(b"\r\nendstream\r\nendobj\r\n")?;
        self.write_all(format!("startxref\r\n{start_xref}\r\n%%EOF\r\n").as_bytes())
    }

    /// One seven-byte row per object, in the layout `/W [1 4 2]` declares: one byte of
    /// type, four of offset-or-container, two of generation-or-index.
    ///
    /// Two bytes bound the index within a container at 65,535, and a container holds a
    /// hundred, so the field cannot overflow before the four-byte object number does.
    pub(super) fn xref_rows(&self, size: u32) -> Vec<u8> {
        let mut rows = Vec::with_capacity(size as usize * 7);
        for id in 0..size {
            let mut row = [0u8; 7];
            match self.located.get(&id) {
                Some(Location::InFile(offset)) => {
                    row[0] = 1;
                    row[1..5].copy_from_slice(&(*offset as u32).to_be_bytes());
                }
                Some(Location::InStream { container, index }) => {
                    row[0] = 2;
                    row[1..5].copy_from_slice(&container.to_be_bytes());
                    row[5..7].copy_from_slice(&(*index as u16).to_be_bytes());
                }
                // Object 0 heads the free list, and nothing else should be missing.
                None => {
                    row[0] = 0;
                    row[5..7].copy_from_slice(&65535u16.to_be_bytes());
                }
            }
            rows.extend_from_slice(&row);
        }
        rows
    }

    /// The cross-reference table and the trailer that names its root.
    pub(super) fn write_xref_and_trailer(
        &mut self,
        total_size: u32,
        root_handle: Handle<Object>,
        info_handle: Option<Handle<Object>>,
        encrypt_id: Option<u32>,
    ) -> PdfResult<()> {
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
        self.write_all(format!("/Root {} 0 R\r\n", self.root_number(root_handle)?).as_bytes())?;
        if let Some(ih) = info_handle {
            if let Some(&id) = self.id_map.get(&ih) {
                self.write_all(format!("/Info {id} 0 R\r\n").as_bytes())?;
            }
        }
        // The trailer's `/ID` is written here rather than through `write_object`, so it
        // is not encrypted — which 7.6.2 requires, and which matters because a reader
        // needs it before it has a key.
        self.write_all(format!("/ID [<{id_hex}> <{id_hex}>]\r\n").as_bytes())?;
        if let Some(id) = encrypt_id {
            self.write_all(format!("/Encrypt {id} 0 R\r\n").as_bytes())?;
        }
        self.write_all(b">>\r\nstartxref\r\n")?;
        self.write_all(start_xref.to_string().as_bytes())?;
        self.write_all(b"\r\n%%EOF\r\n")?;
        Ok(())
    }

    /// The `/Encrypt` dictionary of a document encrypted to certificates (7.6.5).
    ///
    /// `/Recipients` sits inside the crypt filter rather than at the top, which is where
    /// `/V` 4 and 5 put it — and where the reader looks for it. There is no `/O`, `/U`
    /// or `/P` here: the permissions travel inside each recipient's envelope, because
    /// 7.6.5 gives each of them their own.
    ///
    /// `/SubFilter /adbe.pkcs7.s5` is the one that goes with `/V 5`; `s3` and `s4`
    /// belong to the older versions this does not write, for the reason [ADR-0015]
    /// gives about the standard handler.
    ///
    /// [ADR-0015]: ../../../../docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md
    pub(super) fn write_public_key_encrypt_dictionary(
        &mut self,
        id: u32,
        recipients: &[Vec<u8>],
    ) -> PdfResult<()> {
        let entries = recipients
            .iter()
            .map(|entry| format!("<{}>", hex::encode_upper(entry)))
            .collect::<Vec<_>>()
            .join(" ");
        let dictionary = format!(
            "<<\r\n/Filter /Adobe.PubSec\r\n/SubFilter /adbe.pkcs7.s5\r\n/V 5\r\n/Length 256\r\n\
             /EncryptMetadata true\r\n/StmF /DefaultCryptFilter\r\n/StrF /DefaultCryptFilter\r\n\
             /CF << /DefaultCryptFilter << /CFM /AESV3 /AuthEvent /DocOpen /Length 256\r\n\
             /Recipients [ {entries} ] >> >>\r\n>>"
        );
        self.write_all(format!("{id} 0 obj\r\n{dictionary}\r\nendobj\r\n").as_bytes())
    }

    /// Writes the `/Encrypt` dictionary, which is the one dictionary in the file whose
    /// strings are not encrypted.
    ///
    /// 7.6.2: the strings in `/Encrypt` are exempt, and they have to be — `/U` is what a
    /// reader checks the password against, so encrypting it under the key that password
    /// unlocks would leave nothing able to open the file. This bypasses `write_object`
    /// for that reason, the same way the signature `/Contents` does.
    ///
    /// The values come from [`crate::security::EncryptionArtifacts`] rather than from a
    /// caller. A caller that could state `/U` could state one that does not match the
    /// key the handler is encrypting with, and the result would be a file nobody can
    /// open — detectable only by trying.
    pub(super) fn write_encrypt_dictionary(&mut self, id: u32) -> PdfResult<()> {
        // Both, because the two cross-reference forms read different maps: leaving
        // `located` unset made a cross-reference stream mark `/Encrypt` free, and a
        // reader that cannot find the encryption dictionary cannot open the file.
        self.xref.insert(id, self.current_offset());
        self.located.insert(id, Location::InFile(self.current_offset()));

        if let Some(recipients) = self.recipients.clone() {
            return self.write_public_key_encrypt_dictionary(id, &recipients);
        }
        let Some(artifacts) = self.artifacts.clone() else { return Ok(()) };
        let hex = |bytes: &[u8]| hex::encode_upper(bytes);
        // /V 5 /R 6 with a single AES-256 crypt filter applied to both streams and
        // strings. /Length is in bits here and in bytes inside the crypt filter, which
        // is the standard's own inconsistency and not a transcription slip.
        let dictionary = format!(
            "<<\r\n/Filter /Standard\r\n/V 5\r\n/R 6\r\n/Length 256\r\n\
             /CF << /StdCF << /CFM /AESV3 /AuthEvent /DocOpen /Length 32 >> >>\r\n\
             /StmF /StdCF\r\n/StrF /StdCF\r\n\
             /U <{}>\r\n/UE <{}>\r\n/O <{}>\r\n/OE <{}>\r\n/Perms <{}>\r\n\
             /P {}\r\n/EncryptMetadata {}\r\n>>",
            hex(&artifacts.u),
            hex(&artifacts.ue),
            hex(&artifacts.o),
            hex(&artifacts.oe),
            hex(&artifacts.perms),
            artifacts.permissions,
            artifacts.encrypt_metadata,
        );
        self.write_all(format!("{id} 0 obj\r\n{dictionary}\r\nendobj\r\n").as_bytes())
    }
}
