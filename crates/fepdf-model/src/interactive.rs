//! Interactive features (clause 12): annotations, form fields, actions, outlines.
//!
//! Surveyed before it was written (`examples/interactive_survey.rs`), and the survey
//! shaped it twice. All 29,973 annotations in the sample corpus are `/Link`, so a
//! report that only counted annotations would say almost nothing — the subtype
//! breakdown is the information. And no sample carries a single form field: the one
//! `/AcroForm` present declares `/DA`, `/DR` and an **empty** `/Fields`, so the field
//! walk here is exercised by a hand-assembled fixture rather than by the corpus.
//!
//! That gap closed from the other end. [`add_signature_field`] writes a `/FT /Sig`
//! field into this engine's own output, so `publish sign` followed by `inspect
//! interactive` now walks a form this engine built — which is a weaker test than a
//! foreign file would be, since a producer only ever agrees with itself, but it is one
//! more reader of the walk than the corpus supplies.
//!
//! The outline is reported as total, visible, and declared. Comparing `/Count` with
//! the size of the tree looked like a useful check and was not: 12.3.3 defines it as
//! the *visible* count, so it differs on every outline with a collapsed branch — all
//! three that the corpus carries. A check that fires on conforming input is a constant,
//! not a signal (ADR-0008).

use crate::arena::PdfArena;
use crate::decrypt::Credentials;
use crate::document::DictHandle;
use crate::error::{PdfError, PdfResult};
use crate::object::Object;
use crate::reader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Annotations across every page (12.5).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnotationCensus {
    /// How many annotations the pages carry between them.
    pub total: usize,
    /// Count per `/Subtype`, most frequent first.
    pub by_subtype: Vec<(String, usize)>,
    /// How many pages carry at least one.
    pub pages_with: usize,
    /// Annotations with no `/Subtype`, which 12.5.2 requires.
    pub without_subtype: usize,
}

/// The interactive form (12.7), if the catalogue declares one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormFields {
    /// Whether `/AcroForm` is present.
    pub declared: bool,
    /// Terminal fields, walking `/Kids`.
    pub fields: usize,
    /// Count per `/FT` — `Btn`, `Tx`, `Ch`, `Sig`.
    pub by_type: Vec<(String, usize)>,
    /// `/NeedAppearances`, when the form states it.
    pub needs_appearances: Option<bool>,
}

/// The outline (12.3.3).
///
/// The root's `/Count` is **not** the size of the tree: 12.3.3 defines it as the
/// number of *visible* items, so descendants of a closed item — one whose own
/// `/Count` is negative — are excluded. Comparing it with the total number of items
/// therefore disagrees on every well-formed outline with a collapsed branch, which is
/// most of them; all three samples that carry an outline do. The comparison worth
/// making is against a visible count computed the same way.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Outline {
    /// Whether `/Outlines` is present.
    pub present: bool,
    /// The root's `/Count`, as the file writes it.
    pub declared_visible: i64,
    /// Visible items, counted by not descending past a closed one.
    pub visible: usize,
    /// Every item in the tree, open or not.
    pub total: usize,
}

impl Outline {
    /// Whether `/Count` says something the tree does not bear out.
    #[must_use]
    pub fn count_disagrees(&self) -> bool {
        self.present && self.declared_visible != i64::try_from(self.visible).unwrap_or(i64::MAX)
    }
}

/// Every interactive feature of one document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveReport {
    /// Pages reached through the page tree.
    pub pages: usize,
    /// Annotations, by subtype.
    pub annotations: AnnotationCensus,
    /// The interactive form.
    pub form: FormFields,
    /// The outline.
    pub outline: Outline,
    /// Actions by `/S`, from `/OpenAction`, annotation `/A` and `/AA` entries.
    pub actions: Vec<(String, usize)>,
    /// What the engine decided while reading this file (§5.3).
    pub decisions: Vec<crate::interpretation::Decision>,
}

