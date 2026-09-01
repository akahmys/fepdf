use crate::font::FontResource;
use crate::lexer::Token;
use bytes::Bytes;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Rewrites a content stream, normalising its text-showing operators.
pub fn restructure_content_stream(
    data: &[u8],
    fonts: &BTreeMap<String, Arc<FontResource>>,
) -> Bytes {
    let mut output = Vec::new();
    let mut stack = Vec::new();
    let mut current_font: Option<Arc<FontResource>> = None;

    let tokens = crate::lexer::tokenize(data);
    for token in tokens {
        match token {
            Token::Keyword(kw) => {
                handle_keyword(kw, &mut stack, &mut current_font, fonts, &mut output);
            }
            _ => stack.push(token),
        }
    }

    // Flush remaining
    for t in stack {
        write_token(&mut output, t);
    }

    Bytes::from(output)
}

fn handle_keyword(
    op: String,
    stack: &mut Vec<Token>,
    current_font: &mut Option<Arc<FontResource>>,
    fonts: &BTreeMap<String, Arc<FontResource>>,
    output: &mut Vec<u8>,
) {
    match op.as_str() {
        "Tf" => handle_font_selection(stack, current_font, fonts),
        "Tj" | "'" | "\"" | "TJ" => handle_text_show(&op, stack, current_font),
        _ => {}
    }

    for t in stack.drain(..) {
        write_token(output, t);
    }
    output.extend_from_slice(op.as_bytes());
    output.push(b' ');
}

fn handle_font_selection(
    stack: &mut Vec<Token>,
    current_font: &mut Option<Arc<FontResource>>,
    fonts: &BTreeMap<String, Arc<FontResource>>,
) {
    let size_opt = match stack.pop() {
        Some(Token::Real(f)) => Some(f),
        Some(Token::Integer(i)) => Some(i as f64),
        Some(t) => {
            stack.push(t);
            None
        }
        None => None,
    };
    if let Some(size) = size_opt {
        if let Some(Token::Name(name_bytes)) = stack.pop() {
            let name_str = String::from_utf8_lossy(&name_bytes).to_string();
            if let Some(font) = fonts.get(&name_str) {
                *current_font = Some(font.clone());
            }
            stack.push(Token::Name(name_bytes));
            stack.push(Token::Real(size));
        } else {
            stack.push(Token::Real(size));
        }
    }
}

fn handle_text_show(op: &str, stack: &mut [Token], current_font: &Option<Arc<FontResource>>) {
    let Some(font) = current_font.as_ref() else {
        return;
    };

    if op == "TJ" {
        if let Some(pos) = stack.iter().rposition(|t| t == &Token::LeftArray) {
            for token in &mut stack[pos + 1..] {
                apply_text_restructuring(token, font);
            }
        }
    } else if let Some(token) = stack.last_mut() {
        apply_text_restructuring(token, font);
    }
}

fn apply_text_restructuring(token: &mut Token, font: &FontResource) {
    let refined_bytes = match token {
        Token::String(s) => Some(restructure_string(s, font)),
        Token::Hex(s) => Some(restructure_string(s, font)),
        _ => None,
    };
    if let Some(bytes) = refined_bytes {
        *token = Token::Hex(Bytes::from(bytes));
    }
}

fn restructure_string(input: &[u8], font: &FontResource) -> Vec<u8> {
    if !font.has_any_mapping() {
        return input.to_vec();
    }

    let is_type0 = font.subtype.as_str() == "Type0";

    let mut result = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let (consumed, unicode_opt) = font.decode_next(&input[i..]);
        if consumed == 0 {
            result.extend_from_slice(&input[i..]);
            break;
        }

        let original_bytes = &input[i..i + consumed];

        if let Some(u) = unicode_opt.as_ref() {
            let mut mapped = false;

            // Only try to map if it's NOT already Identity-H or if we have a clear unified map.
            // For Identity-H, CID already equals GID in the PDF's view.
            let is_identity =
                font.encoding.as_ref().map(|e| e.name.contains("Identity")).unwrap_or(false);

            if !is_identity {
                if is_type0 {
                    if let Some(c) = u.chars().next()
                        && let Some(gid) = font.unicode_to_gid.get(&c)
                    {
                        let high = (gid >> 8) as u8;
                        let low = (gid & 0xFF) as u8;
                        result.push(high);
                        result.push(low);
                        mapped = true;
                    }
                } else if let Some(code) = font.unified_map.get(u) {
                    #[allow(clippy::cast_possible_truncation)]
                    result.push(*code as u8);
                    mapped = true;
                }
            }

            if !mapped {
                result.extend_from_slice(original_bytes);
            }
        } else {
            result.extend_from_slice(original_bytes);
        }

        i += consumed;
    }
    result
}

