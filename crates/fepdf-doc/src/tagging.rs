//! Whether what a page draws is tagged, marked as an artefact, or neither (14.7.4.2).
//!
//! Checkpoint 01 asks three questions about a content stream that the structure tree
//! cannot answer on its own. The tree says which `/MCID`s belong to which element; it
//! does not say whether every glyph and rule on the page is under one, and PDF/UA-1 7.1
//! is a requirement about *all* content:
//!
//! - **01-003** an `/Artifact` sequence opened inside tagged content;
//! - **01-004** a sequence carrying an `/MCID` opened inside an `/Artifact`;
//! - **01-005** content drawn under neither.
//!
//! **The nesting is the whole of the first two**, and the third needs one more thing: to
//! know which operators put marks on a page. Both come out of the sublimated command
//! stream, which already carries `BDC`/`BMC` with its property list and `EMC`.

use fepdf_model::object::sublimation::{Command, IrObject};
use fepdf_model::{Document, Object, PdfResult};
use std::collections::BTreeMap;

/// What one marked-content sequence is, for the conditions that read the nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sequence {
    /// `/Artifact BMC`, or `/Artifact << … >> BDC` (14.8.2.2).
    Artifact,
    /// A sequence whose property list carries an `/MCID`: content the structure tree can
    /// claim as its own (14.7.4.2).
    Tagged,
    /// Neither. `/OC`, `/Tx`, and any tag whose property list has no `/MCID` — a sequence
    /// that neither tags what is inside it nor marks it as an artefact, so content under
    /// one alone is content under nothing.
    ///
    /// **This is the arm that makes 01-005 fire on a well-formed-looking file**, and it
    /// is the right reading: a structure element reaches content by `/MCID` and by
    /// nothing else, so `/Span BDC` with no property list tags nothing.
    Neither,
}

/// What a page's marked content came to.
#[derive(Debug, Default)]
pub struct PageTagging {
    /// 01-003: `/Artifact` sequences opened inside a sequence carrying an `/MCID`.
    pub artifact_inside_tagged: usize,
    /// 01-004: sequences carrying an `/MCID` opened inside an `/Artifact`.
    pub tagged_inside_artifact: usize,
    /// 01-005: marks made under neither, counted by operator.
    pub untagged_marks: usize,
    /// 01-005, undecided: form XObjects invoked under neither.
    ///
    /// **A form's own content stream may carry the marks**, and this walk does not
    /// descend into it — so a `Do` out here is not an answer, it is a place to look. An
    /// *image* XObject is a different matter and counts above: an image has no marked
    /// content of its own, so nothing inside it can be tagged.
    pub forms_outside: Vec<String>,
}

impl PageTagging {
    /// Whether the page breaks none of the three.
    #[must_use]
    pub fn sound(&self) -> bool {
        self.artifact_inside_tagged == 0
            && self.tagged_inside_artifact == 0
            && self.untagged_marks == 0
            && self.forms_outside.is_empty()
    }
}

/// Reads one page's content stream and says what its marked content came to.
///
/// # Errors
/// Fails when the page is not there, or its content will not decode.
pub fn tagging_of_page(doc: &Document, page: usize) -> PdfResult<PageTagging> {
    let mut out = PageTagging::default();
    let Some(content) = crate::apply::text::page_content(doc, page)? else {
        // A page with no `/Contents` draws nothing, so there is nothing under neither.
        return Ok(out);
    };
    let fonts = crate::apply::text::fonts_of_page(doc, page)?;
    let commands =
        fepdf_model::object::sublimation::parser::Sublimator::new(&fonts).sublimate(&content);
    let properties = named_properties(doc, page);
    let forms = form_xobjects(doc, page);

    let mut open: Vec<Sequence> = Vec::new();
    for command in &commands {
        match command {
            Command::BeginMarkedContent { tag, properties: list } => {
                let here = sequence_of(tag.as_str(), list.as_ref(), &properties);
                note_nesting(here, &open, &mut out);
                open.push(here);
            }
            // **An `EMC` with nothing open is the file's error, not a reason to stop.**
            // Popping an empty stack would make every mark after it read as untagged.
            Command::EndMarkedContent => {
                open.pop();
            }
            other => note_mark(other, &open, &forms, &mut out),
        }
    }
    Ok(out)
}

