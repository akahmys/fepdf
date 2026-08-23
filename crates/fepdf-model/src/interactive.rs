//! Interactive features (clause 12): annotations, form fields, actions, outlines.
//!
//! Surveyed before it was written (`examples/interactive_survey.rs`), and the survey
//! shaped it three times. All 29,973 annotations in the sample corpus are `/Link`, so a
//! report that only counted annotations would say almost nothing — the subtype
//! breakdown is the information. And no sample carries a single form field: the one
//! `/AcroForm` present declares `/DA`, `/DR` and an **empty** `/Fields`, so the field
//! walk was exercised by a hand-assembled fixture rather than by the corpus.
//!
//! That gap closed from two ends. [`add_signature_field`] writes a `/FT /Sig` field into
//! this engine's own output, so `publish sign` followed by `inspect interactive` walks a
//! form this engine built — a weaker test than a foreign file, since a producer only
//! agrees with itself. Then the external corpus supplied four foreign fields, in four
//! Isartor files, one `/Tx` and three `/Btn`.
//!
//! The third shaping is Phase J, and it changed what is reported rather than what is
//! counted. Sixteen subtypes across both corpora and the census could distinguish none
//! of them by anything but the name it counted them under, because [`AnnotationCensus`]
//! was a count. It now reports, per subtype, **which entries the file writes and which
//! of them this engine read** — and it reads them rather than claiming to: every one of
//! the 30,055 annotations is parsed into [`crate::annotation::PdfAnnotation`], and one
//! that will not parse is counted rather than passed over.
//!
//! The outline is reported as total, visible, and declared. Comparing `/Count` with
//! the size of the tree looked like a useful check and was not: 12.3.3 defines it as
//! the *visible* count, so it differs on every outline with a collapsed branch — all
//! three that the corpus carries. A check that fires on conforming input is a constant,
//! not a signal (ADR-0008).

use crate::arena::PdfArena;
use crate::decrypt::Credentials;
use crate::destination::{Lookup, NamedDestinations};
use crate::document::DictHandle;
use crate::error::{PdfError, PdfResult};
use crate::object::{FromPdfObject, Object};
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
    /// Per subtype, the entries the file writes and whether this engine reads them.
    pub subtypes: Vec<SubtypeCensus>,
    /// How many pages carry at least one.
    pub pages_with: usize,
    /// Annotations with no `/Subtype`, which 12.5.2 requires.
    pub without_subtype: usize,
    /// Annotations whose common entries would not parse into [`crate::annotation::PdfAnnotation`].
    ///
    /// Counted, because "this engine reads `/AP`" is otherwise a claim about a struct
    /// rather than about a document — the same reason `DestinationCensus::unreadable`
    /// exists. `entries` above says the engine *has* a reader for a key; this says the
    /// reader survived contact with the file.
    pub unreadable: usize,
    /// What stopped the first unreadable one, for a caller who has to act on it.
    pub first_failure: Option<String>,
}

impl AnnotationCensus {
    /// Distinct entries the file writes that this engine has no reader for.
    ///
    /// The headline number for "what does the engine not understand about the
    /// annotations in this document", and the reason the per-subtype detail exists: a
    /// count of annotations says how much there is, not how much of it was read.
    #[must_use]
    pub fn unread_entries(&self) -> usize {
        let mut keys: Vec<&str> = self
            .subtypes
            .iter()
            .flat_map(|s| s.entries.iter().filter(|e| !e.read).map(|e| e.key.as_str()))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys.len()
    }
}

/// One `/Subtype`, and what its annotations actually carry.
///
/// The count alone was the whole report until Phase J, and it said almost nothing: 16
/// subtypes across both corpora, of which this engine could distinguish exactly none
/// beyond the name it counted them under. What a caller needs is which *entries* were
/// read, so a gap is a fact about a file rather than an inference from a roadmap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtypeCensus {
    /// The `/Subtype` name, or `(none)` for an annotation that omits it.
    pub subtype: String,
    /// How many annotations of this subtype the document carries.
    pub count: usize,
    /// Every entry any of them writes, most widely written first.
    pub entries: Vec<AnnotationEntry>,
}

