//! What the corpus's image XObjects are: filter, colour space, and bit depth.
//!
//! `inspect structure` counts filters, and that was enough while the question was "does
//! a missing codec block text". It is not enough for the question underneath: a JPEG in
//! `/DeviceGray` and a JPEG in `/DeviceRGB` reach the same filter and describe buffers
//! with a different number of components per pixel, and getting that wrong renders a
//! grey scan as noise.
//!
//! ```text
//! cargo run --release --example image_survey -- samples/*.pdf target/external/*/*.pdf
//! ```

use fepdf_model::arena::PdfArena;
use fepdf_model::object::Object;
use fepdf_model::reader;
use std::collections::BTreeMap;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    // (filter, colour space, bits) -> how many image XObjects.
    let mut kinds: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    let mut images = 0usize;
    let mut files_with = 0usize;

    for path in &paths {
        let Ok(data) = std::fs::read(path) else { continue };
        let Ok(raw) = reader::load_document(&data) else { continue };
        let arena = &raw.arena;
        let mut here = 0;

        for i in 0..arena.object_count() {
            let Some(Object::Stream(dh, _)) = arena.get_object(fepdf_model::Handle::new(i)) else {
                continue;
            };
            let Some(dict) = arena.get_dict(dh) else { continue };
            if name_at(arena, &dict, "Subtype").as_deref() != Some("Image") {
                continue;
            }
            here += 1;
            let filter = filters_of(arena, &dict).unwrap_or_else(|| "(none)".into());
            let space = colour_space_of(arena, &dict).unwrap_or_else(|| "(none)".into());
            let bits = dict
                .get(&arena.name("BitsPerComponent"))
                .and_then(|o| o.resolve(arena).as_integer())
                .map_or_else(|| "?".to_string(), |b| b.to_string());
            *kinds.entry((filter, space, bits)).or_default() += 1;
        }
        images += here;
        files_with += usize::from(here > 0);
    }

    println!("{images} image XObjects in {files_with} of {} files\n", paths.len());
    println!("  {:<28} {:<22} {:>4}  images", "filter", "colour space", "bpc");
    let mut rows: Vec<_> = kinds.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for ((filter, space, bits), n) in rows {
        println!("  {filter:<28} {space:<22} {bits:>4}  {n}");
    }
}

type Dict = BTreeMap<fepdf_model::Handle<fepdf_model::PdfName>, Object>;

/// `/Filter`, one name or an array of them, as written.
fn filters_of(arena: &PdfArena, dict: &Dict) -> Option<String> {
    match dict.get(&arena.name("Filter"))?.resolve(arena) {
        Object::Name(h) => arena.get_name_str(h),
        Object::Array(ah) => Some(
            arena
                .get_array(ah)?
                .iter()
                .filter_map(|o| o.resolve(arena).as_name())
                .filter_map(|h| arena.get_name_str(h))
                .collect::<Vec<_>>()
                .join("+"),
        ),
        _ => None,
    }
}

/// `/ColorSpace`, named — or the family of an array form such as `[/Indexed …]`, since
/// that is what decides how many components a sample has.
fn colour_space_of(arena: &PdfArena, dict: &Dict) -> Option<String> {
    let mask = dict.get(&arena.name("ImageMask")).and_then(|o| o.resolve(arena).as_bool());
    if mask == Some(true) {
        return Some("(ImageMask)".into());
    }
    match dict.get(&arena.name("ColorSpace"))?.resolve(arena) {
        Object::Name(h) => arena.get_name_str(h),
        Object::Array(ah) => {
            let first = arena.get_array(ah)?.first()?.resolve(arena).as_name()?;
            arena.get_name_str(first).map(|n| format!("[{n} …]"))
        }
        _ => None,
    }
}

fn name_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    arena.get_name_str(dict.get(&arena.name(key))?.resolve(arena).as_name()?)
}
