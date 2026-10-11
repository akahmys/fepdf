//! Pages made from pictures (ROADMAP AA-5, [ADR-0123]).
//!
//! **One page to a picture, and a TIFF's every page is a picture.** Each is an image
//! XObject drawn over the whole page:
//!
//! - **A JPEG is carried as it is**, under `/DCTDecode`, where its coding is one that
//!   filter reads (8.9.2's baseline and progressive). Nothing is decoded or re-encoded,
//!   so nothing is lost. Its EXIF orientation is honoured by the matrix it is drawn with.
//! - **A PNG is decoded**, its alpha becoming a soft mask (11.6.5.3) and sixteen-bit
//!   samples staying sixteen-bit.
//! - **A TIFF is decoded page by page**, fax-coded scans included. Its thumbnails, which
//!   `NewSubfileType` marks as reduced images of a page, are not pages.
//!
//! An ICC profile the file carries is kept as the image's `/ICCBased` colour space, where
//! its header names the image's colour space. **The page is the picture's size** at the
//! resolution the file states, or 72 dots to the inch where it states none; or, given a
//! sheet, every page is that sheet, and the picture is fitted inside it and centred.
//!
//! [ADR-0123]: ../../../../../docs/adr/0123-a-picture-becomes-a-page-at-its-own-size.md

mod jpeg;
mod png;
mod tiff;

use bytes::Bytes;
use fepdf_model::object::SublimatedData;
use fepdf_model::{Document, Handle, Object, PdfArena, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// 14,400 units: the largest page 2.0 allows without `/UserUnit` (Annex C.2).
const LARGEST: f64 = 14_400.0;

/// Why a picture will not read, in words a refusal can carry (Rule 11: a type, not a
/// `String`).
#[derive(Debug)]
pub struct Unreadable(String);

impl From<String> for Unreadable {
    fn from(why: String) -> Self {
        Self(why)
    }
}

impl From<&str> for Unreadable {
    fn from(why: &str) -> Self {
        Self(why.to_owned())
    }
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A picture's colour space, before any profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Space {
    Gray,
    Rgb,
    Cmyk,
}

impl Space {
    const fn components(self) -> u8 {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
            Self::Cmyk => 4,
        }
    }

    const fn device(self) -> &'static str {
        match self {
            Self::Gray => "DeviceGray",
            Self::Rgb => "DeviceRGB",
            Self::Cmyk => "DeviceCMYK",
        }
    }

    /// The colour space signature an ICC profile's header gives this space (ICC.1, 7.2.6).
    const fn signature(self) -> &'static [u8; 4] {
        match self {
            Self::Gray => b"GRAY",
            Self::Rgb => b"RGB ",
            Self::Cmyk => b"CMYK",
        }
    }
}

/// How a picture's samples are held.
enum Samples {
    /// A JPEG's own bytes, for `/DCTDecode`.
    Dct(Vec<u8>),
    /// Decoded samples, rows packed to whole bytes, sixteen-bit ones big-endian.
    Pixels(Vec<u8>),
}

/// One picture, read and not yet in the document.
struct Picture {
    width: u32,
    height: u32,
    space: Space,
    bits: u8,
    samples: Samples,
    /// `/Decode`, where the samples are the other way round: an Adobe CMYK JPEG, or a
    /// TIFF whose zero is white.
    inverted: bool,
    /// The alpha channel, as a grey image of the same size and depth.
    alpha: Option<Vec<u8>>,
    icc: Option<Vec<u8>>,
    /// Dots to the inch, across and up, as stored.
    dpi: (f64, f64),
    /// EXIF and TIFF's orientation, 1 to 8.
    orientation: u16,
}

/// Puts a page for each picture in `images` at `at`.
///
/// Every picture is read before the document changes, so one that will not read leaves
/// it as it was.
///
/// # Errors
/// Refused when there are no pictures, when a sheet has no area, or when a picture is
/// not a JPEG, PNG or TIFF this reads — saying which, counting from one.
pub fn apply_insert_images(
    doc: &mut Document,
    images: &[Vec<u8>],
    at: usize,
    sheet: Option<[f32; 2]>,
) -> PdfResult<usize> {
    let refuse = |why: String| PdfError::refused("InsertImages", why);
    if images.is_empty() {
        return Err(refuse("there are no pictures to make pages of".to_owned()));
    }
    let sheet = match sheet {
        Some([w, h]) if !(w > 0.0 && h > 0.0) => {
            return Err(refuse(format!("a sheet {w} by {h} has no area")));
        }
        sheet => sheet.map(|[w, h]| (f64::from(w), f64::from(h))),
    };
    let mut pictures = Vec::new();
    for (nth, bytes) in images.iter().enumerate() {
        let read = read(bytes).map_err(|why| refuse(format!("picture {}: {why}", nth + 1)))?;
        pictures.extend(read);
    }
    let clamped = at.min(doc.pages.len());
    let count = pictures.len();
    for (nth, picture) in pictures.iter().enumerate() {
        let page = page(doc.arena(), picture, sheet);
        doc.pages.insert(clamped + nth, page);
    }
    doc.rebuild_page_tree_in_arena()?;
    Ok(count)
}

