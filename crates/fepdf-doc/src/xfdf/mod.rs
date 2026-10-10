//! XFDF annotations (ISO 19444-1:2019, ROADMAP AA-4c): the XML form of FDF.
//!
//! **The same annotations as FDF, in another syntax.** The export chooses what
//! `fdf::export` chooses and writes each by Table 33; the import builds each by Table 34
//! into an arena of its own and puts them on the document through the import FDF uses,
//! so `/NM` matches the same way ([ADR-0117]) and an annotation that arrives without an
//! appearance, which in XFDF is all but a stamp, is drawn from its entries ([ADR-0119]).
//!
//! **What XFDF has no place for is said, not dropped quietly.** A free text's callout
//! line (`/CL`) and inner rectangle (`/RD`), a stamp's picture (6.5.2 calls the appearance
//! "a base 64 encoded string" and does not say of what), and a PDF 2.0 `/Path`: the
//! export records a `Decision` for each.
//!
//! Links and projections are left out, as the FDF export leaves them: a link is not a
//! comment, and a projection lives in a 3D run-time environment (12.5.6.24).
//!
//! [ADR-0117]: ../../../../docs/adr/0117-an-fdf-import-replaces-an-annotation-of-the-same-name.md
//! [ADR-0119]: ../../../../docs/adr/0119-an-annotation-without-an-appearance-is-given-one-from-its-own-entries.md

mod read;
mod write;

pub use read::apply_import;
pub use write::export;

/// XFDF's namespace (5.5.2).
const NAMESPACE: &str = "http://ns.adobe.com/xfdf/";

/// Each annotation element and the `/Subtype` it is (Tables 33 and 34).
const KINDS: &[(&str, &str)] = &[
    ("text", "Text"),
    ("freetext", "FreeText"),
    ("line", "Line"),
    ("square", "Square"),
    ("circle", "Circle"),
    ("polygon", "Polygon"),
    ("polyline", "PolyLine"),
    ("highlight", "Highlight"),
    ("underline", "Underline"),
    ("strikeout", "StrikeOut"),
    ("squiggly", "Squiggly"),
    ("caret", "Caret"),
    ("stamp", "Stamp"),
    ("ink", "Ink"),
    ("fileattachment", "FileAttachment"),
    ("sound", "Sound"),
    ("redact", "Redact"),
];

/// The flags attribute's words and Table 167's bits (Table 5).
const FLAGS: &[(&str, i64)] = &[
    ("invisible", 1),
    ("hidden", 2),
    ("print", 4),
    ("nozoom", 8),
    ("norotate", 16),
    ("noview", 32),
    ("readonly", 64),
    ("locked", 128),
    ("togglenoview", 256),
];

/// The style attribute's words and Table 168's `/S` (Table 21).
const BORDER_STYLES: &[(&str, &str)] =
    &[("solid", "S"), ("dash", "D"), ("bevelled", "B"), ("inset", "I"), ("underline", "U")];

/// Intents XFDF spells differently from the name they stand for (Table 12).
const INTENTS: &[(&str, &str)] =
    &[("polygon-dimension", "PolygonDimension"), ("polyline-dimension", "PolyLineDimension")];

/// Numbers that are one attribute each way: attribute, key (Tables 9, 13, 14, 6).
const NUMBERS: &[(&str, &str)] = &[
    ("leaderLength", "LL"),
    ("leaderExtend", "LLE"),
    ("leader-offset", "LLO"),
    ("rotation", "Rotate"),
    ("opacity", "CA"),
];

/// Text strings that are one attribute each way (Tables 5, 6, 8, 19).
const TEXTS: &[(&str, &str)] = &[
    ("name", "NM"),
    ("title", "T"),
    ("subject", "Subj"),
    ("date", "M"),
    ("creationdate", "CreationDate"),
    ("state", "State"),
    ("statemodel", "StateModel"),
    ("overlay-text", "OverlayText"),
];

/// Lists of numbers that are one attribute each way (Tables 5, 7, 10).
const LISTS: &[(&str, &str)] = &[("rect", "Rect"), ("coords", "QuadPoints"), ("fringe", "RD")];

/// The element a subtype is written as.
fn element_of(subtype: &str) -> Option<&'static str> {
    KINDS.iter().find(|(_, s)| *s == subtype).map(|(e, _)| *e)
}

/// The subtype an element is read as.
fn subtype_of(element: &str) -> Option<&'static str> {
    KINDS.iter().find(|(e, _)| *e == element).map(|(_, s)| *s)
}

/// Table 13's `justification` for a free text, and Table 19's for a redaction.
fn justification(subtype: &str, quadding: i64) -> &'static str {
    match (subtype == "Redact", quadding) {
        (true, 1) => "Centered",
        (true, 2) => "Right-justified",
        (true, _) => "Left-justified",
        (false, 1) => "centered",
        (false, 2) => "right",
        (false, _) => "left",
    }
}

/// `/Q` for either table's word.
fn quadding(word: &str) -> i64 {
    match word.to_ascii_lowercase().as_str() {
        "centered" => 1,
        "right" | "right-justified" => 2,
        _ => 0,
    }
}
