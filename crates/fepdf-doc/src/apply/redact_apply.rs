//! A document's own redaction annotations, applied (12.5.6.23, ROADMAP Y-10).
//!
//! **The second step of the two 12.5.6.23 describes.** A `/Redact` annotation marks what
//! is to go — the quadrilaterals of `/QuadPoints`, or `/Rect` — and says what is drawn in
//! its place. Applying it removes what those regions cover as a caller's regions are
//! removed ([`super::redact::remove_regions`]), removes the annotation, and draws as
//! Table 195 says:
//!
//! - `/RO`, a form, with its origin at the lower-left corner of `/Rect`; nothing else is
//!   read;
//! - else `/IC` over the regions, then `/OverlayText` in the font and colour `/DA` names,
//!   placed by `/Q` and repeated over the region where `/Repeat` is true;
//! - else nothing: **the region is left transparent**.

use super::text::GlyphBox;
use fepdf_model::font::FontResource;
use fepdf_model::interpretation::Decision;
use fepdf_model::lexer::{Lexer, Token};
use fepdf_model::{DictHandle, Document, Handle, Object, PdfArena, PdfResult};
use std::fmt::Write as _;
use std::sync::Arc;

/// One redaction annotation, read.
struct Mark {
    rect: GlyphBox,
    regions: Vec<GlyphBox>,
    dict: DictHandle,
}

/// Applies every redaction annotation on each of `pages`.
///
/// # Errors
/// Fails when a page is not there, or what a region covers cannot be removed.
pub fn apply_redact_annotations(doc: &Document, pages: &[usize]) -> PdfResult<()> {
    for &page in pages {
        let marks = marks_on(doc, page)?;
        if marks.is_empty() {
            continue;
        }
        let regions: Vec<GlyphBox> = marks.iter().flat_map(|m| m.regions.iter().copied()).collect();
        // The annotations go with what they mark: each one's `/Rect` meets its own regions,
        // and an annotation a region meets is removed with the content.
        super::redact::remove_regions(doc, page, &regions)?;
        let mut ops = String::new();
        for mark in &marks {
            ops.push_str(&appearance(doc, page, mark)?);
        }
        if !ops.is_empty() {
            super::redact::paint_over(doc, page, &ops)?;
        }
    }
    Ok(())
}

/// The redaction annotations on `page`, each with the regions it marks.
fn marks_on(doc: &Document, page: usize) -> PdfResult<Vec<Mark>> {
    let arena = doc.arena();
    let page_dict = doc.resolve_to_dict(doc.page_handle(page)?)?;
    let annots = match arena.dict_entry(page_dict, arena.name("Annots")).map(|a| a.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        _ => Vec::new(),
    };
    Ok(annots
        .iter()
        .filter_map(|a| {
            let dict = a.resolve(arena).as_dict_handle()?;
            let subtype = arena.dict_entry(dict, arena.name("Subtype"))?.as_name();
            (subtype == Some(arena.name("Redact"))).then_some(())?;
            let rect = boxes(arena, dict, "Rect", 4).into_iter().next()?;
            let quads = boxes(arena, dict, "QuadPoints", 8);
            let regions = if quads.is_empty() { vec![rect] } else { quads };
            Some(Mark { rect, regions, dict })
        })
        .collect())
}

/// The boxes round each `per` numbers of the array at `key`: a rectangle's corners, or a
/// quadrilateral's four points (Table 182); those with no area left out.
fn boxes(arena: &PdfArena, dict: DictHandle, key: &str, per: usize) -> Vec<GlyphBox> {
    let Some(array) =
        arena.dict_entry(dict, arena.name(key)).and_then(|a| a.resolve(arena).as_array())
    else {
        return Vec::new();
    };
    let numbers: Vec<f64> = arena
        .get_array(array)
        .unwrap_or_default()
        .iter()
        .filter_map(|n| n.resolve(arena).as_f64())
        .collect();
    numbers
        .chunks_exact(per)
        .map(|c| {
            let xs = c.iter().step_by(2);
            let ys = c.iter().skip(1).step_by(2);
            let (x0, x1) = xs.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            let (y0, y1) = ys.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            (x0, y0, x1, y1)
        })
        .filter(|b| b.2 > b.0 && b.3 > b.1)
        .collect()
}

