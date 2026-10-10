//! The annotation kinds `AddAnnotation` makes beyond the first (ROADMAP AA-4f): Polygon
//! and PolyLine (as shapes, in `markup`), Caret, FileAttachment, Screen, Popup,
//! PrinterMark, Watermark, Redact and Projection.
//!
//! **Each writes the entries its table names, and is drawn from them** where `drawn`
//! draws that subtype — a caret, an attachment's push pin, a redaction's outline — so
//! what is made and what is imported look the same. The three whose appearance is their
//! content are drawn by `drawn::made`. A popup and a projection draw nothing (Table 166).
//!
//! Sound, Movie and TrapNet are deprecated in PDF 2.0 and are not made; 3D and RichMedia
//! are shown and not made (the owner, 2026-10-10).

use super::markup::{Area, color, name};
use crate::operation::{AnnotationKind, MediaClip, PrinterMarkKind};
use fepdf_model::object::PdfName;
use fepdf_model::{DictHandle, Document, Handle, Object, PdfError, PdfResult};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// Whether `kind` is a markup annotation (Table 171), which is named and signed.
pub const fn is_markup(kind: &AnnotationKind) -> bool {
    match kind {
        AnnotationKind::Link { .. }
        | AnnotationKind::Screen { .. }
        | AnnotationKind::Popup { .. }
        | AnnotationKind::PrinterMark { .. }
        | AnnotationKind::Watermark { .. } => false,
        AnnotationKind::Highlight { .. }
        | AnnotationKind::TextComment { .. }
        | AnnotationKind::Stamp { .. }
        | AnnotationKind::Underline { .. }
        | AnnotationKind::StrikeOut { .. }
        | AnnotationKind::Squiggly { .. }
        | AnnotationKind::TextBox { .. }
        | AnnotationKind::Typewriter { .. }
        | AnnotationKind::Callout { .. }
        | AnnotationKind::Ink { .. }
        | AnnotationKind::Shape { .. }
        | AnnotationKind::Caret { .. }
        | AnnotationKind::FileAttachment { .. }
        | AnnotationKind::Redact { .. }
        | AnnotationKind::Projection { .. } => true,
    }
}

/// The entries of a kind made here, and its appearance where `drawn` cannot draw it from
/// them — `None` leaves it to `drawn::appearance_for`, which draws a caret, a push pin and
/// a redaction, and nothing for a popup or a projection.
///
/// # Errors
/// When a watermark is refused, or its words will not embed.
pub(super) fn entries(
    doc: &Document,
    dict: &mut Dict,
    kind: &AnnotationKind,
    area: Area,
) -> PdfResult<Option<Object>> {
    Ok(match kind {
        AnnotationKind::Caret { contents, color_rgb, paragraph } => {
            caret(doc, dict, contents, *color_rgb, *paragraph);
            None
        }
        AnnotationKind::FileAttachment { filename, mime_type, data, description } => {
            attachment(doc, dict, (filename, mime_type.as_ref(), data), description.as_ref());
            None
        }
        AnnotationKind::Screen { title, clip } => Some(screen(doc, dict, (title, clip), area)),
        AnnotationKind::Popup { open, .. } => {
            popup(doc, dict, *open);
            None
        }
        AnnotationKind::PrinterMark { mark } => Some(printer_mark(doc, dict, *mark, area)),
        AnnotationKind::Watermark { text, font_size, opacity } => {
            Some(watermark(doc, dict, (text, *font_size, *opacity), area)?)
        }
        AnnotationKind::Redact { overlay_text, interior_rgb } => {
            redact(doc, dict, overlay_text.as_ref(), *interior_rgb, area);
            None
        }
        AnnotationKind::Projection { contents } => {
            projection(doc, dict, contents);
            None
        }
        // Made in `markup`, which does not call this for them.
        AnnotationKind::Link { .. }
        | AnnotationKind::Highlight { .. }
        | AnnotationKind::TextComment { .. }
        | AnnotationKind::Stamp { .. }
        | AnnotationKind::Underline { .. }
        | AnnotationKind::StrikeOut { .. }
        | AnnotationKind::Squiggly { .. }
        | AnnotationKind::TextBox { .. }
        | AnnotationKind::Typewriter { .. }
        | AnnotationKind::Callout { .. }
        | AnnotationKind::Ink { .. }
        | AnnotationKind::Shape { .. } => None,
    })
}

/// A caret (12.5.6.11, Table 180): the words to go in, and `/Sy /P` for a paragraph.
pub(super) fn caret(
    doc: &Document,
    dict: &mut Dict,
    contents: &str,
    rgb: [f32; 3],
    paragraph: bool,
) {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "Caret");
    dict.insert(arena.name("Contents"), Object::Text(contents.to_owned()));
    dict.insert(arena.name("C"), color(arena, rgb));
    name(arena, dict, "Sy", if paragraph { "P" } else { "None" });
}

