//! The appearances of the annotations whose appearance *is* their content — a printer's
//! mark, a watermark, a screen — drawn from what their maker gave (ROADMAP AA-4f).
//!
//! **Only for an annotation this engine makes.** One opened without an appearance is not
//! given one of these ([ADR-0120]): what a printer's mark or a watermark held is not known
//! from its entries, and drawing a colour bar where somebody's file had one of its own
//! would be drawing a thing the file did not say.
//!
//! [ADR-0120]: ../../../../../docs/adr/0120-a-document-is-opened-with-every-annotation-given-an-appearance.md

use super::{Area, Dict, Drawing, Entries, finish};
use crate::operation::PrinterMarkKind;
use fepdf_model::{Document, Object, PdfResult};
use std::fmt::Write as _;

/// The mark `mark`, filling the rectangle (14.11.3), in all four process colours where it
/// is a registration mark, so that it prints on every plate.
pub fn printer_mark(doc: &Document, dict: &Dict, mark: PrinterMarkKind, area: Area) -> Object {
    let (w, h) = (area.width(), area.height());
    let mut content = String::new();
    match mark {
        PrinterMarkKind::RegistrationTarget => {
            let (cx, cy, r) = (w / 2.0, h / 2.0, w.min(h) * 0.35);
            let _ = write!(
                content,
                "1 1 1 1 K 0.5 w\n{}S\n0 {cy:.2} m {w:.2} {cy:.2} l S\n{cx:.2} 0 m {cx:.2} {h:.2} l S\n",
                super::lines::ellipse(cx, cy, r, r)
            );
        }
        PrinterMarkKind::ColorBar => {
            let patches = [
                "1 0 0 0",
                "0 1 0 0",
                "0 0 1 0",
                "0 0 0 1",
                "0 0 0 0.75",
                "0 0 0 0.5",
                "0 0 0 0.25",
            ];
            let step = w / 7.0;
            for (nth, cmyk) in patches.iter().enumerate() {
                let x = step * f64::from(u8::try_from(nth).unwrap_or(0));
                let _ = writeln!(content, "{cmyk} k {x:.2} 0 {step:.2} {h:.2} re f");
            }
        }
    }
    let arena = doc.arena();
    finish(arena, &Entries::new(arena, dict), Drawing::of(content), area)
}

/// `text` across the middle of the rectangle in grey, as large as `size` and no wider
/// than the rectangle, at `/CA`'s opacity.
///
/// # Errors
/// When the face that draws the words will not embed.
pub fn watermark(
    doc: &Document,
    dict: &Dict,
    text: &str,
    size: f64,
    area: Area,
) -> PdfResult<Object> {
    let mut drawing = Drawing::of(String::new());
    super::words::watermark(doc, &mut drawing, text, size, area)?;
    let arena = doc.arena();
    Ok(finish(arena, &Entries::new(arena, dict), drawing, area))
}

/// A screen's stand-in: a frame and a play mark, as one opened without an appearance
/// would have were 12.5.6.18 not to say it has none.
pub fn screen(doc: &Document, dict: &Dict, area: Area) -> Object {
    let arena = doc.arena();
    finish(arena, &Entries::new(arena, dict), super::icons::media(area, false), area)
}
