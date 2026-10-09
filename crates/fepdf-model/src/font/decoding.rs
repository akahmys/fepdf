//! Splitting a string into a font's codes, and the Unicode each one decodes to.

use super::{FontResource, UnicodeSource, is_withheld, resolve_mapping};

impl FontResource {
    /// Decodes the next character code, returning its byte length and text.
    pub fn decode_next(&self, data: &[u8]) -> (usize, Option<String>) {
        let (consumed, text, _) = self.decode_next_sourced(data);
        (consumed, text)
    }

    /// [`FontResource::decode_next`], and **which route named the character**.
    ///
    /// The same three steps in the same order — this adds a label, not a decision. It
    /// exists because "213 of 360 glyphs have no Unicode" says nothing a reader can act
    /// on, while "213 needed the embedded font's own cmap" names the work.
    ///
    /// **There are three chains here, not one.** This is the outer one; its third step
    /// falls through to [`FontResource::unicode_for`], which repeats the `/ToUnicode` and
    /// encoding lookups the first two steps have already tried and failed. Each of the
    /// three carries its own copy of the private-use filter. None of that is changed
    /// here; it is now visible, which is the point of labelling before rebuilding.
    pub fn decode_next_sourced(&self, data: &[u8]) -> (usize, Option<String>, UnicodeSource) {
        if data.is_empty() {
            return (0, None, UnicodeSource::Unmapped);
        }
        if let Some(len) = self.code_length(data) {
            return self.decode_by_codespace(data, len);
        }
        let min_len = self.get_min_len();

        if let Some(res) = self.decode_via_to_unicode(data, min_len)
            && res.1.is_some()
        {
            return (res.0, res.1, UnicodeSource::ToUnicode);
        }
        if let Some(res) = self.decode_via_encoding(data, min_len)
            && res.1.is_some()
        {
            return (res.0, res.1, UnicodeSource::Encoding);
        }
        // The third step re-enters `unicode_for`, which reports its own route.
        self.decode_via_heuristics_sourced(data)
    }

