//! Annotations (12.5): every entry Table 166 defines for all of them, and the entries
//! the subtypes the corpus carries add on top.
//!
//! Surveyed before it was written (`examples/annotation_survey.rs`), because the last
//! thing built here was built on a corpus of one subtype. `samples/` carries 29,973
//! annotations and every one is `/Link`; the 242 external files carry 82 across 16
//! subtypes, including a `/SomePrivateCustomAnnotationType` that no table defines. What
//! the survey found is what this module models:
//!
//! | | annotations | what they carry |
//! | :--- | ---: | :--- |
//! | `/Link` | 30,002 | `/Dest` 27,306, `/Border` 25,423, `/BS` 4,574, `/A` 2,696 |
//! | `/Popup` | 18 | `/Parent` and `/Open`, on all of them |
//! | `/Circle` | 12 | markup entries — `/T`, `/Popup`, `/Subj`, `/CreationDate` — and `/RD`, `/IC`, `/BE` |
//! | `/Widget` | 4 | `/FT`, `/MK`, `/DA`, `/DR`, `/T`: the form field it *is* |
//! | 12 others | 19 | between one and four annotations each |
//!
//! So [`PdfAnnotation`] holds Table 166 entire, [`MarkupEntries`] holds Table 172 —
//! which is where `/T`, `/Popup`, `/Subj` and `/CreationDate` come from, and covers
//! eight of the sixteen subtypes at once — and the subtype-specific readers cover
//! `/Link`, `/Popup`, `/Circle`, `/Movie`, `/Stamp` and `/Widget`.
//!
//! **The line is drawn at "more than once".** Those six are every subtype either corpus
//! writes twice or more; the other ten occur exactly once each, and a sample of one is
//! not a reason to build a type. What they carry is *reported* rather than read —
//! `inspect interactive` names, per subtype, which entries the file writes and which of
//! them this engine read — so the gap is a measurement instead of an inference. Across
//! both corpora that leaves `/Vertices` on a `/Polygon`, `/Sound` on a `/Sound`,
//! `/FixedPrint` on a `/Watermark` and eleven more, each on one annotation.

use crate::arena::PdfArena;
use crate::error::{PdfError, PdfResult};
use crate::graphics::{BlendMode, Rect};
use crate::handle::Handle;
use crate::object::{FromPdfObject, Object, PdfName, PdfSchema};
use fepdf_macros::FromPdfObject;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Entries common to every annotation dictionary (12.5.2, Table 166).
///
/// Nineteen entries, and this held seven of them until Phase J — `/Type`, `/Subtype`,
/// `/Rect`, `/Contents`, `/P`, `/NM` and `/F`. A `/Redact` and a `/Watermark` were the
/// same object to the engine, distinguishable only by the name the census counted them
/// under.
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.2")]
pub struct PdfAnnotation {
    #[pdf_key("Type")]
    /// `/Type`, normally `Annot`.
    ///
    /// The attribute is not decoration: without it the macro takes the *field* name as
    /// the key and this read `/kind`, which no file writes. Every annotation in both
    /// corpora reported `/Type` as an entry with no reader, on a field named after it.
    pub kind: Option<PdfName>,
    #[pdf_key("Subtype")]
    /// `/Subtype`: the annotation's kind.
    pub subtype: PdfName,
    #[pdf_key("Rect")]
    /// `/Rect`: where the annotation sits on the page.
    pub rect: Rect,
    #[pdf_key("Contents")]
    /// `/Contents`: the annotation's text, or a description of its contents for a
    /// subtype that does not display text.
    pub contents: Option<String>,
    #[pdf_key("P")]
    /// `/P`: the page this annotation belongs to.
    pub page: Option<Handle<Object>>,
    #[pdf_key("NM")]
    /// `/NM`: the annotation's name, unique among those on its page.
    pub name: Option<String>,
    #[pdf_key("M")]
    /// `/M`: when the annotation was last modified. A date string (7.9.4) by
    /// preference, but the clause explicitly permits any text, so it is not parsed —
    /// a reader that returned `None` for a non-conforming date would lose what the file
    /// actually said.
    pub modified: Option<String>,
    #[pdf_key("F")]
    /// `/F`: the flags of Table 167, which decide whether the annotation is shown,
    /// printed, or editable.
    pub flags: Option<AnnotationFlags>,
    #[pdf_key("AP")]
    /// `/AP`: the appearance streams that draw it (12.5.5).
    pub appearance: Option<Appearance>,
    #[pdf_key("AS")]
    /// `/AS`: which appearance of a `/N` sub-dictionary is current. Required when the
    /// appearance is a set of states rather than one stream.
    pub appearance_state: Option<PdfName>,
    #[pdf_key("Border")]
    /// `/Border`: the border, in the pre-1.2 form Table 168 defines. `/BS` supersedes
    /// it, and 25,423 `/Link` annotations in the corpus still carry this one.
    pub border: Option<Border>,
    #[pdf_key("C")]
    /// `/C`: the colour of the annotation's background or title bar.
    pub colour: Option<AnnotationColour>,
    #[pdf_key("StructParent")]
    /// `/StructParent`: this annotation's key in the structural parent tree (14.7.5.4).
    pub struct_parent: Option<i64>,
    #[pdf_key("OC")]
    /// `/OC`: the optional content that governs whether it is shown (8.11).
    ///
    /// Reachable, not modelled: optional content is a subsystem, not a scalar, and
    /// nothing else in this engine reads one yet. Naming it is what a caller needs to
    /// find it; claiming to understand it would be the shape ADR-0017 is about.
    pub optional_content: Option<Object>,
    #[pdf_key("AF")]
    /// `/AF`: associated files (14.13). Reachable, not modelled, for the same reason
    /// as `/OC` — and it occurs on no annotation in either corpus.
    pub associated_files: Option<Object>,
    #[pdf_key("ca")]
    /// `/ca`: constant opacity for non-stroking operations, added in 2.0.
    pub fill_alpha: Option<f64>,
    #[pdf_key("CA")]
    /// `/CA`: constant opacity. One `/Circle` in the corpus carries it.
    pub stroke_alpha: Option<f64>,
    #[pdf_key("BM")]
    /// `/BM`: the blend mode used to paint it, added in 2.0.
    pub blend_mode: Option<BlendMode>,
    #[pdf_key("Lang")]
    /// `/Lang`: the natural language of the annotation's text (14.9.2), added in 2.0.
    pub lang: Option<String>,
}

