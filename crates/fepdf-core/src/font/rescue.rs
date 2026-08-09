//! CMap Rescue & Heuristics for broken font mappings.

use crate::font::cmap::CMap;

/// Handles recovery of missing or broken Unicode mappings.
pub struct CMapRescue;

impl CMapRescue {
    /// Attempts to find a suitable CMap based on a resource name or known characteristics.
    pub fn find_rescue_cmap(name: &str) -> Option<CMap> {
        let name_lower = name.to_lowercase();

        // 1. Check for standard Japanese CID collections
        if name_lower.contains("unijis") || name_lower.contains("adobe-japan1") {
            return CMap::load_named("Adobe-Japan1-UCS2");
        }

        // 2. Check for common Japanese font names that imply AJ1
        if name_lower.contains("hira")
            || name_lower.contains("koz")
            || name_lower.contains("mincho")
            || name_lower.contains("明朝")
            || name_lower.contains("gothic")
            || name_lower.contains("ゴシック")
            || name_lower.contains("#82#6c#82#72")
        // MS encoded strings
        {
            return CMap::load_named("Adobe-Japan1-UCS2");
        }

        // 3. Check for simplified Chinese
        if name_lower.contains("gb-") || name_lower.contains("adobe-gb1") {
            return CMap::load_named("Adobe-GB1-UCS2");
        }

        // 4. Check for traditional Chinese
        if name_lower.contains("cns-") || name_lower.contains("adobe-cns1") {
            return CMap::load_named("Adobe-CNS1-UCS2");
        }

        None
    }

    /// Guesses Unicode from a glyph name (e.g., "uni3042" -> "あ").
    pub fn unicode_from_glyph_name(name: &str) -> Option<String> {
        if name.starts_with("uni")
            && name.len() >= 7
            && let Ok(hex) = u32::from_str_radix(&name[3..7], 16)
            && let Some(c) = std::char::from_u32(hex)
        {
            return Some(c.to_string());
        }

        if name.starts_with('u')
            && name.len() >= 5
            && let Ok(hex) = u32::from_str_radix(&name[1..5], 16)
            && let Some(c) = std::char::from_u32(hex)
        {
            return Some(c.to_string());
        }

        None
    }
}
