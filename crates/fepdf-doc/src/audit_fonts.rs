//! Matterhorn failure conditions about font dictionaries and embedded programs (W-21k).
//!
//! **What ingestion leaves alone.** It rewrites a Type 0 font's `/Encoding` and fills a
//! missing `/CIDToGIDMap` (`refine::font`), which is why 31-005 to 31-008 are not here; a
//! TrueType program it rebuilds for drawing is kept beside the font, not written over the
//! file's. The simple fonts' dictionaries, the file's own programs and their `/ToUnicode`
//! maps are read as the file wrote them.

use crate::structure::{AuditFinding, broken, for_a_reader};
use fepdf_model::object::sublimation::{Command, TextArrayItem};
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// The failure conditions this module decides.
pub const FROM_FONTS: [&str; 15] = [
    "31-004", "31-009", "31-017", "31-018", "31-019", "31-020", "31-021", "31-022", "31-023",
    "31-024", "31-025", "31-026", "31-027", "31-028", "31-029",
];

/// What the pages do with one font: the codes they show in it, and those of them that are
/// rendered — in a mode other than 3, the one ISO 14289-1 7.21.4.1 NOTE 2 exempts, since
/// its glyphs are neither stroked, filled nor used to clip.
#[derive(Default)]
struct Use {
    codes: BTreeSet<u8>,
    rendered: BTreeSet<u8>,
}

/// How deep form XObjects are followed for the fonts they use (Rule 6).
pub(crate) const FORM_DEPTH: usize = 8;

