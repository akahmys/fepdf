//! A form XObject with no `/Resources` of its own, given the ones its content names, from
//! what draws it (8.10.2; ROADMAP AC-1,
//! [ADR-0127](../../../../docs/adr/0127-a-form-is-given-the-resources-its-callers-lend-it.md)).
//!
//! **In 1.x a form may draw with its caller's resources**, and Table 93 has `/Resources`
//! optional. Errata Collection 3 makes it "sometimes required" and says that from 2.0 a
//! form XObject is to be independent of the content streams it appears in. So a form that
//! names a resource and holds none is given the entries it names, copied from the page or
//! form that draws it; one that names none is given an empty dictionary.
//!
//! **What is given is not recorded**, for the reason `ARCHITECTURE.md` §4.3 gives about
//! `/Info`: copying an entry from a caller, or giving an empty dictionary, loses nothing,
//! and a 1.x file whose forms draw with their caller's resources conforms — recording it
//! fired on every such file, PDF/A test files meant to pass among them (measured
//! 2026-10-12: 13 forms in 12 of 528 files, none ambiguous or missing).
//!
//! **Where it cannot be told what a name meant, the form is left as it was** and that is
//! recorded: two callers that lend the same name for different objects, or a name no
//! caller has. Choosing one would be a guess, and a form given part of what it names would
//! draw less than it does now. Left as it was, it still draws with its caller's resources.

use crate::Document;
use crate::handle::DictHandle;
use crate::interpretation::Decision;
use crate::object::{Object, PdfName};
use std::collections::{BTreeMap, BTreeSet};

/// A resource category of Table 34, and the names in it.
type Names = BTreeMap<&'static str, BTreeSet<Vec<u8>>>;

/// For one form: each category and name it uses, and every object its callers lend for it.
type Lent = BTreeMap<(&'static str, Vec<u8>), Vec<Object>>;

/// How deep a chain of forms drawing forms is followed (Rule 6).
const MAX_DEPTH: usize = 32;

/// Gives each form XObject the document reaches without `/Resources` the ones its content
/// names, as the module says, and records what was given and what could not be.
pub fn give_forms_resources(doc: &Document) {
    let arena = doc.arena();
    let mut lent: BTreeMap<DictHandle, Lent> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut walk: Vec<(Vec<u8>, Option<DictHandle>, usize)> = Vec::new();
    for &page in &doc.pages {
        let Some(dict) = arena.get_object(page).and_then(|o| o.as_dict_handle()) else { continue };
        // Through the page tree, as `require_page_resources` reads them: a page the
        // push-down did not reach (a tree past its depth, a node that does not resolve)
        // holds its resources in an ancestor.
        let resources = crate::Page::new(arena, page, doc.get_parent_chain(page))
            .resolve_attribute("Resources")
            .and_then(|r| r.resolve(arena).as_dict_handle());
        walk.push((page_content(doc, dict), resources, 0));
    }
    while let Some((content, resources, depth)) = walk.pop() {
        for name in names_used(&content).remove("XObject").unwrap_or_default() {
            let Some((form, stream)) =
                lookup(doc, resources, "XObject", &name).and_then(|f| form_of(doc, &f))
            else {
                continue;
            };
            let own = entry_dict(doc, form, "Resources");
            let body = stream_bytes(doc, &stream);
            if own.is_none() {
                note_lent(doc, lent.entry(form).or_default(), &body, resources);
            }
            let drawn_with = own.or(resources);
            if depth < MAX_DEPTH && seen.insert((form, drawn_with)) {
                walk.push((body, drawn_with, depth + 1));
            }
        }
    }
    settle(doc, &lent);
}

/// Records, for each name `body` uses, what `resources` lends for it.
fn note_lent(doc: &Document, lent: &mut Lent, body: &[u8], resources: Option<DictHandle>) {
    for (category, names) in names_used(body) {
        for name in names {
            let slot = lent.entry((category, name.clone())).or_default();
            if let Some(value) = lookup(doc, resources, category, &name)
                && !slot.contains(&value)
            {
                slot.push(value);
            }
        }
    }
}

/// Gives every form without `/Resources` what [`give_forms_resources`] found for it, where
/// that is unambiguous and whole, and records the forms left as they were.
fn settle(doc: &Document, lent: &BTreeMap<DictHandle, Lent>) {
    let arena = doc.arena();
    let (mut ambiguous, mut missing) = (0, 0);
    for index in 0..arena.object_count() {
        let Some(stream) = arena.get_object(crate::handle::Handle::new(index)) else { continue };
        let Some((form, stream)) = form_of(doc, &stream) else { continue };
        if arena.dict_entry(form, arena.name("Resources")).is_some() {
            continue;
        }
        let uses = names_used(&stream_bytes(doc, &stream));
        if uses.values().all(BTreeSet::is_empty) {
            put_resources(doc, form, BTreeMap::new());
            continue;
        }
        let found = lent.get(&form);
        let lent_for = |key: &(&'static str, Vec<u8>)| found.and_then(|l| l.get(key));
        let keys: Vec<_> =
            uses.iter().flat_map(|(c, ns)| ns.iter().map(move |n| (*c, n.clone()))).collect();
        // Counted apart: one form may have both a name lent twice and one lent by none.
        let is_ambiguous = keys.iter().any(|k| lent_for(k).is_some_and(|v| v.len() > 1));
        let is_missing = keys.iter().any(|k| lent_for(k).is_none_or(Vec::is_empty));
        ambiguous += usize::from(is_ambiguous);
        missing += usize::from(is_missing);
        if !is_ambiguous && !is_missing {
            let mut built: BTreeMap<&'static str, Vec<(Vec<u8>, Object)>> = BTreeMap::new();
            for key in &keys {
                if let Some(value) = lent_for(key).and_then(|v| v.first()) {
                    built.entry(key.0).or_default().push((key.1.clone(), value.clone()));
                }
            }
            put_resources(doc, form, built);
        }
    }
    record(doc, ambiguous, missing);
}

/// One `Decision` for each way a form was left as it was.
fn record(doc: &Document, ambiguous: usize, missing: usize) {
    if ambiguous > 0 {
        doc.record(Decision::ambiguity(
            "8.10.2",
            format!(
                "{ambiguous} form XObjects hold no /Resources, and the pages or forms that \
                 draw them lend one name for different objects"
            ),
            "left them as they were: choosing one would be a guess, and they draw with their caller's",
        ));
    }
    if missing > 0 {
        doc.record(Decision::violation(
            "8.10.2",
            format!(
                "{missing} form XObjects hold no /Resources and name a resource nothing that \
                 draws them has"
            ),
            "left them as they were, drawing with their caller's",
        ));
    }
}

/// Sets `form`'s `/Resources` to `built`, a dictionary of category dictionaries.
fn put_resources(
    doc: &Document,
    form: DictHandle,
    built: BTreeMap<&'static str, Vec<(Vec<u8>, Object)>>,
) {
    let arena = doc.arena();
    let mut resources = BTreeMap::new();
    for (category, entries) in built {
        let inner = entries
            .into_iter()
            .map(|(name, value)| (arena.intern_name(PdfName::from_bytes(&name)), value))
            .collect();
        resources.insert(arena.name(category), Object::Dictionary(arena.alloc_dict(inner)));
    }
    let Some(mut dict) = arena.get_dict(form) else { return };
    dict.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(resources)));
    arena.set_dict(form, dict);
}