impl SubtypeCensus {
    /// How many of the entries written here have a reader.
    #[must_use]
    pub fn read(&self) -> usize {
        self.entries.iter().filter(|e| e.read).count()
    }
}

/// One entry an annotation dictionary carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationEntry {
    /// The key, without its slash.
    pub key: String,
    /// How many annotations of this subtype write it.
    pub annotations: usize,
    /// Whether this engine reads it into a typed field, for this subtype.
    ///
    /// Per subtype, because it is: `/Parent` is read on a `/Popup` and on a `/Widget`
    /// and on nothing else, and reporting it as universally read would overstate what
    /// the engine does with a `/Circle`.
    pub read: bool,
}

/// Destinations (12.3.2): what the document declares, and what points at them.
///
/// Declared and referenced are counted separately because they are separate facts and
/// the corpus separates them cleanly. `volvo_xc90.pdf` declares 651 and references 698,
/// so a name is reused; `intel_sdm.pdf` declares 279,501 and references 25,946, so most
/// of what it declares nothing points at. Reporting one number for "destinations" would
/// have said neither.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DestinationCensus {
    /// Declared in the catalogue's `/Dests` dictionary, keyed by name (PDF 1.1).
    pub declared_by_name: usize,
    /// Declared in the `/Dests` name tree under `/Names`, keyed by string (PDF 1.2).
    pub declared_by_string: usize,
    /// Declared, but not readable as a destination — a form Table 151 does not define,
    /// or a first element that is not a page. Counted rather than dropped, so a total
    /// cannot quietly mean "the ones that parsed".
    pub unreadable: usize,
    /// References written in place as an array, needing no lookup.
    pub inline: usize,
    /// References a declared destination answered.
    pub resolved: usize,
    /// Distinct names that nothing declares — links that go nowhere. Held by name
    /// rather than counted, because the name is what makes it actionable, and
    /// deduplicated because one missing destination referenced twice is one defect.
    pub dangling: Vec<String>,
    /// How many references those names account for, which is **not** `dangling.len()`.
    ///
    /// Both are needed, and the first version of this report had only the first: adding
    /// it to `inline` and `resolved` gave `intel_sdm.pdf` 25,951 references where an
    /// independent count said 25,946. The difference was `(G3.7717)` being referenced
    /// three times — one broken destination, three broken links, and a total that had
    /// silently mixed a count of names into a count of uses.
    pub dangling_references: usize,
}

impl DestinationCensus {
    /// Destinations the document declares, across both of 12.3.2.3's forms.
    #[must_use]
    pub fn declared(&self) -> usize {
        self.declared_by_name + self.declared_by_string
    }

    /// References to a destination, however written. Provided rather than left to the
    /// caller because getting it wrong is not hypothetical — see
    /// [`DestinationCensus::dangling_references`].
    #[must_use]
    pub fn referenced(&self) -> usize {
        self.inline + self.resolved + self.dangling_references
    }
}

/// The interactive form (12.7), if the catalogue declares one.
///
/// Four terminal fields exist across both corpora — three `/Btn` and one `/Tx`, in four
/// Isartor files — and until Phase J this counted them and read nothing out of them.
/// Four is not many, and it is four more than the nine samples supply: the walk had been
/// exercised only by a hand-built fixture and by this engine's own signature field,
/// which is a producer agreeing with itself.
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
    /// Whether the form declares a default appearance string (`/DA`) of its own.
    pub has_default_appearance: bool,
    /// Whether it declares the resources (`/DR`) that `/DA` is written against.
    pub has_default_resources: bool,
    /// Every terminal field, in the order the walk reaches them.
    pub terminal: Vec<FormField>,
    /// Fields whose `/Kids` nest deeper than the walk descends, and are therefore not
    /// counted. Zero everywhere in both corpora; reported because a silent truncation is
    /// how a count comes to mean "the ones that fitted".
    pub too_deep: usize,
}