/// Asks [`FROM_FONTS`] of every font the pages and the forms they draw name.
pub fn audit_fonts(
    doc: &Document,
    findings: &mut Vec<AuditFinding>,
    examined: &mut BTreeSet<&'static str>,
) {
    let arena = doc.arena();
    let Ok(pages) = doc.page_count() else { return };
    let mut fonts = BTreeSet::new();
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let resources =
            fepdf_model::Page::new(arena, handle, doc.get_parent_chain(handle)).resources_handle();
        collect_fonts(arena, &Object::Dictionary(resources), 0, &mut fonts);
    }
    // Only the fonts 31-027 asks about glyph by glyph cost a pass over the content.
    let asked: BTreeSet<Handle<Object>> = fonts
        .iter()
        .copied()
        .filter(|font| {
            let font = Font { doc, arena, handle: *font, name: String::new() };
            font.names_its_glyphs()
                || font.lacks_program()
                || font.non_symbolic_true_type().is_some()
        })
        .collect();
    let codes = if asked.is_empty() { BTreeMap::default() } else { shown_codes(doc, &asked) };
    for font in fonts {
        let reading = Font { doc, arena, handle: font, name: base_font(arena, font) };
        reading.audit(findings);
        let used = codes.get(&font);
        reading.to_unicode_needed(used.map(|u| &u.codes), findings);
        let rendered = used.map(|u| &u.rendered).filter(|r| !r.is_empty());
        reading.drawn_without(rendered.is_some(), findings);
        if let Some(rendered) = rendered {
            reading.looked_up(rendered, findings);
        }
    }
    examined.extend(FROM_FONTS);
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
    fn entry(&self, of: &Object, key: &str) -> Option<Object> {
        let dict = of.resolve(self.arena).as_dict_handle()?;
        self.arena.dict_entry(dict, self.arena.name(key)).map(|v| v.resolve(self.arena))
    }

    fn name_of(&self, of: &Object, key: &str) -> Option<String> {
        let name = self.entry(of, key)?.as_name()?;
        self.arena.get_name(name).map(|n| n.as_str().to_string())
    }

    fn audit(&self, findings: &mut Vec<AuditFinding>) {
        let font = Object::Reference(self.handle);
        self.to_unicode(&font, findings);
        match self.name_of(&font, "Subtype").as_deref() {
            Some("TrueType") => self.true_type(&font, findings),
            Some("Type0") => {
                let descendants = match self.entry(&font, "DescendantFonts") {
                    Some(Object::Array(array)) => self.arena.get_array(array).unwrap_or_default(),
                    _ => Vec::new(),
                };
                for descendant in descendants {
                    self.cid_to_gid_map(&descendant, findings);
                }
            }
            _ => {}
        }
    }

    /// 31-004: a `CIDFontType2`'s `/CIDToGIDMap` is a stream or `/Identity`.
    fn cid_to_gid_map(&self, descendant: &Object, findings: &mut Vec<AuditFinding>) {
        if self.name_of(descendant, "Subtype").as_deref() != Some("CIDFontType2") {
            return;
        }
        let fine = match self.entry(descendant, "CIDToGIDMap") {
            Some(Object::Stream(..)) | None => true,
            Some(other) => other
                .as_name()
                .and_then(|n| self.arena.get_name(n))
                .is_some_and(|n| n.as_str() == "Identity"),
        };
        if !fine {
            findings.push(broken(
                "31-004",
                format!(
                    "/{}: its CIDFontType2's /CIDToGIDMap is neither a stream nor /Identity",
                    self.name
                ),
            ));
        }
    }

    /// 31-019 to 31-021 and 31-023 to 31-026, for a simple TrueType font.
    fn true_type(&self, font: &Object, findings: &mut Vec<AuditFinding>) {
        let Some(descriptor) = self.entry(font, "FontDescriptor") else { return };
        let Some(flags) = self.entry(&descriptor, "Flags").and_then(|f| f.as_f64()) else { return };
        // Bit 3 of /Flags is Symbolic (Table 121).
        #[allow(clippy::cast_possible_truncation)] // a flag word, written as an integer
        let symbolic = (flags as i64) & 4 != 0;
        let tables = Self::embedded_cmaps(descendant_program(self, &descriptor).as_ref());
        let encoding = self.entry(font, "Encoding");
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
                match self.name_of(&dict, "BaseEncoding") {
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
        let differences = self.entry(encoding, "Differences").is_some();
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
        let Some(stream @ Object::Stream(..)) = self.entry(font, "ToUnicode") else { return };
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

/// A resource dictionary, by handle.
pub(crate) type Resources = Handle<BTreeMap<Handle<fepdf_model::PdfName>, Object>>;

/// What the graphics state carries into a form and back out of `q`…`Q`: the font in force,
/// if it is one being asked about, and whether text is rendered.
type TextState = (Option<Handle<Object>>, bool);

/// The codes each of `fonts` is shown with, and whether visibly, read from the content of
/// the pages and of the forms they draw (to `FORM_DEPTH`): which font each `Tf` selects and
/// which rendering mode each `Tr`, through `q` and `Q`, and the bytes each `Tj`, `'`, `"`
/// and `TJ` shows.
fn shown_codes(doc: &Document, fonts: &BTreeSet<Handle<Object>>) -> BTreeMap<Handle<Object>, Use> {
    let mut scan = Scan { doc, fonts, shown: BTreeMap::default() };
    let Ok(pages) = doc.page_count() else { return scan.shown };
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let resources = fepdf_model::Page::new(doc.arena(), handle, doc.get_parent_chain(handle))
            .resources_handle();
        if let Ok(Some(content)) = crate::apply::text::page_content(doc, page) {
            scan.content(resources, &content, (None, true), 0);
        }
    }
    scan.shown
}

/// One pass over content for what it does with `fonts`.
struct Scan<'a> {
    doc: &'a Document,
    fonts: &'a BTreeSet<Handle<Object>>,
    shown: BTreeMap<Handle<Object>, Use>,
}

