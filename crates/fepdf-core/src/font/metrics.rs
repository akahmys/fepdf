use crate::arena::PdfArena;
use crate::handle::Handle;
use crate::object::{Object, PdfName};
use std::collections::BTreeMap;

/// Container for font horizontal and vertical metrics.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    pub first: i32,
    pub last: i32,
    pub widths: BTreeMap<u32, f32>,
    /// CID -> (w1_y, v_x, v_y) for vertical writing.
    pub v_widths: BTreeMap<u32, (f32, f32, f32)>,
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
                    let last_cid = next_obj.as_integer().unwrap_or(0) as u32;
                    let w_val: f32 =
                        Object::resolve(&w_arr[i + 2], arena).as_f64().unwrap_or(1000.0) as f32;
                    for cid in first_cid..=last_cid {
                        metrics.widths.insert(cid, w_val);
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
            let first_cid = Object::resolve(&w2_arr[i], arena).as_integer().unwrap_or(0) as u32;
            let next_obj = Object::resolve(&w2_arr[i + 1], arena);
            if let Object::Array(iah) = next_obj {
                if let Some(i_arr) = arena.get_array(iah) {
                    for (idx, chunk) in i_arr.chunks_exact(3).enumerate() {
                        let w1_y =
                            Object::resolve(&chunk[0], arena).as_f64().unwrap_or(-1000.0) as f32;
                        let v_x = Object::resolve(&chunk[1], arena)
                            .as_f64()
                            .unwrap_or(default_w as f64 / 2.0)
                            as f32;
                        let v_y =
                            Object::resolve(&chunk[2], arena).as_f64().unwrap_or(880.0) as f32;
                        v_widths.insert(first_cid + idx as u32, (w1_y, v_x, v_y));
                    }
                }
                i += 2;
            } else {
                let last_cid = next_obj.as_integer().unwrap_or(0) as u32;
                let w1_y =
                    Object::resolve(&w2_arr[i + 2], arena).as_f64().unwrap_or(-1000.0) as f32;
                let v_x = Object::resolve(&w2_arr[i + 3], arena)
                    .as_f64()
                    .unwrap_or(default_w as f64 / 2.0) as f32;
                let v_y = Object::resolve(&w2_arr[i + 4], arena).as_f64().unwrap_or(880.0) as f32;
                for cid in first_cid..=last_cid {
                    v_widths.insert(cid, (w1_y, v_x, v_y));
                }
                i += 5;
            }
        }
        v_widths
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
            for (idx, w) in arr.iter().enumerate() {
                metrics.widths.insert(
                    (metrics.first + idx as i32) as u32,
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