/// The flags of Table 167, as flags rather than as an integer.
///
/// **Every field is a `bool` and the whole thing is an `Option` on the annotation.** A
/// document that does not write `/F` has said nothing about visibility, which is not the
/// same as writing 0 — the same distinction `ViewerPreferences` makes for Table 147, and
/// for the same reason: a report that conflates "unstated" with "explicitly off" cannot
/// say what the file declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AnnotationFlags {
    /// Bit 1: ignore the annotation if its subtype is unknown.
    pub invisible: bool,
    /// Bit 2: do not display or print it.
    pub hidden: bool,
    /// Bit 3: print it. A PDF/A file must set this on every annotation, which is what
    /// `isartor-6-3-1` exists to break.
    pub print: bool,
    /// Bit 4: do not scale it with the page.
    pub no_zoom: bool,
    /// Bit 5: do not rotate it with the page.
    pub no_rotate: bool,
    /// Bit 6: display but do not view — printed only.
    pub no_view: bool,
    /// Bit 7: do not let the user interact with it.
    pub read_only: bool,
    /// Bit 8: do not let the user delete or move it; its contents stay editable.
    pub locked: bool,
    /// Bit 9: invert `no_view` when the annotation is selected.
    pub toggle_no_view: bool,
    /// Bit 10: do not let the user change its contents.
    pub locked_contents: bool,
}

impl AnnotationFlags {
    /// Reads Table 167's bit positions, which are numbered from 1.
    #[must_use]
    pub fn from_bits(bits: i64) -> Self {
        let bit = |n: u32| bits & (1 << (n - 1)) != 0;
        Self {
            invisible: bit(1),
            hidden: bit(2),
            print: bit(3),
            no_zoom: bit(4),
            no_rotate: bit(5),
            no_view: bit(6),
            read_only: bit(7),
            locked: bit(8),
            toggle_no_view: bit(9),
            locked_contents: bit(10),
        }
    }
}

impl FromPdfObject for AnnotationFlags {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        match obj.resolve(arena) {
            Object::Integer(n) => Ok(Self::from_bits(n)),
            other => Err(PdfError::Other(format!("/F is a bit field, not {other:?}").into())),
        }
    }
}

