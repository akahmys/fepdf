//! Making an annotation: its dictionary, and the appearance it is drawn by (12.5).
//!
//! **Every annotation this engine writes carries an appearance, except a link.**
//! `render_annotations` skips an annotation with none, which is right for a file somebody
//! else wrote and wrong for one this engine writes: what it made, it could not draw
//! (12.5.5). A link's appearance is the border a reader draws round it (12.5.6.5), and a
//! stream there would paint where the file asks for nothing to be painted.
//!
//! **An appearance is drawn in the annotation's own space**: its `/BBox` is
//! `[0 0 width height]` and 12.5.5's algorithm maps that onto `/Rect`, so a point on the
//! page is drawn at itself less the rectangle's lower left corner.

use crate::operation::{AnnotationKind, AnnotationSpec, ShapeForm};
use bytes::Bytes;
use fepdf_model::arena::PdfArena;
use fepdf_model::object::{PdfName, SublimatedData};
use fepdf_model::{DictHandle, Document, Handle, Object, PdfError, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// An upright rectangle on the page: left, bottom, right, top.
#[derive(Debug, Clone, Copy)]
pub struct Area {
    pub(crate) left: f64,
    pub(crate) bottom: f64,
    pub(crate) right: f64,
    pub(crate) top: f64,
}

impl Area {
    /// The rectangle `[x1 y1 x2 y2]` names, whichever corners it names them by.
    fn of(rect: [f32; 4]) -> Self {
        let (x1, y1, x2, y2) =
            (f64::from(rect[0]), f64::from(rect[1]), f64::from(rect[2]), f64::from(rect[3]));
        Self { left: x1.min(x2), bottom: y1.min(y2), right: x1.max(x2), top: y1.max(y2) }
    }

    pub(crate) fn width(self) -> f64 {
        self.right - self.left
    }

    pub(crate) fn height(self) -> f64 {
        self.top - self.bottom
    }

    /// This rectangle grown to take in `point`, with `margin` round it.
    fn taking(self, point: [f32; 2], margin: f64) -> Self {
        let (x, y) = (f64::from(point[0]), f64::from(point[1]));
        Self {
            left: self.left.min(x - margin),
            bottom: self.bottom.min(y - margin),
            right: self.right.max(x + margin),
            top: self.top.max(y + margin),
        }
    }

    /// `point` on the page, in this rectangle's own space.
    fn local(self, point: [f32; 2]) -> (f64, f64) {
        (f64::from(point[0]) - self.left, f64::from(point[1]) - self.bottom)
    }

    fn array(self, arena: &PdfArena) -> Object {
        numbers(arena, &[self.left, self.bottom, self.right, self.top])
    }
}

/// The dictionary of `annot`, with its appearance, on the page `page`.
///
/// # Errors
/// Fails when the annotation would draw nothing — a rectangle of no area where the
/// rectangle is what is drawn, a stroke of no width, no strokes — when a stamp's picture
/// is not a JPEG, or when a text annotation's words have a character no face here draws.
pub fn annotation(
    doc: &Document,
    annot: &AnnotationSpec,
    page: Handle<Object>,
) -> PdfResult<DictHandle> {
    let arena = doc.arena();
    let given = Area::of(annot.rect);
    let drawn = extent(given, &annot.kind)?;
    let mut dict = Dict::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Annot")));
    dict.insert(arena.name("Rect"), drawn.array(arena));
    dict.insert(arena.name("P"), Object::Reference(page));
    // `Print` (Table 167): an annotation a reader put on a page is on the page when it is
    // printed, which is also what PDF/A asks of every annotation it keeps.
    dict.insert(arena.name("F"), Object::Integer(4));

    match kind_entries(doc, &mut dict, &annot.kind, given, drawn)? {
        Some(appearance) => {
            dict.insert(arena.name("AP"), appearance);
        }
        // What `kind_entries` did not draw — a stamp with no picture — is drawn from the
        // entries it wrote, since Table 166 requires an appearance (ADR-0119). A link is
        // exempt, and the drawing says so by answering `None`.
        None => {
            if let Some(appearance) = crate::apply::drawn::appearance_for(doc, &dict)? {
                dict.insert(arena.name("AP"), appearance);
            }
        }
    }
    Ok(arena.alloc_dict(dict))
}

/// What `kind` adds to the dictionary, and the appearance it is drawn by — `None` for a
/// link, which paints nothing.
fn kind_entries(
    doc: &Document,
    dict: &mut Dict,
    kind: &AnnotationKind,
    given: Area,
    drawn: Area,
) -> PdfResult<Option<Object>> {
    let arena = doc.arena();
    Ok(match kind {
        AnnotationKind::Link { destination_page, url } => {
            link_target(doc, dict, *destination_page, url.as_deref())?
        }
        AnnotationKind::TextComment { contents } => Some(note(arena, dict, contents, drawn)),
        AnnotationKind::Stamp { stamp_image_bytes } => {
            stamp(arena, dict, stamp_image_bytes, drawn)?
        }
        AnnotationKind::Highlight { color_rgb } => {
            Some(text_markup(arena, dict, Marking::Highlight, *color_rgb, drawn))
        }
        AnnotationKind::Underline { color_rgb } => {
            Some(text_markup(arena, dict, Marking::Underline, *color_rgb, drawn))
        }
        AnnotationKind::StrikeOut { color_rgb } => {
            Some(text_markup(arena, dict, Marking::StrikeOut, *color_rgb, drawn))
        }
        AnnotationKind::Squiggly { color_rgb } => {
            Some(text_markup(arena, dict, Marking::Squiggly, *color_rgb, drawn))
        }
        AnnotationKind::TextBox { contents, font_size }
        | AnnotationKind::Typewriter { contents, font_size } => {
            let typed = matches!(kind, AnnotationKind::Typewriter { .. });
            let words = Words { text: contents, size: f64::from(*font_size) };
            let place = Placing { area: drawn, text_box: given, points_at: None };
            Some(free_text(doc, dict, &words, &place, typed.then_some("FreeTextTypeWriter"))?)
        }
        AnnotationKind::Callout { contents, font_size, points_at } => {
            let words = Words { text: contents, size: f64::from(*font_size) };
            let place = Placing { area: drawn, text_box: given, points_at: Some(*points_at) };
            Some(free_text(doc, dict, &words, &place, Some("FreeTextCallout"))?)
        }
        AnnotationKind::Ink { strokes, color_rgb, width } => {
            Some(ink(arena, dict, strokes, *color_rgb, f64::from(*width), drawn))
        }
        AnnotationKind::Shape { form, color_rgb, width } => {
            Some(shape(arena, dict, form, *color_rgb, f64::from(*width), drawn))
        }
    })
}

/// The four text markups (12.5.6.10), which differ only in what they draw.
#[derive(Debug, Clone, Copy)]
enum Marking {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

/// The rectangle the annotation occupies: the one it was given, grown to take in any
/// point it draws outside it — a callout's line, a stroke, a line's ends.
///
/// # Errors
/// Fails when what would be drawn is nothing.
fn extent(given: Area, kind: &AnnotationKind) -> PdfResult<Area> {
    let refuse = |why: &str| Err(PdfError::refused("AddAnnotation", why.to_string()));
    let boxed = given.width() > 0.0 && given.height() > 0.0;
    match kind {
        AnnotationKind::Ink { strokes, width, .. } => {
            if *width <= 0.0 {
                return refuse("an ink stroke of no width draws nothing");
            }
            if strokes.is_empty() || strokes.iter().any(|stroke| stroke.len() < 2) {
                return refuse("each ink stroke needs two points at least, and there must be one");
            }
            let margin = f64::from(*width);
            Ok(strokes.iter().flatten().fold(given, |area, point| area.taking(*point, margin)))
        }
        AnnotationKind::Shape { form: ShapeForm::Line { from, to }, width, .. } => {
            if *width <= 0.0 {
                return refuse("a line of no width draws nothing");
            }
            let margin = f64::from(*width);
            Ok(given.taking(*from, margin).taking(*to, margin))
        }
        AnnotationKind::Shape { width, .. } if *width <= 0.0 => {
            refuse("a shape whose outline has no width draws nothing")
        }
        AnnotationKind::Callout { points_at, .. } if boxed => Ok(given.taking(*points_at, 2.0)),
        AnnotationKind::Link { .. }
        | AnnotationKind::TextComment { .. }
        | AnnotationKind::Stamp { .. }
        | AnnotationKind::Highlight { .. }
        | AnnotationKind::Underline { .. }
        | AnnotationKind::StrikeOut { .. }
        | AnnotationKind::Squiggly { .. }
        | AnnotationKind::TextBox { .. }
        | AnnotationKind::Typewriter { .. }
        | AnnotationKind::Callout { .. }
        | AnnotationKind::Shape { .. } => {
            if boxed {
                Ok(given)
            } else {
                refuse("an annotation drawn in a rectangle of no area draws nothing")
            }
        }
    }
}

fn name(arena: &PdfArena, dict: &mut Dict, key: &str, value: &str) {
    dict.insert(arena.name(key), Object::Name(arena.name(value)));
}

fn numbers(arena: &PdfArena, values: &[f64]) -> Object {
    Object::Array(arena.alloc_array(values.iter().map(|v| Object::Real(*v)).collect()))
}

fn color(arena: &PdfArena, rgb: [f32; 3]) -> Object {
    numbers(arena, &rgb.map(f64::from))
}

/// `/BS`: the width an outline is drawn at (Table 168).
fn border(arena: &PdfArena, width: f64) -> Object {
    let mut bs = Dict::new();
    bs.insert(arena.name("W"), Object::Real(width));
    Object::Dictionary(arena.alloc_dict(bs))
}

/// A link, and where it goes: a URI action, or a page of this document (12.5.6.5). It
/// has no appearance, so this answers `None`.
///
/// # Errors
/// Fails when it goes to a page the document does not have — a link written with nowhere
/// to go, as it was, is one a caller one page off is told has worked.
fn link_target(
    doc: &Document,
    dict: &mut Dict,
    destination_page: usize,
    url: Option<&str>,
) -> PdfResult<Option<Object>> {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "Link");
    if let Some(uri) = url {
        let mut action = Dict::new();
        name(arena, &mut action, "Type", "Action");
        name(arena, &mut action, "S", "URI");
        action.insert(arena.name("URI"), Object::String(Bytes::from(uri.to_string())));
        dict.insert(arena.name("A"), Object::Dictionary(arena.alloc_dict(action)));
    } else {
        let target = doc.page_handle(destination_page)?;
        let destination = vec![Object::Reference(target), Object::Name(arena.name("Fit"))];
        dict.insert(arena.name("Dest"), Object::Array(arena.alloc_array(destination)));
    }
    Ok(None)
}

/// A form XObject of `area`'s size holding `drawing`, as a normal appearance (12.5.5).
pub fn appearance(arena: &PdfArena, drawing: &str, area: Area, resources: Option<Dict>) -> Object {
    let mut dict = Dict::new();
    name(arena, &mut dict, "Type", "XObject");
    name(arena, &mut dict, "Subtype", "Form");
    dict.insert(arena.name("BBox"), numbers(arena, &[0.0, 0.0, area.width(), area.height()]));
    // Table 93 requires `/Resources` on a form XObject in PDF 2.0, empty or not.
    let resources = resources.unwrap_or_default();
    dict.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(resources)));
    let stream = Object::Stream(
        arena.alloc_dict(dict),
        Arc::new(SublimatedData::Raw(Bytes::copy_from_slice(drawing.as_bytes()))),
    );
    let mut ap = Dict::new();
    ap.insert(arena.name("N"), Object::Reference(arena.alloc_object(stream)));
    Object::Dictionary(arena.alloc_dict(ap))
}

