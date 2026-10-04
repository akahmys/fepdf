//! The form XObjects a redaction region lies over, entered (ROADMAP Y-10).
//!
//! **A form is content like a page's**, and a region over one covers what it draws. Each
//! form the content draws and a region meets is copied — with resources of its own, so
//! what the redaction adds stays with the copy — and that drawing is pointed at the copy;
//! the copy is then redacted as the page is, with the regions taken into its own space.
//! Another drawing of the same form, on this page or another, keeps the original, and
//! the original goes from these resources once nothing here draws it.
//!
//! **Into the form's own space through the box round the region there**: for a form drawn
//! turned or skewed that is a little more than the region, never less.

use super::image_crop;
use super::target::Target;
use super::text::GlyphBox;
use fepdf_model::lexer::Token;
use fepdf_model::object::SublimatedData;
use fepdf_model::{Document, Handle, Object, PdfError, PdfResult};
use kurbo::{Affine, Point, Rect};
use std::collections::BTreeMap;
use std::sync::Arc;

/// How deep forms drawing forms are entered. A form that draws itself is a cycle 8.10.1
/// does not forbid, and a redaction that followed it would not end.
pub const DEEPEST: usize = 16;

/// One form XObject a content stream draws.
pub struct Drawn {
    /// Where the name before `Do` sits among the tokens.
    pub name_at: usize,
    /// Where the `Do` sits.
    pub do_at: usize,
    /// The form.
    pub form: Handle<Object>,
    /// What takes the form's own space into the space it is drawn in: the transform at
    /// `Do` and the form's `/Matrix`.
    pub placed: Affine,
    /// The form's `/BBox`, in its own space.
    pub bbox: Rect,
}

/// Every form XObject `tokens` draw, from `target`'s resources.
///
/// # Errors
/// As [`Target::resources_read`].
pub fn forms_drawn(doc: &Document, target: Target, tokens: &[Token]) -> PdfResult<Vec<Drawn>> {
    let forms = forms_in(doc, target)?;
    let (mut ctm, mut saved, mut drawn) = (Affine::IDENTITY, Vec::new(), Vec::new());
    let mut operands_from = 0;
    for (index, token) in tokens.iter().enumerate() {
        let Token::Keyword(op) = token else { continue };
        let operands = &tokens[operands_from..index];
        operands_from = index + 1;
        match op.as_str() {
            "q" => saved.push(ctm),
            "Q" => ctm = saved.pop().unwrap_or(ctm),
            "cm" => ctm *= image_crop::matrix_of(operands),
            "Do" => {
                let Some(Token::Name(name)) = operands.last() else { continue };
                let Some((form, matrix, bbox)) =
                    forms.get(&String::from_utf8_lossy(name).to_string())
                else {
                    continue;
                };
                drawn.push(Drawn {
                    name_at: index - 1,
                    do_at: index,
                    form: *form,
                    placed: ctm * *matrix,
                    bbox: *bbox,
                });
            }
            _ => {}
        }
    }
    Ok(drawn)
}

/// The form XObjects `target`'s resources name, by name, with each one's `/Matrix` and
/// `/BBox`.
fn forms_in(
    doc: &Document,
    target: Target,
) -> PdfResult<BTreeMap<String, (Handle<Object>, Affine, Rect)>> {
    let arena = doc.arena();
    let resources = target.resources_read(doc)?;
    let mut found = BTreeMap::new();
    let Some(xobjects) = arena
        .dict_entry(resources, arena.name("XObject"))
        .and_then(|x| x.resolve(arena).as_dict_handle())
    else {
        return Ok(found);
    };
    for (key, value) in arena.get_dict(xobjects).unwrap_or_default() {
        let Some(handle) = value.as_reference() else { continue };
        let Some(Object::Stream(dict, _)) = arena.get_object(handle) else { continue };
        let entry = |k: &str| arena.dict_entry(dict, arena.name(k)).map(|v| v.resolve(arena));
        if entry("Subtype").and_then(|s| s.as_name()) != Some(arena.name("Form")) {
            continue;
        }
        let numbers = |k: &str| -> Option<Vec<f64>> {
            let array = arena.get_array(entry(k)?.as_array()?)?;
            array.iter().map(|n| n.resolve(arena).as_f64()).collect()
        };
        let matrix = numbers("Matrix")
            .filter(|m| m.len() == 6)
            .map_or(Affine::IDENTITY, |m| Affine::new([m[0], m[1], m[2], m[3], m[4], m[5]]));
        let Some(b) = numbers("BBox").filter(|b| b.len() == 4) else { continue };
        if let Some(name) = arena.get_name(key) {
            found.insert(
                name.as_str().to_string(),
                (handle, matrix, Rect::new(b[0], b[1], b[2], b[3])),
            );
        }
    }
    Ok(found)
}

/// `regions` taken into the space of a form placed by `placed` and cut to its `bbox`; the
/// ones that miss it are left out.
pub fn regions_inside(drawn: &Drawn, regions: &[GlyphBox]) -> Vec<GlyphBox> {
    if drawn.placed.determinant().abs() < 1e-12 {
        return Vec::new();
    }
    let inverse = drawn.placed.inverse();
    regions
        .iter()
        .filter_map(|&(x0, y0, x1, y1)| {
            let corners =
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| inverse * Point::new(x, y));
            let around = corners
                .iter()
                .skip(1)
                .fold(Rect::from_points(corners[0], corners[0]), |r, p| r.union_pt(*p));
            let inside = around.intersect(drawn.bbox);
            (inside.width() > 0.0 && inside.height() > 0.0)
                .then_some((inside.x0, inside.y0, inside.x1, inside.y1))
        })
        .collect()
}

/// A box in a form's space, taken into the space it is drawn in.
pub fn placed_box(drawn: &Drawn, inside: GlyphBox) -> GlyphBox {
    let r = drawn.placed.transform_rect_bbox(Rect::new(inside.0, inside.1, inside.2, inside.3));
    (r.x0, r.y0, r.x1, r.y1)
}

/// Copies `form` with resources of its own — the form's, or `target`'s where it has none
/// (7.8.3) — and its `/XObject` dictionary copied too, since a redaction names things in
/// it; the content is shared until it is rewritten.
///
/// # Errors
/// As [`Target::resources_read`].
pub fn copied(doc: &Document, target: Target, form: Handle<Object>) -> PdfResult<Handle<Object>> {
    let arena = doc.arena();
    let Some(Object::Stream(dict_h, data)) = arena.get_object(form) else {
        return Err(PdfError::refused("redact", "a form under the region is not a stream"));
    };
    let mut dict = arena.get_dict(dict_h).unwrap_or_default();
    let key = arena.name("Resources");
    let resources = match dict.get(&key).and_then(|r| r.resolve(arena).as_dict_handle()) {
        Some(own) => own,
        None => target.resources_read(doc)?,
    };
    let mut copy = arena.get_dict(resources).unwrap_or_default();
    let xobject = arena.name("XObject");
    if let Some(names) = copy.get(&xobject).and_then(|x| x.resolve(arena).as_dict_handle()) {
        let names = arena.get_dict(names).unwrap_or_default();
        copy.insert(xobject, Object::Dictionary(arena.alloc_dict(names)));
    }
    dict.insert(key, Object::Dictionary(arena.alloc_dict(copy)));
    let data: Arc<SublimatedData> = data;
    Ok(arena.alloc_object(Object::Stream(arena.alloc_dict(dict), data)))
}