/// `/AP`, the streams that draw the annotation (12.5.5, Table 170).
///
/// Each of the three may be one stream or a *sub-dictionary of states* — a checkbox has
/// `/Off` and `/Yes` under `/N`, and `/AS` says which is current. Both forms are kept
/// rather than flattened, because "one appearance" and "a set of them, one selected"
/// are different facts about the file, and PDF/A conformance turns on the second.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appearance {
    /// `/N`: the normal appearance, the one that is printed.
    pub normal: Option<AppearanceEntry>,
    /// `/R`: the rollover appearance, shown when the pointer is over it.
    pub rollover: Option<AppearanceEntry>,
    /// `/D`: the down appearance, shown while it is being clicked.
    pub down: Option<AppearanceEntry>,
}

/// One of `/AP`'s three entries: a single stream, or a set of named states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppearanceEntry {
    /// One appearance stream.
    Stream(Handle<Object>),
    /// A state name for each appearance the annotation can take. `/AS` selects one.
    States(Vec<String>),
}

impl FromPdfObject for Appearance {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let dict = dict_of(arena, &obj)
            .ok_or_else(|| PdfError::Other("/AP is not a dictionary".into()))?;
        let read = |key: &str| -> Option<AppearanceEntry> {
            let value = dict.get(&arena.name(key))?;
            match value.resolve(arena) {
                Object::Stream(..) => match value {
                    Object::Reference(h) => Some(AppearanceEntry::Stream(*h)),
                    other => arena.find_object(other).map(AppearanceEntry::Stream),
                },
                Object::Dictionary(dh) => {
                    let states = arena.get_dict(dh)?;
                    let mut names: Vec<String> =
                        states.keys().filter_map(|k| arena.get_name_str(*k)).collect();
                    names.sort();
                    Some(AppearanceEntry::States(names))
                }
                _ => None,
            }
        };
        Ok(Self { normal: read("N"), rollover: read("R"), down: read("D") })
    }
}

/// `/Border`, in the form Table 168 defines: two corner radii, a width, and a dash.
///
/// The array may be three elements or four. A file writing `[0 0 0]` has said "no
/// border", which is not the same as omitting the key and letting the default `[0 0 1]`
/// apply — so this reads what is written and does not substitute.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Border {
    /// Horizontal corner radius.
    pub horizontal_radius: f64,
    /// Vertical corner radius.
    pub vertical_radius: f64,
    /// Border width in points. Zero means no border is drawn.
    pub width: f64,
    /// Whether a dash array was written as the fourth element.
    pub dashed: bool,
}

impl FromPdfObject for Border {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let items = array_of(arena, &obj)
            .ok_or_else(|| PdfError::Other("/Border is not an array".into()))?;
        if items.len() < 3 {
            return Err(PdfError::Other(
                format!("/Border needs at least three elements, found {}", items.len()).into(),
            ));
        }
        let number = |i: usize| items.get(i).and_then(|o| o.resolve(arena).as_f64()).unwrap_or(0.0);
        Ok(Self {
            horizontal_radius: number(0),
            vertical_radius: number(1),
            width: number(2),
            dashed: matches!(items.get(3).map(|o| o.resolve(arena)), Some(Object::Array(_))),
        })
    }
}

/// `/C`, whose *length* says which colour space it is in (12.5.2).
///
/// An empty array is not "no colour written" — it means the annotation is transparent,
/// which the standard says in as many words. Modelling this as `Vec<f64>` would leave
/// every caller to rediscover that rule, and the one that forgot would paint a
/// transparent annotation black.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AnnotationColour {
    /// No components: transparent.
    Transparent,
    /// One component: `DeviceGray`.
    Gray(f64),
    /// Three components: `DeviceRGB`.
    Rgb(f64, f64, f64),
    /// Four components: `DeviceCMYK`.
    Cmyk(f64, f64, f64, f64),
}

impl FromPdfObject for AnnotationColour {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let items =
            array_of(arena, &obj).ok_or_else(|| PdfError::Other("/C is not an array".into()))?;
        let n = |i: usize| items.get(i).and_then(|o| o.resolve(arena).as_f64()).unwrap_or(0.0);
        match items.len() {
            0 => Ok(Self::Transparent),
            1 => Ok(Self::Gray(n(0))),
            3 => Ok(Self::Rgb(n(0), n(1), n(2))),
            4 => Ok(Self::Cmyk(n(0), n(1), n(2), n(3))),
            other => Err(PdfError::Other(
                format!("/C has {other} components; 12.5.2 defines 0, 1, 3 and 4").into(),
            )),
        }
    }
}