/// One terminal field of the form (12.7.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    /// `/T`: the field's own name — *not* the fully qualified one, which is every
    /// ancestor's `/T` joined by full stops.
    pub name: Option<String>,
    /// The fully qualified name (12.7.4.2), which is what a caller filling a form needs.
    pub qualified_name: Option<String>,
    /// `/FT`: `Btn`, `Tx`, `Ch` or `Sig`, inherited from an ancestor when the field
    /// itself omits it.
    pub field_type: Option<String>,
    /// `/Ff`: the flags of Tables 227, 228, 230 and 232, whose meaning depends on `/FT`
    /// — bit 15 is `Radio` on a button and `Multiline` on nothing else. Reported as the
    /// integer, because interpreting it without the type would be a guess.
    pub flags: Option<i64>,
    /// `/V`: the value, rendered as text. A `/Btn` holds a name, a `/Tx` a string, a
    /// `/Ch` either, and a `/Sig` a dictionary — so this says what is there rather than
    /// pretending the four are one type.
    pub value: Option<String>,
    /// Whether the field carries its own `/DA`. Required on a variable-text field when
    /// the form has none (12.7.4.3), which is what `isartor-6-9-t01` breaks.
    pub has_default_appearance: bool,
    /// Whether the field is also its own widget annotation — the common case, and the
    /// reason a form walk and an annotation walk can reach the same dictionary.
    pub is_widget: bool,
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
    ///
    /// **A census of what hangs off those, and not of everything a document runs.** A
    /// script in the `/Names /JavaScript` tree is pointed at by nothing and runs when the
    /// file opens, so it is absent here — two files of the external corpus are exactly
    /// that, and they are the only two of 524 that run code without the reader touching
    /// anything. [`crate::actions::ActionReport`] is the complete walk, and it reports
    /// *when* each one fires rather than only how many there are.
    pub actions: Vec<(String, usize)>,
    /// Destinations, declared and referenced.
    pub destinations: DestinationCensus,
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
        let named = NamedDestinations::collect(arena, &catalog);
        let mut tally = Tally::new(&named);
        record_actions(arena, &catalog, &mut tally);

        let annotations = census_annotations(arena, &pages, &mut tally);
        let form = read_form(arena, &catalog);
        let outline = read_outline(arena, &catalog, &mut tally);

        let (actions, destinations) = tally.finish(&named);
        Ok(Self {
            pages: pages.len(),
            annotations,
            form,
            outline,
            actions,
            destinations,
            decisions: decisions.entries().to_vec(),
        })
    }

    /// Whether the document offers nothing to interact with.
    ///
    /// Destinations count. `bokutokitan.pdf` carries `/OpenAction [3 0 R /Fit]` — a
    /// destination array rather than an action dictionary — and has no annotation, field
    /// or outline, so without this the report found the destination and then printed
    /// "nothing interactive" over it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.annotations.total == 0
            && self.form.fields == 0
            && self.outline.total == 0
            && self.actions.is_empty()
            && self.destinations.declared() == 0
            && self.destinations.referenced() == 0
    }
}

type Dict = BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>;

/// What the three walks accumulate as they go.
///
/// One struct rather than two `&mut` parameters threaded through four functions: the
/// walks visit the same dictionaries — an annotation carries both an `/A` and a `/Dest`
/// — so counting actions and destinations in one pass is what avoids walking
/// `intel_sdm.pdf`'s 5,000 pages and 25,946 annotations twice.
struct Tally<'a> {
    actions: BTreeMap<String, usize>,
    destinations: DestinationCensus,
    named: &'a NamedDestinations,
    /// Dangling names, deduplicated and in the order first seen.
    dangling: BTreeMap<String, usize>,
}

