//! Which marked content lies in a `<Formula>` (17-003, UA1:7.7).
//!
//! **Through the parent tree, as a reader finds it.** A sequence's `/MCID` indexes the array
//! its content stream's `/StructParents` names (14.7.4.4), and the element there — or one of
//! its ancestors, through `/P` — is the `<Formula>` whose text 7.7 holds to 9.10.2.

use crate::audit_objects::{entry, name_of};
use fepdf_model::object::sublimation::IrObject;
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// How far up `/P` an element's `<Formula>` is looked for (Rule 6).
const ANCESTORS: usize = 256;

/// The document's `/ParentTree` arrays and role map, for asking of an MCID.
pub(crate) struct Formulas<'a> {
    arena: &'a PdfArena,
    arrays: BTreeMap<i64, Vec<Object>>,
    roles: BTreeMap<String, String>,
}

impl<'a> Formulas<'a> {
    /// The document's, when it has a structure tree with a `<Formula>` in its role map's
    /// reach — and nothing otherwise, which spares every page the question.
    pub(crate) fn of(doc: &'a Document) -> Option<Self> {
        let arena = doc.arena();
        let root = doc.get_structure_root().ok().flatten()?;
        let arrays = crate::parent_tree::array_entries(arena, root);
        let roles = crate::audit_tree::role_map(arena, root);
        let formulas = Self { arena, arrays, roles };
        let any = formulas.arrays.values().flatten().any(|e| formulas.is_formula(e));
        any.then_some(formulas)
    }

    /// The MCIDs of the content stream whose `/StructParents` is `key` that lie in a
    /// `<Formula>`.
    pub(crate) fn mcids(&self, key: i64) -> BTreeSet<i64> {
        self.arrays
            .get(&key)
            .map(|elements| {
                elements
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| self.is_formula(e))
                    .filter_map(|(i, _)| i64::try_from(i).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether `element`, or an ancestor, stands for a `<Formula>`.
    fn is_formula(&self, element: &Object) -> bool {
        let mut at = element.clone();
        for _ in 0..ANCESTORS {
            let Some(tag) = name_of(self.arena, &at, "S") else { return false };
            if crate::audit_tree::standard_type(&self.roles, &tag).as_deref() == Some("Formula") {
                return true;
            }
            let Some(parent) = entry(self.arena, &at, "P") else { return false };
            at = parent;
        }
        false
    }
}

/// What a content stream needs to say whether a sequence is in a `<Formula>`.
#[derive(Default)]
pub(crate) struct Marks {
    /// The MCIDs that are.
    pub(crate) mcids: BTreeSet<i64>,
    /// Its `/Properties` resource: named property lists, which may carry the `/MCID`.
    pub(crate) properties: BTreeMap<String, Handle<Object>>,
    /// Whether the stream is drawn from inside one.
    pub(crate) within: bool,
}

impl Marks {
    /// Whether a sequence opened with `tag` and `properties`, inside a sequence that is in
    /// a `<Formula>` or not as `outer` says, is in one.
    pub(crate) fn opens(
        &self,
        arena: &PdfArena,
        properties: Option<&IrObject>,
        outer: bool,
    ) -> bool {
        if outer {
            return true;
        }
        let mcid = match properties {
            Some(IrObject::Dictionary(inline)) => match inline.get("MCID") {
                Some(IrObject::Integer(mcid)) => Some(*mcid),
                _ => None,
            },
            Some(IrObject::Name(name)) => self
                .properties
                .get(name)
                .and_then(|p| entry(arena, &Object::Reference(*p), "MCID"))
                .and_then(|m| m.as_integer()),
            _ => None,
        };
        mcid.is_some_and(|m| self.mcids.contains(&m))
    }
}