impl InteractiveReport {
    /// Reads `bytes` and reports what a reader could interact with.
    ///
    /// # Errors
    /// Fails when the file cannot be read or names no catalogue.
    pub fn survey(bytes: &[u8]) -> PdfResult<Self> {
        let raw = reader::load_document(bytes)?;
        // Pass 0, as `Document::open` runs it. Without this the report describes the
        // file's *ciphertext*: `samples/unicode_16.pdf` listed `/Lang` as a 32-byte
        // string, which is one AES block and an IV, not a language tag.
        let mut decisions = raw.decisions.clone();
        crate::decrypt::unlock_raw(&raw, Credentials::default(), &mut decisions)?;
        let arena = &raw.arena;
        let catalog = raw
            .trailer
            .and_then(|t| arena.get_dict(t))
            .and_then(|d| d.get(&arena.name("Root")).cloned())
            .and_then(|r| dict_of(arena, &r))
            .ok_or_else(|| PdfError::Arena("the file names no catalogue".into()))?;

        let pages = collect_pages(arena, &catalog);
        let mut actions: BTreeMap<String, usize> = BTreeMap::new();
        record_actions(arena, &catalog, &mut actions);

        let annotations = census_annotations(arena, &pages, &mut actions);
        let form = read_form(arena, &catalog);
        let outline = read_outline(arena, &catalog);

        let mut actions: Vec<(String, usize)> = actions.into_iter().collect();
        actions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        Ok(Self {
            pages: pages.len(),
            annotations,
            form,
            outline,
            actions,
            decisions: decisions.entries().to_vec(),
        })
    }

    /// Whether the document offers nothing to interact with.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.annotations.total == 0
            && self.form.fields == 0
            && self.outline.total == 0
            && self.actions.is_empty()
    }
}

type Dict = BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>;

/// Counts annotations by subtype, folding their actions into `actions`.
fn census_annotations(
    arena: &PdfArena,
    pages: &[Dict],
    actions: &mut BTreeMap<String, usize>,
) -> AnnotationCensus {
    let mut c = AnnotationCensus::default();
    let mut by_subtype: BTreeMap<String, usize> = BTreeMap::new();
    for page in pages {
        let list = array_of(arena, page.get(&arena.name("Annots"))).unwrap_or_default();
        if !list.is_empty() {
            c.pages_with += 1;
        }
        for annot in list {
            c.total += 1;
            let Some(d) = dict_of(arena, &annot) else { continue };
            match d.get(&arena.name("Subtype")).and_then(|s| name_of(arena, s)) {
                Some(sub) => *by_subtype.entry(sub).or_default() += 1,
                None => c.without_subtype += 1,
            }
            record_actions(arena, &d, actions);
        }
    }
    c.by_subtype = by_subtype.into_iter().collect();
    c.by_subtype.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    c
}

/// Reads `/AcroForm`, walking `/Fields` through `/Kids` to the terminal fields.
fn read_form(arena: &PdfArena, catalog: &Dict) -> FormFields {
    let mut form = FormFields::default();
    let Some(acro) = catalog.get(&arena.name("AcroForm")).and_then(|a| dict_of(arena, a)) else {
        return form;
    };
    form.declared = true;
    form.needs_appearances = acro.get(&arena.name("NeedAppearances")).and_then(|v| match v {
        Object::Boolean(b) => Some(*b),
        _ => None,
    });

    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue: Vec<(Object, u32)> = array_of(arena, acro.get(&arena.name("Fields")))
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f, 0))
        .collect();
    while let Some((node, depth)) = queue.pop() {
        if depth > 64 {
            continue;
        }
        let Some(d) = dict_of(arena, &node) else { continue };
        match array_of(arena, d.get(&arena.name("Kids"))) {
            // A node with /Kids that are themselves fields is not terminal. Widget
            // kids are a different thing, but they carry no /FT of their own, so
            // recursing into them costs nothing and finds nothing.
            Some(kids) if !kids.is_empty() => {
                queue.extend(kids.into_iter().map(|k| (k, depth + 1)));
            }
            Some(_) | None => {
                form.fields += 1;
                let ft = d
                    .get(&arena.name("FT"))
                    .and_then(|s| name_of(arena, s))
                    .unwrap_or_else(|| "(none)".into());
                *by_type.entry(ft).or_default() += 1;
            }
        }
    }
    form.by_type = by_type.into_iter().collect();
    form.by_type.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    form
}