impl Scan<'_> {
    /// Reads one content stream, drawn with `resources` and begun in `state`. Content that
    /// reaches none of the fonts, directly or through its forms, is not parsed.
    fn content(&mut self, resources: Resources, content: &[u8], state: TextState, depth: usize) {
        let arena = self.doc.arena();
        let mut reached = BTreeSet::new();
        collect_fonts(arena, &Object::Dictionary(resources), depth, &mut reached);
        if reached.is_disjoint(self.fonts) {
            return;
        }
        let named = names_in(arena, resources, "Font");
        let commands = parse(self.doc, &named, content);
        self.commands(&commands, resources, &named, state, depth);
    }

    /// What `commands` shows in each font, and the forms it draws, followed in turn.
    fn commands(
        &mut self,
        commands: &[Command],
        resources: Resources,
        named: &BTreeMap<String, Handle<Object>>,
        mut current: TextState,
        depth: usize,
    ) {
        use fepdf_model::graphics::TextRenderingMode;
        let mut saved = Vec::new();
        for command in commands {
            let bytes: Vec<&[u8]> = match command {
                Command::PushState => {
                    saved.push(current);
                    continue;
                }
                Command::PopState => {
                    current = saved.pop().unwrap_or(current);
                    continue;
                }
                Command::SetFont { font, .. } => {
                    current.0 = named.get(font).copied().filter(|h| self.fonts.contains(h));
                    continue;
                }
                Command::SetTextRenderMode(mode) => {
                    current.1 = *mode != TextRenderingMode::Invisible;
                    continue;
                }
                Command::DrawXObject(name) => {
                    self.form(resources, name, current, depth);
                    continue;
                }
                Command::ShowText(text) => vec![text.as_ref()],
                Command::ShowTextArray(items) => items
                    .iter()
                    .filter_map(|item| match item {
                        TextArrayItem::Text(text) => Some(text.as_ref()),
                        TextArrayItem::Offset(_) => None,
                    })
                    .collect(),
                _ => continue,
            };
            if let (Some(font), visible) = current {
                self.note(font, visible, bytes.into_iter().flatten().copied());
            }
        }
    }

    /// Records the codes one text-showing operator shows in `font`, and whether rendered.
    fn note(&mut self, font: Handle<Object>, visible: bool, codes: impl Iterator<Item = u8>) {
        let used = self.shown.entry(font).or_default();
        let codes: Vec<u8> = codes.collect();
        if visible {
            used.rendered.extend(&codes);
        }
        used.codes.extend(codes);
    }

    /// The form `name` names in `resources`, read in the state that draws it. A form with no
    /// `/Resources` of its own takes the ones it is drawn with (7.8.3).
    fn form(&mut self, resources: Resources, name: &str, state: TextState, depth: usize) {
        let arena = self.doc.arena();
        let Some(form) = names_in(arena, resources, "XObject").get(name).copied() else { return };
        let Some(own) = form_resources(arena, form, resources) else { return };
        if depth >= FORM_DEPTH {
            return;
        }
        let mut reached = BTreeSet::new();
        collect_fonts(arena, &Object::Dictionary(own), depth + 1, &mut reached);
        if reached.is_disjoint(self.fonts) {
            return;
        }
        let named = names_in(arena, own, "Font");
        if let Some(commands) = form_commands(self.doc, form, &named) {
            self.commands(&commands, own, &named, state, depth + 1);
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
) -> Option<Vec<Command>> {
    let object = doc.arena().get_object(form)?;
    if let Object::Stream(_, data) = &object
        && let fepdf_model::object::SublimatedData::Commands { items } = data.as_ref()
    {
        return Some(items.clone());
    }
    let content = doc.decode_stream(&object).ok()?;
    Some(parse(doc, named, &content))
}

/// Content bytes parsed into commands, with the fonts `named` loaded to read its text by.
pub(crate) fn parse(
    doc: &Document,
    named: &BTreeMap<String, Handle<Object>>,
    content: &[u8],
) -> Vec<Command> {
    let loaded: BTreeMap<String, _> = named
        .iter()
        .filter_map(|(name, font)| Some((name.clone(), doc.get_font(*font).ok()?)))
        .collect();
    fepdf_model::object::sublimation::parser::Sublimator::new(&loaded).sublimate(content)
}

/// The entries of one category of `resources` — `/Font` or `/XObject` — by resource name.
pub(crate) fn names_in(
    arena: &PdfArena,
    resources: Resources,
    category: &str,
) -> BTreeMap<String, Handle<Object>> {
    arena
        .dict_entry(resources, arena.name(category))
        .and_then(|f| f.resolve(arena).as_dict_handle())
        .and_then(|f| arena.get_dict(f))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| {
            Some((arena.get_name(key)?.as_str().to_string(), value.as_reference()?))
        })
        .collect()
}

