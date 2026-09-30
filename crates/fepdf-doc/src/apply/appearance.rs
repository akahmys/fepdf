//! Building a field's appearance from its value (12.7.4.3, "Variable text").
//!
//! **`/NeedAppearances` is deprecated in PDF 2.0**, and setting a field value used to
//! consist of writing the value and then setting that flag — which is a producer telling
//! the reader "you work it out", using an entry this edition lists among the features it
//! deprecates (0.3). This engine's own rule is *do not write what 2.0 deprecates*, and it
//! was applied to encryption ([ADR-0015](../../../../docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md))
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

/// `text` in the font `/DA` names, as hexadecimal codes, and its width in points — when
/// that font loads, its codes are known, and it has every character.
///
/// **Codes, not the text's bytes.** The value was written into a literal string as its
/// UTF-8, which a font shows as codes of its own: 東京 in a Japanese form's `KozMinPr6N`
/// came out as three characters nobody typed. A simple font's code is a byte; a Type 0
/// font's is known only through an `Identity` CMap, where it is the CID — through another,
/// such as `UniJIS-UTF16-H`, what the font maps a character to is a CID and not the code
/// that reaches it, so such a font is not written through here.
fn shown_in(
    doc: &Document,
    resources: &Dict,
    appearance: &DefaultAppearance,
    text: &str,
) -> Option<(String, f64)> {
    use std::fmt::Write as _;
    let arena = doc.arena();
    let fonts = resources.get(&arena.name("Font"))?.resolve(arena).as_dict_handle()?;
    let entry = arena.get_dict(fonts)?.get(&arena.name(&appearance.font))?.clone();
    let font = doc.get_font(entry.as_reference()?).ok()?;
    let identity = font.encoding.as_ref().is_some_and(|e| e.name().starts_with("Identity"));
    if font.is_cid_keyed && !identity {
        return None;
    }
    let (mut codes, mut width) = (String::new(), 0.0_f32);
    for character in text.chars() {
        let code = *font.unified_map.get(&character.to_string())?;
        let bytes = if font.is_cid_keyed {
            u16::try_from(code).ok()?.to_be_bytes().to_vec()
        } else {
            vec![u8::try_from(code).ok()?]
        };
        width += font.glyph_width(&bytes);
        for byte in &bytes {
            let _ = write!(codes, "{byte:02X}");
        }
    }
    Some((codes, f64::from(width) / 1000.0 * appearance.size))
}

/// `text` in a face installed here whose terms permit embedding it, embedded: the font,
/// its codes, and its width in points at `size` — or nothing when no face draws it all.
///
/// # Errors
/// Fails when the face will not embed.
fn shown_in_a_face(
    doc: &Document,
    text: &str,
    size: f64,
) -> PdfResult<Option<(Handle<Object>, String, f64)>> {
    use std::fmt::Write as _;
    let Ok((base_font, program)) = crate::apply::font::face_for(text) else { return Ok(None) };
    let embedded = crate::apply::font::embed_for(doc, &program, &base_font, &[text])?;
    let Ok(glyphs) = fepdf_font::subset::glyphs_for(&program, text) else { return Ok(None) };
    let metrics = fepdf_font::metrics::read_metrics(&program);
    let per_em = metrics.map_or(1000.0, |m| f64::from(m.units_per_em.max(1)));
    let (mut codes, mut width) = (String::new(), 0.0);
    for glyph in glyphs {
        let _ = write!(codes, "{:04X}", embedded.code_of.get(&glyph).copied().unwrap_or(glyph));
        width += f64::from(fepdf_font::metrics::advance_width(&program, glyph).unwrap_or(0));
    }
    Ok(Some((embedded.font, codes, width / per_em * size)))
}