/// Reads `/Outlines`, comparing what `/Count` claims with what the links reach.
fn read_outline(arena: &PdfArena, catalog: &Dict) -> Outline {
    let mut out = Outline::default();
    let Some(root) = catalog.get(&arena.name("Outlines")).and_then(|o| dict_of(arena, o)) else {
        return out;
    };
    out.present = true;
    out.declared_visible = match root.get(&arena.name("Count")) {
        Some(Object::Integer(n)) => *n,
        _ => 0,
    };

    // `shown` tracks whether this item is reached without passing through a closed
    // ancestor. An item's own `/Count` is negative when *it* is closed, which hides
    // its descendants but not itself.
    let mut queue: Vec<(Object, u32, bool)> =
        root.get(&arena.name("First")).into_iter().map(|f| (f.clone(), 0, true)).collect();
    while let Some((node, depth, shown)) = queue.pop() {
        if depth > 64 || out.total > 100_000 {
            continue;
        }
        let Some(d) = dict_of(arena, &node) else { continue };
        out.total += 1;
        if shown {
            out.visible += 1;
        }
        let open = !matches!(d.get(&arena.name("Count")), Some(Object::Integer(n)) if *n < 0);
        if let Some(first) = d.get(&arena.name("First")) {
            queue.push((first.clone(), depth + 1, shown && open));
        }
        if let Some(next) = d.get(&arena.name("Next")) {
            queue.push((next.clone(), depth, shown));
        }
    }
    out
}

/// Folds `/A`, `/OpenAction` and every `/AA` entry of `dict` into `out`, by `/S`.
fn record_actions(arena: &PdfArena, dict: &Dict, out: &mut BTreeMap<String, usize>) {
    for key in ["A", "OpenAction"] {
        if let Some(action) = dict.get(&arena.name(key)).and_then(|a| dict_of(arena, a)) {
            *out.entry(action_kind(arena, &action)).or_default() += 1;
        }
    }
    if let Some(aa) = dict.get(&arena.name("AA")).and_then(|a| dict_of(arena, a)) {
        for value in aa.values() {
            if let Some(inner) = dict_of(arena, value) {
                *out.entry(format!("AA/{}", action_kind(arena, &inner))).or_default() += 1;
            }
        }
    }
}

fn action_kind(arena: &PdfArena, action: &Dict) -> String {
    action.get(&arena.name("S")).and_then(|s| name_of(arena, s)).unwrap_or_else(|| "(no /S)".into())
}

/// Every page dictionary, walking `/Kids` with a depth bound.
fn collect_pages(arena: &PdfArena, catalog: &Dict) -> Vec<Dict> {
    let mut out = Vec::new();
    let Some(root) = catalog.get(&arena.name("Pages")).and_then(|p| dict_of(arena, p)) else {
        return out;
    };
    let mut stack = vec![(root, 0_u32)];
    while let Some((node, depth)) = stack.pop() {
        if depth > 64 {
            continue;
        }
        match array_of(arena, node.get(&arena.name("Kids"))) {
            Some(kids) => {
                for kid in kids.into_iter().rev() {
                    if let Some(d) = dict_of(arena, &kid) {
                        stack.push((d, depth + 1));
                    }
                }
            }
            None => out.push(node),
        }
    }
    out
}