/// An appearance that draws nothing: what a reply carries, since 12.5.6.2 shows it with
/// what it answers and Table 166 still requires it to have one (ADR-0118).
pub fn nothing_drawn(arena: &PdfArena) -> Object {
    appearance(arena, "", Area { left: 0.0, bottom: 0.0, right: 1.0, top: 1.0 }, None)
}

/// A note, and its appearance. The icon is a reader's to replace; what matters is that the
/// file says where the mark is rather than leaving the page blank.
fn note(arena: &PdfArena, dict: &mut Dict, contents: &str, area: Area) -> Object {
    name(arena, dict, "Subtype", "Text");
    dict.insert(arena.name("Contents"), Object::Text(contents.to_string()));
    let (w, h) = (area.width(), area.height());
    let drawing =
        format!("0.98 0.85 0.24 rg\n0 0 {w:.2} {h:.2} re\nf\n0 0 0 RG\n0 0 {w:.2} {h:.2} re\nS\n");
    appearance(arena, &drawing, area, None)
}

/// A text markup: its `/QuadPoints`, colour and appearance (12.5.6.10, Table 179).
///
/// **A highlight multiplies.** It was a filled rectangle painted over the text in normal
/// blending, and measured on `constitution.pdf` a yellow highlight over 日本国憲法 took the
/// dark pixels of the title from 449 to 0: the words it marked were the one thing it hid.
/// `/BM /Multiply` darkens the page by the colour and leaves black text black (11.3.5).
fn text_markup(
    arena: &PdfArena,
    dict: &mut Dict,
    marking: Marking,
    rgb: [f32; 3],
    area: Area,
) -> Object {
    let (subtype, drawing, resources) = markup_drawing(arena, marking, rgb, area);
    name(arena, dict, "Subtype", subtype);
    // Upper left, upper right, lower left, lower right: the order Table 179 gives.
    let quad = [
        area.left,
        area.top,
        area.right,
        area.top,
        area.left,
        area.bottom,
        area.right,
        area.bottom,
    ];
    dict.insert(arena.name("QuadPoints"), numbers(arena, &quad));
    dict.insert(arena.name("C"), color(arena, rgb));
    appearance(arena, &drawing, area, resources)
}

