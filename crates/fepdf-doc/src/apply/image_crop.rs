//! The images a crop puts outside the sheet, cut to the part that remains (ADR-0088).
//!
//! **Left whole under the crop, an image is the whole image in the file.** The text a
//! crop puts outside is removed glyph by glyph; an image drawn half on the kept side was
//! carried entire, so a drawing cut into two sheets sent both halves of every picture on
//! it. This cuts the samples, not the view.
//!
//! **What is kept is everything that shows.** The kept rectangle is taken into the image's
//! own unit square through the inverse of the matrix it is drawn with, and the pixels
//! under its bounding box are what stay. For an image drawn upright that is exactly the
//! part on the sheet; for one turned or skewed it is a little more, never less, because
//! a crop that cut a pixel a reader could see would be taking what was kept.
//!
//! **Reached: an image XObject drawn by `Do` in the page's own content.** An inline
//! image, and an image inside a form XObject, are not reached; they stay whole.

use fepdf_model::filters::{SoftMaskInData, decode_image};
use fepdf_model::interpretation::Decision;
use fepdf_model::lexer::{Lexer, Token};
use fepdf_model::object::SublimatedData;
use fepdf_model::{DictHandle, Document, Handle, Object, PdfResult};
use kurbo::{Affine, Point};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Which part of an image a crop keeps, in its own unit square: left, bottom, right, top.
type Fraction = (f64, f64, f64, f64);

/// What happens to one image drawn on the page.
enum Cut {
    /// All of it shows: it stays as it is.
    Whole,
    /// None of it shows: the `Do` goes.
    Gone,
    /// Part of it shows: that part is drawn instead.
    Part(Fraction),
}

/// Cuts each image drawn on `page` to what `keep` leaves of it.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn cut_images_outside(
    doc: &Document,
    page: usize,
    keep: (f64, f64, f64, f64),
) -> PdfResult<()> {
    let Some(data) = crate::apply::text::page_content(doc, page)? else { return Ok(()) };
    let tokens = tokens_of(&data);
    let images = images_of(doc, page)?;
    let mut replaced: BTreeMap<usize, (usize, Vec<u8>)> = BTreeMap::new();
    let (mut ctm, mut saved) = (Affine::IDENTITY, Vec::new());
    let mut operands_from = 0;
    for (index, token) in tokens.iter().enumerate() {
        let Token::Keyword(op) = token else { continue };
        let operands = &tokens[operands_from..index];
        operands_from = index + 1;
        match op.as_str() {
            "q" => saved.push(ctm),
            "Q" => ctm = saved.pop().unwrap_or(ctm),
            "cm" => ctm *= matrix_of(operands),
            "Do" => {
                let Some(Token::Name(name)) = operands.last() else { continue };
                let Some(image) = images.get(&String::from_utf8_lossy(name).to_string()) else {
                    continue;
                };
                if let Some(written) = redraw(doc, page, *image, ctm, keep)? {
                    replaced.insert(index - 1, (index, written));
                }
            }
            _ => {}
        }
    }
    if replaced.is_empty() {
        return Ok(());
    }
    // Each replacement stands where its operand began, and runs to its `Do`.
    let out = crate::apply::path_crop::rewritten(&tokens, &replaced);
    crate::apply::text::write_page_content(doc, page, out)
}

/// Takes out of the resources `cropped` pages draw with every image no page drawing with
/// the same resources draws any more.
///
/// **Cut on the page is not cut in the file.** A cut is drawn under a name of its own, and
/// the whole image stayed in the resources under its old one, so the writer wrote every
/// pixel the crop was asked to take away. Resources are shared — a page inherits them from
/// the page tree, and a copy of a page names the same dictionary — so an image goes only
/// when no page using that dictionary draws it, nor a form with none of its own (7.8.3).
///
/// # Errors
/// Fails when a page's content cannot be read.
pub fn drop_undrawn_images(doc: &Document, cropped: &[usize]) -> PdfResult<()> {
    let arena = doc.arena();
    let resources_of = |page: usize| {
        let handle = doc.get_page_handle(page)?;
        Some(fepdf_model::Page::new(arena, handle, doc.get_parent_chain(handle)).resources_handle())
    };
    let every: Vec<(usize, DictHandle)> =
        (0..doc.page_count()?).filter_map(|page| Some((page, resources_of(page)?))).collect();
    let touched: std::collections::BTreeSet<DictHandle> =
        cropped.iter().filter_map(|page| resources_of(*page)).collect();
    for resources in touched {
        let mut drawn = std::collections::BTreeSet::new();
        for (page, _) in every.iter().filter(|(_, r)| *r == resources) {
            if let Some(data) = crate::apply::text::page_content(doc, *page)? {
                drawn.extend(names_drawn(&data));
            }
        }
        forget_undrawn(doc, resources, &mut drawn);
    }
    Ok(())
}