/// Entries a *markup* annotation adds (12.5.6.2, Table 172).
///
/// Eight of the sixteen subtypes in the corpus are markup annotations, and this is where
/// most of what they carry lives: `/T` on 29 of them, `/Popup` on 27, `/Subj` on 27,
/// `/CreationDate` on 27. One reader for the lot, rather than eight that repeat.
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.6.2")]
pub struct MarkupEntries {
    #[pdf_key("T")]
    /// `/T`: the text label — by convention the name of who made the annotation.
    pub title: Option<String>,
    #[pdf_key("Popup")]
    /// `/Popup`: the pop-up annotation that displays this one's text.
    pub popup: Option<Handle<Object>>,
    #[pdf_key("RC")]
    /// `/RC`: rich text, as a stream or a string.
    pub rich_contents: Option<Object>,
    #[pdf_key("CreationDate")]
    /// `/CreationDate`: when the annotation was made. Not parsed, for the reason
    /// [`PdfAnnotation::modified`] gives.
    pub created: Option<String>,
    #[pdf_key("IRT")]
    /// `/IRT`: the annotation this one replies to.
    pub in_reply_to: Option<Handle<Object>>,
    #[pdf_key("Subj")]
    /// `/Subj`: what the annotation is about.
    pub subject: Option<String>,
    #[pdf_key("RT")]
    /// `/RT`: whether `/IRT` means a reply or a group.
    pub reply_type: Option<PdfName>,
    #[pdf_key("IT")]
    /// `/IT`: the intent, which narrows what the subtype means.
    pub intent: Option<PdfName>,
    #[pdf_key("ExData")]
    /// `/ExData`: external data for the annotation. Reachable, not modelled.
    pub external_data: Option<Object>,
}

/// The subtypes 12.5.6.2 defines as markup annotations.
///
/// Listed rather than inferred, because the standard's own definition is a list. A
/// subtype not named here gets no markup reader, which is how
/// `/SomePrivateCustomAnnotationType` is handled: it carries `/AP` and `/F` like any
/// annotation and nothing this engine can name beyond that.
pub const MARKUP_SUBTYPES: &[&str] = &[
    "Text",
    "FreeText",
    "Line",
    "Square",
    "Circle",
    "Polygon",
    "PolyLine",
    "Highlight",
    "Underline",
    "Squiggly",
    "StrikeOut",
    "Stamp",
    "Caret",
    "Ink",
    "FileAttachment",
    "Sound",
    "Movie",
    "Redact",
    "Projection",
];

/// Entries a `/Link` adds (12.5.6.5, Table 176).
///
/// First because the corpus is 30,002 of them out of 30,055 annotations. `/Dest` and
/// `/A` are largely wiring: named destinations already resolve through both of
/// 12.3.2.3's forms, and actions are already tallied by kind.
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.6.5")]
pub struct LinkEntries {
    #[pdf_key("A")]
    /// `/A`: the action to perform. 2,696 links in the corpus.
    pub action: Option<Object>,
    #[pdf_key("Dest")]
    /// `/Dest`: where the link goes, when it goes somewhere in this document. 27,306
    /// links in the corpus, and mutually exclusive with `/A`.
    pub destination: Option<Object>,
    #[pdf_key("H")]
    /// `/H`: the highlighting mode used while the link is clicked.
    pub highlight: Option<PdfName>,
    #[pdf_key("PA")]
    /// `/PA`: the URI action a Web Capture link came from (14.10.5.2).
    pub uri_action: Option<Object>,
    #[pdf_key("QuadPoints")]
    /// `/QuadPoints`: the quadrilaterals the link covers, when it is not the whole
    /// `/Rect` — a link broken across two lines of text.
    pub quad_points: Option<Handle<Vec<Object>>>,
    #[pdf_key("BS")]
    /// `/BS`: the border style that supersedes `/Border`. 4,574 links carry it.
    pub border_style: Option<BorderStyle>,
}

