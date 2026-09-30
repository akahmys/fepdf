//! The objects a page draws with `Do`: listing them, and moving, scaling, turning or
//! replacing one (ROADMAP W-E5).
//!
//! **An object is named by its place among the page's `Do` operators**, as a run is by
//! its place among the show-text operators, and the listing and the edit walk the same
//! stream in the same way, so the number read is the number acted on. An edit wraps the
//! `Do` it names and leaves one `Do` there, so the numbering survives it.
//!
//! **An edit is said on the page and done in the object's own space.** "Move it to here"
//! and "turn it about its centre" are page-space transforms; the content stream draws the
//! object under whatever matrix is in force, so the edit is written as the `cm` that turns
//! that matrix into the one asked for, inside a `q … Q` of its own.
//!
//! **A replacement is a new object, not an edit of the old one.** The XObject a page
//! names may be named by other pages too, so the picture is added under a name of its own
//! and only this `Do` is pointed at it; the old one leaves the resources once nothing
//! drawing with them draws it.
//!
//! **Reached: the page's own content.** An object inside a form XObject is not listed.

use crate::operation::XObjectEdit;
use fepdf_model::lexer::{Lexer, Token};
use fepdf_model::{Document, Handle, Object, PdfError, PdfResult};
use kurbo::{Affine, Point, Rect};
use std::collections::BTreeMap;

/// One object a page draws: what kind, under what name, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnObject {
    /// Its place among the page's `Do` operators, counting from zero.
    pub index: usize,
    /// The resource name the content stream draws it by.
    pub name: String,
    /// `true` for an image, `false` for a form XObject.
    pub image: bool,
    /// Its four corners on the page, going round it: the image's unit square, or the
    /// form's `/BBox`, taken through the matrices in force.
    pub corners: [(f64, f64); 4],
}

impl DrawnObject {
    /// The upright rectangle round it on the page.
    #[must_use]
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let xs = self.corners.map(|c| c.0);
        let ys = self.corners.map(|c| c.1);
        let low = |v: [f64; 4]| v.iter().copied().fold(f64::MAX, f64::min);
        let high = |v: [f64; 4]| v.iter().copied().fold(f64::MIN, f64::max);
        (low(xs), low(ys), high(xs), high(ys))
    }
}

/// A `Do` found in the page's content: where it is, and the matrix it draws under.
struct Found {
    /// The index of the name operand; the `Do` is the token after it.
    at: usize,
    name: String,
    ctm: Affine,
}

/// Every object the page draws, in the order its content draws them.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn objects_of_page(doc: &Document, page: usize) -> PdfResult<Vec<DrawnObject>> {
    let Some(data) = crate::apply::text::page_content(doc, page)? else { return Ok(Vec::new()) };
    let named = named_xobjects(doc, page)?;
    let (_, found) = walk(&data);
    Ok(found
        .iter()
        .enumerate()
        .filter_map(|(index, draw)| {
            let (handle, image) = named.get(&draw.name)?;
            let local = local_box(doc, *handle, *image);
            let corners = [
                Point::new(local.x0, local.y0),
                Point::new(local.x1, local.y0),
                Point::new(local.x1, local.y1),
                Point::new(local.x0, local.y1),
            ]
            .map(|p| {
                let on_page = draw.ctm * p;
                (on_page.x, on_page.y)
            });
            Some(DrawnObject { index, name: draw.name.clone(), image: *image, corners })
        })
        .collect())
}

/// Applies `edit` to object `object` of `page`.
///
/// # Errors
/// Fails when the page or the object is not there, when a scale is not positive, or when a
/// replacement is not a JPEG or the object it would replace is not an image.
pub fn apply_edit_xobject(
    doc: &Document,
    page: usize,
    object: usize,
    edit: &XObjectEdit,
) -> PdfResult<()> {
    let listed = objects_of_page(doc, page)?;
    let Some(target) = listed.iter().find(|o| o.index == object) else {
        return Err(PdfError::Other(
            format!("this page draws {} objects and no object {object}", listed.len()).into(),
        ));
    };
    let Some(data) = crate::apply::text::page_content(doc, page)? else { return Ok(()) };
    let (tokens, found) = walk(&data);
    let Some(draw) = found.get(object) else { return Ok(()) };

    let (transform, name) = match edit {
        XObjectEdit::Move { to } => (move_to(target, *to), draw.name.clone()),
        XObjectEdit::Scale { by } => {
            if !(*by > 0.0 && by.is_finite()) {
                return Err(PdfError::Other(format!("a scale of {by} draws nothing").into()));
            }
            (about_centre(target, Affine::scale(*by)), draw.name.clone())
        }
        XObjectEdit::Rotate { degrees } => {
            (about_centre(target, Affine::rotate(degrees.to_radians())), draw.name.clone())
        }
        XObjectEdit::Replace { jpeg } => {
            if !target.image {
                return Err(PdfError::Other(
                    format!(
                        "object {object} is a form, and only an image is replaced by a picture"
                    )
                    .into(),
                ));
            }
            let image = crate::apply::markup::jpeg_image(doc.arena(), jpeg)?;
            (Affine::IDENTITY, name_in_page(doc, page, image)?)
        }
    };
    // The page-space `transform` after the matrix in force is `ctm⁻¹ · transform · ctm`
    // in the object's own space, which is what a `cm` there multiplies in.
    let local = draw.ctm.inverse() * transform * draw.ctm;
    let matrix: Vec<String> = local.as_coeffs().iter().map(|v| format!("{v:.6}")).collect();
    let written = format!("q {} cm /{name} Do Q ", matrix.join(" "));
    let mut replaced = BTreeMap::new();
    replaced.insert(draw.at, (draw.at + 1, written.into_bytes()));
    let out = crate::apply::path_crop::rewritten(&tokens, &replaced);
    crate::apply::text::write_page_content(doc, page, out)?;
    if matches!(edit, XObjectEdit::Replace { .. }) {
        // The picture replaced stays wherever another page, or another `Do` here, draws
        // it, and goes from the file where nothing does: left in the resources, it was
        // written, and a replacement meant to take it out had sent it (ROADMAP Y-F20).
        crate::apply::image_crop::drop_undrawn_images(doc, &[page])?;
    }
    Ok(())
}