/// What a signature field says about itself (12.7.5.5 and Table 255).
///
/// The signature is not here: `/ByteRange` and `/Contents` are the writer's, because
/// only the writer knows where in the file they land. This is the structure the
/// signature hangs from.
#[derive(Debug, Clone, Default)]
pub struct SignatureField {
    /// Which page carries the widget, counting from zero.
    pub page_index: usize,
    /// The field name, `/T`.
    pub field_name: String,
    /// `/M`, the time of signing, as a PDF date string. The caller supplies it: what
    /// time it is is not something the object model should decide.
    pub signed_at: String,
    /// `/Name`. Table 255 puts this at "only when it is not possible to extract the
    /// name from the signature", so a certificate that states a common name should
    /// leave this `None` rather than repeat it.
    pub signer: Option<String>,
    /// `/Reason`.
    pub reason: Option<String>,
    /// `/Location`.
    pub location: Option<String>,
    /// `/ContactInfo`.
    pub contact: Option<String>,
}

/// Adds an invisible signature field, returning the signature dictionary to be signed.
///
/// Invisible — `/Rect [0 0 0 0]` — because a visible signature is an appearance stream,
/// and a widget with a rectangle and no `/AP` is a box viewers draw empty. The field
/// still exists, is still listed in `/AcroForm`, and the signature it holds still covers
/// the file; what it does not do is claim a picture that was never drawn.
///
/// `/SigFlags` is set to 3 — signatures exist, and the file is append-only. The second
/// bit is a warning to other tools, and an accurate one: this engine rewrites documents
/// whole ([ADR-0012]), so any save invalidates what it signed.
///
/// # Errors
/// If the catalogue has no page tree, or `page_index` is past its last page.
///
/// [ADR-0012]: ../../../../docs/adr/0012-saving-produces-a-new-document.md
pub fn add_signature_field(
    arena: &PdfArena,
    catalog: crate::handle::Handle<Object>,
    field: &SignatureField,
) -> PdfResult<crate::handle::Handle<Object>> {
    let catalog_dict_handle = arena
        .get_object(catalog)
        .and_then(|o| o.as_dict_handle())
        .ok_or_else(|| PdfError::Other("the catalogue is not a dictionary".into()))?;
    let mut catalog_dict = arena
        .get_dict(catalog_dict_handle)
        .ok_or_else(|| PdfError::Other("the catalogue is missing".into()))?;

    let pages = page_handles(arena, &catalog_dict);
    let page = *pages.get(field.page_index).ok_or_else(|| {
        PdfError::Other(
            format!(
                "the signature is for page {} of a document with {}",
                field.page_index + 1,
                pages.len()
            )
            .into(),
        )
    })?;

    let signature_handle = signature_dictionary(arena, field);
    let widget_handle = signature_widget(arena, field, signature_handle, page);

    append_to_array(arena, page, "Annots", Object::Reference(widget_handle))?;
    let form = acro_form(arena, &mut catalog_dict, catalog_dict_handle);
    append_to_array(arena, form, "Fields", Object::Reference(widget_handle))?;
    set_entry(arena, form, "SigFlags", Object::Integer(3))?;

    Ok(signature_handle)
}

/// The `/Type /Sig` dictionary, minus the two fields the writer supplies.
fn signature_dictionary(arena: &PdfArena, field: &SignatureField) -> crate::handle::Handle<Object> {
    let mut signature: Dict = BTreeMap::new();
    signature.insert(arena.name("Type"), Object::Name(arena.name("Sig")));
    // 12.8.1: the handler that validates it, and the form the signature takes.
    signature.insert(arena.name("Filter"), Object::Name(arena.name("Adobe.PPKLite")));
    signature.insert(arena.name("SubFilter"), Object::Name(arena.name("ETSI.CAdES.detached")));
    signature.insert(arena.name("M"), Object::Text(field.signed_at.clone()));
    for (key, value) in [
        ("Name", &field.signer),
        ("Reason", &field.reason),
        ("Location", &field.location),
        ("ContactInfo", &field.contact),
    ] {
        if let Some(v) = value {
            signature.insert(arena.name(key), Object::Text(v.clone()));
        }
    }
    arena.alloc_object(Object::Dictionary(arena.alloc_dict(signature)))
}