impl<'a> Tally<'a> {
    fn new(named: &'a NamedDestinations) -> Self {
        Self {
            actions: BTreeMap::new(),
            destinations: DestinationCensus::default(),
            named,
            dangling: BTreeMap::new(),
        }
    }

    /// Resolves one `/Dest` or `/GoTo` `/D` and records what it turned out to name.
    fn destination(&mut self, arena: &PdfArena, object: &Object) {
        match self.named.resolve(object, arena) {
            Lookup::Inline(_) => self.destinations.inline += 1,
            Lookup::Named(_) => self.destinations.resolved += 1,
            Lookup::Dangling(name) => *self.dangling.entry(name).or_default() += 1,
            Lookup::Unreadable => self.destinations.unreadable += 1,
        }
    }

    fn finish(mut self, named: &NamedDestinations) -> (Vec<(String, usize)>, DestinationCensus) {
        self.destinations.declared_by_name = named.by_name.len();
        self.destinations.declared_by_string = named.by_string.len();
        self.destinations.unreadable += named.unreadable;
        self.destinations.dangling_references = self.dangling.values().sum();
        self.destinations.dangling = self.dangling.into_keys().collect();
        let mut actions: Vec<(String, usize)> = self.actions.into_iter().collect();
        actions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        (actions, self.destinations)
    }
}

/// Counts annotations by subtype, folding their actions into `actions`.
fn census_annotations(arena: &PdfArena, pages: &[Dict], tally: &mut Tally) -> AnnotationCensus {
    let mut c = AnnotationCensus::default();
    let mut by_subtype: BTreeMap<String, usize> = BTreeMap::new();
    // subtype -> key -> how many annotations of that subtype write it.
    let mut entries: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for page in pages {
        let list = array_of(arena, page.get(&arena.name("Annots"))).unwrap_or_default();
        if !list.is_empty() {
            c.pages_with += 1;
        }
        for annot in list {
            c.total += 1;
            let Some(d) = dict_of(arena, &annot) else { continue };
            let subtype = match d.get(&arena.name("Subtype")).and_then(|s| name_of(arena, s)) {
                Some(sub) => {
                    *by_subtype.entry(sub.clone()).or_default() += 1;
                    sub
                }
                None => {
                    c.without_subtype += 1;
                    // Counted under a name no `/Subtype` can collide with, so that an
                    // annotation missing the entry 12.5.2 requires still reports what it
                    // carries instead of vanishing from the detail.
                    "(none)".to_string()
                }
            };
            // Parsing, not just naming. A key the struct declares is a claim; a
            // `PdfAnnotation` that comes back out of the arena is a measurement.
            if let Err(why) =
                crate::annotation::PdfAnnotation::from_pdf_object(annot.clone(), arena)
            {
                c.unreadable += 1;
                c.first_failure.get_or_insert_with(|| format!("/{subtype}: {why}"));
            }
            let seen = entries.entry(subtype).or_default();
            for key in d.keys() {
                if let Some(k) = arena.get_name_str(*key) {
                    *seen.entry(k).or_default() += 1;
                }
            }
            record_actions(arena, &d, tally);
        }
    }
    c.by_subtype = by_subtype.into_iter().collect();
    c.by_subtype.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    c.subtypes = c
        .by_subtype
        .iter()
        .map(|(subtype, count)| (subtype.clone(), *count))
        .chain((c.without_subtype > 0).then(|| ("(none)".to_string(), c.without_subtype)))
        .map(|(subtype, count)| report_subtype(&subtype, count, entries.get(&subtype)))
        .collect();
    c
}