/// 01-003 and 01-004, which are the same question asked in both directions.
fn note_nesting(here: Sequence, open: &[Sequence], out: &mut PageTagging) {
    match here {
        Sequence::Artifact if open.contains(&Sequence::Tagged) => {
            out.artifact_inside_tagged += 1;
        }
        Sequence::Tagged if open.contains(&Sequence::Artifact) => {
            out.tagged_inside_artifact += 1;
        }
        Sequence::Artifact | Sequence::Tagged | Sequence::Neither => {}
    }
}

/// 01-005: a command that puts a mark on the page, under neither a tag nor an artefact.
fn note_mark(
    command: &Command,
    open: &[Sequence],
    forms: &BTreeMap<String, bool>,
    out: &mut PageTagging,
) {
    if open.contains(&Sequence::Artifact) || open.contains(&Sequence::Tagged) {
        return;
    }
    match command {
        // **What the operator paints, not what it sets.** A colour, a matrix or a text
        // matrix leaves no mark; these seven do. `Clip` is not among them: `W n` ends a
        // path without painting it.
        Command::ShowText(_)
        | Command::ShowTextArray(_)
        | Command::Fill(_)
        | Command::Stroke(_)
        | Command::FillStroke(_, _)
        | Command::DrawInlineImage { .. } => out.untagged_marks += 1,
        Command::DrawXObject(name) => match forms.get(name) {
            // A form's own stream may carry the marks; this walk does not descend.
            Some(true) => out.forms_outside.push(name.clone()),
            // An image, or a name the page's resources do not answer for.
            Some(false) | None => out.untagged_marks += 1,
        },
        _ => {}
    }
}

/// What a `BDC` or `BMC` opens.
fn sequence_of(
    tag: &str,
    list: Option<&IrObject>,
    properties: &BTreeMap<String, bool>,
) -> Sequence {
    if tag == "Artifact" {
        return Sequence::Artifact;
    }
    match list {
        Some(IrObject::Dictionary(entries)) if entries.contains_key("MCID") => Sequence::Tagged,
        // **A named property list is resolved rather than guessed at.** `/P /Pr1 BDC`
        // holds its `/MCID` in the page's `/Properties`, and reading the name as "no
        // `/MCID`" would report a tagged page as untagged throughout.
        Some(IrObject::Name(name)) if properties.get(name).copied().unwrap_or(false) => {
            Sequence::Tagged
        }
        Some(_) | None => Sequence::Neither,
    }
}

/// The page's `/Properties` resource: which named property lists carry an `/MCID`.
fn named_properties(doc: &Document, page: usize) -> BTreeMap<String, bool> {
    resource_dict(doc, page, "Properties")
        .into_iter()
        .map(|(name, dict)| {
            let has = doc
                .arena()
                .get_dict(dict)
                .unwrap_or_default()
                .contains_key(&doc.arena().name("MCID"));
            (name, has)
        })
        .collect()
}

/// The page's `/XObject` resource: which names are form XObjects rather than images.
fn form_xobjects(doc: &Document, page: usize) -> BTreeMap<String, bool> {
    let arena = doc.arena();
    let subtype = arena.name("Subtype");
    resource_dict(doc, page, "XObject")
        .into_iter()
        .map(|(name, dict)| {
            let is_form = arena
                .get_dict(dict)
                .unwrap_or_default()
                .get(&subtype)
                .and_then(|value| value.resolve(arena).as_name())
                .and_then(|handle| arena.get_name(handle))
                .is_some_and(|kind| kind.as_str() == "Form");
            (name, is_form)
        })
        .collect()
}

/// One of the page's resource sub-dictionaries, as name to dictionary handle.
fn resource_dict(
    doc: &Document,
    page: usize,
    key: &str,
) -> Vec<(String, fepdf_model::Handle<BTreeMap<fepdf_model::Handle<fepdf_model::PdfName>, Object>>)>
{
    let arena = doc.arena();
    let Some(page_h) = doc.get_page_handle(page) else {
        return Vec::new();
    };
    let chain = doc.get_parent_chain(page_h);
    let view = fepdf_model::Page::new(arena, page_h, chain);
    let resources = arena.get_dict(view.resources_handle()).unwrap_or_default();
    let Some(sub) = resources.get(&arena.name(key)).and_then(|o| o.resolve(arena).as_dict_handle())
    else {
        return Vec::new();
    };
    arena
        .get_dict(sub)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name_h, value)| {
            let name = arena.get_name(name_h)?.as_str().to_string();
            Some((name, value.resolve(arena).as_dict_handle()?))
        })
        .collect()
}
