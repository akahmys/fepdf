//! Building a field's appearance from its value (12.7.4.3, "Variable text").
//!
//! **`/NeedAppearances` is deprecated in PDF 2.0**, and setting a field value used to
//! consist of writing the value and then setting that flag — which is a producer telling
//! the reader "you work it out", using an entry this edition lists among the features it
//! deprecates (0.3). This engine's own rule is *do not write what 2.0 deprecates*, and it
//! was applied to encryption ([ADR-0015](../../../docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md))
//! and not to forms.
//!
//! So the appearance is built here instead. 12.7.4.3 says what it has to be: a form
//! XObject whose `/BBox` is the widget's rectangle moved to the origin, whose
//! `/Resources` come from the interactive form's `/DR`, and whose content carries the
//! text between `/Tx BMC` and `EMC` with the field's `/DA` string setting the font.
//!
//! **What is approximated, and what the clause permits.** A `/DA` with a size of zero
//! means auto-size, and the standard says that size is *an implementation dependent
//! function* — so the choice here is conforming by construction, and it is written down
//! rather than left to be reverse-engineered. The baseline is not specified at all; it is
//! placed to centre a single line. Quadding *is* specified, and needs the width of the
//! text, so the font named in `/DA` is loaded from `/DR` and its glyph widths are summed.

use bytes::Bytes;
use fepdf_model::arena::PdfArena;
use fepdf_model::interpretation::Decision;
use fepdf_model::object::SublimatedData;
use fepdf_model::{Document, Handle, Object, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// How far the text sits from the left or right edge of the box, in points. Two is what
/// the widget border conventionally occupies, and a value the clause does not fix.
const INSET: f64 = 2.0;

/// The font and size a `/DA` string selects (12.7.4.3).
struct DefaultAppearance {
    /// The resource name of the font, without its solidus.
    font: String,
    /// The size in points, or zero for auto.
    size: f64,
    /// The whole string, replayed into the appearance so the colour and any other state
    /// operators it carries survive.
    verbatim: String,
}

/// Reads the `Tf` operator out of a default appearance string.
///
/// The clause requires at minimum a `Tf` with its two operands; everything else in the
/// string is graphics state this function does not need to understand, because it is
/// replayed unchanged.
fn parse_default_appearance(da: &str) -> Option<DefaultAppearance> {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let at = tokens.iter().position(|t| *t == "Tf")?;
    let size = tokens.get(at.checked_sub(1)?)?.parse().ok()?;
    let font = tokens.get(at.checked_sub(2)?)?.strip_prefix('/')?.to_string();
    Some(DefaultAppearance { font, size, verbatim: da.to_string() })
}

/// The rectangle of a widget, as width and height.
fn widget_size(arena: &PdfArena, widget: &Dict) -> Option<(f64, f64)> {
    let Object::Array(handle) = widget.get(&arena.name("Rect"))?.resolve(arena) else {
        return None;
    };
    let rect = arena.get_array(handle)?;
    let at = |i: usize| rect.get(i).and_then(|v| v.resolve(arena).as_f64());
    let (x1, y1, x2, y2) = (at(0)?, at(1)?, at(2)?, at(3)?);
    Some(((x2 - x1).abs(), (y2 - y1).abs()))
}

/// The width of `text` in the font `/DA` names, in points, when that font can be loaded.
///
/// `None` where it cannot: the resource is missing, or the font will not load. The caller
/// then leaves the text left-aligned and says so, rather than guessing a width and
/// putting the text somewhere the file did not ask for.
fn text_width(
    doc: &Document,
    resources: &Dict,
    appearance: &DefaultAppearance,
    text: &str,
) -> Option<f64> {
    let arena = doc.arena();
    let fonts = resources.get(&arena.name("Font"))?.resolve(arena).as_dict_handle()?;
    let entry = arena.get_dict(fonts)?.get(&arena.name(&appearance.font))?.clone();
    let font = doc.get_font(entry.as_reference()?).ok()?;
    let sum: f32 = text.bytes().map(|byte| font.glyph_width(&[byte])).sum();
    Some(f64::from(sum) / 1000.0 * appearance.size)
}

/// Where the text starts, from the quadding the field asks for (Table 228's `/Q`).
fn left_edge(quadding: i64, box_width: f64, text_width: Option<f64>) -> f64 {
    let Some(width) = text_width else { return INSET };
    match quadding {
        1 => ((box_width - width) / 2.0).max(INSET),
        2 => (box_width - width - INSET).max(INSET),
        _ => INSET,
    }
}

/// The appearance stream's content, between `/Tx BMC` and `EMC` as the clause shows it.
fn appearance_content(
    appearance: &DefaultAppearance,
    text: &str,
    size: f64,
    x: f64,
    y: f64,
) -> String {
    let escaped = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let da = &appearance.verbatim;
    format!(
        "/Tx BMC\nq\nBT\n{da}\n/{} {size} Tf\n1 0 0 1 {x:.2} {y:.2} Tm\n({escaped}) Tj\nET\nQ\nEMC\n",
        appearance.font
    )
}

/// Rebuilds a widget's normal appearance for a text value, returning whether it could.
///
/// # Errors
/// Fails only where the arena refuses a handle it just produced.
pub fn set_text_appearance(
    doc: &Document,
    widget_dh: Handle<Dict>,
    acro: &Dict,
    da: &str,
    quadding: i64,
    text: &str,
) -> PdfResult<bool> {
    let arena = doc.arena();
    let mut widget = arena.get_dict(widget_dh).unwrap_or_default();
    let Some((width, height)) = widget_size(arena, &widget) else { return Ok(false) };
    let Some(appearance) = parse_default_appearance(da) else { return Ok(false) };

    // A zero size means auto, and 12.7.4.3 makes the function implementation dependent.
    // One line in a box: leave the inset above and below, and stop at twelve points so a
    // tall box does not produce text nobody would have chosen.
    let size = if appearance.size > 0.0 {
        appearance.size
    } else {
        2.0f64.mul_add(-INSET, height).clamp(4.0, 12.0)
    };
    let sized = DefaultAppearance { size, ..appearance };

    let resources = acro
        .get(&arena.name("DR"))
        .and_then(|dr| dr.resolve(arena).as_dict_handle())
        .and_then(|dh| arena.get_dict(dh))
        .unwrap_or_default();
    let measured = text_width(doc, &resources, &sized, text);
    let x = left_edge(quadding, width, measured);
    // The baseline is not specified. Centring the em box puts a single line where a
    // reader expects it, and is what every implementation this was compared against does.
    let y = ((height - size) / 2.0).max(INSET) + size * 0.22;

    let content = appearance_content(&sized, text, size, x, y);
    let stream = form_xobject(arena, &resources, width, height, &content);
    let mut appearances = BTreeMap::new();
    appearances.insert(arena.name("N"), Object::Reference(stream));
    widget.insert(arena.name("AP"), Object::Dictionary(arena.alloc_dict(appearances)));
    arena.set_dict(widget_dh, widget);

    if quadding != 0 && measured.is_none() {
        doc.record(Decision::violation(
            "12.7.4.3",
            format!(
                "/Q asks for quadding {quadding} and the font /{} did not load from /DR",
                sized.font
            ),
            "left the text at the left edge, because placing it needs the width the font gives",
        ));
    }
    Ok(true)
}

/// A form XObject holding `content`, sized to the widget (12.7.4.3, 8.10).
fn form_xobject(
    arena: &PdfArena,
    resources: &Dict,
    width: f64,
    height: f64,
    content: &str,
) -> Handle<Object> {
    let box_ = arena.alloc_array(vec![
        Object::Real(0.0),
        Object::Real(0.0),
        Object::Real(width),
        Object::Real(height),
    ]);
    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("XObject")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Form")));
    dict.insert(arena.name("BBox"), Object::Array(box_));
    dict.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(resources.clone())));
    let dict_h = arena.alloc_dict(dict);
    let data = SublimatedData::Raw(Bytes::from(content.to_string().into_bytes()));
    arena.alloc_object(Object::Stream(dict_h, Arc::new(data)))
}

