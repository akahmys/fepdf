//! The Adobe Glyph List, and a glyph name read as the text it stands for (ISO 32000-2
//! 9.10.2).
//!
//! **Adobe's own list, not a transcription.** `data/glyphlist.txt` is the file Adobe
//! publishes in `adobe-type-tools/agl-aglfn`, carried with its licence: 4,281 names. What
//! stood here was 61 of them typed out by hand, three of which disagreed with the list —
//! `quoteright` and `quoteleft` read as the ASCII `'` and `` ` `` where the list says
//! U+2019 and U+2018, and `quotehook`, which the list does not have — so every other name
//! a `/Differences` array or a CFF charset used came back as nothing.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Adobe's `glyphlist.txt`, as published: `name;HHHH[ HHHH…]` a line, `#` for a comment.
const GLYPH_LIST: &str = include_str!("../data/glyphlist.txt");

/// The list, read once: each name and the text it stands for.
fn list() -> &'static BTreeMap<&'static str, String> {
    static LIST: OnceLock<BTreeMap<&'static str, String>> = OnceLock::new();
    LIST.get_or_init(|| {
        GLYPH_LIST
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| {
                let (name, codes) = line.trim().split_once(';')?;
                let text: Option<String> = codes
                    .split_whitespace()
                    .map(|hex| u32::from_str_radix(hex, 16).ok().and_then(char::from_u32))
                    .collect();
                Some((name, text?))
            })
            .collect()
    })
}

/// Whether `name` is one of the Adobe Glyph List's own names.
///
/// **Membership, not readability.** `uni0041` reads as `A` by the naming convention and is
/// not a name the list contains; PDF/UA-1 asks the second question (7.21.6, 7.21.7).
#[must_use]
pub fn in_glyph_list(name: &str) -> bool {
    list().contains_key(name)
}

/// Maps a glyph name to its corresponding Unicode string: by the `uniXXXX` and `uXXXX[XX]`
/// conventions, then by the list.
pub fn lookup(name: &str) -> Option<String> {
    if let Some(s) = lookup_pattern(name) {
        return Some(s);
    }
    list().get(name).cloned()
}

fn lookup_pattern(name: &str) -> Option<String> {
    if name.starts_with("uni") && name.len() >= 7 {
        if let Ok(val) = u32::from_str_radix(&name[3..7], 16)
            && let Some(c) = std::char::from_u32(val)
        {
            return Some(c.to_string());
        }
    } else if name.starts_with('u')
        && name.len() >= 5
        && let Ok(val) = u32::from_str_radix(&name[1..], 16)
        && let Some(c) = std::char::from_u32(val)
    {
        return Some(c.to_string());
    }
    None
}

/// The list as Adobe publishes it, and the names the hand table got wrong.
#[cfg(test)]
mod the_list {
    use super::{in_glyph_list, list, lookup};

    /// **All of it**: the published file's 4,281 entries.
    #[test]
    fn every_name_is_read() {
        assert_eq!(list().len(), 4281);
    }

    /// The three the hand table disagreed with the list about.
    #[test]
    fn the_quotes_are_the_lists() {
        assert_eq!(lookup("quoteright").as_deref(), Some("\u{2019}"));
        assert_eq!(lookup("quoteleft").as_deref(), Some("\u{2018}"));
        assert!(!in_glyph_list("quotehook"));
    }

    /// Names the hand table never had, and a name with several code points.
    #[test]
    fn the_rest_of_the_list_is_there() {
        assert_eq!(lookup("eacute").as_deref(), Some("é"));
        assert_eq!(lookup("Aogonek").as_deref(), Some("Ą"));
        assert_eq!(lookup("dalethatafpatah").map(|s| s.chars().count()), Some(2));
        assert!(in_glyph_list("eacute") && !in_glyph_list("uni00E9"));
    }
}