fn write_token(output: &mut Vec<u8>, token: Token) {
    token.write_to(output);
}

/// `0x18`–`0x1F`, the accents PDFDocEncoding puts where ASCII has controls.
const PDF_DOC_ACCENTS: [char; 8] = [
    '\u{02D8}', '\u{02C7}', '\u{02C6}', '\u{02D9}', '\u{02DD}', '\u{02DB}', '\u{02DA}', '\u{02DC}',
];

/// `0x80`–`0xA0`, the range where PDFDocEncoding departs from ISO Latin-1.
///
/// Taken from Annex D.2. Thirty-eight of the forty-one departures were read back out
/// of the table in the standard and matched (see the tests); `bullet`, `Zcaron` and
/// `zcaron` did not survive text extraction from the PDF of the standard, and are
/// fixed by elimination — their neighbours on both sides are confirmed, and `0x9F` is
/// the only undefined slot in the run.
const PDF_DOC_HIGH: [char; 33] = [
    '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', '\u{2014}', '\u{2013}', '\u{0192}', '\u{2044}',
    '\u{2039}', '\u{203A}', '\u{2212}', '\u{2030}', '\u{201E}', '\u{201C}', '\u{201D}', '\u{2018}',
    '\u{2019}', '\u{201A}', '\u{2122}', '\u{FB01}', '\u{FB02}', '\u{0141}', '\u{0152}', '\u{0160}',
    '\u{0178}', '\u{017D}', '\u{0131}', '\u{0142}', '\u{0153}', '\u{0161}', '\u{017E}', '\u{FFFD}',
    '\u{20AC}',
];

/// One byte of PDFDocEncoding (ISO 32000-2, Annex D.2).
fn pdf_doc_char(byte: u8) -> char {
    match byte {
        0x18..=0x1F => PDF_DOC_ACCENTS[(byte - 0x18) as usize],
        0x80..=0xA0 => PDF_DOC_HIGH[(byte - 0x80) as usize],
        // ASCII below, and from 0xA1 up PDFDocEncoding agrees with Latin-1, whose code
        // points are their own Unicode scalar values.
        _ => byte as char,
    }
}

/// How a string's encoding was settled, for the caller that has a decision log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEncoding {
    /// A byte order mark named the encoding: 7.9.2.2 UTF-16BE, or PDF 2.0 UTF-8.
    Declared,
    /// No byte order mark, so PDFDocEncoding — what 7.9.2.2 defines as the default.
    PdfDoc,
    /// Repaired: no byte order mark, but the bytes are UTF-16 and not text under
    /// PDFDocEncoding. Non-conformant, and the value is a guess.
    RepairedNakedUtf16,
    /// Repaired: a `FF FE` mark, which is UTF-16LE. 7.9.2.2 defines only `FE FF`, so
    /// this is non-conformant, but it is unambiguous about what was meant.
    RepairedUtf16Le,
}