/// The names a content stream uses, by the category of Table 34 each operator looks in.
///
/// **Read a token at a time, and an inline image passed over as the sublimator passes
/// over one** (`Sublimator::parse_inline_image`): its samples are not syntax, and the
/// bytes ` EI` among them do not end it where its size says how long it is. Only the
/// operands [`named`] reads are kept, so a run of operands with no operator holds three.
fn names_used(content: &[u8]) -> Names {
    use fepdf_syntax::lexer::{Lexer, Token};
    let fonts = BTreeMap::new();
    let images = crate::object::sublimation::parser::Sublimator::new(&fonts);
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(content));
    let mut found = Names::new();
    let mut operands = Operands::default();
    loop {
        match lexer.next_token() {
            Ok(Token::EOF) | Err(_) => break,
            Ok(Token::Keyword(op)) => {
                if op == "BI" {
                    images.parse_inline_image(&mut lexer);
                } else if let Some((category, name)) = named(&op, &operands) {
                    found.entry(category).or_default().insert(name);
                }
                operands = Operands::default();
            }
            Ok(token) => operands.push(token),
        }
    }
    found
}

/// The operands before an operator that [`named`] reads: the first, the second and the
/// last. Kept rather than all of them, which a stream of operands and no operator would
/// grow without bound.
#[derive(Default)]
struct Operands {
    first: Option<fepdf_syntax::lexer::Token>,
    second: Option<fepdf_syntax::lexer::Token>,
    last: Option<fepdf_syntax::lexer::Token>,
}

impl Operands {
    fn push(&mut self, token: fepdf_syntax::lexer::Token) {
        if self.first.is_none() {
            self.first = Some(token.clone());
        } else if self.second.is_none() {
            self.second = Some(token.clone());
        }
        self.last = Some(token);
    }
}