/// The page-space move that puts the object's lower left corner at `to`.
fn move_to(target: &DrawnObject, to: (f64, f64)) -> Affine {
    let (left, bottom, _, _) = target.bounds();
    Affine::translate((to.0 - left, to.1 - bottom))
}

/// `transform`, done about the centre of the object's bounds on the page.
fn about_centre(target: &DrawnObject, transform: Affine) -> Affine {
    let (left, bottom, right, top) = target.bounds();
    let centre = (f64::midpoint(left, right), f64::midpoint(bottom, top));
    Affine::translate(centre) * transform * Affine::translate((-centre.0, -centre.1))
}

/// The tokens of `data`, and every `Do` in it with the matrix it draws under.
fn walk(data: &[u8]) -> (Vec<Token>, Vec<Found>) {
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(data));
    let mut tokens = Vec::new();
    while let Ok(token) = lexer.next_token() {
        if token == Token::EOF {
            break;
        }
        tokens.push(token);
    }
    let (mut ctm, mut saved) = (Affine::IDENTITY, Vec::new());
    let mut found = Vec::new();
    let mut operands_from = 0;
    for (index, token) in tokens.iter().enumerate() {
        let Token::Keyword(op) = token else { continue };
        let operands = &tokens[operands_from..index];
        operands_from = index + 1;
        match op.as_str() {
            "q" => saved.push(ctm),
            "Q" => ctm = saved.pop().unwrap_or(ctm),
            "cm" => {
                let numbers: Vec<f64> = operands
                    .iter()
                    .filter_map(|t| match t {
                        Token::Integer(n) => i32::try_from(*n).ok().map(f64::from),
                        Token::Real(n) => Some(*n),
                        _ => None,
                    })
                    .collect();
                if let [a, b, c, d, e, f] = numbers[..] {
                    ctm *= Affine::new([a, b, c, d, e, f]);
                }
            }
            "Do" => {
                if let Some(Token::Name(name)) = operands.last() {
                    found.push(Found {
                        at: index - 1,
                        name: String::from_utf8_lossy(name).into_owned(),
                        ctm,
                    });
                }
            }
            _ => {}
        }
    }
    (tokens, found)
}

/// The XObjects the page's resources name, with whether each is an image.
fn named_xobjects(
    doc: &Document,
    page: usize,
) -> PdfResult<BTreeMap<String, (Handle<Object>, bool)>> {
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let resources =
        fepdf_model::Page::new(arena, page_h, doc.get_parent_chain(page_h)).resources_handle();
    let (subtype, image) = (arena.name("Subtype"), arena.name("Image"));
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
            && let Some(name) = arena.get_name(key)
        {
            let is_image = arena.dict_entry(dict, subtype).and_then(|s| s.as_name()) == Some(image);
            found.insert(name.as_str().to_string(), (handle, is_image));
        }
    }
    Ok(found)
}

/// Where the object draws in its own space: the unit square for an image (8.9.4), and a
/// form's `/BBox` through its `/Matrix` (8.10.1).
fn local_box(doc: &Document, handle: Handle<Object>, image: bool) -> Rect {
    let unit = Rect::new(0.0, 0.0, 1.0, 1.0);
    if image {
        return unit;
    }
    let arena = doc.arena();
    let Some(Object::Stream(dict, _)) = arena.get_object(handle) else { return unit };
    let numbers = |key: &str| -> Vec<f64> {
        match arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena)) {
            Some(Object::Array(a)) => {
                arena.get_array(a).unwrap_or_default().iter().filter_map(Object::as_f64).collect()
            }
            _ => Vec::new(),
        }
    };
    let [x0, y0, x1, y1] = numbers("BBox")[..] else { return unit };
    let six = numbers("Matrix");
    let matrix = if six.len() == 6 {
        Affine::new([six[0], six[1], six[2], six[3], six[4], six[5]])
    } else {
        Affine::IDENTITY
    };
    matrix.transform_rect_bbox(Rect::new(x0, y0, x1, y1))
}

/// Names `image` in the page's resources, under a name nothing there uses.
fn name_in_page(doc: &Document, page: usize, image: Handle<Object>) -> PdfResult<String> {
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let resources = crate::apply::annotations::ensure_page_resources(doc, page_h, &mut page_dict);
    arena.set_dict(page_dh, page_dict);
    let key = arena.name("XObject");
    let mut xobjects = arena
        .dict_entry(resources, key)
        .and_then(|x| x.resolve(arena).as_dict_handle())
        .and_then(|x| arena.get_dict(x))
        .unwrap_or_default();
    let name = (0..=xobjects.len())
        .map(|n| format!("fepdfPicture{n}"))
        .find(|n| !xobjects.contains_key(&arena.name(n)))
        .unwrap_or_default();
    xobjects.insert(arena.name(&name), Object::Reference(image));
    let mut dict = arena.get_dict(resources).unwrap_or_default();
    dict.insert(key, Object::Dictionary(arena.alloc_dict(xobjects)));
    arena.set_dict(resources, dict);
    Ok(name)
}