/// The field and its widget, as one dictionary.
///
/// 12.7.5.5 allows a field with a single associated widget to be merged with it, which
/// is what every producer does and what 12.5.6.19 permits.
fn signature_widget(
    arena: &PdfArena,
    field: &SignatureField,
    signature: crate::handle::Handle<Object>,
    page: crate::handle::Handle<Object>,
) -> crate::handle::Handle<Object> {
    let mut widget: Dict = BTreeMap::new();
    widget.insert(arena.name("Type"), Object::Name(arena.name("Annot")));
    widget.insert(arena.name("Subtype"), Object::Name(arena.name("Widget")));
    widget.insert(arena.name("FT"), Object::Name(arena.name("Sig")));
    widget.insert(arena.name("T"), Object::Text(field.field_name.clone()));
    widget.insert(arena.name("V"), Object::Reference(signature));
    widget
        .insert(arena.name("Rect"), Object::Array(arena.alloc_array(vec![Object::Integer(0); 4])));
    // Table 167: Print (bit 3) so it survives printing, Locked (bit 8) so a viewer does
    // not offer to move a field that has already been signed.
    widget.insert(arena.name("F"), Object::Integer(132));
    widget.insert(arena.name("P"), Object::Reference(page));
    arena.alloc_object(Object::Dictionary(arena.alloc_dict(widget)))
}

/// The catalogue's `/AcroForm`, made if it is not there.
fn acro_form(
    arena: &PdfArena,
    catalog: &mut Dict,
    catalog_handle: DictHandle,
) -> crate::handle::Handle<Object> {
    if let Some(Object::Reference(h)) = catalog.get(&arena.name("AcroForm")) {
        return *h;
    }
    let form = arena.alloc_object(Object::Dictionary(arena.alloc_dict(BTreeMap::new())));
    catalog.insert(arena.name("AcroForm"), Object::Reference(form));
    arena.set_dict(catalog_handle, catalog.clone());
    form
}

/// Appends to a dictionary's array-valued entry, making the array if it is absent.
///
/// The entry may be the array itself or a reference to one; both occur, and a page whose
/// `/Annots` is indirect is not a page whose annotations may be dropped.
fn append_to_array(
    arena: &PdfArena,
    owner: crate::handle::Handle<Object>,
    key: &str,
    value: Object,
) -> PdfResult<()> {
    let handle = arena
        .get_object(owner)
        .and_then(|o| o.as_dict_handle())
        .ok_or_else(|| PdfError::Other(format!("cannot add /{key} to a non-dictionary").into()))?;
    let mut dict = arena.get_dict(handle).ok_or_else(|| {
        PdfError::Other(format!("cannot add /{key} to a missing dictionary").into())
    })?;

    match dict.get(&arena.name(key)).and_then(|o| o.resolve(arena).as_array()) {
        Some(array_handle) => {
            let mut items = arena.get_array(array_handle).unwrap_or_default();
            items.push(value);
            arena.set_array(array_handle, items);
        }
        None => {
            dict.insert(arena.name(key), Object::Array(arena.alloc_array(vec![value])));
            arena.set_dict(handle, dict);
        }
    }
    Ok(())
}

fn set_entry(
    arena: &PdfArena,
    owner: crate::handle::Handle<Object>,
    key: &str,
    value: Object,
) -> PdfResult<()> {
    let handle = arena
        .get_object(owner)
        .and_then(|o| o.as_dict_handle())
        .ok_or_else(|| PdfError::Other(format!("cannot set /{key} on a non-dictionary").into()))?;
    let mut dict = arena.get_dict(handle).ok_or_else(|| {
        PdfError::Other(format!("cannot set /{key} on a missing dictionary").into())
    })?;
    dict.insert(arena.name(key), value);
    arena.set_dict(handle, dict);
    Ok(())
}