/// A file attachment (12.5.6.15, Table 184): the file embedded in `/FS`, a push pin, and
/// what it is as the annotation's text.
pub(super) fn attachment(
    doc: &Document,
    dict: &mut Dict,
    (filename, mime_type, data): (&str, Option<&String>, &[u8]),
    description: Option<&String>,
) {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "FileAttachment");
    let spec = super::metadata::create_embedded_filespec(
        arena,
        filename.to_owned(),
        mime_type.cloned(),
        description.cloned(),
        u64::try_from(data.len()).unwrap_or(u64::MAX),
        data.to_vec(),
        None,
    );
    dict.insert(arena.name("FS"), Object::Reference(spec));
    name(arena, dict, "Name", "PushPin");
    let said = description.map_or(filename, String::as_str);
    dict.insert(arena.name("Contents"), Object::Text(said.to_owned()));
}

/// A screen (12.5.6.18, Table 190): its title, and a rendition action playing `clip`
/// (12.6.4.14), whose `/AN` names the screen once it has a number ([`link_back`]); and its
/// stand-in appearance.
fn screen(
    doc: &Document,
    dict: &mut Dict,
    (title, clip): (&Option<String>, &Option<MediaClip>),
    area: Area,
) -> Object {
    entitle(doc, dict, title.as_ref(), clip.as_ref());
    super::drawn::made::screen(doc, dict, area)
}

/// A screen's subtype, title and rendition.
fn entitle(doc: &Document, dict: &mut Dict, title: Option<&String>, clip: Option<&MediaClip>) {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "Screen");
    if let Some(title) = title {
        dict.insert(arena.name("T"), Object::Text(title.clone()));
    }
    let Some(clip) = clip else { return };
    let file = super::metadata::create_embedded_filespec(
        arena,
        clip.filename.clone(),
        Some(clip.mime_type.clone()),
        None,
        u64::try_from(clip.data.len()).unwrap_or(u64::MAX),
        clip.data.clone(),
        None,
    );
    // Table 287: a temporary file may be made to play it.
    let mut permissions = Dict::new();
    permissions.insert(arena.name("TF"), Object::String(bytes::Bytes::from_static(b"TEMPACCESS")));
    let mut media = Dict::new();
    name(arena, &mut media, "Type", "MediaClip");
    name(arena, &mut media, "S", "MCD");
    media.insert(arena.name("CT"), Object::String(bytes::Bytes::from(clip.mime_type.clone())));
    media.insert(arena.name("D"), Object::Reference(file));
    media.insert(arena.name("P"), Object::Dictionary(arena.alloc_dict(permissions)));
    let mut rendition = Dict::new();
    name(arena, &mut rendition, "Type", "Rendition");
    name(arena, &mut rendition, "S", "MR");
    rendition.insert(arena.name("C"), Object::Dictionary(arena.alloc_dict(media)));
    let mut action = Dict::new();
    name(arena, &mut action, "Type", "Action");
    name(arena, &mut action, "S", "Rendition");
    action.insert(arena.name("R"), Object::Dictionary(arena.alloc_dict(rendition)));
    // Table 214: play it, which is operation 0.
    action.insert(arena.name("OP"), Object::Integer(0));
    dict.insert(arena.name("A"), Object::Dictionary(arena.alloc_dict(action)));
}

/// A printer's mark (14.11.3, Table 398): `/MN` names it, and the mark is its appearance.
fn printer_mark(doc: &Document, dict: &mut Dict, mark: PrinterMarkKind, area: Area) -> Object {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "PrinterMark");
    let called = match mark {
        PrinterMarkKind::RegistrationTarget => "RegistrationTarget",
        PrinterMarkKind::ColorBar => "ColorBar",
    };
    name(arena, dict, "MN", called);
    super::drawn::made::printer_mark(doc, dict, mark, area)
}

/// A watermark (12.5.6.22): its words as `/Contents` and drawn, and its opacity as `/CA`.
///
/// # Errors
/// Refused when no face here draws the words: a watermark with none is nothing.
fn watermark(
    doc: &Document,
    dict: &mut Dict,
    (text, size, opacity): (&str, f32, f32),
    area: Area,
) -> PdfResult<Object> {
    let refuse = |why: String| Err(PdfError::refused("AddAnnotation", why));
    if text.trim().is_empty() {
        return refuse("a watermark with no words draws nothing".to_owned());
    }
    if let Err(why) = super::font::face_for(text) {
        return refuse(format!("no face here draws the watermark {text:?}: {why}"));
    }
    if !(opacity > 0.0 && opacity <= 1.0) {
        return refuse(format!("a watermark {opacity} opaque is not one from above 0 to 1"));
    }
    let arena = doc.arena();
    name(arena, dict, "Subtype", "Watermark");
    dict.insert(arena.name("Contents"), Object::Text(text.to_owned()));
    if opacity < 1.0 {
        dict.insert(arena.name("CA"), Object::Real(f64::from(opacity)));
    }
    super::drawn::made::watermark(doc, dict, text, f64::from(size), area)
}