/// What `mark` says is drawn in its place, as operators in default user space.
fn appearance(doc: &Document, page: usize, mark: &Mark) -> PdfResult<String> {
    let arena = doc.arena();
    let entry = |key: &str| arena.dict_entry(mark.dict, arena.name(key)).map(|v| v.resolve(arena));
    if let Some(overlay) =
        arena.dict_entry(mark.dict, arena.name("RO")).and_then(|r| r.as_reference())
    {
        let resources = super::target::Target::Page(page).resources(doc)?;
        let name = super::image_crop::name_in(doc, resources, overlay);
        return Ok(format!("q 1 0 0 1 {} {} cm /{name} Do Q\n", mark.rect.0, mark.rect.1));
    }
    let mut ops = String::new();
    let ic: Option<Vec<f64>> = entry("IC")
        .and_then(|c| c.as_array())
        .and_then(|a| arena.get_array(a))
        .map(|a| a.iter().filter_map(|n| n.resolve(arena).as_f64()).collect());
    if let Some(&[r, g, b]) = ic.as_deref() {
        ops.push_str(&super::redact::fill_of(&mark.regions, &format!("{r} {g} {b} rg")));
    }
    if let Some(text) = entry("OverlayText").and_then(|t| text_of(&t)) {
        ops.push_str(&overlay_text(doc, page, mark, &text)?);
    }
    Ok(ops)
}

/// A text string's characters (7.9.2.2): UTF-16BE behind its byte order mark, else the
/// bytes as Latin-1, which PDFDocEncoding agrees with for what an overlay writes.
fn text_of(value: &Object) -> Option<String> {
    if let Some(text) = value.as_text() {
        return Some(text.to_string());
    }
    let bytes = value.as_string()?;
    if let Some(units) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> =
            units.as_chunks::<2>().0.iter().map(|p| u16::from_be_bytes(*p)).collect();
        return String::from_utf16(&units).ok();
    }
    Some(bytes.iter().map(|b| char::from(*b)).collect())
}

/// `/OverlayText` set in the font `/DA` names from the form's `/DR`, clipped to `/Rect`,
/// placed by `/Q` on the middle line or repeated down the region by `/Repeat`. A font the
/// form does not define, or one that cannot draw the text, draws nothing, and says so.
fn overlay_text(doc: &Document, page: usize, mark: &Mark, text: &str) -> PdfResult<String> {
    let arena = doc.arena();
    let da =
        arena.dict_entry(mark.dict, arena.name("DA")).map(|d| d.resolve(arena)).and_then(|d| {
            d.as_string().map(<[u8]>::to_vec).or_else(|| d.as_text().map(|t| t.as_bytes().to_vec()))
        });
    let Some((font_name, size, colour)) = da.as_deref().and_then(appearance_string) else {
        return Ok(unset(doc, "its /DA names no font"));
    };
    let Some((handle, font)) = form_font(doc, &font_name) else {
        return Ok(unset(doc, "the form's /DR does not define the font its /DA names"));
    };
    let Some(codes) = text.chars().map(|c| font.code_for(c)).collect::<Option<Vec<_>>>() else {
        return Ok(unset(doc, "the font its /DA names does not draw the text"));
    };
    let width: f64 = codes.iter().map(|c| f64::from(font.glyph_width(c)) / 1000.0 * size).sum();
    let resources = super::target::Target::Page(page).resources(doc)?;
    let name = name_font(doc, resources, handle);
    let (x0, y0, x1, y1) = mark.rect;
    let quadding =
        arena.dict_entry(mark.dict, arena.name("Q")).and_then(|q| q.as_integer()).unwrap_or(0);
    let repeat =
        arena.dict_entry(mark.dict, arena.name("Repeat")).and_then(|r| r.as_bool()) == Some(true);
    let hex: String = codes.concat().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02X}");
        s
    });
    let mut ops =
        format!("q {x0} {y0} {} {} re W n {colour} BT /{name} {size} Tf\n", x1 - x0, y1 - y0);
    if repeat && width > 0.0 {
        let rows = std::iter::successors(Some(y1 - size), |y| Some(y - size))
            .take_while(|y| *y >= y0 - size);
        for y in rows {
            let across =
                std::iter::successors(Some(x0), |x| Some(x + width)).take_while(|x| *x < x1);
            for x in across {
                let _ = writeln!(ops, "1 0 0 1 {x} {y} Tm <{hex}> Tj");
            }
        }
    } else {
        let x = match quadding {
            1 => (x0 + x1 - width) / 2.0,
            2 => x1 - width,
            _ => x0,
        };
        let _ = writeln!(ops, "1 0 0 1 {x} {} Tm <{hex}> Tj", (y0 + y1 - size) / 2.0);
    }
    ops.push_str("ET Q\n");
    Ok(ops)
}

