use crate::arena::PdfArena;
use crate::handle::Handle;
use crate::object::{Object, PdfName};
use std::collections::BTreeMap;

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
    /// Width used for codes absent from the table (`/MissingWidth` or `/DW`).
    pub default_width: f32,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            first: 0,
            last: 0,
            widths: BTreeMap::new(),
            v_widths: BTreeMap::new(),
            default_width: 1000.0,
        }
    }
}
impl FontMetrics {
    /// Parses CID-keyed font metrics from a CIDFont dictionary (W and DW).
    pub fn parse_cid(df_dict: &BTreeMap<Handle<PdfName>, Object>, arena: &PdfArena) -> Self {
        let mut metrics = Self { default_width: 1000.0, ..Self::default() };

        if let Some(dw_obj) = df_dict.get(&arena.name("DW")) {
            metrics.default_width =
                Object::resolve(dw_obj, arena).as_f64().unwrap_or(1000.0) as f32;
        }

        if let Some(Object::Array(wah)) =
            df_dict.get(&arena.name("W")).map(|o: &Object| Object::resolve(o, arena))
            && let Some(w_arr) = arena.get_array(wah)
        {
            let mut i: usize = 0;
            while i < w_arr.len() {
                let first_cid = Object::resolve(&w_arr[i], arena).as_integer().unwrap_or(0) as u32;
                if i + 1 >= w_arr.len() {
                    break;
                }
                let next_obj = Object::resolve(&w_arr[i + 1], arena);
                if let Object::Array(iah) = next_obj {
                    if let Some(i_arr) = arena.get_array(iah) {
                        for (idx, w_obj) in i_arr.iter().enumerate() {
                            let w_val: f32 =
                                Object::resolve(w_obj, arena).as_f64().unwrap_or(1000.0) as f32;
                            metrics.widths.insert(first_cid + idx as u32, w_val);
                        }
                    }
                    i += 2;
                } else {
                    if i + 2 >= w_arr.len() {
                        break;
                    }
                    let last_cid = next_obj.as_integer().unwrap_or(0) as u32;
                    let w_val: f32 =
                        Object::resolve(&w_arr[i + 2], arena).as_f64().unwrap_or(1000.0) as f32;
                    if first_cid <= last_cid {
                        for cid in first_cid..=last_cid {
                            metrics.widths.insert(cid, w_val);
                        }
                    }
                    i += 3;
                }
            }
        }

        metrics.v_widths = Self::parse_v2(df_dict, arena, metrics.default_width);
        metrics
    }

    /// Parses vertical metrics from a CIDFont dictionary (W2 and DW2).
    fn parse_v2(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        default_w: f32,
    ) -> BTreeMap<u32, (f32, f32, f32)> {
        let mut v_widths = BTreeMap::new();
        let Some(Object::Array(wah)) =
            df_dict.get(&arena.name("W2")).map(|o: &Object| Object::resolve(o, arena))
        else {
            return v_widths;
        };
        let Some(w2_arr) = arena.get_array(wah) else { return v_widths };

        let mut i: usize = 0;
        while i < w2_arr.len() {
            i = Self::parse_v2_entry(&w2_arr, i, arena, default_w, &mut v_widths);
        }
        v_widths
    }

    fn parse_v2_entry(
        w2_arr: &[Object],
        i: usize,
        arena: &PdfArena,
        default_w: f32,
        v_widths: &mut BTreeMap<u32, (f32, f32, f32)>,
    ) -> usize {
        let first_cid = Object::resolve(&w2_arr[i], arena).as_integer().unwrap_or(0) as u32;
        if i + 1 >= w2_arr.len() {
            return w2_arr.len();
        }
        let next_obj = Object::resolve(&w2_arr[i + 1], arena);
        if let Object::Array(iah) = next_obj {
            if let Some(i_arr) = arena.get_array(iah) {
                for (idx, chunk) in i_arr.chunks_exact(3).enumerate() {
                    let w1_y = Object::resolve(&chunk[0], arena).as_f64().unwrap_or(-1000.0) as f32;
                    let v_x = Object::resolve(&chunk[1], arena)
                        .as_f64()
                        .unwrap_or(f64::from(default_w) / 2.0) as f32;
                    let v_y = Object::resolve(&chunk[2], arena).as_f64().unwrap_or(880.0) as f32;
                    v_widths.insert(first_cid + idx as u32, (w1_y, v_x, v_y));
                }
            }
            i + 2
        } else {
            if i + 4 >= w2_arr.len() {
                return w2_arr.len();
            }
            let last_cid = next_obj.as_integer().unwrap_or(0) as u32;
            let w1_y = Object::resolve(&w2_arr[i + 2], arena).as_f64().unwrap_or(-1000.0) as f32;
            let v_x = Object::resolve(&w2_arr[i + 3], arena)
                .as_f64()
                .unwrap_or(f64::from(default_w) / 2.0) as f32;
            let v_y = Object::resolve(&w2_arr[i + 4], arena).as_f64().unwrap_or(880.0) as f32;
            if first_cid <= last_cid {
                for cid in first_cid..=last_cid {
                    v_widths.insert(cid, (w1_y, v_x, v_y));
                }
            }
            i + 5
        }
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