/// Takes out of `resources` the images `drawn` does not name, after adding the names the
/// forms there with no resources of their own draw.
fn forget_undrawn(
    doc: &Document,
    resources: DictHandle,
    drawn: &mut std::collections::BTreeSet<String>,
) {
    let arena = doc.arena();
    let Some(xobjects) = arena
        .dict_entry(resources, arena.name("XObject"))
        .and_then(|x| x.resolve(arena).as_dict_handle())
    else {
        return;
    };
    let mut entries = arena.get_dict(xobjects).unwrap_or_default();
    let kind = |value: &Object| match value.as_reference().and_then(|h| arena.get_object(h)) {
        Some(Object::Stream(dict, data)) => {
            let subtype = arena.dict_entry(dict, arena.name("Subtype")).and_then(|s| s.as_name());
            let own = arena.dict_entry(dict, arena.name("Resources")).is_some();
            Some((subtype, own, Object::Stream(dict, data)))
        }
        _ => None,
    };
    for value in entries.values() {
        if let Some((subtype, false, stream)) = kind(value)
            && subtype == Some(arena.name("Form"))
            && let Ok(data) = doc.decode_stream(&stream)
        {
            drawn.extend(names_drawn(&data));
        }
    }
    let before = entries.len();
    entries.retain(|name, value| {
        let image = kind(value).is_some_and(|(subtype, _, _)| subtype == Some(arena.name("Image")));
        !image || arena.get_name(*name).is_some_and(|n| drawn.contains(n.as_str()))
    });
    if entries.len() != before {
        arena.set_dict(xobjects, entries);
    }
}

/// The names a content stream draws with `Do`.
fn names_drawn(data: &[u8]) -> Vec<String> {
    let tokens = tokens_of(data);
    tokens
        .windows(2)
        .filter_map(|pair| match pair {
            [Token::Name(name), Token::Keyword(op)] if op == "Do" => {
                Some(String::from_utf8_lossy(name).to_string())
            }
            _ => None,
        })
        .collect()
}

/// The tokens of a content stream, in order.
fn tokens_of(data: &[u8]) -> Vec<Token> {
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(data));
    let mut tokens = Vec::new();
    while let Ok(token) = lexer.next_token() {
        if token == Token::EOF {
            break;
        }
        tokens.push(token);
    }
    tokens
}

/// The matrix a `cm`'s six operands write.
fn matrix_of(operands: &[Token]) -> Affine {
    let numbers: Vec<f64> = operands
        .iter()
        .filter_map(|token| match token {
            Token::Integer(n) => i32::try_from(*n).ok().map(f64::from),
            Token::Real(n) => Some(*n),
            _ => None,
        })
        .collect();
    let at = |i: usize| numbers.get(i).copied().unwrap_or(0.0);
    if numbers.len() < 6 {
        return Affine::IDENTITY;
    }
    Affine::new([at(0), at(1), at(2), at(3), at(4), at(5)])
}