/// `/BS`, the border style dictionary that supersedes `/Border` (12.5.4, Table 169).
///
/// Shared rather than repeated: `/Link` carries one on 4,574 annotations, and `/Circle`,
/// `/Movie` and `/Screen` each carry one too. Modelling it once is what makes
/// [`LinkEntries::border_style`] a type rather than an `Object`.
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.4")]
pub struct BorderStyle {
    #[pdf_key("W")]
    /// `/W`: width in points. Zero means no border, which is a statement — hence
    /// `Option` rather than the default of 1 substituted silently.
    pub width: Option<f64>,
    #[pdf_key("S")]
    /// `/S`: the style — `S` solid, `D` dashed, `B` bevelled, `I` inset, `U` underline.
    pub style: Option<PdfName>,
    #[pdf_key("D")]
    /// `/D`: the dash pattern, when `/S` is `D`.
    pub dash: Option<Handle<Vec<Object>>>,
}

/// Entries a `/Square` or a `/Circle` adds (12.5.6.8, Table 178).
///
/// Third in the corpus's order, and the last subtype that occurs more than four times:
/// twelve `/Circle` annotations, all in one file, carrying `/BS`, `/IC` and `/RD`.
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.6.8")]
pub struct SquareCircleEntries {
    #[pdf_key("BS")]
    /// `/BS`: the border style.
    pub border_style: Option<BorderStyle>,
    #[pdf_key("IC")]
    /// `/IC`: the interior colour, in the same length-decides-the-space form as `/C`.
    pub interior_colour: Option<AnnotationColour>,
    #[pdf_key("BE")]
    /// `/BE`: the border effect — a cloud, and how tightly drawn. Reachable, not
    /// modelled: no annotation in either corpus carries one.
    pub border_effect: Option<Object>,
    #[pdf_key("RD")]
    /// `/RD`: how far the drawn shape is inset from `/Rect`, as four numbers.
    pub inset: Option<Handle<Vec<Object>>>,
}

/// Entries a `/Movie` adds (12.5.6.17, Table 187).
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.6.17")]
pub struct MovieEntries {
    #[pdf_key("Movie")]
    /// `/Movie`: the movie itself. Reachable, not modelled — clause 13.4 is deprecated
    /// in 2.0 in favour of `/RichMedia`, and building a reader for it now would be
    /// building for a subsystem the standard is retiring.
    pub movie: Option<Object>,
    #[pdf_key("A")]
    /// `/A`: whether and how to play it — a boolean, or an activation dictionary.
    pub activation: Option<Object>,
}

/// Entries a `/Stamp` adds (12.5.6.12, Table 181).
///
/// One entry, and the four `/Stamp` annotations in the corpus write none of it. What
/// they *do* write — `/DA`, `/IC`, `/QuadPoints`, `/RO` — no table defines for a stamp;
/// those are `/Redact` entries, on an annotation that is not one. The report says
/// "not read" for them, which is the truthful answer: this engine has no reader, and
/// there is no clause to write one against.
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.6.12")]
pub struct StampEntries {
    #[pdf_key("Name")]
    /// `/Name`: which standard stamp it is — `Approved`, `Draft`, and so on.
    pub name: Option<PdfName>,
}

/// Entries a `/Popup` adds (12.5.6.14, Table 184).
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.6.14")]
pub struct PopupEntries {
    #[pdf_key("Parent")]
    /// `/Parent`: the markup annotation this pops up for. All 18 in the corpus have one.
    pub parent: Option<Handle<Object>>,
    #[pdf_key("Open")]
    /// `/Open`: whether it is displayed open. All 18 carry it.
    pub open: Option<bool>,
}

/// Entries a `/Widget` adds (12.5.6.19, Table 189) — the visible half of a form field.
///
/// A widget and its field are frequently **one dictionary**: all four in the corpus carry
/// `/FT`, `/T` and `/DA` on the annotation itself. That is why this reader exists beside
/// the form walk rather than instead of it.
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "12.5.6.19")]
pub struct WidgetEntries {
    #[pdf_key("H")]
    /// `/H`: highlighting mode.
    pub highlight: Option<PdfName>,
    #[pdf_key("MK")]
    /// `/MK`: the appearance characteristics — border and background colours, caption.
    /// Reachable, not modelled. All four widgets in the corpus carry one.
    pub appearance_characteristics: Option<Object>,
    #[pdf_key("A")]
    /// `/A`: the action performed when the widget is activated.
    pub action: Option<Object>,
    #[pdf_key("AA")]
    /// `/AA`: actions triggered by other events — focus, blur, keystroke, format.
    pub additional_actions: Option<Object>,
    #[pdf_key("BS")]
    /// `/BS`: border style.
    pub border_style: Option<BorderStyle>,
    #[pdf_key("Parent")]
    /// `/Parent`: the field this widget belongs to, when they are separate objects.
    pub parent: Option<Handle<Object>>,
}