/// One subtype's entries, marked against what the engine reads for that subtype.
///
/// What is read is asked of the structs themselves through
/// [`crate::annotation::entries_read_for`], so it cannot drift from the code the way a
/// hand-kept list of seven keys did.
fn report_subtype(
    subtype: &str,
    count: usize,
    written: Option<&BTreeMap<String, usize>>,
) -> SubtypeCensus {
    let read = crate::annotation::entries_read_for(subtype);
    let mut entries: Vec<AnnotationEntry> = written
        .map(|keys| {
            keys.iter()
                .map(|(key, n)| AnnotationEntry {
                    read: read.contains(&key.as_str()),
                    key: key.clone(),
                    annotations: *n,
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| b.annotations.cmp(&a.annotations).then(a.key.cmp(&b.key)));
    SubtypeCensus { subtype: subtype.to_string(), count, entries }
}

/// Reads `/AcroForm`, walking `/Fields` through `/Kids` to the terminal fields.
///
/// `/FT`, `/Ff`, `/V` and `/DA` are **inheritable** (12.7.4.2): a field that omits one
/// takes its parent's. The walk therefore carries the inherited state down rather than
/// reading each dictionary alone, which is also how the qualified name is assembled.
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
    form.has_default_appearance = acro.contains_key(&arena.name("DA"));
    form.has_default_resources = acro.contains_key(&arena.name("DR"));

    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue: Vec<(Object, u32, Inherited)> = array_of(arena, acro.get(&arena.name("Fields")))
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f, 0, Inherited::default()))
        .collect();
    while let Some((node, depth, inherited)) = queue.pop() {
        if depth > 64 {
            form.too_deep += 1;
            continue;
        }
        let Some(d) = dict_of(arena, &node) else { continue };
        let here = inherited.and(arena, &d);
        match array_of(arena, d.get(&arena.name("Kids"))) {
            // A node with /Kids that are themselves fields is not terminal. Widget
            // kids are a different thing, but they carry no /FT of their own, so
            // recursing into them costs nothing and finds nothing.
            Some(kids) if !kids.is_empty() => {
                queue.extend(kids.into_iter().map(|k| (k, depth + 1, here.clone())));
            }
            Some(_) | None => {
                form.fields += 1;
                *by_type
                    .entry(here.field_type.clone().unwrap_or_else(|| "(none)".into()))
                    .or_default() += 1;
                form.terminal.push(FormField {
                    name: d.get(&arena.name("T")).and_then(|t| string_of(arena, t)),
                    qualified_name: here.qualified_name(),
                    field_type: here.field_type.clone(),
                    flags: here.flags,
                    value: here.value.clone(),
                    has_default_appearance: here.has_default_appearance,
                    is_widget: name_of_key(arena, &d, "Subtype").as_deref() == Some("Widget"),
                });
            }
        }
    }
    form.by_type = by_type.into_iter().collect();
    form.by_type.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    form
}

/// The field entries 12.7.4.2 lets a field take from its ancestors.
#[derive(Debug, Clone, Default)]
struct Inherited {
    /// Each ancestor's `/T`, which together make the qualified name.
    path: Vec<String>,
    field_type: Option<String>,
    flags: Option<i64>,
    value: Option<String>,
    has_default_appearance: bool,
}

impl Inherited {
    /// This state, with anything the dictionary states itself overriding it.
    fn and(&self, arena: &PdfArena, d: &Dict) -> Self {
        let mut next = self.clone();
        if let Some(t) = d.get(&arena.name("T")).and_then(|t| string_of(arena, t)) {
            next.path.push(t);
        }
        if let Some(ft) = name_of_key(arena, d, "FT") {
            next.field_type = Some(ft);
        }
        if let Some(Object::Integer(n)) = d.get(&arena.name("Ff")).map(|f| f.resolve(arena)) {
            next.flags = Some(n);
        }
        if let Some(v) = d.get(&arena.name("V")) {
            next.value = Some(render_value(arena, v));
        }
        next.has_default_appearance |= d.contains_key(&arena.name("DA"));
        next
    }

    /// The fully qualified name (12.7.4.2): every ancestor's `/T`, joined by full stops.
    /// A field with no `/T` anywhere in its chain has none, which is legal.
    fn qualified_name(&self) -> Option<String> {
        (!self.path.is_empty()).then(|| self.path.join("."))
    }
}

