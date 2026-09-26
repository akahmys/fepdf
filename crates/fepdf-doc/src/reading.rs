//! What a synthesiser is handed (ROADMAP W-19a).
//!
//! The text of a tagged document in the order its structure gives, each passage in its
//! language, with the lexicons that say how to pronounce it.
//!
//! **The order is the structure tree's, not the page's.** 14.8.2.3 makes the logical
//! structure the order content is meant to be read in, and a two-column page read in
//! drawing order interleaves its columns. Content no element claims — an artifact, or a
//! page nobody tagged — is not read, because nothing says where in the reading it goes.
//!
//! **What an element says in place of its content is read in place of its content.**
//! `/ActualText` is an exact replacement (14.9.4), `/Alt` a description of the element and
//! its children (14.9.3), `/E` the expansion of an abbreviation (14.9.5); each stands for
//! the whole element, so nothing below it is read as well. `/Phoneme` (14.9.6) is carried
//! beside the words rather than instead of them, since a synthesiser that does not read
//! the alphabet it names still has the words to fall back on.
//!
//! **Text is composed a passage at a time, by extraction's own composer.** The tree is
//! planned first — which marks each passage reads — and a page is then read once, each
//! glyph going to the composer of the passage its mark belongs to. So a passage is spaced
//! and ordered as `extract_text` would space and order it, including between its marks,
//! and reads nothing another passage's marks drew.

use crate::remediation::TextExtractionBackend;
use crate::struct_tree::{Part, StructureTreeNode};
use fepdf_content::{FallbackFontType, RenderBackend, SMaskData, TextGlyph, TextState};
use fepdf_model::graphics::{BlendMode, Color, PixelFormat, StrokeStyle, WindingRule};
use fepdf_model::{Document, Object};
use kurbo::{Affine, BezPath};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Where a passage's words came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spoken {
    /// The text its marked content draws.
    Content,
    /// Its `/ActualText` (14.9.4).
    ActualText,
    /// Its `/Alt` (14.9.3).
    Alt,
    /// Its `/E` (14.9.5).
    Expansion,
}

/// One stretch of the reading, in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passage {
    /// What is said.
    pub text: String,
    /// The natural language it is in (14.9.2): the element's own `/Lang`, or the nearest
    /// ancestor's, or the catalogue's. `None` when nothing declares one.
    pub lang: Option<String>,
    /// The structure type of the element it belongs to.
    pub tag: String,
    /// The page it is on, counting from zero, when the element names one.
    pub page: Option<usize>,
    /// Where the words came from.
    pub spoken: Spoken,
    /// How to pronounce it, when the element says (14.9.6): the alphabet, and the
    /// transcription in it.
    pub phoneme: Option<(String, String)>,
}

/// What a synthesiser needs to read a document aloud.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading {
    /// The pronunciation lexicons the structure tree root names (Table 354), as PLS XML,
    /// in the order it names them — which is the order a match is looked for in.
    pub lexicons: Vec<Vec<u8>>,
    /// The passages, in the order the structure tree gives.
    pub passages: Vec<Passage>,
}

/// How deep a structure tree is walked before the rest of it is not read.
///
/// The tree was read with a visited set, so it holds no cycle; this bounds a tree that is
/// merely very deep (Rule 6).
const DEPTH: usize = 256;

/// One passage of the reading, and the marks its words are drawn by, before the pages
/// are read.
struct Planned {
    passage: Passage,
    /// The marks it reads, in order, each with the page it is on. Empty for a passage
    /// whose words the element gave in place of its content.
    marks: Vec<(usize, u32)>,
}

/// Which passage reads what, for a tree whose pages have not been read yet.
pub struct Plan {
    planned: Vec<Planned>,
}

impl Plan {
    /// Plans the reading of the tree under `root`.
    #[must_use]
    pub fn of(root: &StructureTreeNode) -> Self {
        let mut plan = Self { planned: Vec::new() };
        plan.read(root, 0);
        plan
    }

    /// The pages a passage reads marks on.
    #[must_use]
    pub fn pages(&self) -> std::collections::BTreeSet<usize> {
        self.planned.iter().flat_map(|p| p.marks.iter().map(|(page, _)| *page)).collect()
    }

    /// For the marks on `page`, which passage each belongs to — what
    /// [`MarkTextBackend::new`] sorts that page's glyphs by.
    #[must_use]
    pub fn passages_on(&self, page: usize) -> BTreeMap<u32, usize> {
        let mut owners = BTreeMap::new();
        for (index, planned) in self.planned.iter().enumerate() {
            for (on, mcid) in &planned.marks {
                if *on == page {
                    owners.insert(*mcid, index);
                }
            }
        }
        owners
    }

