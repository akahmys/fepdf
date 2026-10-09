use crate::arena::PdfArena;
use crate::handle::Handle;
use crate::object::{Object, PdfName};
use std::collections::BTreeMap;

/// How many CIDs the `/W` and `/W2` ranges of one font are expanded into, one entry each,
/// before a range is kept as a range instead.
///
/// `c_first c_last w` covers every CID between; read one CID at a time, `0 4294967295 500`
/// was four billion insertions, and the document never finished opening (ROADMAP Z-2,
/// after PrintCraft's hayro). ISO 32000-2 sets no maximum CID — Annex C says only that
/// earlier versions recommended 65,535 — so a range is not cut short; it is stored whole
/// instead of expanded. The largest CID collections hold about 65,000, so 2^20 leaves
/// every real font expanded as it always was.
pub const EXPANDED_CIDS: u64 = 1 << 20;

/// A CID from a `/W` or `/W2` entry: a non-negative integer that fits in 32 bits.
fn cid(obj: &Object, arena: &PdfArena) -> Option<u32> {
    u32::try_from(Object::resolve(obj, arena).as_integer()?).ok()
}

/// What `/W2` gives, while it is read: the CIDs expanded, the ranges kept whole, and how
/// many more CIDs may be expanded.
#[derive(Default)]
struct VerticalWidths {
    expanded: BTreeMap<u32, (f32, f32, f32)>,
    ranges: Vec<(u32, u32, (f32, f32, f32))>,
    budget: u64,
}