/// The category and name the operator `op` looks up, given its operands.
fn named(op: &str, operands: &Operands) -> Option<(&'static str, Vec<u8>)> {
    use fepdf_syntax::lexer::Token;
    let name = |token: &Option<Token>| match token {
        Some(Token::Name(n)) => Some(n.to_vec()),
        _ => None,
    };
    let last = name(&operands.last);
    match op {
        "Tf" => Some(("Font", name(&operands.first)?)),
        "Do" => Some(("XObject", last?)),
        "gs" => Some(("ExtGState", last?)),
        "sh" => Some(("Shading", last?)),
        "scn" | "SCN" => Some(("Pattern", last?)),
        "BDC" | "DP" => Some(("Properties", name(&operands.second)?)),
        "cs" | "CS" => {
            let name = last?;
            let family = [&b"DeviceGray"[..], b"DeviceRGB", b"DeviceCMYK", b"Pattern"];
            (!family.contains(&name.as_slice())).then_some(("ColorSpace", name))
        }
        _ => None,
    }
}

/// What `resources` holds under `category` for `name`.
fn lookup(
    doc: &Document,
    resources: Option<DictHandle>,
    category: &str,
    name: &[u8],
) -> Option<Object> {
    let arena = doc.arena();
    let inner = entry_dict(doc, resources?, category)?;
    arena.dict_entry(inner, arena.intern_name(PdfName::from_bytes(name)))
}

/// The form XObject `object` is or refers to: its dictionary, and the stream.
fn form_of(doc: &Document, object: &Object) -> Option<(DictHandle, Object)> {
    let stream = object.resolve(doc.arena());
    let Object::Stream(handle, _) = stream else { return None };
    (name_of(doc, handle, "Subtype").as_deref() == Some("Form")).then_some((handle, stream))
}

/// The dictionary at `key` of `holder`, resolved.
fn entry_dict(doc: &Document, holder: DictHandle, key: &str) -> Option<DictHandle> {
    let arena = doc.arena();
    arena.dict_entry(holder, arena.name(key))?.resolve(arena).as_dict_handle()
}

/// The name at `key` of `holder`.
fn name_of(doc: &Document, holder: DictHandle, key: &str) -> Option<String> {
    let arena = doc.arena();
    arena
        .dict_entry(holder, arena.name(key))?
        .resolve(arena)
        .as_name()
        .and_then(|n| arena.get_name_str(n))
}

/// The decoded bytes of `stream`.
fn stream_bytes(doc: &Document, stream: &Object) -> Vec<u8> {
    doc.decode_stream(stream).map(|b| b.to_vec()).unwrap_or_default()
}

/// A page's content, its streams joined as 7.8.2 joins them.
fn page_content(doc: &Document, page: DictHandle) -> Vec<u8> {
    let arena = doc.arena();
    let streams = match arena.dict_entry(page, arena.name("Contents")) {
        Some(Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        Some(other) => match other.resolve(arena) {
            Object::Array(a) => arena.get_array(a).unwrap_or_default(),
            _ => vec![other],
        },
        None => Vec::new(),
    };
    let mut joined = Vec::new();
    for stream in streams {
        if let Ok(bytes) = doc.decode_stream(&stream.resolve(arena)) {
            joined.extend_from_slice(&bytes);
            joined.push(b'\n');
        }
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::names_used;

    fn used(content: &[u8], category: &str) -> Vec<String> {
        names_used(content)
            .remove(category)
            .unwrap_or_default()
            .into_iter()
            .map(|n| String::from_utf8_lossy(&n).into_owned())
            .collect()
    }

    /// `cs` and `CS` name a colour space resource, and a family name is not one.
    #[test]
    fn a_colour_space_is_named_unless_it_is_a_family() {
        let content = b"/CS1 cs /CS2 CS /DeviceRGB cs /DeviceCMYK CS /DeviceGray cs /Pattern cs";
        assert_eq!(used(content, "ColorSpace"), ["CS1", "CS2"]);
    }

    /// `sh` names a shading, and `scn`/`SCN` a pattern only when a name ends their operands.
    #[test]
    fn shadings_and_patterns_are_named() {
        assert_eq!(used(b"/Sh1 sh", "Shading"), ["Sh1"]);
        assert_eq!(used(b"/P1 scn 0.5 0.5 0.5 /P2 SCN 1 0 0 scn", "Pattern"), ["P1", "P2"]);
    }

    /// `BDC` and `DP` name a property list by their second operand; one written in place
    /// names none.
    #[test]
    fn a_property_list_is_named_by_the_second_operand() {
        let content = b"/Span /MC0 BDC EMC /Tag /Pr1 DP /Span << /MCID 0 >> BDC EMC";
        assert_eq!(used(content, "Properties"), ["MC0", "Pr1"]);
    }

    /// **An inline image's samples are not read as operators**, even where they hold
    /// ` EI ` and something that looks like `Tf`: twelve one-byte samples, then `EI`.
    #[test]
    fn an_inline_images_samples_are_passed_over() {
        let content = b"BI /W 12 /H 1 /BPC 8 /CS /G ID  EI /Fx Tf  \nEI\nBT /F1 12 Tf ET";
        assert_eq!(used(content, "Font"), ["F1"]);
    }
}