/// The image XObjects the page's resources name, by name.
fn images_of(doc: &Document, page: usize) -> PdfResult<BTreeMap<String, Handle<Object>>> {
    let arena = doc.arena();
    let page_h = doc
        .get_page_handle(page)
        .ok_or_else(|| fepdf_model::PdfError::Other("the page is not there".into()))?;
    let resources =
        fepdf_model::Page::new(arena, page_h, doc.get_parent_chain(page_h)).resources_handle();
    let subtype = arena.name("Subtype");
    let image = arena.name("Image");
    let mut found = BTreeMap::new();
    let Some(xobjects) = arena
        .dict_entry(resources, arena.name("XObject"))
        .and_then(|x| x.resolve(arena).as_dict_handle())
    else {
        return Ok(found);
    };
    for (key, value) in arena.get_dict(xobjects).unwrap_or_default() {
        let Some(handle) = value.as_reference() else { continue };
        if let Some(Object::Stream(dict, _)) = arena.get_object(handle)
            && arena.dict_entry(dict, subtype).and_then(|s| s.as_name()) == Some(image)
            && let Some(name) = arena.get_name(key)
        {
            found.insert(name.as_str().to_string(), handle);
        }
    }
    Ok(found)
}

/// What `keep` leaves of an image drawn with `ctm`.
fn cut_of(ctm: Affine, keep: (f64, f64, f64, f64)) -> Cut {
    if ctm.determinant().abs() < 1e-12 {
        return Cut::Whole;
    }
    let inverse = ctm.inverse();
    let corners = [(keep.0, keep.1), (keep.2, keep.1), (keep.2, keep.3), (keep.0, keep.3)]
        .map(|(x, y)| inverse * Point::new(x, y));
    let low = |f: fn(&Point) -> f64| corners.iter().map(f).fold(f64::MAX, f64::min).max(0.0);
    let high = |f: fn(&Point) -> f64| corners.iter().map(f).fold(f64::MIN, f64::max).min(1.0);
    let (left, bottom, right, top) = (low(|p| p.x), low(|p| p.y), high(|p| p.x), high(|p| p.y));
    if left >= right || bottom >= top {
        return Cut::Gone;
    }
    if left <= 0.0 && bottom <= 0.0 && right >= 1.0 && top >= 1.0 {
        return Cut::Whole;
    }
    Cut::Part((left, bottom, right, top))
}

/// What replaces the `Do` of `image`, or `None` to leave it.
fn redraw(
    doc: &Document,
    page: usize,
    image: Handle<Object>,
    ctm: Affine,
    keep: (f64, f64, f64, f64),
) -> PdfResult<Option<Vec<u8>>> {
    let fraction = match cut_of(ctm, keep) {
        Cut::Whole => return Ok(None),
        Cut::Gone => return Ok(Some(Vec::new())),
        Cut::Part(fraction) => fraction,
    };
    let Some((cut, exact)) = cut_image(doc, image, fraction, 0) else {
        doc.record(Decision::ambiguity(
            "8.9.5",
            "an image the crop put partly outside the sheet could not be decoded to cut",
            "left it whole; what the crop put outside it is still in the file",
        ));
        return Ok(None);
    };
    let name = name_in_page(doc, page, cut)?;
    let (left, bottom, right, top) = exact;
    Ok(Some(
        format!(
            "q {:.6} 0 0 {:.6} {left:.6} {bottom:.6} cm /{name} Do Q ",
            right - left,
            top - bottom
        )
        .into_bytes(),
    ))
}

