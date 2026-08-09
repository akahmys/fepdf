use crate::font::FontResource;
use crate::lexer::Token;
use bytes::Bytes;
use std::collections::BTreeMap;
use std::sync::Arc;

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

fn handle_text_show(op: &str, stack: &mut [Token], current_font: &mut Option<Arc<FontResource>>) {
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

pub fn recover_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        // UTF-16BE
        let utf16_data: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16_lossy(&utf16_data)
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        // UTF-16LE (Non-standard but exists in some broken PDFs)
        let utf16_data: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16_lossy(&utf16_data)
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        // UTF-8 with BOM
        String::from_utf8_lossy(&bytes[3..]).to_string()
    } else {
        // HEURISTIC: Check for naked UTF-16BE (common in some non-compliant PDFs)
        // Look for alternating null bytes (0x00 0xXX or 0xXX 0x00)
        if bytes.len() >= 4 && bytes.len().is_multiple_of(2) {
            let be_zeros = bytes.chunks_exact(2).filter(|c| c[0] == 0).count();
            let le_zeros = bytes.chunks_exact(2).filter(|c| c[1] == 0).count();
            let total_chunks = bytes.len() / 2;

            if be_zeros > total_chunks / 2 {
                let utf16_data: Vec<u16> =
                    bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                return String::from_utf16_lossy(&utf16_data);
            } else if le_zeros > total_chunks / 2 {
                let utf16_data: Vec<u16> =
                    bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                return String::from_utf16_lossy(&utf16_data);
            }
        }

        // Use chardetng for Japanese/Legacy encoding detection (Refinery 2.1 upgrade)
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(bytes, true);
        let encoding = detector.guess(None, true);

        // Only override if it's a specific East Asian encoding, otherwise stick to PDFDocEncoding
        if encoding.name() == "Shift_JIS" || encoding.name() == "EUC-JP" || encoding.name() == "GBK"
        {
            let (decoded, _, malformed) = encoding.decode(bytes);
            if !malformed {
                return decoded.into_owned();
            }
        }

        // AGGRESSIVE FALLBACK: If chardetng failed (common for short strings),
        // try Shift-JIS explicitly if bytes look like Japanese Shift-JIS.
        if bytes.iter().any(|&b| b > 0x7F) {
            let (decoded, _, malformed) = encoding_rs::SHIFT_JIS.decode(bytes);
            if !malformed && decoded.chars().any(|c| (c as u32) >= 0x3000) {
                return decoded.into_owned();
            }
        }

        // Fallback to PDFDocEncoding (which is mostly ISO-8859-1 / ASCII)

        String::from_utf8_lossy(bytes).to_string()
    }
}

#[allow(clippy::collapsible_if)]
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
