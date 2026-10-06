//! Matterhorn failure conditions about font dictionaries and embedded programs (W-21k).
//!
//! **What ingestion leaves alone.** It gives an embedded `CIDFontType2` the
//! `/CIDToGIDMap /Identity` Table 115 requires of it, with a `Decision` (ROADMAP Y-F15);
//! a TrueType program it rebuilds for drawing is kept beside the font, not written over the
//! file's. Every font dictionary but that entry, the file's own programs and their
//! `/ToUnicode` maps are read as the file wrote them; a Type 0 font's CMap is asked in
//! `audit_cmaps`.

use crate::structure::{AuditFinding, broken, for_a_reader};
use fepdf_model::access::{entry, name_in, names_in};
use fepdf_model::object::sublimation::{Command, IrObject, TextArrayItem};
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// The failure conditions this module decides.
pub const FROM_FONTS: [&str; 33] = [
    "08-001", "08-002", "10-001", "12-001", "13-001", "11-001", "17-003", "31-004", "31-005",
    "31-006", "31-007", "31-008", "31-009", "31-011", "31-016", "31-030", "31-012", "31-013",
    "31-014", "31-015", "31-017", "31-018", "31-019", "31-020", "31-021", "31-022", "31-023",
    "31-024", "31-025", "31-026", "31-027", "31-028", "31-029",
];

/// What the pages do with one font: the codes they show in it, and those of them that are
/// rendered — in a mode other than 3, the one ISO 14289-1 7.21.4.1 NOTE 2 exempts, since
/// its glyphs are neither stroked, filled nor used to clip.
///
/// **Each string read both ways**, as one-byte codes and as two-byte ones, because which a
/// font's codes are is the font's to say and is decided when the font is.
#[derive(Default)]
struct Use {
    codes: BTreeSet<u8>,
    rendered: BTreeSet<u8>,
    pairs: BTreeSet<u32>,
    rendered_pairs: BTreeSet<u32>,
}

impl Use {
    /// What the pages show in the font, for 31-011 and 31-030.
    fn as_shown(&self) -> crate::glyph_select::Shown<'_> {
        crate::glyph_select::Shown {
            codes: &self.codes,
            rendered: &self.rendered,
            pairs: &self.pairs,
            rendered_pairs: &self.rendered_pairs,
        }
    }
}

/// How deep form XObjects are followed for the fonts they use (Rule 6).
pub(crate) const FORM_DEPTH: usize = 8;

/// Asks [`FROM_FONTS`] of every font the pages and the forms they draw name.
///
/// Answers how many marked sequences in the content state a `/Lang` in an inline property
/// list, for 11-007 to count beside the ones the catalogue's walk reaches.
pub fn audit_fonts(
    doc: &Document,
    findings: &mut Vec<AuditFinding>,
    examined: &mut BTreeSet<&'static str>,
) -> usize {
    let arena = doc.arena();
    let Ok(pages) = doc.page_count() else { return 0 };
    let mut fonts = BTreeSet::new();
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let resources =
            fepdf_model::Page::new(arena, handle, doc.get_parent_chain(handle)).resources_handle();
        collect_fonts(arena, &Object::Dictionary(resources), 0, &mut fonts);
    }
    // 31-030 is about every font's text, so every font's codes are read.
    let Scanned { codes, in_formulas, unlanguaged, graphics, unread, inline_languages } =
        shown_codes(doc, &fonts);
    crate::page_languages::report(doc, &unlanguaged, findings);
    let text = codes.values().any(|u| !u.codes.is_empty());
    crate::audit_presence::content_questions(text, graphics, unread, findings);
    for font in fonts {
        one_font(doc, font, (codes.get(&font), in_formulas.get(&font)), findings);
    }
    // 17-003 is about the text of `<Formula>` elements, which a document with no structure
    // tree has none of to be sound about.
    let tagged = doc.get_structure_root().ok().flatten().is_some();
    examined.extend(FROM_FONTS.iter().filter(|c| tagged || **c != "17-003"));
    inline_languages
}

/// Every condition asked of one font, whose text shows `used`, `in_formula` of it inside a
/// `<Formula>`.
fn one_font(
    doc: &Document,
    font: Handle<Object>,
    (used, in_formula): (Option<&Use>, Option<&Use>),
    findings: &mut Vec<AuditFinding>,
) {
    let arena = doc.arena();
    let reading = Font { doc, arena, handle: font, name: base_font(arena, font) };
    reading.audit(findings);
    crate::audit_subsets::subset_claims(doc, &Object::Reference(font), &reading.name, findings);
    reading.to_unicode_needed(used.map(|u| &u.codes), findings);
    let rendered = used.map(|u| &u.rendered).filter(|r| !r.is_empty());
    reading.drawn_without(rendered.is_some(), findings);
    if let Some(rendered) = rendered {
        reading.looked_up(rendered, findings);
    }
    let Some(used) = used else { return };
    let mapped = crate::unicode_map::mapped(doc, font, &used.codes, &used.pairs);
    crate::unicode_map::report("10-001", &reading.name, &mapped, findings);
    if let Some(math) = in_formula {
        let mapped = crate::unicode_map::mapped(doc, font, &math.codes, &math.pairs);
        crate::unicode_map::report("17-003", &reading.name, &mapped, findings);
    }
    let embedded = !reading.lacks_program();
    crate::glyph_select::selected(doc, font, &reading.name, embedded, &used.as_shown(), findings);
}