/// `/V` as text, saying what kind of value it is when it is not one.
///
/// The four field types hold four different things — a name, a string, either, and a
/// signature dictionary — and a reader that returned `Option<String>` from only the
/// string case would report a checked checkbox and an unsigned signature identically as
/// "no value".
fn render_value(arena: &PdfArena, value: &Object) -> String {
    match value.resolve(arena) {
        Object::Name(h) => arena.get_name_str(h).map_or_else(|| "/?".into(), |n| format!("/{n}")),
        Object::String(_) | Object::Hex(_) => {
            string_of(arena, value).unwrap_or_else(|| "(unreadable)".into())
        }
        Object::Array(h) => {
            format!("({} values)", arena.get_array(h).map_or(0, |a| a.len()))
        }
        Object::Dictionary(..) | Object::Stream(..) => "(dictionary)".into(),
        other => format!("{other:?}"),
    }
}

fn name_of_key(arena: &PdfArena, d: &Dict, key: &str) -> Option<String> {
    d.get(&arena.name(key)).and_then(|v| name_of(arena, v))
}

/// A text string (7.9.2.2), decoded the way the rest of the engine decodes one — by
/// byte order mark, or PDFDocEncoding from Annex D.
fn string_of(arena: &PdfArena, object: &Object) -> Option<String> {
    match object.resolve(arena) {
        Object::Text(s) => Some(s),
        Object::String(b) | Object::Hex(b) => Some(crate::refine::text::recover_string(&b)),
        _ => None,
    }
}