/// The field entries a `/Widget` carries when it *is* its own form field, and which
/// the form walk reads (12.7.4.2).
///
/// A list rather than a derived one, because the reader is a walk and not a struct:
/// `interactive::read_form` descends `/Kids` carrying the inheritable entries down. It
/// is checked by `the_widget_entries_this_engine_reads_are_the_ones_the_form_walk_reads`
/// rather than trusted.
///
/// `/DA` is here as *presence*: the walk reports whether the field or the form has one,
/// which is what 12.7.4.3 turns on, and does not parse the appearance string. `/DR` is
/// deliberately absent — the form's is read, a field's own is not, which is exactly what
/// `isartor-6-9-t01-fail-a.pdf` writes.
///
/// The caveat this cannot express: a widget the form's `/Fields` does not reach is never
/// walked, so these entries are read for a widget *in a form*, not for any widget.
pub const FIELD_ENTRIES_READ: &[&str] = &["FT", "T", "Ff", "V", "DA", "Kids"];

/// Which entries this engine reads for an annotation of `subtype`.
///
/// Derived from the structs above through [`PdfSchema::pdf_keys`], so it cannot drift
/// from what the code does — the mistake this whole module exists to correct was a
/// *list* of seven keys that stopped matching the standard's nineteen.
#[must_use]
pub fn entries_read_for(subtype: &str) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = PdfAnnotation::pdf_keys().to_vec();
    if MARKUP_SUBTYPES.contains(&subtype) {
        keys.extend(MarkupEntries::pdf_keys());
    }
    // In the order the corpus presents them, and stopping where it stops saying
    // anything: every subtype it writes more than once has a reader, and the ten it
    // writes exactly once do not. A sample of one is not a reason to build a type.
    match subtype {
        "Link" => keys.extend(LinkEntries::pdf_keys()),
        "Popup" => keys.extend(PopupEntries::pdf_keys()),
        "Square" | "Circle" => keys.extend(SquareCircleEntries::pdf_keys()),
        "Movie" => keys.extend(MovieEntries::pdf_keys()),
        "Stamp" => keys.extend(StampEntries::pdf_keys()),
        "Widget" => {
            keys.extend(WidgetEntries::pdf_keys());
            keys.extend(FIELD_ENTRIES_READ);
        }
        _ => {}
    }
    keys.sort_unstable();
    keys.dedup();
    keys
}

fn dict_of(arena: &PdfArena, object: &Object) -> Option<BTreeMap<Handle<PdfName>, Object>> {
    match object.resolve(arena) {
        Object::Dictionary(h) | Object::Stream(h, _) => arena.get_dict(h),
        _ => None,
    }
}