/// Container for font horizontal and vertical metrics.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    /// First character code covered.
    pub first: i32,
    /// Last character code covered, inclusive.
    pub last: i32,
    /// Advance widths, keyed by character code.
    pub widths: BTreeMap<u32, f32>,
    /// CID -> (w1_y, v_x, v_y) for vertical writing.
    pub v_widths: BTreeMap<u32, (f32, f32, f32)>,
    /// `/W` ranges kept as ranges, `(first, last, width)`, once [`EXPANDED_CIDS`] are in
    /// `widths`: consulted where `widths` has no entry, the latest first.
    pub width_ranges: Vec<(u32, u32, f32)>,
    /// `/W2` ranges kept as ranges, as `width_ranges` is for `/W`.
    pub v_width_ranges: Vec<(u32, u32, (f32, f32, f32))>,
    /// Width used for codes absent from the table (`/MissingWidth` or `/DW`).
    pub default_width: f32,
    /// `/DW2`, as `(position_y, displacement_y)` — 9.7.4.3's default vertical metrics.
    ///
    /// The standard's own defaults are `[880 -1000]`, and those were **hard-coded in the
    /// accessor** until 2026-09-06 while `/DW` beside it was read from the file. A font
    /// declaring anything else was laid out as though it had not.
    pub default_vertical: (f32, f32),
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            first: 0,
            last: 0,
            widths: BTreeMap::new(),
            v_widths: BTreeMap::new(),
            width_ranges: Vec::new(),
            v_width_ranges: Vec::new(),
            default_width: 1000.0,
            default_vertical: (880.0, -1000.0),
        }
    }
}
impl FontMetrics {
    /// `/DW2`, the default vertical metrics a CID font declares (9.7.4.3).
    ///
    /// `[vy w1y]`, and `None` for anything else — a malformed entry leaves the standard's
    /// `[880 -1000]` *whole* rather than half of it, because a font cannot declare one
    /// number and inherit the other. A declared position vector against a default
    /// displacement is a layout nobody asked for.
    fn parse_dw2(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<(f32, f32)> {
        let Object::Array(dw2) = Object::resolve(df_dict.get(&arena.name("DW2"))?, arena) else {
            return None;
        };
        let items = arena.get_array(dw2)?;
        let [vy, w1y] = &items[..] else { return None };
        let vy = Object::resolve(vy, arena).as_f64()?;
        let w1y = Object::resolve(w1y, arena).as_f64()?;
        Some((vy as f32, w1y as f32))
    }

    /// Parses CID-keyed font metrics from a CIDFont dictionary (W and DW).
    pub fn parse_cid(df_dict: &BTreeMap<Handle<PdfName>, Object>, arena: &PdfArena) -> Self {
        let mut metrics = Self { default_width: 1000.0, ..Self::default() };

        if let Some(dw_obj) = df_dict.get(&arena.name("DW")) {
            metrics.default_width =
                Object::resolve(dw_obj, arena).as_f64().unwrap_or(1000.0) as f32;
        }

        if let Some(declared) = Self::parse_dw2(df_dict, arena) {
            metrics.default_vertical = declared;
        }

        if let Some(Object::Array(wah)) =
            df_dict.get(&arena.name("W")).map(|o: &Object| Object::resolve(o, arena))
            && let Some(w_arr) = arena.get_array(wah)
        {
            metrics.parse_w(&w_arr, arena);
        }

        let vertical = Self::parse_v2(df_dict, arena, metrics.default_width);
        metrics.v_widths = vertical.expanded;
        metrics.v_width_ranges = vertical.ranges;
        metrics
    }

    /// `/W` (9.7.4.3): `c [w1 w2 …]` entries and `c_first c_last w` ranges, the ranges
    /// expanded until [`EXPANDED_CIDS`] are and kept as ranges after.
    fn parse_w(&mut self, w_arr: &[Object], arena: &PdfArena) {
        let mut budget = EXPANDED_CIDS;
        let mut i: usize = 0;
        while let (Some(first_obj), Some(next_obj)) = (w_arr.get(i), w_arr.get(i + 1)) {
            let first_cid = cid(first_obj, arena);
            let next_obj = Object::resolve(next_obj, arena);
            if let Object::Array(iah) = next_obj {
                if let (Some(first), Some(i_arr)) = (first_cid, arena.get_array(iah)) {
                    for (idx, w_obj) in i_arr.iter().enumerate() {
                        let Some(c) = u32::try_from(idx).ok().and_then(|k| first.checked_add(k))
                        else {
                            break;
                        };
                        let w_val = Object::resolve(w_obj, arena).as_f64().unwrap_or(1000.0);
                        self.widths.insert(c, w_val as f32);
                    }
                }
                i += 2;
                continue;
            }
            let Some(w_obj) = w_arr.get(i + 2) else { break };
            let w_val = Object::resolve(w_obj, arena).as_f64().unwrap_or(1000.0) as f32;
            if let (Some(first), Some(last)) = (first_cid, cid(&next_obj, arena))
                && first <= last
            {
                let count = u64::from(last - first) + 1;
                if count <= budget {
                    budget -= count;
                    for c in first..=last {
                        self.widths.insert(c, w_val);
                    }
                } else {
                    self.width_ranges.push((first, last, w_val));
                }
            }
            i += 3;
        }
    }
    /// Parses vertical metrics from a CIDFont dictionary (W2 and DW2): the CIDs it
    /// expands, and the ranges it keeps as ranges past [`EXPANDED_CIDS`].
    fn parse_v2(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        default_w: f32,
    ) -> VerticalWidths {
        let mut v = VerticalWidths { budget: EXPANDED_CIDS, ..VerticalWidths::default() };
        let Some(Object::Array(wah)) =
            df_dict.get(&arena.name("W2")).map(|o: &Object| Object::resolve(o, arena))
        else {
            return v;
        };
        let Some(w2_arr) = arena.get_array(wah) else { return v };

        let mut i: usize = 0;
        while i < w2_arr.len() {
            i = Self::parse_v2_entry(&w2_arr, i, arena, default_w, &mut v);
        }
        v
    }

    fn parse_v2_entry(
        w2_arr: &[Object],
        i: usize,
        arena: &PdfArena,
        default_w: f32,
        v: &mut VerticalWidths,
    ) -> usize {
        let (Some(first_obj), Some(next_obj)) = (w2_arr.get(i), w2_arr.get(i + 1)) else {
            return w2_arr.len();
        };
        let first_cid = cid(first_obj, arena);
        let metric = |w1: &Object, vx: &Object, vy: &Object| {
            let w1_y = Object::resolve(w1, arena).as_f64().unwrap_or(-1000.0) as f32;
            let v_x = Object::resolve(vx, arena).as_f64().unwrap_or(f64::from(default_w) / 2.0);
            let v_y = Object::resolve(vy, arena).as_f64().unwrap_or(880.0) as f32;
            (w1_y, v_x as f32, v_y)
        };
        let next_obj = Object::resolve(next_obj, arena);
        if let Object::Array(iah) = next_obj {
            if let (Some(first), Some(i_arr)) = (first_cid, arena.get_array(iah)) {
                for (idx, chunk) in i_arr.as_chunks::<3>().0.iter().enumerate() {
                    let Some(c) = u32::try_from(idx).ok().and_then(|k| first.checked_add(k)) else {
                        break;
                    };
                    let [w1, vx, vy] = chunk;
                    v.expanded.insert(c, metric(w1, vx, vy));
                }
            }
            return i + 2;
        }
        let Some([w1, vx, vy]) = w2_arr.get(i + 2..i + 5) else {
            return w2_arr.len();
        };
        let value = metric(w1, vx, vy);
        if let (Some(first), Some(last)) = (first_cid, cid(&next_obj, arena))
            && first <= last
        {
            let count = u64::from(last - first) + 1;
            if count <= v.budget {
                v.budget -= count;
                for c in first..=last {
                    v.expanded.insert(c, value);
                }
            } else {
                v.ranges.push((first, last, value));
            }
        }
        i + 5
    }

    /// Parses standard horizontal metrics (FirstChar, LastChar, Widths).
    pub fn parse_standard(dict: &BTreeMap<Handle<PdfName>, Object>, arena: &PdfArena) -> Self {
        let mut metrics = Self {
            first: dict
                .get(&arena.name("FirstChar"))
                .and_then(|o: &Object| Object::resolve(o, arena).as_integer())
                .unwrap_or(0) as i32,
            last: dict
                .get(&arena.name("LastChar"))
                .and_then(|o: &Object| Object::resolve(o, arena).as_integer())
                .unwrap_or(0) as i32,
            ..Default::default()
        };

        if let Some(Object::Array(ah)) =
            dict.get(&arena.name("Widths")).map(|o: &Object| Object::resolve(o, arena))
            && let Some(arr) = arena.get_array(ah)
        {
            let first_code = metrics.first.max(0) as u32;
            for (idx, w) in arr.iter().enumerate() {
                metrics.widths.insert(
                    first_code + idx as u32,
                    Object::resolve(w, arena).as_f64().unwrap_or(0.0) as f32,
                );
            }
        }
        metrics
    }

    /// Parses Type 3 font metrics.
    pub fn parse_type3(dict: &BTreeMap<Handle<PdfName>, Object>, arena: &PdfArena) -> Self {
        let mut metrics = Self::default();
        if let Some(Object::Integer(f)) =
            dict.get(&arena.name("FirstChar")).map(|o: &Object| Object::resolve(o, arena))
        {
            metrics.first = f as i32;
        }
        if let Some(Object::Integer(l)) =
            dict.get(&arena.name("LastChar")).map(|o: &Object| Object::resolve(o, arena))
        {
            metrics.last = l as i32;
        }
        if let Some(Object::Array(ah)) =
            dict.get(&arena.name("Widths")).map(|o: &Object| Object::resolve(o, arena))
            && let Some(arr) = arena.get_array(ah)
        {
            for (idx, w) in arr.iter().enumerate() {
                metrics.widths.insert(
                    (metrics.first + idx as i32) as u32,
                    Object::resolve(w, arena).as_f64().unwrap_or(0.0) as f32,
                );
            }
        }
        metrics
    }
}

/// Classification of font category for advance width estimation heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontCategory {
    /// Monospaced font (fixed pitch, e.g. Courier, Consolas).
    Monospace,
    /// Proportional Serif font (e.g. Times, Georgia, Mincho).
    Serif,
    /// Proportional Sans-Serif font (e.g. Helvetica, Arial, Gothic).
    #[default]
    SansSerif,
    /// CJK font (Chinese, Japanese, Korean).
    Cjk,
    /// Symbolic or Dingbat font.
    Symbol,
}