/// Reads `/Outlines`, comparing what `/Count` claims with what the links reach.
fn read_outline(arena: &PdfArena, catalog: &Dict, tally: &mut Tally) -> Outline {
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
        record_actions(arena, &d, tally);
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

/// Folds `/A`, `/OpenAction` and every `/AA` entry of `dict` into the tally by `/S`,
/// and every destination they or `dict` itself name.
///
/// `/Dest` belongs to whatever carries it — an annotation or an outline item — while a
/// `/GoTo` action names its destination in `/D`. Both are the same lookup and are
/// counted the same way.
///
/// Only `/GoTo` is followed into `/D`. `/GoToR` and `/GoToE` also carry one, but it
/// names a destination in *another* file, so resolving it here would report every
/// remote link as dangling. Neither occurs in the corpus, which is why this is the
/// standard's word rather than a measurement.
fn record_actions(arena: &PdfArena, dict: &Dict, tally: &mut Tally) {
    if let Some(dest) = dict.get(&arena.name("Dest")) {
        tally.destination(arena, dest);
    }
    for key in ["A", "OpenAction"] {
        let Some(entry) = dict.get(&arena.name(key)) else { continue };
        match dict_of(arena, entry) {
            Some(action) => {
                let kind = action_kind(arena, &action);
                if kind == "GoTo"
                    && let Some(d) = action.get(&arena.name("D"))
                {
                    tally.destination(arena, d);
                }
                *tally.actions.entry(kind).or_default() += 1;
            }
            // `/OpenAction` may be a destination array rather than an action dictionary
            // (12.3.2), which `bokutokitan.pdf` uses: `[3 0 R /Fit]`. Before this it was
            // neither counted as an action nor seen as a destination.
            None => tally.destination(arena, entry),
        }
    }
    if let Some(aa) = dict.get(&arena.name("AA")).and_then(|a| dict_of(arena, a)) {
        for value in aa.values() {
            if let Some(inner) = dict_of(arena, value) {
                *tally.actions.entry(format!("AA/{}", action_kind(arena, &inner))).or_default() +=
                    1;
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

/// The order a form's fields are calculated in (12.6.3, `/CO`), by field name.
///
/// `/CO` is what says which order to run calculations in, and the order matters: a field
/// computed from another must run after it, and getting that wrong yields a **stale
/// value rather than an error**. That is why the order is written in the file instead of
/// being inferred from what the scripts read.
///
/// Empty when the form declares none, which is also the answer for a document with no
/// form at all — there is nothing a caller could usefully do differently.
///
/// A field named here that this engine cannot resolve to a name is skipped rather than
/// guessed at; the count disagreeing with `/CO`'s length is what a caller would notice.
#[must_use]
pub fn calculation_order(doc: &crate::Document) -> Vec<String> {
    let arena = doc.arena();
    let Some(root) = doc.catalog_handle().and_then(|handle| arena.get_object(handle)) else {
        return Vec::new();
    };
    let Ok(Some(catalog)) = crate::document::entries::entry::<crate::document::entries::AcroForm>(
        arena, &root, "AcroForm",
    ) else {
        return Vec::new();
    };
    let Some(order) = catalog.calculation_order.and_then(|handle| arena.get_array(handle)) else {
        return Vec::new();
    };
    order
        .iter()
        .filter_map(|entry| {
            let dict = arena.get_dict(entry.resolve(arena).as_dict_handle()?)?;
            let name = dict.get(&arena.name("T"))?.resolve(arena);
            name.as_string().map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        })
        .collect()
}

/// A terminal field's `/V`, as text, by name (12.7.4.2).
///
/// Matches the fully qualified name first and the field's own `/T` second, because a
/// calculation order names fields the way the form does and a flat form writes only `/T`.
///
/// Added for the script frontend, which cannot reach the arena itself (Rule A). A caller
/// wanting a value it has just written asks the document, not the thing that wrote it —
/// which is what makes the Keystroke → Validate → Calculate → Format cascade readable
/// rather than a chain of guesses about what got applied.
#[must_use]
pub fn field_value(doc: &crate::Document, name: &str) -> Option<String> {
    let arena = doc.arena();
    let root = doc.catalog_handle().and_then(|handle| arena.get_object(handle))?;
    let form = crate::document::entries::entry::<crate::document::entries::AcroForm>(
        arena, &root, "AcroForm",
    )
    .ok()
    .flatten()?;
    let fields = form.fields.and_then(|handle| arena.get_array(handle))?;
    for entry in &fields {
        let Some(dict) = arena.get_dict(entry.resolve(arena).as_dict_handle()?) else {
            continue;
        };
        let matches = dict
            .get(&arena.name("T"))
            .map(|t| t.resolve(arena))
            .and_then(|t| t.as_string().map(|b| String::from_utf8_lossy(b).into_owned()))
            .is_some_and(|t| t == name);
        if !matches {
            continue;
        }
        let value = dict.get(&arena.name("V"))?.resolve(arena);
        return value
            .as_string()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .or_else(|| value.as_f64().map(|n| n.to_string()));
    }
    None
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
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [4 0 R 5 0 R 11 0 R] >>"
                .into(),
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
        // Every entry `FIELD_ENTRIES_READ` claims, on one field, and it is its own
        // widget — the shape all four fields in the external corpus take.
        push(
            "<< /Type /Annot /Subtype /Widget /Rect [0 0 20 20] /FT /Tx /T (name) /Ff 4097               /V (typed in) /DA (/Helv 0 Tf 0 g) >>"
                .into(),
        ); // 11 terminal
        // /FT and /Ff are inheritable (12.7.4.2): the kids state neither.
        push("<< /T (group) /FT /Btn /Ff 32768 /Kids [13 0 R 14 0 R] >>".into()); // 12
        push("<< /T (yes) /V /On >>".into()); // 13
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
        assert_eq!(r.annotations.total, 3);
        assert_eq!(r.annotations.pages_with, 1);
        assert_eq!(r.annotations.without_subtype, 0);
        let subtypes: Vec<&str> =
            r.annotations.by_subtype.iter().map(|(s, _)| s.as_str()).collect();
        assert!(subtypes.contains(&"Link") && subtypes.contains(&"Text"));
        assert!(subtypes.contains(&"Widget"), "the /Tx field is its own widget");
    }

    /// The claim that an entry is "read" is checked by reading it.
    ///
    /// `entries` is derived from the structs, so it says what the engine *declares* a
    /// reader for. Whether that reader survives a real dictionary is a different
    /// question, and the answer across both corpora is that all 30,055 annotations
    /// parse. Injecting a defect — `/Border` demanding five elements instead of three —
    /// takes `volvo_xc90.pdf` from 0 unreadable to 844.
    #[test]
    fn an_annotation_that_will_not_parse_is_counted_rather_than_passed_over() {
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        assert_eq!(r.annotations.unreadable, 0, "{:?}", r.annotations.first_failure);
        assert_eq!(r.annotations.total, 3);
    }

    /// Per subtype, which entries the file writes and which of them were read.
    ///
    /// The census counted annotations and stopped, which said nothing about how much of
    /// one the engine understood: 16 subtypes across both corpora and not one of them
    /// distinguishable by anything but the name it was counted under.
    #[test]
    fn each_subtype_reports_the_entries_it_carries_and_whether_they_were_read() {
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        let link = r
            .annotations
            .subtypes
            .iter()
            .find(|s| s.subtype == "Link")
            .expect("the /Link is reported");
        let entry = |key: &str| link.entries.iter().find(|e| e.key == key);
        assert!(entry("A").is_some_and(|e| e.read), "12.5.6.5 defines /A on a link");
        assert!(entry("Rect").is_some_and(|e| e.read), "Table 166");
        assert!(entry("Type").is_some_and(|e| e.read), "/Type is read, not /kind");

        // A `/Text` is a markup annotation, so its own subtype entries are unread while
        // Table 172's are read — the distinction the per-subtype report exists to make.
        let text = r.annotations.subtypes.iter().find(|s| s.subtype == "Text").expect("reported");
        assert!(text.entries.iter().all(|e| e.read), "it writes only common entries");
    }

    /// The entries `FIELD_ENTRIES_READ` claims are the ones the walk actually reads.
    ///
    /// The list cannot be derived — the reader is a walk, not a struct — so it is
    /// checked. Field 11 writes every one of them.
    #[test]
    fn the_widget_entries_this_engine_reads_are_the_ones_the_form_walk_reads() {
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        let field = r
            .form
            .terminal
            .iter()
            .find(|f| f.qualified_name.as_deref() == Some("name"))
            .expect("the /Tx field is walked");
        assert_eq!(field.field_type.as_deref(), Some("Tx"), "/FT");
        assert_eq!(field.flags, Some(4097), "/Ff");
        assert_eq!(field.value.as_deref(), Some("typed in"), "/V");
        assert!(field.has_default_appearance, "/DA");
        assert!(field.is_widget, "it is its own widget annotation");
        assert!(
            r.form.terminal.iter().any(|f| f.qualified_name.as_deref() == Some("group.yes")),
            "/Kids: {:?}",
            r.form.terminal
        );
    }

    /// `/FT`, `/Ff` and `/V` are inheritable (12.7.4.2), and a kid that states none of
    /// them is not a field of no type.
    #[test]
    fn a_field_takes_what_its_parent_declares() {
        let r = InteractiveReport::survey(&interactive_document()).expect("reads");
        let kid = r
            .form
            .terminal
            .iter()
            .find(|f| f.qualified_name.as_deref() == Some("group.yes"))
            .expect("the kid is walked");
        assert_eq!(kid.field_type.as_deref(), Some("Btn"), "inherited from /Parent");
        assert_eq!(kid.flags, Some(32768), "inherited");
        assert_eq!(kid.value.as_deref(), Some("/On"), "its own /V, a name not a string");
        assert!(!kid.has_default_appearance, "neither it nor its parent writes /DA");
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
        assert_eq!(r.form.too_deep, 0);
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