    /// The passages, given what each page's backend composed for them.
    ///
    /// A passage that comes out saying nothing — a paragraph of spaces, or marks that drew
    /// no glyph — is left out.
    #[must_use]
    pub fn passages(self, composed: &BTreeMap<usize, BTreeMap<usize, String>>) -> Vec<Passage> {
        let mut out = Vec::new();
        for (index, mut planned) in self.planned.into_iter().enumerate() {
            if !planned.marks.is_empty() {
                let mut pages: Vec<usize> = planned.marks.iter().map(|(page, _)| *page).collect();
                pages.dedup();
                let words: Vec<&str> = pages
                    .iter()
                    .filter_map(|page| composed.get(page)?.get(&index).map(String::as_str))
                    .collect();
                words.join("\n").trim().clone_into(&mut planned.passage.text);
            }
            if !planned.passage.text.trim().is_empty() {
                out.push(planned.passage);
            }
        }
        out
    }

    /// Plans `node`, and what it holds after it.
    fn read(&mut self, node: &StructureTreeNode, depth: usize) {
        if depth > DEPTH {
            return;
        }
        let instead = [
            (&node.actual_text, Spoken::ActualText),
            (&node.alt_text, Spoken::Alt),
            (&node.expansion, Spoken::Expansion),
        ];
        if let Some((text, spoken)) =
            instead.into_iter().find_map(|(text, spoken)| text.clone().map(|t| (t, spoken)))
        {
            self.planned.push(Planned { passage: passage(node, text, spoken), marks: Vec::new() });
            return;
        }
        if node.phoneme.is_some() {
            // An exact replacement for the element and its children, so everything under
            // it is one passage rather than read as the children would be.
            let mut marks = Vec::new();
            gather(node, &mut marks, depth);
            self.planned
                .push(Planned { passage: passage(node, String::new(), Spoken::Content), marks });
            return;
        }
        let mut marks = Vec::new();
        for part in &node.order {
            match part {
                Part::Mark(mcid) => marks.extend(node.page_index.map(|page| (page, *mcid))),
                Part::Child(index) => {
                    self.flush(node, &mut marks);
                    if let Some(child) = node.children.get(*index) {
                        self.read(child, depth + 1);
                    }
                }
            }
        }
        self.flush(node, &mut marks);
    }

    /// Ends the passage of `node`'s marks under way, if it has any.
    fn flush(&mut self, node: &StructureTreeNode, marks: &mut Vec<(usize, u32)>) {
        if !marks.is_empty() {
            let marks = std::mem::take(marks);
            self.planned
                .push(Planned { passage: passage(node, String::new(), Spoken::Content), marks });
        }
    }
}

/// A passage of `node`'s, saying `text`.
fn passage(node: &StructureTreeNode, text: String, spoken: Spoken) -> Passage {
    Passage {
        text,
        lang: node.lang.clone(),
        tag: node.tag.clone(),
        page: node.page_index,
        spoken,
        phoneme: node.phoneme.clone().map(|p| (node.phonetic_alphabet.clone(), p)),
    }
}

/// Every mark under `node`, in order.
fn gather(node: &StructureTreeNode, marks: &mut Vec<(usize, u32)>, depth: usize) {
    if depth > DEPTH {
        return;
    }
    for part in &node.order {
        match part {
            Part::Mark(mcid) => marks.extend(node.page_index.map(|page| (page, *mcid))),
            Part::Child(index) => {
                if let Some(child) = node.children.get(*index) {
                    gather(child, marks, depth + 1);
                }
            }
        }
    }
}

/// The pronunciation lexicons the structure tree root names, in its order (Table 354).
///
/// Each is the embedded file its file specification carries, decoded. One that cannot be
/// read is left out rather than failing the reading: a lexicon is a hint (14.9.6 says a
/// processor need not use one), and the words are read without it.
#[must_use]
pub fn lexicons(doc: &Document) -> Vec<Vec<u8>> {
    let arena = doc.arena();
    let dict_of = |object: &Object| object.resolve(arena).as_dict_handle();
    let Some(root) = doc
        .catalog_handle()
        .and_then(|catalog| doc.resolve_to_dict(catalog).ok())
        .and_then(|catalog| arena.dict_entry(catalog, arena.name("StructTreeRoot")))
        .and_then(|root| dict_of(&root))
    else {
        return Vec::new();
    };
    let specs = match arena
        .dict_entry(root, arena.name("PronunciationLexicon"))
        .map(|l| l.resolve(arena))
    {
        Some(Object::Array(named)) => arena.get_array(named).unwrap_or_default(),
        Some(single @ Object::Dictionary(_)) => vec![single],
        _ => Vec::new(),
    };
    specs
        .iter()
        .filter_map(|spec| {
            let files = arena.dict_entry(dict_of(spec)?, arena.name("EF"))?;
            let files = dict_of(&files)?;
            let file = arena
                .dict_entry(files, arena.name("UF"))
                .or_else(|| arena.dict_entry(files, arena.name("F")))?;
            doc.decode_stream(&file.resolve(arena)).ok().map(|bytes| bytes.to_vec())
        })
        .collect()
}