impl FontCategory {
    /// Classifies font category from base font name and descriptor flags.
    pub fn from_name_and_flags(base_font: &str, flags: u32, is_cid_keyed: bool) -> Self {
        let name_lower = base_font.to_ascii_lowercase();
        if is_cid_keyed
            || name_lower.contains("mincho")
            || name_lower.contains("gothic")
            || name_lower.contains("明朝")
            || name_lower.contains("ゴシック")
            || name_lower.contains("song")
            || name_lower.contains("kai")
            || name_lower.contains("heiti")
            || name_lower.contains("batang")
            || name_lower.contains("dotum")
        {
            return Self::Cjk;
        }
        // Bit 1 of Flags is FixedPitch (1 << 0)
        if (flags & 1) != 0
            || name_lower.contains("courier")
            || name_lower.contains("mono")
            || name_lower.contains("consolas")
        {
            return Self::Monospace;
        }
        // Bit 2 of Flags is Serif (1 << 1)
        if (flags & (1 << 1)) != 0
            || name_lower.contains("times")
            || name_lower.contains("serif")
            || name_lower.contains("georgia")
            || name_lower.contains("palatino")
        {
            return Self::Serif;
        }
        // Bit 3 of Flags is Symbolic (1 << 2)
        if (flags & (1 << 2)) != 0
            || name_lower.contains("symbol")
            || name_lower.contains("dingbat")
            || name_lower.contains("wingdings")
        {
            return Self::Symbol;
        }
        Self::SansSerif
    }