/// The fonts a resource dictionary names, and those of the forms it names, by handle.
fn collect_fonts(
    arena: &PdfArena,
    resources: &Object,
    depth: usize,
    fonts: &mut BTreeSet<Handle<Object>>,
) {
    let Some(resources) = resources.resolve(arena).as_dict_handle() else { return };
    let entries = |key: &str| {
        arena
            .dict_entry(resources, arena.name(key))
            .and_then(|d| d.resolve(arena).as_dict_handle())
            .and_then(|d| arena.get_dict(d))
            .unwrap_or_default()
    };
    fonts.extend(entries("Font").values().filter_map(Object::as_reference));
    if depth >= FORM_DEPTH {
        return;
    }
    for form in entries("XObject").values() {
        let Some(Object::Stream(dict, _)) = form.as_reference().and_then(|h| arena.get_object(h))
        else {
            continue;
        };
        if let Some(inner) = arena.dict_entry(dict, arena.name("Resources")) {
            collect_fonts(arena, &inner, depth + 1, fonts);
        }
    }
}

/// A font's `/BaseFont`, for a finding to name it by.
fn base_font(arena: &PdfArena, font: Handle<Object>) -> String {
    Object::Reference(font)
        .resolve(arena)
        .as_dict_handle()
        .and_then(|d| arena.dict_entry(d, arena.name("BaseFont")))
        .and_then(|b| b.as_name())
        .and_then(|n| arena.get_name(n))
        .map_or_else(|| "(unnamed)".to_owned(), |n| n.as_str().to_string())
}

/// One font being asked the conditions.
struct Font<'a> {
    doc: &'a Document,
    arena: &'a PdfArena,
    handle: Handle<Object>,
    name: String,
}