/// The page objects in order, by handle rather than by value: a widget has to name its
/// page with `/P`, and a page has to be mutated to carry the widget.
fn page_handles(arena: &PdfArena, catalog: &Dict) -> Vec<crate::handle::Handle<Object>> {
    let mut out = Vec::new();
    let Some(Object::Reference(root)) = catalog.get(&arena.name("Pages")) else {
        return out;
    };
    let mut stack = vec![(*root, 0_u32)];
    while let Some((handle, depth)) = stack.pop() {
        if depth > 64 {
            continue;
        }
        let Some(node) = arena.get_object(handle).and_then(|o| o.as_dict_handle()) else {
            continue;
        };
        let Some(node) = arena.get_dict(node) else { continue };
        match array_of(arena, node.get(&arena.name("Kids"))) {
            Some(kids) => {
                for kid in kids.into_iter().rev() {
                    if let Object::Reference(kid) = kid {
                        stack.push((kid, depth + 1));
                    }
                }
            }
            None => out.push(handle),
        }
    }
    out
}

pub(crate) fn dict_of(arena: &PdfArena, object: &Object) -> Option<Dict> {
    let handle: DictHandle = object.resolve(arena).as_dict_handle()?;
    arena.get_dict(handle)
}

pub(crate) fn array_of(arena: &PdfArena, object: Option<&Object>) -> Option<Vec<Object>> {
    arena.get_array(object?.resolve(arena).as_array()?)
}