/// A region marked for redaction (12.5.6.23, Table 194): `/QuadPoints` of the rectangle,
/// the fill it will have, and the words over it.
pub(super) fn redact(
    doc: &Document,
    dict: &mut Dict,
    overlay_text: Option<&String>,
    interior: Option<[f32; 3]>,
    area: Area,
) {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "Redact");
    let (l, b, r, t) = (area.left, area.bottom, area.right, area.top);
    dict.insert(arena.name("QuadPoints"), super::markup::numbers(arena, &[l, t, r, t, l, b, r, b]));
    if let Some(rgb) = interior {
        dict.insert(arena.name("IC"), color(arena, rgb));
    }
    if let Some(words) = overlay_text {
        dict.insert(arena.name("OverlayText"), Object::Text(words.clone()));
        // Table 194 requires `/DA` with `/OverlayText`.
        dict.insert(arena.name("DA"), Object::Text("/Helv 12 Tf 0 g".to_owned()));
    }
}

/// A projection (12.5.6.24): a comment, and nothing drawn.
pub(super) fn projection(doc: &Document, dict: &mut Dict, contents: &str) {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "Projection");
    dict.insert(arena.name("Contents"), Object::Text(contents.to_owned()));
}

/// A popup (12.5.6.14): whether it opens shown; its parent is put in by [`link_back`].
pub(super) fn popup(doc: &Document, dict: &mut Dict, open: bool) {
    let arena = doc.arena();
    name(arena, dict, "Subtype", "Popup");
    dict.insert(arena.name("Open"), Object::Boolean(open));
}

/// What can be written only once the annotation is an object: a screen's rendition names
/// it in `/AN`, and a popup and its parent name each other (`/Parent`, `/Popup`).
///
/// # Errors
/// Refused when a popup's parent is not a markup annotation on the page, held as an
/// object, without a popup already.
pub fn link_back(
    doc: &Document,
    (page, kind): (usize, &AnnotationKind),
    made: DictHandle,
    object: Handle<Object>,
) -> PdfResult<()> {
    let arena = doc.arena();
    if let AnnotationKind::Screen { clip: Some(_), .. } = kind
        && let Some(action) =
            arena.dict_entry(made, arena.name("A")).and_then(|a| a.as_dict_handle())
    {
        let mut entries = arena.get_dict(action).unwrap_or_default();
        entries.insert(arena.name("AN"), Object::Reference(object));
        arena.set_dict(action, entries);
    }
    if let AnnotationKind::Popup { parent, .. } = kind {
        let (parent_object, parent_dict) = parent_of(doc, page, *parent)?;
        let mut popup = arena.get_dict(made).unwrap_or_default();
        popup.insert(arena.name("Parent"), Object::Reference(parent_object));
        arena.set_dict(made, popup);
        let mut entries = arena.get_dict(parent_dict).unwrap_or_default();
        entries.insert(arena.name("Popup"), Object::Reference(object));
        arena.set_dict(parent_dict, entries);
    }
    Ok(())
}

/// The markup annotation a popup is made for: its object and its dictionary.
fn parent_of(doc: &Document, page: usize, index: usize) -> PdfResult<(Handle<Object>, DictHandle)> {
    let arena = doc.arena();
    let refuse = |why: String| Err(PdfError::refused("AddAnnotation", why));
    let annots = super::redact_annots::annotations_on(doc, page)?;
    let Some(Object::Reference(object)) = annots.get(index).cloned() else {
        return refuse(format!("page {page} has no annotation {index} held as an object"));
    };
    let Some(dict) = Object::Reference(object).resolve(arena).as_dict_handle() else {
        return refuse(format!("annotation {index} on page {page} is not a dictionary"));
    };
    let subtype = arena
        .dict_entry(dict, arena.name("Subtype"))
        .and_then(|s| s.as_name())
        .and_then(|s| arena.get_name_str(s))
        .unwrap_or_default();
    if !fepdf_model::annotation::MARKUP_SUBTYPES.contains(&subtype.as_str()) {
        return refuse(format!("a /{subtype} is not a markup annotation, and has no popup"));
    }
    if arena.dict_entry(dict, arena.name("Popup")).is_some() {
        return refuse(format!("annotation {index} on page {page} has a popup already"));
    }
    Ok((object, dict))
}
