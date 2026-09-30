//! A text layer an OCR engine read, laid over the page it read (ROADMAP W-O1, ADR-0086).
//!
//! **The engine does not read a scan; it binds what does.** An OCR engine is handed the
//! page as a picture and hands back words with the boxes it found them in; this writes
//! them where they were found, in text rendering mode 3 (9.3.6), which neither fills nor
//! strokes — so the page looks exactly as it did and its words can be found, selected and
//! read aloud.
//!
//! **Each word is set to its box.** The size is the box's height, and the horizontal
//! scaling (`Tz`, 9.3.4) stretches the face's own widths to the box's width, so a
//! selection lands on the word a reader sees rather than on where this face would have
//! put it.
//!
//! **Along the line as it is shown.** A page under `/Rotate` shows its user space turned
//! (7.7.3.3), so a line across what a reader sees runs up or down it. The words were set
//! across user space whatever the turn: on a page turned 90°, a line 14 points high on the
//! screen was set at 97 points and squeezed into its width, its glyphs running across the
//! line a reader was selecting along.

use crate::operation::TextLayerItem;
use fepdf_model::{Document, PdfError, PdfResult};
use std::fmt::Write as _;

/// Writes `items` over page `page`, invisibly.
///
/// # Errors
/// Fails when the page is not there, when there is nothing to write or a box has no area,
/// and when no installed face draws every character.
pub fn apply_add_text_layer(doc: &Document, page: usize, items: &[TextLayerItem]) -> PdfResult<()> {
    let refuse = |why: String| Err(PdfError::Other(why.into()));
    if items.is_empty() {
        return refuse("a text layer with nothing in it".to_owned());
    }
    for item in items {
        let [x0, y0, x1, y1] = item.rect;
        if item.text.trim().is_empty() || !(x1 > x0 && y1 > y0) {
            return refuse(format!("{:?} in {:?} is not a word in a box", item.text, item.rect));
        }
        if item.text.contains(['\n', '\r']) {
            return refuse(format!("{:?} is more than one line; a line is an item", item.text));
        }
    }
    let Some(page_h) = doc.get_page_handle(page) else {
        return refuse(format!("there is no page {}", page + 1));
    };
    let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
    let (base_font, program) = crate::apply::font::face_for(&texts.concat())
        .map_err(|why| PdfError::Other(format!("the layer cannot be set: {why:?}").into()))?;
    let embedded = crate::apply::font::embed_for(doc, &program, &base_font, &texts)?;

    let arena = doc.arena();
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let name = crate::apply::font::name_font_in_page(doc, page_h, &mut page_dict, embedded.font);
    // In a `q … Q` of its own, and with the rendering mode set inside its `BT`, so the
    // content after it — a later layer, a decoration — is drawn as it would have been.
    let turn = crate::apply::page::page_turn(doc, page);
    let mut drawing = String::from("q\n");
    for item in items {
        drawing.push_str(&set_in_box((&program, &embedded, &name), item, turn)?);
    }
    drawing.push_str("Q\n");
    crate::apply::font::append_content(doc, page_dh, &mut page_dict, drawing.into_bytes());
    arena.set_dict(page_dh, page_dict);
    Ok(())
}

/// One item: its codes, at its box's height, stretched to its box's width, invisible —
/// height and width as a reader sees them on a page turned by `turn` degrees.
fn set_in_box(
    (program, embedded, font): (&[u8], &crate::apply::font::Embedded, &str),
    item: &TextLayerItem,
    turn: i64,
) -> PdfResult<String> {
    let glyphs = fepdf_font::subset::glyphs_for(program, &item.text)
        .map_err(|c| PdfError::Other(format!("this face draws no {c:?}").into()))?;
    let metrics = fepdf_font::metrics::read_metrics(program);
    let per_em = metrics.as_ref().map_or(1000.0, |m| f64::from(m.units_per_em.max(1)));
    let descent = metrics.as_ref().map_or(0.0, |m| f64::from(m.descent).abs()) / per_em;
    let [x0, y0, x1, y1] = item.rect;
    let turned = turn == 90 || turn == 270;
    let (length, height) = if turned { (y1 - y0, x1 - x0) } else { (x1 - x0, y1 - y0) };
    // The box holds the descenders as it held them on the scan: the size is what leaves
    // room for them below the baseline, and the baseline sits that far above the foot.
    let size = height / (1.0 + descent);
    let lift = size * descent;
    // The direction the line runs and the direction "up" is, in user space, and where the
    // line starts: user space turned back by the page's turn (7.7.3.3).
    let (along, up, origin) = match turn {
        90 => ((0.0, 1.0), (-1.0, 0.0), (x1 - lift, y0)),
        180 => ((-1.0, 0.0), (0.0, -1.0), (x1, y1 - lift)),
        270 => ((0.0, -1.0), (1.0, 0.0), (x0 + lift, y1)),
        _ => ((1.0, 0.0), (0.0, 1.0), (x0, y0 + lift)),
    };
    let natural: f64 = glyphs
        .iter()
        .map(|gid| f64::from(fepdf_font::metrics::advance_width(program, *gid).unwrap_or(0)))
        .sum::<f64>()
        / per_em
        * size;
    // A face that states no widths is left unstretched rather than divided by nothing.
    let scaling = if natural > 0.0 { length / natural * 100.0 } else { 100.0 };
    let mut codes = String::with_capacity(glyphs.len() * 4);
    for gid in &glyphs {
        let code = embedded.code_of.get(gid).copied().unwrap_or(*gid);
        let _ = write!(codes, "{code:04X}");
    }
    let matrix =
        format!("{} {} {} {} {:.3} {:.3}", along.0, along.1, up.0, up.1, origin.0, origin.1);
    Ok(format!("BT\n3 Tr\n/{font} {size:.3} Tf\n{scaling:.3} Tz\n{matrix} Tm\n<{codes}> Tj\nET\n"))
}