/// The subtype a text markup is, what it draws, and the resources that needs.
fn markup_drawing(
    arena: &PdfArena,
    marking: Marking,
    rgb: [f32; 3],
    area: Area,
) -> (&'static str, String, Option<Dict>) {
    let (wide, high) = (area.width(), area.height());
    let pen = stroking(rgb, (high / 14.0).max(0.5));
    let rule = |at: f64| format!("{pen}0 {at:.2} m {wide:.2} {at:.2} l S\n");
    match marking {
        Marking::Underline => ("Underline", rule(high * 0.08), None),
        Marking::StrikeOut => ("StrikeOut", rule(high / 2.0), None),
        Marking::Squiggly => ("Squiggly", format!("{pen}{}S\n", zigzag(wide, high / 12.0)), None),
        Marking::Highlight => {
            let mut state = Dict::new();
            name(arena, &mut state, "BM", "Multiply");
            let mut states = Dict::new();
            states.insert(arena.name("G0"), Object::Dictionary(arena.alloc_dict(state)));
            let mut resources = Dict::new();
            resources.insert(arena.name("ExtGState"), Object::Dictionary(arena.alloc_dict(states)));
            let drawing = format!("/G0 gs\n{}0 0 {wide:.2} {high:.2} re\nf\n", filling(rgb));
            ("Highlight", drawing, Some(resources))
        }
    }
}