/// Decodes PDF string bytes to text, reporting how the encoding was settled.
///
/// 7.9.2.2 admits exactly three encodings for a text string: PDFDocEncoding, UTF-16BE
/// with a byte order mark, and (PDF 2.0) UTF-8 with one. Anything without a mark is
/// therefore PDFDocEncoding by definition, and there is nothing to detect.
///
/// This used to run `chardetng` and then an explicit Shift-JIS pass over any string
/// without a mark. Measured across the corpus, every non-ASCII text string carries a
/// UTF-16BE mark except `intel_sdm.pdf`'s `/Title`, which is conforming PDFDocEncoding
/// — so the detector was reached by exactly one string in the corpus and corrupted it,
/// reading `0xAE 'registered'` as a halfwidth katakana and `0x90 'quoteright'` as the
/// leading byte of a kanji. Its guard asked whether the result contained a character
/// above U+3000, which the corruption itself satisfied. Shift-JIS is not one of the
/// encodings the standard admits, and the heuristic helped no file.
pub fn recover_string_reporting(bytes: &[u8]) -> (String, StringEncoding) {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        (decode_utf16(rest, u16::from_be_bytes), StringEncoding::Declared)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (decode_utf16(rest, u16::from_le_bytes), StringEncoding::RepairedUtf16Le)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (String::from_utf8_lossy(rest).into_owned(), StringEncoding::Declared)
    } else if let Some(text) = naked_utf16(bytes) {
        (text, StringEncoding::RepairedNakedUtf16)
    } else {
        (bytes.iter().copied().map(pdf_doc_char).collect(), StringEncoding::PdfDoc)
    }
}

/// Decodes PDF string bytes to text.
pub fn recover_string(bytes: &[u8]) -> String {
    recover_string_reporting(bytes).0
}

fn decode_utf16(bytes: &[u8], order: fn([u8; 2]) -> u16) -> String {
    let (chunks, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = chunks.iter().map(|&c| order(c)).collect();
    String::from_utf16_lossy(&units)
}

/// UTF-16 written without the byte order mark 7.9.2.2 requires.
///
/// Kept where the Shift-JIS detector was deleted because the two differ in kind: this
/// one is guarded by a property no PDFDocEncoding text can have — over half the bytes
/// in one position being NUL, which PDFDocEncoding has no way to spell — so it repairs
/// non-conformant files without being able to claim a conforming one. It fires on no
/// corpus file. The caller is told, so the repair is not silent.
fn naked_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let pairs = bytes.len() / 2;
    let (chunks, _) = bytes.as_chunks::<2>();
    let leading_nul = chunks.iter().filter(|c| c[0] == 0).count();
    let trailing_nul = chunks.iter().filter(|c| c[1] == 0).count();
    if leading_nul > pairs / 2 {
        Some(decode_utf16(bytes, u16::from_be_bytes))
    } else if trailing_nul > pairs / 2 {
        Some(decode_utf16(bytes, u16::from_le_bytes))
    } else {
        None
    }
}