/// A [`RenderBackend`] that composes the text of each passage of a page apart.
///
/// **One composer per passage, fed only what that passage's marks drew.** Each is
/// extraction's own, so its spacing and line order are extraction's; the matrix in force
/// is handed to a composer before each of its glyphs, because a composer made halfway
/// down a page has seen none of the `cm`s before it.
#[derive(Default)]
pub struct MarkTextBackend {
    /// Which passage each mark on this page belongs to; a mark in none is not read.
    owners: BTreeMap<u32, usize>,
    ctm: Affine,
    saved: Vec<Affine>,
    /// The marks open now, outermost first. Text goes to the innermost.
    open: Vec<u32>,
    composers: BTreeMap<usize, TextExtractionBackend>,
    /// Which passage's composer each open `/ActualText` went to, if any took it.
    actual: Vec<Option<usize>>,
    /// An `/ActualText` begun before the mark of the same `BDC` was opened.
    ///
    /// The interpreter begins a section's replacement text before it opens its mark, so
    /// the two arrive in that order for a `BDC` that carries both.
    pending: Option<String>,
    /// Whether the page's own content has been read.
    ///
    /// `render_page` hands the mark boxes over between the page's content and its
    /// annotations, and an annotation's appearance is a stream of its own whose `/MCID 0`
    /// is not the page's (14.7.4.2) — so what arrives after that is not read.
    finished: bool,
}

impl MarkTextBackend {
    /// A backend for a page whose marks belong to the passages `owners` names.
    #[must_use]
    pub fn new(owners: BTreeMap<u32, usize>) -> Self {
        Self { owners, ctm: Affine::IDENTITY, ..Self::default() }
    }

    /// The text each passage's marks drew, by passage.
    #[must_use]
    pub fn into_text(self) -> BTreeMap<usize, String> {
        self.composers.into_iter().map(|(owner, composer)| (owner, composer.finish())).collect()
    }

    /// The composer of the passage the innermost open mark belongs to, if one does.
    fn composer(&mut self) -> Option<(usize, &mut TextExtractionBackend)> {
        if self.finished {
            return None;
        }
        let owner = *self.owners.get(self.open.last()?)?;
        Some((owner, self.composers.entry(owner).or_default()))
    }
}

impl RenderBackend for MarkTextBackend {
    fn receive_mark_bounds(&mut self, _bounds: BTreeMap<u32, kurbo::Rect>) {
        self.finished = true;
    }

    fn mark_opened(&mut self, mcid: u32) {
        if self.finished {
            return;
        }
        self.open.push(mcid);
        if let Some(text) = self.pending.take()
            && let Some((owner, composer)) = self.composer()
        {
            composer.begin_actual_text(&text);
            if let Some(took) = self.actual.last_mut() {
                *took = Some(owner);
            }
        }
    }

    fn mark_closed(&mut self) {
        self.open.pop();
    }

    fn begin_actual_text(&mut self, text: &str) {
        if self.finished {
            return;
        }
        if let Some((owner, composer)) = self.composer() {
            composer.begin_actual_text(text);
            self.actual.push(Some(owner));
        } else {
            self.pending = Some(text.to_owned());
            self.actual.push(None);
        }
    }

    fn end_actual_text(&mut self) {
        self.pending = None;
        if let Some(Some(owner)) = self.actual.pop()
            && let Some(composer) = self.composers.get_mut(&owner)
        {
            composer.end_actual_text();
        }
    }

    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        size: f64,
        transform: Affine,
        state: TextState,
        op_index: Option<usize>,
    ) {
        let ctm = self.ctm;
        let Some((_, composer)) = self.composer() else { return };
        composer.set_transform(ctm);
        composer.show_text(glyphs, size, transform, state, op_index);
    }

    fn transform(&mut self, affine: Affine) {
        self.ctm *= affine;
    }
    fn set_transform(&mut self, affine: Affine) {
        self.ctm = affine;
    }
    fn push_state(&mut self) {
        self.saved.push(self.ctm);
    }
    fn pop_state(&mut self) {
        if let Some(ctm) = self.saved.pop() {
            self.ctm = ctm;
        }
    }

    // What remains draws, and a reading draws nothing.
    fn fill_path(&mut self, _path: &BezPath, _color: &Color, _rule: WindingRule) {}
    fn stroke_path(&mut self, _path: &BezPath, _color: &Color, _style: &StrokeStyle) {}
    fn push_clip(&mut self, _path: &BezPath, _rule: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn draw_image(
        &mut self,
        _data: &[u8],
        _width: u32,
        _height: u32,
        _format: PixelFormat,
        _smask: Option<SMaskData>,
    ) {
    }
    fn set_fill_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_alpha(&mut self, _alpha: f64) {}
    fn set_fill_color(&mut self, _color: Color) {}
    fn set_stroke_color(&mut self, _color: Color) {}
    fn set_blend_mode(&mut self, _mode: BlendMode) {}
    fn set_font(&mut self, _name: &str) {}
    fn set_text_render_mode(&mut self, _mode: fepdf_model::graphics::TextRenderingMode) {}
    fn set_char_spacing(&mut self, _spacing: f64) {}
    fn set_word_spacing(&mut self, _spacing: f64) {}
    fn define_font(
        &mut self,
        _name: &str,
        _base: Option<&str>,
        _data: Option<Arc<Vec<u8>>>,
        _index: Option<usize>,
        _cid_map: Option<BTreeMap<u32, u32>>,
        _fallback: FallbackFontType,
        _is_cid: bool,
    ) {
    }
}