/// The operators that stroke in `rgb` at `width`.
fn stroking(rgb: [f32; 3], width: f64) -> String {
    let [red, green, blue] = rgb;
    format!("{red} {green} {blue} RG {width:.2} w\n")
}

/// The operator that fills in `rgb`.
fn filling(rgb: [f32; 3]) -> String {
    let [red, green, blue] = rgb;
    format!("{red} {green} {blue} rg\n")
}

/// A wavy line along the foot of a box `width` wide, `amplitude` high.
fn zigzag(width: f64, amplitude: f64) -> String {
    use std::fmt::Write as _;
    let step = (amplitude * 2.0).max(1.0);
    let mut path = format!("0 {amplitude:.2} m\n");
    // Counted rather than walked, so the last peak lands on the right edge exactly.
    let peaks = crate::apply::markup::whole(width / step);
    for nth in 1..=peaks {
        let across = (step * f64::from(nth)).min(width);
        let height = if nth % 2 == 0 { amplitude * 2.0 } else { 0.0 };
        let _ = writeln!(path, "{across:.2} {height:.2} l");
    }
    path
}

/// How many whole steps fit, rounded up, as a count the loop can use.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn whole(steps: f64) -> u32 {
    // Bounded well inside `u32`: the widest page is 14,400 units and a step is at least 1.
    steps.ceil().clamp(0.0, 1.0e6) as u32
}