/// The part `fraction` of `image` as a new image, and the fraction it exactly covers —
/// which is a whole number of pixels, so a little more than was asked.
///
/// `depth` is how many images deep this is: a soft mask is cut with its image, and a
/// soft mask's own `/SMask` is not followed — 11.6.5.2 gives a soft-mask image none, and
/// one that named itself would otherwise be followed for ever (Rule 6).
fn cut_image(
    doc: &Document,
    image: Handle<Object>,
    fraction: Fraction,
    depth: usize,
) -> Option<(Handle<Object>, Fraction)> {
    let arena = doc.arena();
    let Some(Object::Stream(dict_h, _)) = arena.get_object(image) else { return None };
    let dict = arena.get_dict(dict_h)?;
    let decoded = Decoded::of(doc, image)?;
    let box_ = Pixels::of(fraction, decoded.width, decoded.height);
    let cut = box_.cut(&decoded.samples, decoded.width, decoded.components, decoded.bits);

    let mut new_dict = dict.clone();
    for key in ["Filter", "DecodeParms", "Length", "SMaskInData"] {
        new_dict.remove(&arena.name(key));
    }
    let integer_of = |n: usize| Object::Integer(i64::try_from(n).unwrap_or(i64::MAX));
    new_dict.insert(arena.name("Width"), integer_of(box_.columns()));
    new_dict.insert(arena.name("Height"), integer_of(box_.rows()));
    if !decoded.mask {
        new_dict.insert(arena.name("BitsPerComponent"), integer_of(decoded.bits));
        new_dict.entry(arena.name("ColorSpace")).or_insert_with(|| {
            Object::Name(arena.name(match decoded.components {
                1 => "DeviceGray",
                4 => "DeviceCMYK",
                _ => "DeviceRGB",
            }))
        });
    }
    let exact = box_.fraction(decoded.width, decoded.height);
    // A soft mask is cut to the same part of the picture, at its own resolution.
    if depth == 0
        && let Some(smask) = dict.get(&arena.name("SMask")).and_then(Object::as_reference)
    {
        let (cut_mask, _) = cut_image(doc, smask, exact, depth + 1)?;
        new_dict.insert(arena.name("SMask"), Object::Reference(cut_mask));
    }
    let stream = Object::Stream(
        arena.alloc_dict(new_dict),
        Arc::new(SublimatedData::Raw(bytes::Bytes::from(cut))),
    );
    Some((arena.alloc_object(stream), exact))
}

/// An image's samples, decoded, and what it takes to read them.
struct Decoded {
    samples: bytes::Bytes,
    width: usize,
    height: usize,
    components: usize,
    bits: usize,
    /// Whether it is a stencil (`/ImageMask`), which has no colour space to write.
    mask: bool,
}

impl Decoded {
    /// Decodes `image`, or `None` where it cannot be cut: a filter this engine cannot
    /// decode, samples whose layout does not add up, or a JPEG 2000 carrying its own
    /// mask (`/SMaskInData`) in a channel cutting would drop — which would make it opaque.
    fn of(doc: &Document, image: Handle<Object>) -> Option<Self> {
        let arena = doc.arena();
        let Some(Object::Stream(dict_h, data)) = arena.get_object(image) else { return None };
        if matches!(data.as_ref(), SublimatedData::Image { .. }) {
            return None;
        }
        let dict = arena.get_dict(dict_h)?;
        let integer =
            |key: &str| dict.get(&arena.name(key)).and_then(|v| v.resolve(arena).as_integer());
        if integer("SMaskInData").unwrap_or(0) != 0 {
            return None;
        }
        let width = usize::try_from(integer("Width")?).ok()?;
        let height = usize::try_from(integer("Height")?).ok()?;
        let mask = dict.get(&arena.name("ImageMask")).and_then(|m| m.resolve(arena).as_bool())
            == Some(true);
        let bits = if mask {
            1
        } else {
            usize::try_from(integer("BitsPerComponent").unwrap_or(8)).ok()?
        };
        let raw = arena.get_stream_bytes(&data).ok()?;
        let samples = decode_image(&raw, &dict, arena, SoftMaskInData::Ignored).ok()?.samples;
        let components =
            (1..=4).find(|c| height * (width * c * bits).div_ceil(8) == samples.len())?;
        Some(Self { samples, width, height, components, bits, mask })
    }
}

/// A block of pixels: columns and rows, from the top left, end exclusive.
struct Pixels {
    left: usize,
    right: usize,
    top: usize,
    bottom: usize,
}

impl Pixels {
    /// The pixels under `fraction` of an image `width` by `height`. Row 0 is the top of the
    /// picture, which is the top of its unit square (8.9.4).
    fn of(fraction: Fraction, width: usize, height: usize) -> Self {
        let whole = |value: f64, to: usize| index(value * real(to)).min(to);
        let (left, bottom, right, top) = fraction;
        let (mut columns, mut rows) = (
            (whole(left, width), whole(right, width).max(1)),
            (whole(1.0 - top, height), whole(1.0 - bottom, height).max(1)),
        );
        if right * real(width) > real(columns.1) {
            columns.1 = (columns.1 + 1).min(width);
        }
        if (1.0 - bottom) * real(height) > real(rows.1) {
            rows.1 = (rows.1 + 1).min(height);
        }
        columns.0 = columns.0.min(columns.1 - 1);
        rows.0 = rows.0.min(rows.1 - 1);
        Self { left: columns.0, right: columns.1, top: rows.0, bottom: rows.1 }
    }