    /// The codes `bytes` is made of, as this font reads them: by its CMap's codespace, or
    /// two bytes each for a composite font under `Identity` and one for a simple font.
    #[must_use]
    pub fn codes<'a>(&self, bytes: &'a [u8]) -> Vec<&'a [u8]> {
        let fixed = if self.is_cid_keyed { 2 } else { 1 };
        let mut out = Vec::new();
        let mut rest = bytes;
        while !rest.is_empty() {
            let len = self.code_length(rest).unwrap_or(fixed).clamp(1, rest.len());
            let Some((code, after)) = rest.split_at_checked(len) else { break };
            out.push(code);
            rest = after;
        }
        out
    }

    /// The code this font draws `character` by, when it has one: a simple font's byte, or
    /// a composite font's CID written as its CMap codes that CID — which under `Identity`
    /// is the CID itself, and under any other CMap is not.
    #[must_use]
    pub fn code_for(&self, character: char) -> Option<Vec<u8>> {
        let key = character.to_string();
        let mapped = self
            .unified_map
            .get(&key)
            .copied()
            .or_else(|| self.collection_unicode_to_cid.as_ref()?.get(&key).copied());
        if !self.is_cid_keyed {
            return Some(vec![u8::try_from(mapped?).ok()?]);
        }
        match self.encoding.as_ref() {
            // **A character has several CIDs, and a CMap reaches some of them**: Adobe-Japan1
            // gives `A` a proportional CID and a half-width one, and `90ms-RKSJ-H` codes
            // only the second. Each CID the collection gives the character is tried.
            Some(cmap) if !cmap.name.starts_with("Identity") => mapped
                .into_iter()
                .chain(self.collection_cids_of(&key))
                .find_map(|cid| cmap.code_for_cid(cid)),
            _ => Some(u16::try_from(mapped?).ok()?.to_be_bytes().to_vec()),
        }
    }

    /// Every CID the font's collection gives `text`, in order, read the other way through
    /// the collection's CID-to-Unicode table.
    pub(super) fn collection_cids_of(&self, text: &str) -> Vec<u32> {
        let Some(table) = self.collection_map.as_ref() else { return Vec::new() };
        table
            .mappings
            .iter()
            .filter(|(_, value)| value.as_str() == text)
            .filter_map(|(code, _)| <[u8; 2]>::try_from(code.as_slice()).ok())
            .map(|code| u32::from(u16::from_be_bytes(code)))
            .collect()
    }

    /// How long the code at the start of `data` is, by the codespace ranges of a Type 0
    /// font's CMap (9.7.6.2) — for a CMap other than `Identity`, whose codes are all two
    /// bytes and are read by the route below.
    ///
    /// **Every Type 0 font's codes were taken two at a time**, whatever its CMap said:
    /// `90ms-RKSJ-H` gives ASCII and half-width katakana one byte and kanji two, and
    /// `ABCあい` read as `≲𠌫`, each Latin letter eaten with the byte after it. A code that
    /// matches no range is taken at the shortest length a range has, which is the least a
    /// reader can step over (9.7.6.3).
    pub(super) fn code_length(&self, data: &[u8]) -> Option<usize> {
        if self.subtype.as_str() != "Type0" && !self.is_cid_keyed {
            return None;
        }
        let cmap = self.encoding.as_ref()?;
        if cmap.name.starts_with("Identity") || cmap.codespace_ranges.is_empty() {
            return None;
        }
        let matches = |(start, end): &(Vec<u8>, Vec<u8>)| {
            data.get(..start.len()).is_some_and(|code| {
                code.iter().zip(start.iter().zip(end)).all(|(b, (lo, hi))| (lo..=hi).contains(&b))
            })
        };
        let found = (1..=4).find(|len| {
            cmap.codespace_ranges.iter().any(|range| range.0.len() == *len && matches(range))
        });
        let shortest = cmap.codespace_ranges.iter().map(|(start, _)| start.len()).min()?;
        Some(found.unwrap_or(shortest).min(data.len()).max(1))
    }

    /// The code of `len` bytes at the start of `data`, read as 9.10.2 orders it: its
    /// `/ToUnicode` entry, or the character its CID is in the font's collection — the CID
    /// the CMap gives the code, not the code's own bytes, which are a CID only under
    /// `Identity`.
    pub(super) fn decode_by_codespace(
        &self,
        data: &[u8],
        len: usize,
    ) -> (usize, Option<String>, UnicodeSource) {
        let code = data.get(..len).unwrap_or(data);
        let kept = |text: String| {
            text.chars().next().filter(|c| is_withheld(*c, true).is_none()).map(|_| text)
        };
        if let Some(text) = self.to_unicode.as_ref().and_then(|map| map.map(code)).and_then(kept) {
            return (len, Some(text), UnicodeSource::ToUnicode);
        }
        let cid = self.encoding.as_ref().map_or(0, |cmap| cmap.to_cid(code));
        let text = u16::try_from(cid)
            .ok()
            .and_then(|cid| self.collection_map.as_ref()?.map(&cid.to_be_bytes()).and_then(kept));
        match text {
            Some(text) => (len, Some(text), UnicodeSource::CidCollection),
            None => (len, None, UnicodeSource::Unmapped),
        }
    }

    pub(super) fn get_min_len(&self) -> Option<usize> {
        let subtype = self.subtype.as_str();
        let is_multibyte =
            subtype == "Type0" || subtype == "CIDFontType0" || subtype == "CIDFontType2";
        let is_identity =
            self.encoding.as_ref().map(|e| e.name.contains("Identity")).unwrap_or(false);
        if is_multibyte || is_identity { Some(2) } else { None }
    }

    pub(super) fn decode_via_to_unicode(
        &self,
        data: &[u8],
        min_len: Option<usize>,
    ) -> Option<(usize, Option<String>)> {
        let tu = self.to_unicode.as_ref()?;
        let (len, u): (usize, Option<String>) = tu.decode_next_with_min_len(data, min_len)?;
        if let Some(u_str) = u {
            if u_str.chars().next().is_some_and(|c| is_withheld(c, false).is_some()) {
                return Some((len, None));
            }
            return Some((len, Some(u_str)));
        }
        Some((len, None))
    }

    pub(super) fn decode_via_encoding(
        &self,
        data: &[u8],
        min_len: Option<usize>,
    ) -> Option<(usize, Option<String>)> {
        let enc = self.encoding.as_ref()?;
        let (len, u): (usize, Option<String>) = enc.decode_next_with_min_len(data, min_len)?;
        if let Some(u_str) = u {
            let (uni, withheld) = resolve_mapping(u_str);
            return Some((len, if withheld.is_some() { None } else { Some(uni) }));
        }
        Some((len, None))
    }

    /// The CID collection's table, for a multibyte or identity-encoded font.
    ///
    /// **Only for those.** Applying it to a simple font evaporates the next byte, because
    /// the collection's CIDs are two bytes wide in this mapping and a one-byte code would
    /// swallow its successor.
    pub(super) fn decode_via_cid_collection(
        &self,
        data: &[u8],
    ) -> Option<(usize, Option<String>, UnicodeSource)> {
        let aj1 = self.collection_map.as_ref()?;
        let consumed = 2; // AJ1 CIDs are always 2 bytes in our mapping
        if data.len() < consumed {
            return None;
        }
        let u = aj1.map(data.get(..consumed)?)?;
        let c = u.chars().next()?;
        if is_withheld(c, true).is_some() {
            return None;
        }
        Some((consumed, Some(u), UnicodeSource::CidCollection))
    }

    pub(super) fn decode_via_heuristics_sourced(
        &self,
        data: &[u8],
    ) -> (usize, Option<String>, UnicodeSource) {
        let subtype = self.subtype.as_str();
        let is_multibyte =
            subtype == "Type0" || subtype == "CIDFontType0" || subtype == "CIDFontType2";
        let is_identity =
            self.encoding.as_ref().map(|e| e.name.contains("Identity")).unwrap_or(false);

        let consumed = if is_multibyte || is_identity { 2 } else { 1 };
        let Some(code_bytes) = data.get(..consumed) else {
            return (data.len(), None, UnicodeSource::Unmapped);
        };

        // 1. Try Adobe-Japan1 (AJ1) mapping for Japanese CIDFonts
        if (is_multibyte || is_identity)
            && let Some(found) = self.decode_via_cid_collection(data)
        {
            return found;
        }

        if !is_multibyte
            && !is_identity
            && !self.is_legacy_distiller
            && let Some(&code) = data.first()
            && (32..127).contains(&code)
        {
            return (
                1,
                Some(String::from_utf8_lossy(&[code]).to_string()),
                UnicodeSource::AsciiGuess,
            );
        }

        let is_simple = subtype == "Type1" || subtype == "TrueType" || subtype == "Type3";
        let has_reliable_map = self.to_unicode.is_some() || self.encoding.is_some();

        if is_simple
            && !self.is_legacy_distiller
            && !has_reliable_map
            && let &[code] = code_bytes
            && (0x20..=0x7E).contains(&code)
        {
            return (consumed, Some((code as char).to_string()), UnicodeSource::AsciiGuess);
        }

        let (text, source) = self.unicode_for(code_bytes);
        (consumed, text, source)
    }
}