/// A stamp: its picture, or the icon a reader falls back to when there is none
/// (12.5.6.12).
///
/// **The picture is a JPEG, carried as it is under `/DCTDecode`.** The bytes used to go
/// into an image XObject with no `/Width`, `/Height`, `/ColorSpace` or `/Filter` — an
/// image no reader can decode, whatever the bytes were — so the size and components are
/// read from the JPEG's own frame header.
///
/// # Errors
/// Fails when the bytes are not a JPEG this can read the frame of.
fn stamp(
    arena: &PdfArena,
    dict: &mut Dict,
    picture: &[u8],
    area: Area,
) -> PdfResult<Option<Object>> {
    name(arena, dict, "Subtype", "Stamp");
    if picture.is_empty() {
        name(arena, dict, "Name", "Draft");
        return Ok(None);
    }
    let image = jpeg_image(arena, picture)?;
    let mut images = Dict::new();
    images.insert(arena.name("Im0"), Object::Reference(image));
    let mut resources = Dict::new();
    resources.insert(arena.name("XObject"), Object::Dictionary(arena.alloc_dict(images)));
    let drawing = format!("q\n{:.2} 0 0 {:.2} 0 0 cm\n/Im0 Do\nQ\n", area.width(), area.height());
    Ok(Some(appearance(arena, &drawing, area, Some(resources))))
}

/// An image XObject carrying `picture`, a JPEG, as it is under `/DCTDecode`.
///
/// Its size and components are read from the JPEG's own frame header, which is what a
/// reader needs to decode it; an image XObject without them decodes as nothing.
///
/// # Errors
/// Fails when the bytes are not a JPEG this can read the frame of.
pub fn jpeg_image(arena: &PdfArena, picture: &[u8]) -> PdfResult<Handle<Object>> {
    let Some((width, height, components)) = jpeg_frame(picture) else {
        return Err(PdfError::refused(
            "place a picture",
            "the picture has to be a JPEG, and this is not one this can read the size of",
        ));
    };
    let space = match components {
        1 => "DeviceGray",
        4 => "DeviceCMYK",
        _ => "DeviceRGB",
    };
    let mut image = Dict::new();
    name(arena, &mut image, "Type", "XObject");
    name(arena, &mut image, "Subtype", "Image");
    image.insert(arena.name("Width"), Object::Integer(i64::from(width)));
    image.insert(arena.name("Height"), Object::Integer(i64::from(height)));
    name(arena, &mut image, "ColorSpace", space);
    image.insert(arena.name("BitsPerComponent"), Object::Integer(8));
    name(arena, &mut image, "Filter", "DCTDecode");
    let stream = Object::Stream(
        arena.alloc_dict(image),
        Arc::new(SublimatedData::Raw(Bytes::copy_from_slice(picture))),
    );
    Ok(arena.alloc_object(stream))
}

/// A JPEG's width, height and number of components, from its first frame header (SOF).
fn jpeg_frame(bytes: &[u8]) -> Option<(u16, u16, u8)> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut at = 2;
    while let Some(&[mark, marker, high, low]) = bytes.get(at..at + 4) {
        if mark != 0xFF {
            return None;
        }
        let length = usize::from(u16::from_be_bytes([high, low]));
        // SOF0 to SOF15, which are not DHT (C4), JPG (C8) or DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let &[h0, h1, w0, w1, components] = bytes.get(at + 5..at + 10)? else {
                return None;
            };
            let height = u16::from_be_bytes([h0, h1]);
            let width = u16::from_be_bytes([w0, w1]);
            return (width > 0 && height > 0).then_some((width, height, components));
        }
        at += 2 + length;
    }
    None
}

/// Freehand strokes (12.5.6.13): `/InkList` in page space, and the same drawn.
fn ink(
    arena: &PdfArena,
    dict: &mut Dict,
    strokes: &[Vec<[f32; 2]>],
    rgb: [f32; 3],
    width: f64,
    area: Area,
) -> Object {
    use std::fmt::Write as _;
    name(arena, dict, "Subtype", "Ink");
    let list: Vec<Object> = strokes
        .iter()
        .map(|stroke| {
            let flat: Vec<f64> =
                stroke.iter().flat_map(|p| [f64::from(p[0]), f64::from(p[1])]).collect();
            numbers(arena, &flat)
        })
        .collect();
    dict.insert(arena.name("InkList"), Object::Array(arena.alloc_array(list)));
    dict.insert(arena.name("C"), color(arena, rgb));
    dict.insert(arena.name("BS"), border(arena, width));
    let mut drawing = format!("{}1 J 1 j\n", stroking(rgb, width));
    for stroke in strokes {
        for (nth, point) in stroke.iter().enumerate() {
            let (across, up) = area.local(*point);
            let _ = writeln!(drawing, "{across:.2} {up:.2} {}", if nth == 0 { "m" } else { "l" });
        }
        drawing.push_str("S\n");
    }
    appearance(arena, &drawing, area, None)
}