/// Records that an overlay was not drawn, and why; draws nothing.
fn unset(doc: &Document, why: &str) -> String {
    doc.decisions.push(Decision::repaired(
        "12.5.6.23",
        format!("a redaction annotation's /OverlayText could not be set: {why}"),
        "applied the redaction and drew no overlay text",
    ));
    String::new()
}

/// The font name, size and colour operator an appearance string sets (12.7.4.3).
fn appearance_string(da: &[u8]) -> Option<(String, f64, String)> {
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(da));
    let (mut operands, mut font, mut colour) = (Vec::new(), None, "0 g".to_string());
    while let Ok(token) = lexer.next_token() {
        match token {
            Token::EOF => break,
            Token::Keyword(op) => {
                let mut written = Vec::new();
                for operand in &operands {
                    Token::write_to(operand, &mut written);
                }
                match op.as_str() {
                    "Tf" => font = tf(&operands),
                    "g" | "rg" | "k" => {
                        colour = format!("{}{op}", String::from_utf8_lossy(&written));
                    }
                    _ => {}
                }
                operands.clear();
            }
            other => operands.push(other),
        }
    }
    let (name, size) = font?;
    Some((name, size, colour))
}

/// A `Tf`'s font name and size, out of its operands.
fn tf(operands: &[Token]) -> Option<(String, f64)> {
    let Token::Name(name) = operands.first()? else { return None };
    let size = match operands.get(1)? {
        Token::Integer(n) => i32::try_from(*n).ok().map(f64::from)?,
        Token::Real(r) => *r,
        _ => return None,
    };
    Some((String::from_utf8_lossy(name).to_string(), if size > 0.0 { size } else { 12.0 }))
}

/// The font the interactive form's `/DR` defines as `name`.
fn form_font(doc: &Document, name: &str) -> Option<(Handle<Object>, Arc<FontResource>)> {
    let arena = doc.arena();
    let catalog = doc.resolve_to_dict(doc.catalog_handle()?).ok()?;
    let form =
        arena.dict_entry(catalog, arena.name("AcroForm"))?.resolve(arena).as_dict_handle()?;
    let dr = arena.dict_entry(form, arena.name("DR"))?.resolve(arena).as_dict_handle()?;
    let fonts = arena.dict_entry(dr, arena.name("Font"))?.resolve(arena).as_dict_handle()?;
    let handle = arena.dict_entry(fonts, arena.name(name))?.as_reference()?;
    Some((handle, doc.get_font(handle).ok()?))
}

/// Names `font` in `resources`' `/Font`, under a name nothing there uses.
fn name_font(doc: &Document, resources: DictHandle, font: Handle<Object>) -> String {
    let arena = doc.arena();
    let key = arena.name("Font");
    let mut fonts = arena
        .dict_entry(resources, key)
        .and_then(|f| f.resolve(arena).as_dict_handle())
        .and_then(|f| arena.get_dict(f))
        .unwrap_or_default();
    let name = (0..=fonts.len())
        .map(|n| format!("fepdfOverlay{n}"))
        .find(|n| !fonts.contains_key(&arena.name(n)))
        .unwrap_or_default();
    fonts.insert(arena.name(&name), Object::Reference(font));
    let mut dict = arena.get_dict(resources).unwrap_or_default();
    dict.insert(key, Object::Dictionary(arena.alloc_dict(fonts)));
    arena.set_dict(resources, dict);
    name
}