/// `resources` with `font` named `name` among its fonts, the rest as they were.
fn with_font(arena: &PdfArena, resources: &Dict, name: &str, font: Handle<Object>) -> Dict {
    let key = arena.name("Font");
    let mut fonts = resources
        .get(&key)
        .and_then(|f| f.resolve(arena).as_dict_handle())
        .and_then(|f| arena.get_dict(f))
        .unwrap_or_default();
    fonts.insert(arena.name(name), Object::Reference(font));
    let mut out = resources.clone();
    out.insert(key, Object::Dictionary(arena.alloc_dict(fonts)));
    out
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

/// The appearance stream's content, between `/Tx BMC` and `EMC` as the clause shows it:
/// `codes`, in hexadecimal, shown in the font named `font`.
fn appearance_content(
    appearance: &DefaultAppearance,
    (font, codes): (&str, &str),
    size: f64,
    (x, y): (f64, f64),
) -> String {
    let da = &appearance.verbatim;
    format!(
        "/Tx BMC\nq\nBT\n{da}\n/{font} {size} Tf\n1 0 0 1 {x:.2} {y:.2} Tm\n<{codes}> Tj\nET\nQ\nEMC\n"
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
    let Some((font, codes, measured, resources)) = shown(doc, &sized, resources, text)? else {
        return Ok(false);
    };
    let x = left_edge(quadding, width, Some(measured));
    // The baseline is not specified. Centring the em box puts a single line where a
    // reader expects it, and is what every implementation this was compared against does.
    let y = ((height - size) / 2.0).max(INSET) + size * 0.22;

    let content = appearance_content(&sized, (&font, &codes), size, (x, y));
    let stream = form_xobject(arena, &resources, width, height, &content);
    let mut appearances = BTreeMap::new();
    appearances.insert(arena.name("N"), Object::Reference(stream));
    widget.insert(arena.name("AP"), Object::Dictionary(arena.alloc_dict(appearances)));
    arena.set_dict(widget_dh, widget);
    Ok(true)
}

/// How `text` is shown: in the `/DA` font where it can be, else in a face embedded for
/// it — the font's resource name, the codes, the width, and the resources that name it.
/// Nothing, and a decision saying so, where no face here draws it.
///
/// # Errors
/// Fails when a face found for it will not embed.
fn shown(
    doc: &Document,
    sized: &DefaultAppearance,
    resources: Dict,
    text: &str,
) -> PdfResult<Option<(String, String, f64, Dict)>> {
    if let Some((codes, width)) = shown_in(doc, &resources, sized, text) {
        return Ok(Some((sized.font.clone(), codes, width, resources)));
    }
    let arena = doc.arena();
    let Some((font, codes, width)) = shown_in_a_face(doc, text, sized.size)? else {
        doc.record(Decision::violation(
            "12.7.4.3",
            format!("neither /{} nor a face installed here draws {text:?}", sized.font),
            "left the widget's appearance as it was",
        ));
        return Ok(None);
    };
    doc.record(Decision::ambiguity(
        "12.7.4.3",
        format!("the /DA font /{} cannot show {text:?} by codes this engine knows", sized.font),
        "drew the value in a face embedded for it; a reader regenerating it uses /DA",
    ));
    let name = "FepdfF0";
    Ok(Some((name.to_owned(), codes, width, with_font(arena, &resources, name, font))))
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
pub fn set_button_state(
    doc: &Document,
    widget_dh: Handle<Dict>,
    state: Handle<fepdf_model::object::PdfName>,
) -> bool {
    let arena = doc.arena();
    let mut widget = arena.get_dict(widget_dh).unwrap_or_default();
    if !button_states(doc, widget_dh).contains(&state) {
        let named = arena.get_name_str(state).unwrap_or_default();
        doc.record(Decision::violation(
            "12.7.5.2.3",
            format!("a button widget has no appearance for the state /{named}"),
            "left /AS as it was; naming a state with no appearance would draw nothing",
        ));
        return false;
    }
    widget.insert(arena.name("AS"), Object::Name(state));
    arena.set_dict(widget_dh, widget);
    true
}

/// The states a button widget has a normal appearance for — `/AP /N`'s keys (12.7.5.2.3).
///
/// **Held as names, not as text.** The on state of a check box is whatever name its
/// producer chose — `sample_02c.pdf` names each box after itself, in Shift-JIS bytes — and
/// a name is compared by its bytes, which is what the handle is.
pub fn button_states(
    doc: &Document,
    widget_dh: Handle<Dict>,
) -> Vec<Handle<fepdf_model::object::PdfName>> {
    let arena = doc.arena();
    arena
        .dict_entry(widget_dh, arena.name("AP"))
        .and_then(|ap| ap.resolve(arena).as_dict_handle())
        .and_then(|ap| arena.dict_entry(ap, arena.name("N")))
        .and_then(|n| n.resolve(arena).as_dict_handle())
        .and_then(|normal| arena.get_dict(normal))
        .map(|normal| normal.into_keys().collect())
        .unwrap_or_default()
}