impl Font<'_> {
    fn audit(&self, findings: &mut Vec<AuditFinding>) {
        let font = Object::Reference(self.handle);
        self.to_unicode(&font, findings);
        match name_in(self.arena, &font, "Subtype").as_deref() {
            Some("TrueType") => self.true_type(&font, findings),
            Some("Type0") => {
                let descendants = match entry(self.arena, &font, "DescendantFonts") {
                    Some(Object::Array(array)) => self.arena.get_array(array).unwrap_or_default(),
                    _ => Vec::new(),
                };
                for descendant in descendants {
                    self.cid_to_gid_map(&descendant, findings);
                }
                crate::audit_cmaps::cmap(self.doc, &font, &self.name, findings);
            }
            _ => {}
        }
    }

    /// 31-005: a `CIDFontType2` has a `/CIDToGIDMap`; 31-004: it is a stream or
    /// `/Identity`.
    ///
    /// **An absent map is 31-005's**, which says so in as many words (ROADMAP Y-F16); it
    /// was 31-004's until 31-005 was asked. Loading gives an embedded font `/Identity` and
    /// records that it did, so the record is read for one, and the entry for the rest.
    fn cid_to_gid_map(&self, descendant: &Object, findings: &mut Vec<AuditFinding>) {
        if name_in(self.arena, descendant, "Subtype").as_deref() != Some("CIDFontType2") {
            return;
        }
        let own = name_in(self.arena, descendant, "BaseFont").unwrap_or_default();
        let found = fepdf_model::ingest::missing_cid_to_gid_map(&own);
        let filled = self.doc.decisions.entries().iter().any(|d| d.found == found);
        let identity = match entry(self.arena, descendant, "CIDToGIDMap") {
            None => None,
            Some(Object::Stream(..)) => Some(true),
            Some(other) => Some(
                other
                    .as_name()
                    .and_then(|n| self.arena.get_name(n))
                    .is_some_and(|n| n.as_str() == "Identity"),
            ),
        };
        match (filled, identity) {
            (true, _) | (false, None) => findings.push(broken(
                "31-005",
                format!("/{}: its CIDFontType2 has no /CIDToGIDMap", self.name),
            )),
            (false, Some(false)) => findings.push(broken(
                "31-004",
                format!(
                    "/{}: its CIDFontType2's /CIDToGIDMap is neither a stream nor /Identity",
                    self.name
                ),
            )),
            (false, Some(true)) => {}
        }
    }

    /// 31-019 to 31-021 and 31-023 to 31-026, for a simple TrueType font.
    fn true_type(&self, font: &Object, findings: &mut Vec<AuditFinding>) {
        let Some(descriptor) = entry(self.arena, font, "FontDescriptor") else { return };
        let Some(flags) = entry(self.arena, &descriptor, "Flags").and_then(|f| f.as_f64()) else {
            return;
        };
        // Bit 3 of /Flags is Symbolic (Table 121).
        #[allow(clippy::cast_possible_truncation)] // a flag word, written as an integer
        let symbolic = (flags as i64) & 4 != 0;
        let tables = Self::embedded_cmaps(descendant_program(self, &descriptor).as_ref());
        let encoding = entry(self.arena, font, "Encoding");
        let say = |condition, what: String| broken(condition, format!("/{}: {what}", self.name));
        if symbolic {
            if encoding.is_some() {
                findings.push(say("31-024", "a symbolic TrueType font carries /Encoding".into()));
            }
            match tables.as_deref() {
                Some([]) => findings.push(say("31-025", "its program has no cmap".into())),
                Some(many) if many.len() > 1 && !many.contains(&(3, 0)) => findings.push(say(
                    "31-026",
                    format!("its program has {} cmaps and none is (3,0)", many.len()),
                )),
                _ => {}
            }
            return;
        }
        self.non_symbolic(encoding, tables.as_deref(), findings);
    }

    /// 31-019, 31-020, 31-021 and 31-023, for a non-symbolic TrueType font.
    fn non_symbolic(
        &self,
        encoding: Option<Object>,
        tables: Option<&[(u16, u16)]>,
        findings: &mut Vec<AuditFinding>,
    ) {
        let say = |condition, what: String| broken(condition, format!("/{}: {what}", self.name));
        let allowed = |name: Option<String>| {
            name.is_some_and(|n| n == "MacRomanEncoding" || n == "WinAnsiEncoding")
        };
        match encoding {
            None => {
                findings
                    .push(say("31-019", "a non-symbolic TrueType font has no /Encoding".into()));
            }
            Some(named @ Object::Name(_)) => {
                let name = named
                    .as_name()
                    .and_then(|n| self.arena.get_name(n))
                    .map(|n| n.as_str().to_string());
                if !allowed(name.clone()) {
                    findings.push(say(
                        "31-021",
                        format!("its /Encoding is /{}", name.unwrap_or_default()),
                    ));
                }
            }
            Some(dict) => {
                match name_in(self.arena, &dict, "BaseEncoding") {
                    None => findings.push(say(
                        "31-020",
                        "its /Encoding dictionary has no /BaseEncoding".into(),
                    )),
                    Some(base) if !allowed(Some(base.clone())) => {
                        findings.push(say("31-021", format!("its /BaseEncoding is /{base}")));
                    }
                    Some(_) => {}
                }
                self.differences_rules(&dict, tables, findings);
            }
        }
    }

    /// 31-022 and 31-023: a non-symbolic TrueType font's `/Differences` names only glyphs
    /// Adobe's list has, and comes with a (3,1) cmap in the program.
    fn differences_rules(
        &self,
        encoding: &Object,
        tables: Option<&[(u16, u16)]>,
        findings: &mut Vec<AuditFinding>,
    ) {
        let say = |condition, what: String| broken(condition, format!("/{}: {what}", self.name));
        let unlisted: Vec<String> = self
            .differences(encoding)
            .into_values()
            .filter(|name| !fepdf_font::agl::in_glyph_list(name))
            .collect();
        if !unlisted.is_empty() {
            findings.push(say(
                "31-022",
                format!("its /Differences names {unlisted:?}, which Adobe's list does not"),
            ));
        }
        let differences = entry(self.arena, encoding, "Differences").is_some();
        if differences && tables.is_some_and(|t| !t.contains(&(3, 1))) {
            findings.push(say(
                "31-023",
                "it has /Differences and its program has no (3,1) cmap".into(),
            ));
        }
    }

    /// The (platform, encoding) of each `cmap` subtable of an embedded TrueType program,
    /// or nothing when no program is embedded or it will not read.
    fn embedded_cmaps(program: Option<&Vec<u8>>) -> Option<Vec<(u16, u16)>> {
        let program = program?;
        let Some((start, end)) = fepdf_font::reconstruction::find_table_range(program, b"cmap")
        else {
            return Some(Vec::new());
        };
        let table = program.get(start..end)?;
        let read = |at: usize| table.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
        let count = usize::from(read(2)?);
        (0..count).map(|i| Some((read(4 + i * 8)?, read(6 + i * 8)?))).collect()
    }

    /// 31-028 and 31-029: what a `/ToUnicode` maps codes to.
    fn to_unicode(&self, font: &Object, findings: &mut Vec<AuditFinding>) {
        let Some(stream @ Object::Stream(..)) = entry(self.arena, font, "ToUnicode") else {
            return;
        };
        let Ok(bytes) = self.doc.decode_stream(&stream) else { return };
        let Ok(cmap) = fepdf_font::cmap::CMap::parse(&bytes) else { return };
        let values: Vec<u32> =
            cmap.mappings.values().flat_map(|s| s.chars().map(u32::from)).collect();
        let ranges: Vec<(u32, u32)> = cmap
            .bf_ranges
            .iter()
            .map(|r| (r.base, r.base.saturating_add(r.end.saturating_sub(r.start))))
            .collect();
        let hits = |target: u32| {
            values.contains(&target) || ranges.iter().any(|(a, b)| (*a..=*b).contains(&target))
        };
        if hits(0) {
            findings.push(broken(
                "31-028",
                format!("/{}: its /ToUnicode maps a code to U+0000", self.name),
            ));
        }
        if hits(0xFEFF) || hits(0xFFFE) {
            findings.push(broken(
                "31-029",
                format!("/{}: its /ToUnicode maps a code to U+FEFF or U+FFFE", self.name),
            ));
        }
    }
}

