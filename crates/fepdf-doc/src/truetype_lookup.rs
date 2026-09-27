//! 31-018: whether a non-symbolic TrueType font's rendered codes reach a glyph.
//!
//! Through the cmap lookups ISO 32000-1 9.6.6.4 describes, and nothing else.

use fepdf_font::sfnt_cmap::CmapSubtable;
use std::collections::{BTreeMap, BTreeSet};

/// The codes among `rendered` that reach no glyph in `program` by their names in `names`.
///
/// Through the (3,1) subtable by the name's Unicode value from Adobe's list, or, where the
/// program has no (3,1), through the (1,0) subtable by the name's Mac OS Roman code.
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
    let windows = CmapSubtable::find(program, 3, 1);
    let (subtable, by_unicode) = match windows {
        Some(subtable) => (subtable, true),
        None => (CmapSubtable::find(program, 1, 0)?, false),
    };
    let point = |name: &str| -> Option<u32> {
        if by_unicode {
            let text = fepdf_font::agl::lookup(name)?;
            let mut chars = text.chars();
            let only = chars.next().filter(|_| chars.next().is_none())?;
            Some(u32::from(only))
        } else {
            crate::annex_d::mac_os_roman_code(name).map(u32::from)
        }
    };
    Some(
        rendered
            .iter()
            .copied()
            .filter(|code| {
                names
                    .get(code)
                    .and_then(|name| point(name))
                    .and_then(|p| subtable.glyph(p))
                    .is_none()
            })
            .collect(),
    )
}