/// The Latin standard 14 fonts, whose built-in encoding is StandardEncoding (9.6.2.2).
const STANDARD_LATIN: [&str; 12] = [
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
        if self.name_of(&font, "Subtype").as_deref() == Some("Type0") {
            let Some(Object::Array(descendants)) = self.entry(&font, "DescendantFonts") else {
                return None;
            };
            let descendant =
                self.arena.get_array(descendants).unwrap_or_default().into_iter().next()?;
            return self.entry(&descendant, "FontDescriptor");
        }
        self.entry(&font, "FontDescriptor")
    }

    /// Whether the font has a program to draw with and none is embedded — Type 3 fonts
    /// draw with content streams and have none to embed (9.6.5).
    fn lacks_program(&self) -> bool {
        let font = Object::Reference(self.handle);
        if self.name_of(&font, "Subtype").as_deref() == Some("Type3") {
            return false;
        }
        let Some(descriptor) = self.descriptor() else { return true };
        !["FontFile", "FontFile2", "FontFile3"]
            .iter()
            .any(|key| self.entry(&descriptor, key).is_some())
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
        if self.name_of(&font, "Subtype").as_deref() != Some("TrueType") {
            return None;
        }
        let descriptor = self.entry(&font, "FontDescriptor")?;
        let flags = self.entry(&descriptor, "Flags").and_then(|f| f.as_f64());
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
            self.true_type_names(&Object::Reference(self.handle)),
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

    /// The code-to-name table 9.6.6.4 builds for a non-symbolic TrueType font: a named
    /// encoding's Annex D names, or a dictionary's base, its `/Differences` over it, and
    /// StandardEncoding for what is left. An `/Encoding` naming anything else, or none,
    /// has no table (31-019 and 31-021 say so).
    fn true_type_names(&self, font: &Object) -> Option<BTreeMap<u8, String>> {
        let annex = |name: &str| -> Option<BTreeMap<u8, String>> {
            let table: &[(u8, &str)] = match name {
                "WinAnsiEncoding" => &crate::annex_d::WIN_ANSI,
                "MacRomanEncoding" => &crate::annex_d::MAC_ROMAN,
                _ => return None,
            };
            Some(table.iter().map(|(code, name)| (*code, (*name).to_owned())).collect())
        };
        let encoding = self.entry(font, "Encoding")?;
        if let Some(name) = encoding.as_name().and_then(|n| self.arena.get_name(n)) {
            return annex(name.as_str());
        }
        let mut table = match self.name_of(&encoding, "BaseEncoding") {
            Some(base) => annex(&base)?,
            None => BTreeMap::new(),
        };
        table.extend(self.differences(&encoding));
        for (code, name) in crate::annex_d::STANDARD_ENCODING {
            table.entry(code).or_insert_with(|| name.to_owned());
        }
        Some(table)
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

    /// Whether 31-027 turns on the names of the glyphs this font's text shows: a Type 1
    /// or Type 3 font with no `/ToUnicode` and no Latin encoding named.
    fn names_its_glyphs(&self) -> bool {
        let font = Object::Reference(self.handle);
        let simple = matches!(
            self.name_of(&font, "Subtype").as_deref(),
            Some("Type1" | "MMType1" | "Type3")
        );
        simple && self.entry(&font, "ToUnicode").is_none() && !self.latin_encoding(&font)
    }

    /// Whether the font's `/Encoding`, or its `/BaseEncoding`, is one 31-027 accepts.
    fn latin_encoding(&self, font: &Object) -> bool {
        let named = self.entry(font, "Encoding").and_then(|e| {
            e.as_name()
                .and_then(|n| self.arena.get_name(n))
                .map(|n| n.as_str().to_string())
                .or_else(|| self.name_of(&e, "BaseEncoding"))
        });
        named.as_deref().is_some_and(|n| {
            ["MacRomanEncoding", "MacExpertEncoding", "WinAnsiEncoding"].contains(&n)
        })
    }

    /// A simple font's `/Differences`, as code to name.
    fn differences(&self, encoding: &Object) -> BTreeMap<u8, String> {
        let mut names = BTreeMap::new();
        let Some(Object::Array(array)) = self.entry(encoding, "Differences") else { return names };
        let mut code: Option<u8> = None;
        for item in self.arena.get_array(array).unwrap_or_default() {
            match item.resolve(self.arena) {
                Object::Integer(start) => code = u8::try_from(start).ok(),
                other => {
                    let (Some(at), Some(name)) =
                        (code, other.as_name().and_then(|n| self.arena.get_name(n)))
                    else {
                        continue;
                    };
                    names.insert(at, name.as_str().to_string());
                    code = at.checked_add(1);
                }
            }
        }
        names
    }

    /// 31-027: a font with no `/ToUnicode` is one whose text can be read without it —
    /// by a named Latin encoding, by glyph names Adobe's list or the Symbol font names,
    /// by one of Adobe's four CJK collections, or by being a non-symbolic TrueType font.
    fn to_unicode_needed(&self, shown: Option<&BTreeSet<u8>>, findings: &mut Vec<AuditFinding>) {
        let font = Object::Reference(self.handle);
        if self.entry(&font, "ToUnicode").is_some() {
            return;
        }
        let encoding = self.entry(&font, "Encoding");
        let named = encoding.as_ref().and_then(|e| {
            e.as_name()
                .and_then(|n| self.arena.get_name(n))
                .map(|n| n.as_str().to_string())
                .or_else(|| self.name_of(e, "BaseEncoding"))
        });
        if self.latin_encoding(&font) {
            return;
        }
        let say =
            |what: String| broken("31-027", format!("/{} has no /ToUnicode and {what}", self.name));
        match self.name_of(&font, "Subtype").as_deref() {
            Some("TrueType") => {
                let flags = self
                    .entry(&font, "FontDescriptor")
                    .and_then(|d| self.entry(&d, "Flags"))
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
        let Some(Object::Array(descendants)) = self.entry(font, "DescendantFonts") else {
            return false;
        };
        let Some(descendant) =
            self.arena.get_array(descendants).unwrap_or_default().into_iter().next()
        else {
            return false;
        };
        let Some(info) = self.entry(&descendant, "CIDSystemInfo") else { return false };
        let text = |key: &str| match self.entry(&info, key) {
            Some(Object::String(b) | Object::Hex(b)) => String::from_utf8_lossy(&b).into_owned(),
            Some(Object::Text(t)) => t,
            _ => String::new(),
        };
        text("Registry") == "Adobe"
            && ["GB1", "CNS1", "Japan1", "Korea1"].contains(&text("Ordering").as_str())
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
            fepdf_font::agl::in_glyph_list(name) || crate::annex_d::SYMBOL_NAMES.contains(&name)
        };
        let (mut unnamed, mut unlisted) = (Vec::new(), BTreeSet::new());
        for code in shown.into_iter().flatten() {
            let name = differences.get(code).map(String::as_str).or_else(|| {
                standard
                    .then(|| {
                        crate::annex_d::STANDARD_ENCODING
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
    let stream = font.entry(descriptor, "FontFile2")?;
    font.doc.decode_stream(&stream).ok().map(|b| b.to_vec())
}