/// Whether `bytes` begin as a JPEG, a PNG or a TIFF does — what [`apply_insert_images`]
/// reads, said before it is asked to.
#[must_use]
pub fn is_picture(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some([0xFF, 0xD8, ..] | [0x89, b'P', b'N', b'G'] | b"II*\0" | b"MM\0*")
    )
}

/// The pictures in `bytes`, by what its first bytes say it is.
fn read(bytes: &[u8]) -> Result<Vec<Picture>, Unreadable> {
    match bytes.get(..4) {
        Some([0xFF, 0xD8, ..]) => jpeg::read(bytes).map(|p| vec![p]),
        Some([0x89, b'P', b'N', b'G']) => png::read(bytes).map(|p| vec![p]),
        Some(b"II*\0" | b"MM\0*") => tiff::read(bytes),
        _ => Err("it is not a JPEG, a PNG or a TIFF".into()),
    }
}

/// A page holding `picture`, and the objects it draws.
fn page(arena: &PdfArena, picture: &Picture, sheet: Option<(f64, f64)>) -> Handle<Object> {
    let (matrix, (width, height)) = placed(picture, sheet);
    let drawing = format!("q {} cm /Im0 Do Q\n", matrix.map(|n| format!("{n:.4}")).join(" "));
    let contents = arena.alloc_object(Object::Stream(
        arena.alloc_dict(Dict::new()),
        Arc::new(SublimatedData::Raw(Bytes::from(drawing))),
    ));
    let mut xobjects = Dict::new();
    xobjects.insert(arena.name("Im0"), Object::Reference(xobject(arena, picture)));
    let mut resources = Dict::new();
    resources.insert(arena.name("XObject"), Object::Dictionary(arena.alloc_dict(xobjects)));
    let mut dict = Dict::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Page")));
    dict.insert(arena.name("MediaBox"), numbers(arena, &[0.0, 0.0, width, height]));
    dict.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(resources)));
    dict.insert(arena.name("Contents"), Object::Reference(contents));
    arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict)))
}

/// The matrix the picture is drawn with, and the page's size.
///
/// EXIF orientation 5 to 8 turns the picture a quarter, so what is stored across is shown
/// up: the size shown swaps its two sides, and the resolutions with them.
fn placed(picture: &Picture, sheet: Option<(f64, f64)>) -> ([f64; 6], (f64, f64)) {
    let turned = picture.orientation >= 5;
    let (across, up) = (f64::from(picture.width) * 72.0, f64::from(picture.height) * 72.0);
    let (wide, high) = (across / picture.dpi.0, up / picture.dpi.1);
    let shown = if turned { (high, wide) } else { (wide, high) };
    let (page, drawn, offset) = match sheet {
        Some((sw, sh)) => {
            let scale = (sw / shown.0).min(sh / shown.1);
            let drawn = (shown.0 * scale, shown.1 * scale);
            ((sw, sh), drawn, ((sw - drawn.0) / 2.0, (sh - drawn.1) / 2.0))
        }
        // No larger than a page may be: a picture of more than 200 inches is shrunk.
        None => {
            let scale = (LARGEST / shown.0).min(LARGEST / shown.1).min(1.0);
            let drawn = (shown.0 * scale, shown.1 * scale);
            (drawn, drawn, (0.0, 0.0))
        }
    };
    let mut matrix = oriented(picture.orientation, drawn.0, drawn.1);
    let [.., across, up] = &mut matrix;
    *across += offset.0;
    *up += offset.1;
    (matrix, page)
}

/// The matrix that draws an image's unit square into a `w` by `h` box, turned and
/// mirrored as EXIF orientation `orientation` says the stored picture is to be shown.
///
/// In image space the stored first row is at the top, `v = 1`, and its first column at
/// the left, `u = 0`. Orientation 6, a phone held upright, has the first row shown down
/// the right-hand side and the first column along the top.
fn oriented(orientation: u16, w: f64, h: f64) -> [f64; 6] {
    match orientation {
        2 => [-w, 0.0, 0.0, h, w, 0.0],
        3 => [-w, 0.0, 0.0, -h, w, h],
        4 => [w, 0.0, 0.0, -h, 0.0, h],
        5 => [0.0, -h, -w, 0.0, w, h],
        6 => [0.0, -h, w, 0.0, 0.0, h],
        7 => [0.0, h, w, 0.0, 0.0, 0.0],
        8 => [0.0, h, -w, 0.0, w, 0.0],
        _ => [w, 0.0, 0.0, h, 0.0, 0.0],
    }
}