/// A rectangle, an ellipse or a line (12.5.6.8, 12.5.6.7), outlined inside its
/// rectangle so a wide outline is not cut in half by the `/BBox`.
fn shape(
    arena: &PdfArena,
    dict: &mut Dict,
    form: &ShapeForm,
    rgb: [f32; 3],
    width: f64,
    area: Area,
) -> Object {
    let pen = stroking(rgb, width);
    let inset = width / 2.0;
    let (wide, high) = (area.width() - width, area.height() - width);
    let drawing = match form {
        ShapeForm::Rectangle => {
            name(arena, dict, "Subtype", "Square");
            format!("{pen}{inset:.2} {inset:.2} {wide:.2} {high:.2} re S\n")
        }
        ShapeForm::Ellipse => {
            name(arena, dict, "Subtype", "Circle");
            let (across, up) = (wide / 2.0, high / 2.0);
            format!("{pen}{}S\n", ellipse(inset + across, inset + up, across, up))
        }
        ShapeForm::Line { from, to } => {
            name(arena, dict, "Subtype", "Line");
            let ends = [from[0], from[1], to[0], to[1]].map(f64::from);
            dict.insert(arena.name("L"), numbers(arena, &ends));
            let ((x1, y1), (x2, y2)) = (area.local(*from), area.local(*to));
            format!("{pen}1 J\n{x1:.2} {y1:.2} m {x2:.2} {y2:.2} l S\n")
        }
    };
    dict.insert(arena.name("C"), color(arena, rgb));
    dict.insert(arena.name("BS"), border(arena, width));
    appearance(arena, &drawing, area, None)
}

/// An ellipse centred on `(cx, cy)` as four Bézier arcs, the usual 0.5523 approximation.
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> String {
    let (kx, ky) = (rx * 0.552_284_75, ry * 0.552_284_75);
    format!(
        "{:.2} {cy:.2} m\n\
         {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c\n\
         {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c\n\
         {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c\n\
         {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c\n",
        cx + rx,
        cx + rx,
        cy + ky,
        cx + kx,
        cy + ry,
        cy + ry,
        cx - kx,
        cy + ry,
        cx - rx,
        cy + ky,
        cx - rx,
        cx - rx,
        cy - ky,
        cx - kx,
        cy - ry,
        cy - ry,
        cx + kx,
        cy - ry,
        cx + rx,
        cy - ky,
        cx + rx,
    )
}

/// The words of a text annotation and the size they are set at.
struct Words<'a> {
    text: &'a str,
    size: f64,
}

/// Text on the page (12.5.6.6, `/FreeText`): a box, a typewriter's line, or a callout.
///
/// **Set in a face that draws every character, embedded**, as the page stamps are, so a
/// Japanese note needs nothing a Latin one does not. A line break starts a new line and
/// nothing wraps: a line longer than the box runs past it, and the `/BBox` shows where.
///
/// # Errors
/// Fails when there is nothing to say, when the size is not positive, or when no face here
/// draws a character of the text.
fn free_text(
    doc: &Document,
    dict: &mut Dict,
    words: &Words<'_>,
    place: &Placing,
    intent: Option<&str>,
) -> PdfResult<Object> {
    let arena = doc.arena();
    if words.text.trim().is_empty() || words.size <= 0.0 {
        return Err(PdfError::refused(
            "AddAnnotation",
            "a text annotation needs words and a size to set them at",
        ));
    }
    name(arena, dict, "Subtype", "FreeText");
    if let Some(intent) = intent {
        name(arena, dict, "IT", intent);
    }
    dict.insert(arena.name("Contents"), Object::Text(words.text.to_string()));
    // Required (Table 177), and what a reader regenerates the appearance from if it will.
    let size = words.size;
    dict.insert(arena.name("DA"), Object::String(Bytes::from(format!("/Helv {size:.2} Tf 0 g"))));

    let lines: Vec<&str> = words.text.lines().collect();
    let face = crate::apply::font::face_for(&lines.concat()).map_err(|why| {
        PdfError::refused("AddAnnotation", format!("{:?} cannot be set: {why}", words.text))
    })?;
    let embedded = crate::apply::font::embed_for(doc, &face.1, &face.0, &lines)?;

    let mut drawing = frame(place, intent != Some("FreeTextTypeWriter"));
    if let Some((line, pointing)) = leader(place) {
        drawing.push_str(&pointing);
        dict.insert(arena.name("CL"), numbers(arena, &line));
    }
    drawing.push_str(&set_lines(&face.1, &embedded, &lines, place, size)?);

    let mut fonts = Dict::new();
    fonts.insert(arena.name("F0"), Object::Reference(embedded.font));
    let mut resources = Dict::new();
    resources.insert(arena.name("Font"), Object::Dictionary(arena.alloc_dict(fonts)));
    Ok(appearance(arena, &drawing, place.area, Some(resources)))
}