/// The strings a text-showing operator shows: `Tj`, `'`, `"`, and `TJ`'s strings.
fn shown(command: &Command) -> Option<Vec<&[u8]>> {
    match command {
        Command::ShowText(text) => Some(vec![text.as_ref()]),
        Command::ShowTextArray(items) => Some(
            items
                .iter()
                .filter_map(|item| match item {
                    TextArrayItem::Text(text) => Some(text.as_ref()),
                    TextArrayItem::Offset(_) => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

/// A resource dictionary, by handle.
pub(crate) type Resources = Handle<BTreeMap<Handle<fepdf_model::PdfName>, Object>>;

/// What the graphics state carries into a form and back out of `q`…`Q`: the font in force,
/// if it is one being asked about, and whether text is rendered.
type TextState = (Option<Handle<Object>>, bool);

/// The codes each of `fonts` is shown with, and whether visibly, read from the content of
/// the pages and of the forms they draw (to `FORM_DEPTH`): which font each `Tf` selects and
/// which rendering mode each `Tr`, through `q` and `Q`, and the bytes each `Tj`, `'`, `"`
/// and `TJ` shows.
///
/// **And those shown inside a `<Formula>`, apart**, for 17-003; and, where the catalogue
/// states no language, the pages showing text no `/Lang` reaches, for 11-001.
fn shown_codes(doc: &Document, fonts: &BTreeSet<Handle<Object>>) -> Scanned {
    let tree = crate::formula_marks::Tree::of(doc);
    let mut scan = Scan {
        doc,
        fonts,
        tree: tree.as_ref(),
        ask_language: !crate::page_languages::catalogue_states_one(doc),
        page: 0,
        shown: BTreeMap::default(),
        in_formulas: BTreeMap::default(),
        unlanguaged: BTreeSet::new(),
        graphics: 0,
        unread: 0,
        inline_languages: 0,
    };
    let Ok(pages) = doc.page_count() else { return Scanned::default() };
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let resources = fepdf_model::Page::new(doc.arena(), handle, doc.get_parent_chain(handle))
            .resources_handle();
        scan.page(page, resources);
    }
    let used = |shown: BTreeMap<Handle<Object>, Seen>| -> BTreeMap<Handle<Object>, Use> {
        shown.into_iter().map(|(font, seen)| (font, seen.used())).collect()
    };
    Scanned {
        codes: used(scan.shown),
        in_formulas: used(scan.in_formulas),
        unlanguaged: scan.unlanguaged,
        graphics: scan.graphics,
        unread: scan.unread,
        inline_languages: scan.inline_languages,
    }
}

/// What the pass over the content found.
#[derive(Default)]
struct Scanned {
    /// What each font shows.
    codes: BTreeMap<Handle<Object>, Use>,
    /// What each font shows inside a `<Formula>`.
    in_formulas: BTreeMap<Handle<Object>, Use>,
    /// The pages, counted from 0, showing text in no stated language.
    unlanguaged: BTreeSet<usize>,
    /// Graphics objects in neither an `/Artifact` nor a `<Figure>`.
    graphics: usize,
    /// Pages whose content would not read.
    unread: usize,
    /// Marked sequences stating a `/Lang` in an inline property list.
    inline_languages: usize,
}

/// What the scan has seen shown in one font, as bits: one per one-byte code and one per
/// two-byte code, each set once for any text and once for rendered text.
///
/// **Bits, not sets, while the pages are read.** Every byte of every string lands here, and
/// on `intel_sdm.pdf` inserting each into four ordered sets was most of the audit's time.
struct Seen {
    codes: [u64; 4],
    rendered: [u64; 4],
    pairs: Vec<u64>,
    rendered_pairs: Vec<u64>,
}

impl Default for Seen {
    fn default() -> Self {
        Self {
            codes: [0; 4],
            rendered: [0; 4],
            pairs: vec![0; 1024],
            rendered_pairs: vec![0; 1024],
        }
    }
}

/// Sets bit `at`.
fn set(bits: &mut [u64], at: usize) {
    if let Some(word) = bits.get_mut(at / 64) {
        *word |= 1 << (at % 64);
    }
}

/// The bits set, as numbers.
fn members(bits: &[u64]) -> impl Iterator<Item = usize> + '_ {
    bits.iter().enumerate().flat_map(|(i, word)| {
        (0..64).filter(move |b| word & (1 << b) != 0).map(move |b| i * 64 + b)
    })
}

impl Seen {
    /// The bits as the sets the conditions read.
    fn used(&self) -> Use {
        let bytes = |bits: &[u64]| members(bits).filter_map(|c| u8::try_from(c).ok()).collect();
        let pairs = |bits: &[u64]| members(bits).filter_map(|c| u32::try_from(c).ok()).collect();
        Use {
            codes: bytes(&self.codes),
            rendered: bytes(&self.rendered),
            pairs: pairs(&self.pairs),
            rendered_pairs: pairs(&self.rendered_pairs),
        }
    }
}

/// One pass over content for what it does with `fonts`.
struct Scan<'a> {
    doc: &'a Document,
    fonts: &'a BTreeSet<Handle<Object>>,
    tree: Option<&'a crate::formula_marks::Tree<'a>>,
    ask_language: bool,
    page: usize,
    shown: BTreeMap<Handle<Object>, Seen>,
    in_formulas: BTreeMap<Handle<Object>, Seen>,
    unlanguaged: BTreeSet<usize>,
    /// Graphics objects in neither an `/Artifact` nor a `<Figure>`.
    graphics: usize,
    /// Pages whose content would not read.
    unread: usize,
    /// Marked sequences stating a `/Lang` in an inline property list (11-007).
    inline_languages: usize,
}

impl Scan<'_> {
    /// Reads one page's content, drawn with `resources`. Content that
    /// reaches none of the fonts, directly or through its forms, is not parsed.
    fn page(&mut self, page: usize, resources: Resources) {
        let arena = self.doc.arena();
        let named = names_in(arena, resources, "Font");
        let key = self.doc.get_page_handle(page).and_then(|h| {
            entry(arena, &Object::Reference(h), "StructParents").and_then(|k| k.as_integer())
        });
        self.page = page;
        let marks = self.marks(key, resources, crate::formula_marks::Facts::default());
        match fepdf_doc::apply::text::page_commands(self.doc, page, &loaded(self.doc, &named)) {
            Ok(Some(commands)) => {
                self.commands(&commands, resources, &named, (None, true), (0, &marks));
            }
            Ok(None) => {}
            Err(_) => self.unread += 1,
        }
    }

    /// What a content stream whose `/StructParents` is `key`, drawn with `resources` from
    /// inside content that is `within`, needs to say what each of its sequences is.
    fn marks(
        &self,
        key: Option<i64>,
        resources: Resources,
        within: crate::formula_marks::Facts,
    ) -> crate::formula_marks::Marks {
        crate::formula_marks::Marks {
            facts: self.tree.zip(key).map(|(tree, k)| tree.facts(k)).unwrap_or_default(),
            properties: names_in(self.doc.arena(), resources, "Properties"),
            within,
        }
    }

    /// What `commands` shows in each font, and the forms it draws, followed in turn.
    fn commands(
        &mut self,
        content: &fepdf_doc::apply::text::Content,
        resources: Resources,
        named: &BTreeMap<String, Handle<Object>>,
        mut current: TextState,
        (depth, marks): (usize, &crate::formula_marks::Marks),
    ) {
        use fepdf_model::graphics::TextRenderingMode;
        let (mut saved, mut open) = (Vec::new(), Vec::new());
        for command in content.commands() {
            let within = open.last().copied().unwrap_or(marks.within);
            match command {
                Command::BeginMarkedContent { tag, properties } => {
                    open.push(self.opens(marks, tag.as_str(), properties.as_ref(), within));
                }
                Command::EndMarkedContent => {
                    open.pop();
                }
                Command::PushState => saved.push(current),
                Command::PopState => current = saved.pop().unwrap_or(current),
                Command::SetFont { font, .. } => {
                    current.0 = named.get(font).copied().filter(|h| self.fonts.contains(h));
                }
                Command::SetTextRenderMode(mode) => {
                    current.1 = *mode != TextRenderingMode::Invisible;
                }
                Command::DrawXObject(name) => self.form(resources, name, current, (depth, within)),
                Command::Fill(_)
                | Command::Stroke(_)
                | Command::FillStroke(..)
                | Command::DrawInlineImage { .. } => self.graphic(within),
                // `sh` paints a shading (8.7.4.2), which the parser passes through as it
                // was written; it was counted as nothing, and 13-001 called sound a page
                // that paints one outside any `/Artifact` or `<Figure>`.
                Command::RawOperator { name, .. } if name == "sh" => self.graphic(within),
                other => {
                    if let (Some(font), visible, Some(bytes)) = (current.0, current.1, shown(other))
                    {
                        self.note(font, visible, &bytes);
                        if within.formula {
                            self.note_in_formula(font, &bytes);
                        }
                        let text = bytes.iter().any(|b| !b.is_empty());
                        if self.ask_language && text && !within.language && !within.artifact {
                            self.unlanguaged.insert(self.page);
                        }
                    }
                }
            }
        }
    }

    /// What a sequence opened with `tag` and `properties` is, and whether it states a
    /// language inline (11-007).
    fn opens(
        &mut self,
        marks: &crate::formula_marks::Marks,
        tag: &str,
        properties: Option<&IrObject>,
        within: crate::formula_marks::Facts,
    ) -> crate::formula_marks::Facts {
        if let Some(IrObject::Dictionary(inline)) = properties
            && inline.contains_key("Lang")
        {
            self.inline_languages += 1;
        }
        marks.opens(self.doc.arena(), tag, properties, within)
    }

    /// Counts a graphics object that is in neither an `/Artifact` nor a `<Figure>` (13-001).
    fn graphic(&mut self, within: crate::formula_marks::Facts) {
        if !within.artifact && !within.figure {
            self.graphics += 1;
        }
    }

    /// Records the codes a text-showing operator inside a `<Formula>` shows in `font`.
    fn note_in_formula(&mut self, font: Handle<Object>, strings: &[&[u8]]) {
        let seen = self.in_formulas.entry(font).or_default();
        for byte in strings.iter().copied().flatten() {
            set(&mut seen.codes, usize::from(*byte));
        }
        for pair in strings.iter().flat_map(|s| s.as_chunks::<2>().0) {
            set(&mut seen.pairs, usize::from(u16::from_be_bytes(*pair)));
        }
    }

    /// Records the codes one text-showing operator shows in `font`, and whether rendered.
    fn note(&mut self, font: Handle<Object>, visible: bool, strings: &[&[u8]]) {
        let seen = self.shown.entry(font).or_default();
        for string in strings {
            for byte in *string {
                set(&mut seen.codes, usize::from(*byte));
                if visible {
                    set(&mut seen.rendered, usize::from(*byte));
                }
            }
            for pair in string.as_chunks::<2>().0 {
                let code = usize::from(u16::from_be_bytes(*pair));
                set(&mut seen.pairs, code);
                if visible {
                    set(&mut seen.rendered_pairs, code);
                }
            }
        }
    }

    /// The form `name` names in `resources`, read in the state that draws it. A form with no
    /// `/Resources` of its own takes the ones it is drawn with (7.8.3).
    fn form(
        &mut self,
        resources: Resources,
        name: &str,
        state: TextState,
        (depth, within): (usize, crate::formula_marks::Facts),
    ) {
        let arena = self.doc.arena();
        let Some(form) = names_in(arena, resources, "XObject").get(name).copied() else { return };
        let Some(own) = form_resources(arena, form, resources) else {
            // An image, which is a graphics object in its own right.
            self.graphic(within);
            return;
        };
        if depth >= FORM_DEPTH {
            return;
        }
        let named = names_in(arena, own, "Font");
        let key =
            entry(arena, &Object::Reference(form), "StructParents").and_then(|k| k.as_integer());
        let marks = self.marks(key, own, within);
        if let Some(content) = form_commands(self.doc, form, &named) {
            self.commands(&content, own, &named, state, (depth + 1, &marks));
        }
    }
}

/// The resources a form draws with, when `form` is a form XObject: its own, or, when it
/// has none, the ones it is drawn with (7.8.3).
pub(crate) fn form_resources(
    arena: &PdfArena,
    form: Handle<Object>,
    drawn_with: Resources,
) -> Option<Resources> {
    let Some(Object::Stream(dict, _)) = arena.get_object(form) else { return None };
    let subtype = arena.dict_entry(dict, arena.name("Subtype")).and_then(|s| s.as_name());
    if subtype.and_then(|s| arena.get_name(s)).is_none_or(|s| s.as_str() != "Form") {
        return None;
    }
    let own = arena
        .dict_entry(dict, arena.name("Resources"))
        .and_then(|r| r.resolve(arena).as_dict_handle());
    Some(own.unwrap_or(drawn_with))
}

/// A form's content as commands: the ones ingestion already parsed it into, or its bytes
/// parsed here with the fonts `named`.
pub(crate) fn form_commands(
    doc: &Document,
    form: Handle<Object>,
    named: &BTreeMap<String, Handle<Object>>,
) -> Option<fepdf_doc::apply::text::Content> {
    fepdf_doc::apply::text::Content::of_stream(
        doc,
        &doc.arena().get_object(form)?,
        &loaded(doc, named),
    )
}

/// The fonts `named`, loaded, by resource name.
pub(crate) fn loaded(
    doc: &Document,
    named: &BTreeMap<String, Handle<Object>>,
) -> BTreeMap<String, std::sync::Arc<fepdf_model::font::FontResource>> {
    named
        .iter()
        .filter_map(|(name, font)| Some((name.clone(), doc.get_font(*font).ok()?)))
        .collect()
}

/// The Latin standard 14 fonts, whose built-in encoding is StandardEncoding (9.6.2.2).
pub(crate) const STANDARD_LATIN: [&str; 12] = [
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
];

impl Font<'_> {
    /// The font descriptor that describes the program: the font's own, or its CIDFont's.
    fn descriptor(&self) -> Option<Object> {
        let font = Object::Reference(self.handle);
        if name_in(self.arena, &font, "Subtype").as_deref() == Some("Type0") {
            let Some(Object::Array(descendants)) = entry(self.arena, &font, "DescendantFonts")
            else {
                return None;
            };
            let descendant =
                self.arena.get_array(descendants).unwrap_or_default().into_iter().next()?;
            return entry(self.arena, &descendant, "FontDescriptor");
        }
        entry(self.arena, &font, "FontDescriptor")
    }

    /// Whether the font has a program to draw with and none is embedded — Type 3 fonts
    /// draw with content streams and have none to embed (9.6.5).
    fn lacks_program(&self) -> bool {
        let font = Object::Reference(self.handle);
        if name_in(self.arena, &font, "Subtype").as_deref() == Some("Type3") {
            return false;
        }
        let Some(descriptor) = self.descriptor() else { return true };
        !["FontFile", "FontFile2", "FontFile3"]
            .iter()
            .any(|key| entry(self.arena, &descriptor, key).is_some())
    }

    /// Whether this is a non-symbolic TrueType font whose embedded program has neither a
    /// (3,1) nor a (1,0) cmap (ISO 32000-1 9.6.6.4 names the two a non-symbolic font is
    /// read through).
    fn lacks_latin_cmap(&self) -> bool {
        let Some(descriptor) = self.non_symbolic_true_type() else { return false };
        Self::embedded_cmaps(descendant_program(self, &descriptor).as_ref())
            .is_some_and(|tables| !tables.contains(&(3, 1)) && !tables.contains(&(1, 0)))
    }

    /// The font descriptor of a TrueType font whose Symbolic flag is clear.
    fn non_symbolic_true_type(&self) -> Option<Object> {
        let font = Object::Reference(self.handle);
        if name_in(self.arena, &font, "Subtype").as_deref() != Some("TrueType") {
            return None;
        }
        let descriptor = entry(self.arena, &font, "FontDescriptor")?;
        let flags = entry(self.arena, &descriptor, "Flags").and_then(|f| f.as_f64());
        #[allow(clippy::cast_possible_truncation)] // a flag word, written as an integer
        flags.is_some_and(|f| (f as i64) & 4 == 0).then_some(descriptor)
    }

    /// 31-018: every code a non-symbolic TrueType font renders reaches a glyph through the
    /// lookup ISO 32000-1 9.6.6.4 describes — its name, then the (3,1) subtable by the
    /// name's Unicode value, or where there is none the (1,0) subtable by the name's Mac OS
    /// Roman code. The `post` fallback 9.6.6.4 allows is not a cmap entry, and 7.21.6 asks
    /// for the cmap entries to carry every lookup.
    fn looked_up(&self, rendered: &BTreeSet<u8>, findings: &mut Vec<AuditFinding>) {
        let Some(descriptor) = self.non_symbolic_true_type() else { return };
        let (Some(names), Some(program)) = (
            crate::glyph_map::true_type_names(self.arena, &Object::Reference(self.handle)),
            descendant_program(self, &descriptor),
        ) else {
            return;
        };
        let Some(lost) = crate::truetype_lookup::unreachable(&program, &names, rendered) else {
            return;
        };
        if let Some(first) = lost.first() {
            findings.push(broken(
                "31-018",
                format!(
                    "/{}: {} of the codes it renders reach no glyph through its non-symbolic \
                     cmap, the first 0x{first:02X}",
                    self.name,
                    lost.len()
                ),
            ));
        }
    }

    /// 31-009 and 31-017, which are about a font whose text is rendered.
    fn drawn_without(&self, rendered: bool, findings: &mut Vec<AuditFinding>) {
        if !rendered {
            return;
        }
        if self.lacks_program() {
            findings.push(broken(
                "31-009",
                format!("/{}: text is drawn in it and its program is not embedded", self.name),
            ));
        }
        if self.lacks_latin_cmap() {
            findings.push(broken(
                "31-017",
                format!("/{}: a non-symbolic TrueType font drawn with, whose program has no (3,1) or (1,0) cmap", self.name),
            ));
        }
    }

    /// Whether the font's `/Encoding`, or its `/BaseEncoding`, is one 31-027 accepts.
    fn latin_encoding(&self, font: &Object) -> bool {
        let named = entry(self.arena, font, "Encoding").and_then(|e| {
            e.as_name()
                .and_then(|n| self.arena.get_name(n))
                .map(|n| n.as_str().to_string())
                .or_else(|| name_in(self.arena, &e, "BaseEncoding"))
        });
        named.as_deref().is_some_and(|n| {
            ["MacRomanEncoding", "MacExpertEncoding", "WinAnsiEncoding"].contains(&n)
        })
    }

    /// A simple font's `/Differences`, as code to name.
    fn differences(&self, encoding: &Object) -> BTreeMap<u8, String> {
        crate::glyph_map::differences(self.arena, encoding)
    }

    /// 31-027: a font with no `/ToUnicode` is one whose text can be read without it —
    /// by a named Latin encoding, by glyph names Adobe's list or the Symbol font names,
    /// by one of Adobe's four CJK collections, or by being a non-symbolic TrueType font.
    fn to_unicode_needed(&self, shown: Option<&BTreeSet<u8>>, findings: &mut Vec<AuditFinding>) {
        let font = Object::Reference(self.handle);
        if entry(self.arena, &font, "ToUnicode").is_some() {
            return;
        }
        let encoding = entry(self.arena, &font, "Encoding");
        let named = encoding.as_ref().and_then(|e| {
            e.as_name()
                .and_then(|n| self.arena.get_name(n))
                .map(|n| n.as_str().to_string())
                .or_else(|| name_in(self.arena, e, "BaseEncoding"))
        });
        if self.latin_encoding(&font) {
            return;
        }
        let say =
            |what: String| broken("31-027", format!("/{} has no /ToUnicode and {what}", self.name));
        match name_in(self.arena, &font, "Subtype").as_deref() {
            Some("TrueType") => {
                let flags = entry(self.arena, &font, "FontDescriptor")
                    .and_then(|d| entry(self.arena, &d, "Flags"))
                    .and_then(|f| f.as_f64());
                #[allow(clippy::cast_possible_truncation)] // a flag word, written as an integer
                if flags.is_some_and(|f| (f as i64) & 4 != 0) {
                    findings.push(say("is a symbolic TrueType font".into()));
                }
            }
            Some("Type0") => {
                if !self.adobe_collection(&font) {
                    findings.push(say(
                        "its CIDFont uses none of Adobe's GB1, CNS1, Japan1 or Korea1".into(),
                    ));
                }
            }
            Some("Type1" | "MMType1" | "Type3") => {
                // With no base named, the base is the font's built-in encoding (9.6.6.1),
                // which for the Latin standard 14 is StandardEncoding.
                let standard = named.as_deref() == Some("StandardEncoding")
                    || (named.is_none() && STANDARD_LATIN.contains(&self.name.as_str()));
                let differences = encoding.map(|e| self.differences(&e)).unwrap_or_default();
                self.glyph_names(shown, &differences, standard, findings);
            }
            _ => {}
        }
    }

    /// Whether a Type 0 font's CIDFont is in one of Adobe's four CJK collections.
    fn adobe_collection(&self, font: &Object) -> bool {
        crate::unicode_map::adobe_collection(self.arena, font)
    }

    /// 31-027 for a Type 1 or Type 3 font: the names of the glyphs its text shows.
    fn glyph_names(
        &self,
        shown: Option<&BTreeSet<u8>>,
        differences: &BTreeMap<u8, String>,
        standard: bool,
        findings: &mut Vec<AuditFinding>,
    ) {
        let readable = |name: &str| {
            fepdf_font::agl::in_glyph_list(name)
                || fepdf_font::latin_names::SYMBOL_NAMES.contains(&name)
        };
        let (mut unnamed, mut unlisted) = (Vec::new(), BTreeSet::new());
        for code in shown.into_iter().flatten() {
            let name = differences.get(code).map(String::as_str).or_else(|| {
                standard
                    .then(|| {
                        fepdf_font::latin_names::STANDARD_ENCODING
                            .iter()
                            .find(|(c, _)| c == code)
                            .map(|(_, n)| *n)
                    })
                    .flatten()
            });
            match name {
                Some(name) if !readable(name) => {
                    unlisted.insert(name.to_owned());
                }
                Some(_) => {}
                None => unnamed.push(*code),
            }
        }
        if !unlisted.is_empty() {
            findings.push(broken("31-027", format!(
                "/{} has no /ToUnicode and shows glyphs named {unlisted:?}, in neither Adobe's list nor the Symbol font",
                self.name
            )));
        } else if !unnamed.is_empty() {
            findings.push(for_a_reader("31-027", format!(
                "/{} has no /ToUnicode and shows {} codes whose glyph names only its program knows — look at it",
                self.name, unnamed.len()
            )));
        }
    }
}

/// A TrueType font's embedded program (`/FontFile2`), decoded.
fn descendant_program(font: &Font<'_>, descriptor: &Object) -> Option<Vec<u8>> {
    let stream = entry(font.arena, descriptor, "FontFile2")?;
    font.doc.decode_stream(&stream).ok().map(|b| b.to_vec())
}