/// Points a checkbox or radio widget at the appearance for `state` (12.7.5.2.3).
///
/// A button's appearances are already in the file, keyed by the state name — there is
/// nothing to build, only `/AS` to set. A state the widget has no appearance for is
/// reported rather than written, because writing it would leave a widget whose `/AS`
/// names nothing.
pub fn set_button_state(doc: &Document, widget_dh: Handle<Dict>, state: &str) -> bool {
    let arena = doc.arena();
    let mut widget = arena.get_dict(widget_dh).unwrap_or_default();
    let known = widget
        .get(&arena.name("AP"))
        .and_then(|ap| ap.resolve(arena).as_dict_handle())
        .and_then(|dh| arena.get_dict(dh))
        .and_then(|ap| ap.get(&arena.name("N")).and_then(|n| n.resolve(arena).as_dict_handle()))
        .and_then(|dh| arena.get_dict(dh))
        .is_some_and(|normal| normal.contains_key(&arena.name(state)));
    if !known {
        doc.record(Decision::violation(
            "12.7.5.2.3",
            format!("a button widget has no appearance for the state /{state}"),
            "left /AS as it was; naming a state with no appearance would draw nothing",
        ));
        return false;
    }
    widget.insert(arena.name("AS"), Object::Name(arena.name(state)));
    arena.set_dict(widget_dh, widget);
    true
}