/// Where a text annotation's box sits inside the rectangle it occupies, and what its line
/// points at, if it has one.
struct Placing {
    /// The whole rectangle — the box, and a callout's line with it.
    area: Area,
    /// The box the words are set in.
    text_box: Area,
    /// Where a callout's line points, on the page.
    points_at: Option<[f32; 2]>,
}

impl Placing {
    /// The box's lower left corner, in the annotation's own space.
    fn corner(&self) -> (f64, f64) {
        (self.text_box.left - self.area.left, self.text_box.bottom - self.area.bottom)
    }
}

/// The box's outline, or nothing for a typewriter's words, which stand on the page bare.
fn frame(place: &Placing, boxed: bool) -> String {
    if !boxed {
        return String::new();
    }
    let (left, bottom) = place.corner();
    format!(
        "0 0 0 RG 1 w\n{:.2} {:.2} {:.2} {:.2} re S\n",
        left + 0.5,
        bottom + 0.5,
        place.text_box.width() - 1.0,
        place.text_box.height() - 1.0
    )
}

/// A callout's `/CL` in page space, and the line drawn: from what it points at to the
/// nearest point of the box's edge.
fn leader(place: &Placing) -> Option<([f64; 4], String)> {
    let point = place.points_at?;
    let (left, bottom) = place.corner();
    let (from_x, from_y) = place.area.local(point);
    let to_x = from_x.clamp(left, left + place.text_box.width());
    let to_y = from_y.clamp(bottom, bottom + place.text_box.height());
    let line = [
        f64::from(point[0]),
        f64::from(point[1]),
        to_x + place.area.left,
        to_y + place.area.bottom,
    ];
    Some((line, format!("0 0 0 RG 1 w\n{from_x:.2} {from_y:.2} m {to_x:.2} {to_y:.2} l S\n")))
}

/// The words, a line to each line of the text, from the top of the box down.
fn set_lines(
    program: &[u8],
    embedded: &crate::apply::font::Embedded,
    lines: &[&str],
    place: &Placing,
    size: f64,
) -> PdfResult<String> {
    use std::fmt::Write as _;
    let (left, bottom) = place.corner();
    let pad = 2.0;
    let mut baseline = size.mul_add(-0.88, bottom + place.text_box.height() - pad);
    let mut drawing = format!("BT\n/F0 {size:.2} Tf 0 g\n");
    for line in lines {
        if !line.is_empty() {
            let codes = codes_of(program, embedded, line)?;
            let _ = writeln!(drawing, "1 0 0 1 {:.2} {baseline:.2} Tm\n<{codes}> Tj", left + pad);
        }
        baseline = size.mul_add(-1.2, baseline);
    }
    drawing.push_str("ET\n");
    Ok(drawing)
}

/// The codes `line` is shown by in a face embedded by `embed_for`, as hexadecimal.
pub fn codes_of(
    program: &[u8],
    embedded: &crate::apply::font::Embedded,
    line: &str,
) -> PdfResult<String> {
    use std::fmt::Write as _;
    let glyphs = fepdf_font::subset::glyphs_for(program, line)
        .map_err(|c| PdfError::refused("AddAnnotation", format!("this face draws no {c:?}")))?;
    let mut codes = String::with_capacity(glyphs.len() * 4);
    for glyph in glyphs {
        let _ = write!(codes, "{:04X}", embedded.code_of.get(&glyph).copied().unwrap_or(glyph));
    }
    Ok(codes)
}