/// Decodes a name object's bytes (ISO 32000-2, 7.3.5).
///
/// Names are not text strings and have their own rule: where a name is built from
/// externally specified text, "the sequence of bytes making up the name object should
/// be interpreted according to UTF-8". They were going through the text-string decoder,
/// which meant PDF names were being offered to a Shift-JIS detector.
pub fn recover_name(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[allow(clippy::collapsible_if)]
/// Encodes text into PDF string bytes using the named encoding.
pub fn encode_string(s: &str, encoding: &str) -> Vec<u8> {
    if encoding == "pdfdoc" || encoding == "auto" {
        if let Some(encoded) = try_encode_pdfdoc(s) {
            return encoded;
        }
    }

    if encoding == "utf8" || encoding == "auto" {
        let mut result = vec![0xEF, 0xBB, 0xBF];
        result.extend_from_slice(s.as_bytes());
        result
    } else {
        // Fallback to UTF-16BE
        let mut result = vec![0xFE, 0xFF];
        for c in s.encode_utf16() {
            result.extend_from_slice(&c.to_be_bytes());
        }
        result
    }
}

#[allow(clippy::cast_possible_truncation)]
fn try_encode_pdfdoc(s: &str) -> Option<Vec<u8>> {
    let mut result = Vec::with_capacity(s.len());
    for c in s.chars() {
        let cp = c as u32;
        if (0x20..=0x7E).contains(&cp) {
            result.push(cp as u8);
        } else {
            // Mapping for a few common PDFDocEncoding characters
            // For a full implementation, a lookup table would be needed.
            // Using a restricted subset for safety in this pass.
            match cp {
                0x00A0..=0x00FF => result.push(cp as u8), // ISO-8859-1 overlap
                0x20AC => result.push(0xA0), // Euro (special mapping in some versions, but let's be careful)
                0x2022 => result.push(0x80), // Bullet
                _ => return None,            // Cannot encode
            }
        }
    }
    Some(result)
}

#[cfg(test)]
mod string_encoding {
    use super::*;

    /// The bytes of `intel_sdm.pdf`'s `/Title`, which is conforming PDFDocEncoding and
    /// was the one string in the corpus that reached the Shift-JIS detector. Its XMP
    /// packet spells the same title in UTF-8, so the file states the right answer twice
    /// and the two must agree.
    #[test]
    fn pdf_doc_encoding_is_not_offered_to_a_japanese_detector() {
        let raw = b"Intel\xae 64 and IA-32 Software Developer\x90s Manual";
        let (text, how) = recover_string_reporting(raw);
        assert_eq!(text, "Intel® 64 and IA-32 Software Developer\u{2019}s Manual");
        assert_eq!(how, StringEncoding::PdfDoc);
    }

    /// Annex D.2, at the four points the standard states outright: `0x8B` perthousand
    /// and `0x83` ellipsis are given in the prose of 7.9.2.2, `0xAE` registered and
    /// `0xA0` Euro in the table itself.
    #[test]
    fn the_departures_from_latin_1_match_annex_d() {
        assert_eq!(pdf_doc_char(0x8B), '\u{2030}');
        assert_eq!(pdf_doc_char(0x83), '\u{2026}');
        assert_eq!(pdf_doc_char(0xAE), '\u{00AE}');
        assert_eq!(pdf_doc_char(0xA0), '\u{20AC}');
        // 0xA0 is the exception that proves the tail: from 0xA1 up it is Latin-1.
        assert_eq!(pdf_doc_char(0xA1), '\u{00A1}');
        assert_eq!(pdf_doc_char(0xFF), '\u{00FF}');
        // The one undefined slot in the run.
        assert_eq!(pdf_doc_char(0x9F), '\u{FFFD}');
    }

    #[test]
    fn a_byte_order_mark_settles_the_encoding() {
        let utf16be = [0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
        assert_eq!(
            recover_string_reporting(&utf16be),
            ("AB".to_string(), StringEncoding::Declared)
        );
        let utf8 = b"\xEF\xBB\xBFAB";
        assert_eq!(recover_string_reporting(utf8), ("AB".to_string(), StringEncoding::Declared));
    }

    /// The repair is kept, but it reports itself rather than passing as a plain read.
    #[test]
    fn utf16_without_a_mark_is_reported_as_a_repair() {
        let naked = [0x00, 0x41, 0x00, 0x42, 0x00, 0x43];
        let (text, how) = recover_string_reporting(&naked);
        assert_eq!(text, "ABC");
        assert_eq!(how, StringEncoding::RepairedNakedUtf16);

        let le = [0xFF, 0xFE, 0x41, 0x00];
        assert_eq!(
            recover_string_reporting(&le),
            ("A".to_string(), StringEncoding::RepairedUtf16Le)
        );
    }

    /// The guard that lets the repair coexist with PDFDocEncoding: a text string cannot
    /// spell NUL, so a majority of NULs in one position is not something it can produce.
    #[test]
    fn pdf_doc_text_is_never_mistaken_for_naked_utf16() {
        for raw in [&b"Intel\xae 64 and IA-32"[..], &b"\x90\x91\x92\x93"[..], &b"ab"[..]] {
            assert_eq!(recover_string_reporting(raw).1, StringEncoding::PdfDoc, "{raw:?}");
        }
    }

    /// 7.3.5: names are UTF-8, not text strings.
    #[test]
    fn names_are_utf8() {
        assert_eq!(recover_name("Adobe®".as_bytes()), "Adobe®");
        // The same bytes read as a text string mean something else entirely.
        assert_eq!(recover_string(b"\xC2\xAE"), "Â®");
    }
}