    /// Estimates advance width (in 1/1000 em units) for a given character code or Unicode scalar.
    pub fn estimate_char_width(self, char_code: u32) -> f32 {
        match self {
            Self::Monospace => 600.0,
            Self::Cjk => {
                if char_code < 0x80 {
                    500.0
                } else {
                    1000.0
                }
            }
            Self::Symbol => 600.0,
            Self::Serif => Self::estimate_proportional_width(char_code, true),
            Self::SansSerif => Self::estimate_proportional_width(char_code, false),
        }
    }

    fn estimate_proportional_width(code: u32, is_serif: bool) -> f32 {
        match code {
            0x20 => 250.0,
            // Narrow characters (i, j, l, t, f, r, punctuation)
            0x69 /* 'i' */ | 0x6C /* 'l' */ => if is_serif { 278.0 } else { 222.0 },
            0x6A /* 'j' */ | 0x74 /* 't' */ | 0x66 /* 'f' */ | 0x72 /* 'r' */ => if is_serif { 333.0 } else { 278.0 },
            0x49 /* 'I' */ | 0x4A /* 'J' */ => if is_serif { 361.0 } else { 278.0 },
            0x21 /* '!' */ | 0x2E /* '.' */ | 0x2C /* ',' */ | 0x3A /* ':' */ | 0x3B /* ';' */ | 0x27 /* '\'' */ | 0x7C /* '|' */ => 250.0,
            // Wide characters (m, w, M, W, @, %)
            0x6D /* 'm' */ | 0x77 /* 'w' */ => if is_serif { 778.0 } else { 833.0 },
            0x4D /* 'M' */ | 0x57 /* 'W' */ => if is_serif { 889.0 } else { 833.0 },
            0x40 /* '@' */ | 0x25 /* '%' */ => 850.0,
            // Digits
            0x30..=0x39 /* '0'..='9' */ => 500.0,
            // Uppercase letters
            0x41..=0x5A /* 'A'..='Z' */ => if is_serif { 680.0 } else { 667.0 },
            // Lowercase letters
            0x61..=0x7A /* 'a'..='z' */ => if is_serif { 500.0 } else { 520.0 },
            // Default ASCII
            0..=0x7F => 500.0,
            // Default full-width / non-ASCII
            _ => 1000.0,
        }
    }
}