pub(crate) fn name_of(arena: &PdfArena, object: &Object) -> Option<String> {
    match object.resolve(arena) {
        Object::Name(h) => arena.get_name_str(h),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document carrying the things the sample corpus does not: form fields of more
    /// than one `/FT`, a field tree with `/Kids`, a non-`Link` annotation, and an
    /// outline whose `/Count` disagrees with what its links reach.
    ///
    /// Written because no sample has a single form field — `samples/intel_sdm.pdf` is
    /// the only one declaring `/AcroForm`, and its `/Fields` is empty. A walk with no
    /// data to walk is a walk that has never run.
    fn interactive_document() -> Vec<u8> {
        let mut objs: Vec<String> = Vec::new();
        let mut push = |s: String| objs.push(s);

        push(
            "<< /Type /Catalog /Pages 2 0 R /AcroForm 10 0 R /Outlines 20 0 R \
              /OpenAction << /S /GoTo /D [3 0 R /Fit] >> >>"
                .into(),
        ); // 1
        push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into()); // 2
        push(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [4 0 R 5 0 R] >>".into(),
        ); // 3
        push(
            "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] \
              /A << /S /URI /URI (https://example.invalid) >> >>"
                .into(),
        ); // 4
        push("<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] >>".into()); // 5

        // 6..9 unused, so the numbering below stays readable.
        push("null".into()); // 6
        push("null".into()); // 7
        push("null".into()); // 8
        push("null".into()); // 9

        push("<< /Fields [11 0 R 12 0 R] /NeedAppearances true >>".into()); // 10 AcroForm
        push("<< /FT /Tx /T (name) >>".into()); // 11 terminal
        push("<< /T (group) /Kids [13 0 R 14 0 R] >>".into()); // 12 non-terminal
        push("<< /FT /Btn /T (yes) >>".into()); // 13
        push("<< /FT /Sig /T (sig) >>".into()); // 14

        for _ in 15..20 {
            push("null".into());
        }
        // Two visible items, one of which is closed over a third. A conforming
        // /Count therefore reads 2 while the tree holds 3.
        push("<< /Type /Outlines /First 21 0 R /Count 2 >>".into()); // 20
        push("<< /Title (one) /Next 22 0 R >>".into()); // 21
        push("<< /Title (two) /First 23 0 R /Count -1 >>".into()); // 22
        push("<< /Title (hidden under a closed parent) >>".into()); // 23

        let mut out = String::from("%PDF-2.0\n");
        let mut offsets = vec![0_usize];
        for (i, body) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
        }
        let xref_at = out.len();
        out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for off in offsets.iter().skip(1) {
            out.push_str(&format!("{off:010} 00000 n \n"));
        }
        out.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objs.len() + 1
        ));
        out.into_bytes()
    }

    #[test]
    fn annotations_are_counted_by_subtype() {
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        assert_eq!(r.annotations.total, 2);
        assert_eq!(r.annotations.pages_with, 1);
        assert_eq!(r.annotations.without_subtype, 0);
        let subtypes: Vec<&str> =
            r.annotations.by_subtype.iter().map(|(s, _)| s.as_str()).collect();
        assert!(subtypes.contains(&"Link") && subtypes.contains(&"Text"));
    }

    #[test]
    fn the_field_walk_descends_through_kids() {
        // The corpus cannot exercise this: its only /AcroForm has an empty /Fields.
        // Three terminal fields, one of them two levels down.
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        assert!(r.form.declared);
        assert_eq!(r.form.needs_appearances, Some(true));
        assert_eq!(r.form.fields, 3, "one direct, two under /Kids: {:?}", r.form.by_type);
        let kinds: Vec<&str> = r.form.by_type.iter().map(|(s, _)| s.as_str()).collect();
        assert!(kinds.contains(&"Tx") && kinds.contains(&"Btn") && kinds.contains(&"Sig"));
    }

    #[test]
    fn the_outline_is_reported_as_declared_and_as_walked() {
        // /Count is a claim the file makes. Reporting only the claim is how a page
        // tree came to be believed over its own contents (ADR-0006).
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        assert!(r.outline.present);
        assert_eq!(r.outline.total, 3, "three items exist");
        assert_eq!(r.outline.visible, 2, "the third sits under a closed parent");
        assert_eq!(r.outline.declared_visible, 2, "which is what /Count states");
        assert!(
            !r.outline.count_disagrees(),
            "a collapsed branch is normal and must not be reported as a discrepancy"
        );
    }

    #[test]
    fn an_outline_count_that_is_wrong_is_reported() {
        // The check has to still bite when /Count really is wrong, or the previous
        // test would be satisfied by never reporting anything.
        let doc = assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Outlines /First 5 0 R /Count 9 >>",
            "<< /Title (only one) >>",
        ]);
        let r = InteractiveReport::survey(&doc).expect("reads");
        assert_eq!(r.outline.visible, 1);
        assert_eq!(r.outline.declared_visible, 9);
        assert!(r.outline.count_disagrees());
    }

    #[test]
    fn actions_are_gathered_from_the_catalogue_and_the_annotations() {
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        let kinds: BTreeMap<&str, usize> =
            r.actions.iter().map(|(s, n)| (s.as_str(), *n)).collect();
        assert_eq!(kinds.get("GoTo"), Some(&1), "the catalogue's /OpenAction");
        assert_eq!(kinds.get("URI"), Some(&1), "the link annotation's /A");
    }

    /// Assembles `objs` as objects 1..=n with a cross-reference table and trailer.
    fn assemble(objs: &[&str]) -> Vec<u8> {
        let mut out = String::from("%PDF-2.0\n");
        let mut offsets = Vec::new();
        for (i, body) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
        }
        let xref_at = out.len();
        out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for off in &offsets {
            out.push_str(&format!("{off:010} 00000 n \n"));
        }
        out.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objs.len() + 1
        ));
        out.into_bytes()
    }

    #[test]
    fn a_document_with_nothing_interactive_says_so() {
        let plain = assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        ]);
        let r = InteractiveReport::survey(&plain).expect("reads");
        assert_eq!(r.pages, 1);
        assert!(r.is_empty());
        assert!(!r.form.declared);
        assert!(!r.outline.present);
    }
}