/// The image XObject (8.9.5), with its soft mask and its profile.
fn xobject(arena: &PdfArena, picture: &Picture) -> Handle<Object> {
    let mut dict = image_dict(arena, picture.width, picture.height, picture.bits);
    dict.insert(arena.name("ColorSpace"), colour_space(arena, picture));
    if picture.inverted {
        let decode: Vec<f64> = (0..picture.space.components()).flat_map(|_| [1.0, 0.0]).collect();
        dict.insert(arena.name("Decode"), numbers(arena, &decode));
    }
    if let Some(alpha) = &picture.alpha {
        let mut mask = image_dict(arena, picture.width, picture.height, picture.bits);
        mask.insert(arena.name("ColorSpace"), Object::Name(arena.name("DeviceGray")));
        let mask = arena.alloc_object(Object::Stream(arena.alloc_dict(mask), held(alpha)));
        dict.insert(arena.name("SMask"), Object::Reference(mask));
    }
    let data = match &picture.samples {
        Samples::Dct(bytes) => {
            dict.insert(arena.name("Filter"), Object::Name(arena.name("DCTDecode")));
            Arc::new(SublimatedData::Raw(Bytes::copy_from_slice(bytes)))
        }
        Samples::Pixels(pixels) => held(pixels),
    };
    arena.alloc_object(Object::Stream(arena.alloc_dict(dict), data))
}

fn image_dict(arena: &PdfArena, width: u32, height: u32, bits: u8) -> Dict {
    let mut dict = Dict::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("XObject")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Image")));
    dict.insert(arena.name("Width"), Object::Integer(i64::from(width)));
    dict.insert(arena.name("Height"), Object::Integer(i64::from(height)));
    dict.insert(arena.name("BitsPerComponent"), Object::Integer(i64::from(bits)));
    dict
}

/// `/ICCBased` where the file's profile is one for the image's space (8.6.5.5), and the
/// device space otherwise: a profile for another space would describe colours the
/// samples do not hold.
fn colour_space(arena: &PdfArena, picture: &Picture) -> Object {
    let device = Object::Name(arena.name(picture.space.device()));
    let Some(profile) = picture
        .icc
        .as_ref()
        .filter(|p| p.get(16..20) == Some(picture.space.signature().as_slice()))
    else {
        return device;
    };
    let mut dict = Dict::new();
    dict.insert(arena.name("N"), Object::Integer(i64::from(picture.space.components())));
    dict.insert(arena.name("Alternate"), device);
    let stream = arena.alloc_object(Object::Stream(arena.alloc_dict(dict), held(profile)));
    let based = vec![Object::Name(arena.name("ICCBased")), Object::Reference(stream)];
    Object::Array(arena.alloc_array(based))
}

/// Bytes held compressed in memory, which the writer writes under `/FlateDecode`.
fn held(bytes: &[u8]) -> Arc<SublimatedData> {
    Arc::new(match fepdf_model::filters::flate::deflate(bytes) {
        Ok(data) => SublimatedData::Compressed { original_len: bytes.len(), data },
        Err(_) => SublimatedData::Raw(Bytes::copy_from_slice(bytes)),
    })
}

fn numbers(arena: &PdfArena, values: &[f64]) -> Object {
    Object::Array(arena.alloc_array(values.iter().map(|v| Object::Real(*v)).collect()))
}

/// Dots to the inch from a resolution and its unit: 2 inches, 3 centimetres (TIFF tag
/// 296, EXIF's the same, JFIF's 1 and 2). None where there is no usable figure.
fn per_inch(value: f64, unit: u16, (inch, centimetre): (u16, u16)) -> Option<f64> {
    let dpi = if unit == inch {
        value
    } else if unit == centimetre {
        value * 2.54
    } else {
        return None;
    };
    (dpi.is_finite() && dpi >= 1.0).then_some(dpi)
}

/// Splits interleaved samples into colour and alpha, where the last of `channels` is
/// alpha and each sample is `width` bytes. The alpha is dropped where it is opaque
/// everywhere, which is no mask at all.
fn split_alpha(samples: &[u8], channels: usize, width: usize) -> (Vec<u8>, Option<Vec<u8>>) {
    let pixel = channels * width;
    let mut colour = Vec::with_capacity(samples.len() / channels.max(1) * (channels - 1));
    let mut alpha = Vec::with_capacity(samples.len() / channels.max(1));
    for chunk in samples.chunks_exact(pixel) {
        let (c, a) = chunk.split_at(pixel - width);
        colour.extend_from_slice(c);
        alpha.extend_from_slice(a);
    }
    let opaque = alpha.iter().all(|&b| b == 0xFF);
    (colour, (!opaque).then_some(alpha))
}
