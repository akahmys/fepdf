//! What a marked sequence's content is: in a `<Formula>` (17-003), and in a language.
//!
//! **Through the parent tree, as a reader finds it.** A sequence's `/MCID` indexes the array
//! its content stream's `/StructParents` names (14.7.4.4), and the element there, with its
//! ancestors through `/P`, says whether the content is a `<Formula>`'s (UA1:7.7) and which
//! language it is in (ISO 32000-1 14.9.2.3). A `Span` sequence outside the structure states
//! its own language in its property list.

use fepdf_model::access::{entry, name_in};
use fepdf_model::object::sublimation::IrObject;
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::BTreeMap;

/// How far up `/P` an element's `<Formula>` or `/Lang` is looked for (Rule 6).
const ANCESTORS: usize = 256;

/// What a sequence's content is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // four independent facts: none excludes another
pub(crate) struct Facts {
    /// In a `<Formula>`.
    pub(crate) formula: bool,
    /// In a `<Figure>` (13-001).
    pub(crate) figure: bool,
    /// In a language a `/Lang` below the catalogue states, not empty.
    pub(crate) language: bool,
    /// In an `/Artifact`, which is not the document's content.
    pub(crate) artifact: bool,
}

/// The document's `/ParentTree` arrays and role map, for asking of an MCID.
pub(crate) struct Tree<'a> {
    arena: &'a PdfArena,
    arrays: BTreeMap<i64, Vec<Object>>,
    roles: BTreeMap<String, String>,
}

impl<'a> Tree<'a> {
    /// The document's, when it has a structure tree.
    pub(crate) fn of(doc: &'a Document) -> Option<Self> {
        let arena = doc.arena();
        let root = doc.get_structure_root().ok().flatten()?;
        let arrays = fepdf_doc::parent_tree::array_entries(arena, root);
        let roles = crate::audit_tree::role_map(arena, root);
        Some(Self { arena, arrays, roles })
    }

    /// What the content of each MCID of the stream whose `/StructParents` is `key` is.
    pub(crate) fn facts(&self, key: i64) -> BTreeMap<i64, Facts> {
        let Some(elements) = self.arrays.get(&key) else { return BTreeMap::new() };
        elements
            .iter()
            .enumerate()
            .filter_map(|(i, element)| Some((i64::try_from(i).ok()?, self.of_element(element))))
            .collect()
    }

    /// Whether `element`, or an ancestor, stands for a `<Formula>`; and whether the nearest
    /// `/Lang` among them states a language.
    fn of_element(&self, element: &Object) -> Facts {
        let mut facts = Facts::default();
        let mut language = None;
        let mut at = element.clone();
        for _ in 0..ANCESTORS {
            let Some(tag) = name_in(self.arena, &at, "S") else { break };
            match crate::audit_tree::standard_type(&self.roles, &tag).as_deref() {
                Some("Formula") => facts.formula = true,
                Some("Figure") => facts.figure = true,
                _ => {}
            }
            if language.is_none() {
                language = stated(self.arena, entry(self.arena, &at, "Lang"));
            }
            let Some(parent) = entry(self.arena, &at, "P") else { break };
            at = parent;
        }
        facts.language = language.unwrap_or(false);
        facts
    }
}

/// Whether a `/Lang` value states a language: `Some(false)` for the empty string, which
/// 14.9.2.2 says means the language is unknown; `None` where there is no `/Lang`.
pub(crate) fn stated(arena: &PdfArena, lang: Option<Object>) -> Option<bool> {
    match lang.map(|l| l.resolve(arena))? {
        Object::Text(text) => Some(!text.trim().is_empty()),
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(!fepdf_model::refine::text::recover_string(&bytes).trim().is_empty())
        }
        _ => Some(false),
    }
}

/// What a content stream needs to say what each of its sequences is.
#[derive(Default)]
pub(crate) struct Marks {
    /// Each MCID's facts.
    pub(crate) facts: BTreeMap<i64, Facts>,
    /// Its `/Properties` resource: named property lists, which may carry `/MCID` or `/Lang`.
    pub(crate) properties: BTreeMap<String, Handle<Object>>,
    /// What the content it is drawn from inside is.
    pub(crate) within: Facts,
}

impl Marks {
    /// What a sequence opened with `tag` and `properties`, inside content that is `outer`,
    /// is. **A sequence with an MCID takes its element's language**, which is the
    /// catalogue's where its elements state none; a `Span` outside the structure its own
    /// `/Lang`; any other sequence its surroundings'.
    pub(crate) fn opens(
        &self,
        arena: &PdfArena,
        tag: &str,
        properties: Option<&IrObject>,
        outer: Facts,
    ) -> Facts {
        let mut facts = outer;
        facts.artifact |= tag == "Artifact";
        let (mcid, lang) = match properties {
            Some(IrObject::Dictionary(inline)) => (
                match inline.get("MCID") {
                    Some(IrObject::Integer(mcid)) => Some(*mcid),
                    _ => None,
                },
                match inline.get("Lang") {
                    Some(IrObject::String(b) | IrObject::Hex(b)) => {
                        Some(!fepdf_model::refine::text::recover_string(b).trim().is_empty())
                    }
                    Some(_) => Some(false),
                    None => None,
                },
            ),
            Some(IrObject::Name(name)) => self.properties.get(name).map_or((None, None), |p| {
                let list = Object::Reference(*p);
                (
                    entry(arena, &list, "MCID").and_then(|m| m.as_integer()),
                    stated(arena, entry(arena, &list, "Lang")),
                )
            }),
            _ => (None, None),
        };
        match mcid {
            Some(mcid) => {
                let element = self.facts.get(&mcid).copied().unwrap_or_default();
                facts.formula |= element.formula;
                facts.figure |= element.figure;
                facts.language = element.language;
            }
            None if tag == "Span" => facts.language = lang.unwrap_or(facts.language),
            None => {}
        }
        facts
    }
}