/// Detects writing mode (Horizontal=0, Vertical=1) from Encoding or CMap.
pub fn detect_wmode(dict: &BTreeMap<Handle<PdfName>, Object>, arena: &PdfArena) -> i32 {
    let enc_obj = dict.get(&arena.name("Encoding"));
    if let Some(enc) = enc_obj {
        let resolved = Object::resolve(enc, arena);
        match resolved {
            Object::Name(h) => {
                if let Some(n) = arena.get_name(h) {
                    let bytes = n.as_bytes();
                    if bytes.ends_with(b"-V") || bytes == b"V" {
                        return 1;
                    }
                }
            }
            Object::Stream(dh, _) => {
                if let Some(d) = arena.get_dict(dh)
                    && let Some(n_handle) = d
                        .get(&arena.name("CMapName"))
                        .and_then(|o: &Object| Object::resolve(o, arena).as_name())
                    && let Some(n) = arena.get_name(n_handle)
                {
                    let bytes = n.as_bytes();
                    if bytes.ends_with(b"-V") || bytes == b"V" {
                        return 1;
                    }
                }
            }
            _ => {}
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_category_classification() {
        assert_eq!(
            FontCategory::from_name_and_flags("CourierNew", 0, false),
            FontCategory::Monospace
        );
        assert_eq!(FontCategory::from_name_and_flags("Times-Roman", 0, false), FontCategory::Serif);
        assert_eq!(
            FontCategory::from_name_and_flags("Helvetica", 0, false),
            FontCategory::SansSerif
        );
        assert_eq!(FontCategory::from_name_and_flags("MS-Gothic", 0, false), FontCategory::Cjk);
        assert_eq!(FontCategory::from_name_and_flags("Symbol", 0, false), FontCategory::Symbol);
        assert_eq!(
            FontCategory::from_name_and_flags("CustomFont", 1 << 0, false),
            FontCategory::Monospace
        );
        assert_eq!(
            FontCategory::from_name_and_flags("CustomFont", 1 << 1, false),
            FontCategory::Serif
        );
    }

    #[test]
    fn test_font_category_advance_width_estimation() {
        let sans = FontCategory::SansSerif;
        assert_eq!(sans.estimate_char_width(0x20), 250.0);
        assert_eq!(sans.estimate_char_width(b'i' as u32), 222.0);
        assert_eq!(sans.estimate_char_width(b'm' as u32), 833.0);
        assert_eq!(sans.estimate_char_width(b'a' as u32), 520.0);

        let mono = FontCategory::Monospace;
        assert_eq!(mono.estimate_char_width(b'i' as u32), 600.0);
        assert_eq!(mono.estimate_char_width(b'w' as u32), 600.0);

        let cjk = FontCategory::Cjk;
        assert_eq!(cjk.estimate_char_width(b'A' as u32), 500.0);
        assert_eq!(cjk.estimate_char_width(0x4E00), 1000.0);
    }

    #[test]
    fn test_font_metrics_out_of_bounds_safety() {
        let arena = PdfArena::new();
        let mut dict = BTreeMap::new();
        // Construct malformed W array with odd/incomplete length
        let w_arr = vec![Object::Integer(10)];
        let w_handle = arena.alloc_array(w_arr);
        dict.insert(arena.name("W"), Object::Array(w_handle));

        // Should not panic on truncated W array
        let metrics = FontMetrics::parse_cid(&dict, &arena);
        assert!(metrics.widths.is_empty());
    }

    #[test]
    fn test_font_metrics_negative_first_char_safety() {
        let arena = PdfArena::new();
        let mut dict = BTreeMap::new();
        dict.insert(arena.name("FirstChar"), Object::Integer(-5));
        let arr_handle = arena.alloc_array(vec![Object::Real(500.0)]);
        dict.insert(arena.name("Widths"), Object::Array(arr_handle));

        let metrics = FontMetrics::parse_standard(&dict, &arena);
        assert_eq!(metrics.widths.get(&0), Some(&500.0));
    }
}