    fn columns(&self) -> usize {
        self.right - self.left
    }

    fn rows(&self) -> usize {
        self.bottom - self.top
    }

    /// The part of the unit square these pixels cover.
    fn fraction(&self, width: usize, height: usize) -> Fraction {
        let (across, down) = (real(width), real(height));
        (
            real(self.left) / across,
            1.0 - real(self.bottom) / down,
            real(self.right) / across,
            1.0 - real(self.top) / down,
        )
    }

    /// These pixels of `samples`, packed as the image packs them: each row starting on a
    /// byte (8.9.3), each sample `bits` wide.
    fn cut(&self, samples: &[u8], width: usize, components: usize, bits: usize) -> Vec<u8> {
        let row_in = (width * components * bits).div_ceil(8);
        let row_out = (self.columns() * components * bits).div_ceil(8);
        let mut out = vec![0u8; row_out * self.rows()];
        let (first, count) = (self.left * components, self.columns() * components);
        for (nth, row) in (self.top..self.bottom).enumerate() {
            let source = &samples[row * row_in..(row + 1) * row_in];
            let target = &mut out[nth * row_out..(nth + 1) * row_out];
            if bits.is_multiple_of(8) {
                let bytes = bits / 8;
                target.copy_from_slice(&source[first * bytes..(first + count) * bytes]);
            } else {
                for sample in 0..count {
                    let value = read_bits(source, (first + sample) * bits, bits);
                    write_bits(target, sample * bits, bits, value);
                }
            }
        }
        out
    }
}

/// A count of pixels as a position. An image dimension fits in a `u32` (7.3.3).
fn real(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// A rounded-down index from a non-negative position.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn index(value: f64) -> usize {
    value.floor().max(0.0) as usize
}

fn read_bits(bytes: &[u8], at: usize, bits: usize) -> u8 {
    let mut value = 0u8;
    for bit in at..at + bits {
        let on = bytes[bit / 8] >> (7 - bit % 8) & 1;
        value = (value << 1) | on;
    }
    value
}

fn write_bits(bytes: &mut [u8], at: usize, bits: usize, value: u8) {
    for offset in 0..bits {
        let bit = at + offset;
        if value >> (bits - 1 - offset) & 1 == 1 {
            bytes[bit / 8] |= 1 << (7 - bit % 8);
        }
    }
}

/// Names `image` in the page's resources, under a name nothing there uses.
fn name_in_page(doc: &Document, page: usize, image: Handle<Object>) -> PdfResult<String> {
    let arena = doc.arena();
    let page_h = doc
        .get_page_handle(page)
        .ok_or_else(|| fepdf_model::PdfError::Other("the page is not there".into()))?;
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let resources = crate::apply::annotations::ensure_page_resources(doc, page_h, &mut page_dict);
    arena.set_dict(page_dh, page_dict);
    let key = arena.name("XObject");
    let existing: Option<DictHandle> =
        arena.dict_entry(resources, key).and_then(|x| x.resolve(arena).as_dict_handle());
    let mut xobjects = existing.and_then(|x| arena.get_dict(x)).unwrap_or_default();
    let taken = |name: &str| xobjects.contains_key(&arena.name(name));
    // One more name than there are entries, so one of them is free.
    let name = (0..=xobjects.len())
        .map(|n| format!("fepdfCut{n}"))
        .find(|n| !taken(n))
        .unwrap_or_default();
    xobjects.insert(arena.name(&name), Object::Reference(image));
    let mut dict = arena.get_dict(resources).unwrap_or_default();
    dict.insert(key, Object::Dictionary(arena.alloc_dict(xobjects)));
    arena.set_dict(resources, dict);
    Ok(name)
}
