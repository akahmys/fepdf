//! 31-018: whether a non-symbolic TrueType font's rendered codes reach a glyph.
//!
//! Through the cmap lookups ISO 32000-1 9.6.6.4 describes, and nothing else.

use fepdf_font::sfnt_cmap::CmapSubtable;
use std::collections::{BTreeMap, BTreeSet};

/// The lookup 9.6.6.4 makes for a non-symbolic TrueType font: through the (3,1) subtable
/// by a name's Unicode value from Adobe's list, or, where the program has no (3,1), through
/// the (1,0) subtable by the name's Mac OS Roman code.
pub(crate) struct Lookup<'a> {
    subtable: CmapSubtable<'a>,
    by_unicode: bool,
}

impl<'a> Lookup<'a> {
    /// The lookup `program` supports, or nothing when it has neither subtable (31-017's to
    /// say) or will not read.
    pub(crate) fn of(program: &'a [u8]) -> Option<Self> {
        match CmapSubtable::find(program, 3, 1) {
            Some(subtable) => Some(Self { subtable, by_unicode: true }),
            None => Some(Self { subtable: CmapSubtable::find(program, 1, 0)?, by_unicode: false }),
        }
    }

    /// The code the subtable is asked for by `name`, when the name has one.
    fn point(&self, name: &str) -> Option<u32> {
        if self.by_unicode {
            let text = fepdf_font::agl::lookup(name)?;
            let mut chars = text.chars();
            let only = chars.next().filter(|_| chars.next().is_none())?;
            Some(u32::from(only))
        } else {
            fepdf_font::latin_names::mac_os_roman_code(name).map(u32::from)
        }
    }

    /// The glyph `name` selects — 0, `.notdef`, when the subtable does not map it — or
    /// nothing when the name has no code to ask by.
    pub(crate) fn glyph_of(&self, name: &str) -> Option<u16> {
        self.point(name).map(|p| self.subtable.glyph_or_notdef(p))
    }
}

/// The codes among `rendered` that reach no glyph in `program` by their names in `names`,
/// through [`Lookup`].
///
/// **Nothing when the program has neither subtable**, which is 31-017's to say, or will not
/// read. A code with no name in the table, or a name that is not one character, reaches
/// nothing.
#[must_use]
pub fn unreachable(
    program: &[u8],
    names: &BTreeMap<u8, String>,
    rendered: &BTreeSet<u8>,
) -> Option<Vec<u8>> {
    let lookup = Lookup::of(program)?;
    Some(
        rendered
            .iter()
            .copied()
            .filter(|code| {
                names.get(code).and_then(|name| lookup.glyph_of(name)).is_none_or(|g| g == 0)
            })
            .collect(),
    )
}