fn array_of(arena: &PdfArena, object: &Object) -> Option<Vec<Object>> {
    match object.resolve(arena) {
        Object::Array(h) => arena.get_array(h),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Table 166 has nineteen entries and every one of them is read.
    ///
    /// Written as the count and the list, because the failure this replaces was a list
    /// that had stopped being checked against anything.
    #[test]
    fn every_entry_of_table_166_is_read() {
        let keys = PdfAnnotation::pdf_keys();
        for expected in [
            "Type",
            "Subtype",
            "Rect",
            "Contents",
            "P",
            "NM",
            "M",
            "F",
            "AP",
            "AS",
            "Border",
            "C",
            "StructParent",
            "OC",
            "AF",
            "ca",
            "CA",
            "BM",
            "Lang",
        ] {
            assert!(keys.contains(&expected), "Table 166's /{expected} is not read: {keys:?}");
        }
        assert_eq!(keys.len(), 19, "Table 166 defines nineteen entries: {keys:?}");
    }

    /// `/ca` and `/CA` are different entries, and a case-insensitive lookup would
    /// silently make them one.
    #[test]
    fn the_two_opacity_entries_stay_apart() {
        let keys = PdfAnnotation::pdf_keys();
        assert!(keys.contains(&"ca") && keys.contains(&"CA"));
    }

    /// A markup subtype reads Table 172 as well; a subtype no table defines does not.
    #[test]
    fn a_markup_subtype_reads_more_than_a_private_one() {
        let circle = entries_read_for("Circle");
        assert!(circle.contains(&"Subj"), "a /Circle is a markup annotation: {circle:?}");
        let private = entries_read_for("SomePrivateCustomAnnotationType");
        assert!(!private.contains(&"Subj"));
        assert!(private.contains(&"AP"), "it is still an annotation: {private:?}");
        assert_eq!(private.len(), 19, "common entries and nothing else");
    }

    /// A widget merged with its field reads the field's entries too.
    #[test]
    fn a_widget_reads_what_the_form_walk_reads() {
        let widget = entries_read_for("Widget");
        for key in FIELD_ENTRIES_READ {
            assert!(widget.contains(key), "the form walk reads /{key}: {widget:?}");
        }
        assert!(
            !widget.contains(&"DR"),
            "a field's own /DR is not read — only the form's: {widget:?}"
        );
    }

    /// `/Link` is not a markup annotation, whatever its volume in the corpus.
    #[test]
    fn a_link_reads_its_own_entries_and_not_the_markup_ones() {
        let link = entries_read_for("Link");
        assert!(link.contains(&"Dest") && link.contains(&"A"));
        assert!(!link.contains(&"Popup"), "12.5.6.2 does not list /Link: {link:?}");
    }

    /// The flags are bit positions numbered from 1, and getting that off by one would
    /// report every annotation as invisible.
    #[test]
    fn the_flag_bits_are_numbered_from_one() {
        let printed = AnnotationFlags::from_bits(4);
        assert!(printed.print, "bit 3 is /Print");
        assert!(!printed.hidden && !printed.invisible);
        let hidden_and_locked = AnnotationFlags::from_bits(2 | 128);
        assert!(hidden_and_locked.hidden && hidden_and_locked.locked);
    }

    /// An empty `/C` means transparent, which is a statement and not an absence.
    #[test]
    fn an_empty_colour_array_is_transparent() {
        let arena = PdfArena::new();
        let empty = Object::Array(arena.alloc_array(vec![]));
        assert_eq!(
            AnnotationColour::from_pdf_object(empty, &arena).unwrap(),
            AnnotationColour::Transparent
        );
        let grey = Object::Array(arena.alloc_array(vec![Object::Real(0.5)]));
        assert_eq!(
            AnnotationColour::from_pdf_object(grey, &arena).unwrap(),
            AnnotationColour::Gray(0.5)
        );
        let two = Object::Array(arena.alloc_array(vec![Object::Real(0.5), Object::Real(0.5)]));
        assert!(
            AnnotationColour::from_pdf_object(two, &arena).is_err(),
            "two components is not a colour space 12.5.2 defines"
        );
    }
}

/// Where an annotation's appearance goes on the page (12.5.5, "Algorithm: appearance
/// streams").
///
/// **Not a matter of taste, and easy to get wrong in a way that looks nearly right.** The
/// appearance is a form XObject with its own coordinate system; the annotation says where
/// it belongs with `/Rect`. The clause spells the mapping out in three steps, and the
/// middle one is the part a naive implementation skips:
///
/// 1. Transform the appearance's `/BBox` by its `/Matrix`. That produces a quadrilateral
///    of arbitrary orientation, and the *transformed appearance box* is the smallest
///    upright rectangle around it — not the quadrilateral, and not the original box.
/// 2. Compute `A`, which scales and translates that upright rectangle onto `/Rect`.
/// 3. The answer is `Matrix × A`, so the appearance's own matrix still applies **inside**
///    the mapping rather than being replaced by it.
///
/// A transformed box with no width or height cannot be scaled onto anything, so the
/// appearance is placed at the rectangle's corner without scaling rather than divided by
/// zero.
#[must_use]
pub fn appearance_placement(
    bbox: [f64; 4],
    matrix: crate::graphics::Matrix,
    rect: &crate::graphics::Rect,
) -> crate::graphics::Matrix {
    let corners = [(bbox[0], bbox[1]), (bbox[2], bbox[1]), (bbox[2], bbox[3]), (bbox[0], bbox[3])];
    let m = matrix.0;
    let mapped: Vec<(f64, f64)> = corners
        .iter()
        .map(|(x, y)| (m[0].mul_add(*x, m[2] * *y) + m[4], m[1].mul_add(*x, m[3] * *y) + m[5]))
        .collect();
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (x, y) in mapped {
        lo_x = lo_x.min(x);
        lo_y = lo_y.min(y);
        hi_x = hi_x.max(x);
        hi_y = hi_y.max(y);
    }

    let (rect_x, rect_y) = (rect.x1.min(rect.x2), rect.y1.min(rect.y2));
    let (rect_w, rect_h) = ((rect.x2 - rect.x1).abs(), (rect.y2 - rect.y1).abs());
    let (box_w, box_h) = (hi_x - lo_x, hi_y - lo_y);
    let sx = if box_w > 0.0 { rect_w / box_w } else { 1.0 };
    let sy = if box_h > 0.0 { rect_h / box_h } else { 1.0 };
    let a = crate::graphics::Matrix::new(
        sx,
        0.0,
        0.0,
        sy,
        sx.mul_add(-lo_x, rect_x),
        sy.mul_add(-lo_y, rect_y),
    );
    // `AA = Matrix x A` in the clause's row-vector notation means *apply `Matrix`, then
    // `A`*. `Matrix::concat` applies its argument first, so the two names sit the other
    // way round — writing `matrix.concat(&a)` scales before rotating and puts a rotated
    // appearance outside the rectangle it was measured against.
    a.concat(&matrix)
}

#[cfg(test)]
mod appearance_tests {
    use super::appearance_placement;
    use crate::graphics::{Matrix, Rect};

    /// An identity matrix and a bbox at the origin: the appearance is scaled onto the
    /// rectangle and moved to its corner.
    #[test]
    fn an_upright_appearance_is_scaled_onto_the_rectangle() {
        let placed = appearance_placement(
            [0.0, 0.0, 10.0, 10.0],
            Matrix::default(),
            &Rect::new(100.0, 200.0, 120.0, 240.0),
        );
        assert!((placed.0[0] - 2.0).abs() < 1e-9, "x scales 10 to 20: {placed:?}");
        assert!((placed.0[3] - 4.0).abs() < 1e-9, "y scales 10 to 40: {placed:?}");
        assert!((placed.0[4] - 100.0).abs() < 1e-9);
        assert!((placed.0[5] - 200.0).abs() < 1e-9);
    }

    /// **The step a naive implementation skips.** A rotated `/Matrix` turns the bbox into
    /// a quadrilateral, and what gets mapped onto `/Rect` is the smallest upright
    /// rectangle around it. Rotating a 20x10 box by 90 degrees makes it 10 wide and 20
    /// tall, so a square rectangle scales it by 2 in x and 1 in y — not by 1 and 1.
    #[test]
    fn a_rotated_appearance_is_measured_by_its_upright_extent() {
        let quarter_turn = Matrix::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
        let placed = appearance_placement(
            [0.0, 0.0, 20.0, 10.0],
            quarter_turn,
            &Rect::new(0.0, 0.0, 20.0, 20.0),
        );
        // AA = Matrix x A, so the rotation survives and the scale is A's.
        let corner = |x: f64, y: f64| {
            let m = placed.0;
            (m[0].mul_add(x, m[2] * y) + m[4], m[1].mul_add(x, m[3] * y) + m[5])
        };
        let (x0, y0) = corner(0.0, 0.0);
        let (x1, y1) = corner(20.0, 10.0);
        assert!((x0.min(x1) - 0.0).abs() < 1e-9, "the box lands on the rectangle: {x0} {x1}");
        assert!((x0.max(x1) - 20.0).abs() < 1e-9);
        assert!((y0.min(y1) - 0.0).abs() < 1e-9);
        assert!((y0.max(y1) - 20.0).abs() < 1e-9);
    }

    /// A degenerate box is placed rather than divided by.
    #[test]
    fn a_bounding_box_with_no_area_does_not_divide_by_zero() {
        let placed = appearance_placement(
            [5.0, 5.0, 5.0, 5.0],
            Matrix::default(),
            &Rect::new(10.0, 20.0, 30.0, 40.0),
        );
        assert!(placed.0.iter().all(|v| v.is_finite()), "{placed:?}");
    }
}
