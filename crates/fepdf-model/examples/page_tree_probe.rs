//! Compares what a file's page tree declares against what it actually holds.
//!
//! A cheap cross-check against an independent viewer: if `/Count` at the root
//! disagrees with another reader's page count, the two are resolving different
//! revisions of the file. That is how the incremental-update ordering bug was found.

use fepdf_model::arena::PdfArena;
use fepdf_model::handle::Handle;
use fepdf_model::object::Object;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path)?;
        let raw = fepdf_model::reader::load_document(&data)?;
        let arena = &raw.arena;
        let declared = raw.trailer.and_then(|t| root_page_count(arena, t));
        let (pages, nodes, mismatched) = walk(arena);
        println!(
            "{path}: /Type /Page objects={pages}  /Type /Pages nodes={nodes}  \
             root /Count={declared:?}  nodes whose /Count disagrees with /Kids={mismatched}"
        );
    }
    Ok(())
}

/// The `/Count` a reader would use: the trailer's `/Root`, then its `/Pages`.
fn root_page_count(arena: &PdfArena, trailer: reader_dict::Handle) -> Option<i64> {
    let root = reference(arena, &arena.get_dict(trailer)?, "Root")?;
    let catalog = dictionary(arena, root)?;
    let pages = reference(arena, &catalog, "Pages")?;
    match dictionary(arena, pages)?.get(&arena.name("Count"))? {
        Object::Integer(v) => Some(*v),
        _ => None,
    }
}

/// Counts page and page-tree objects, and the nodes whose `/Count` is not `/Kids`.
fn walk(arena: &PdfArena) -> (u32, u32, u32) {
    let (page, pages, kids, count) =
        (arena.name("Page"), arena.name("Pages"), arena.name("Kids"), arena.name("Count"));
    let type_key = arena.name("Type");
    let (mut n_page, mut n_pages, mut mismatched) = (0, 0, 0);

    for number in 0..arena.object_count() {
        let Some(dict) = dictionary(arena, Handle::new(number)) else { continue };
        let Some(Object::Name(kind)) = dict.get(&type_key) else { continue };
        if *kind == page {
            n_page += 1;
        } else if *kind == pages {
            n_pages += 1;
            let declared = match dict.get(&count) {
                Some(Object::Integer(v)) => Some(*v),
                _ => None,
            };
            let actual = match dict.get(&kids) {
                Some(Object::Array(a)) => arena.get_array(*a).map(|v| v.len() as i64),
                _ => None,
            };
            if declared.is_some() && actual.is_some() && declared != actual {
                mismatched += 1;
            }
        }
    }
    (n_page, n_pages, mismatched)
}

/// The dictionary an object handle names, if it names one.
fn dictionary(arena: &PdfArena, handle: Handle<Object>) -> Option<reader_dict::Map> {
    match arena.get_object(handle)? {
        Object::Dictionary(d) => arena.get_dict(d),
        _ => None,
    }
}

/// An indirect reference held under `key`.
fn reference(arena: &PdfArena, dict: &reader_dict::Map, key: &str) -> Option<Handle<Object>> {
    match dict.get(&arena.name(key))? {
        Object::Reference(h) => Some(*h),
        _ => None,
    }
}

/// Spellings for the dictionary types, which are otherwise unwieldy.
mod reader_dict {
    /// A handle to a dictionary in the arena.
    pub type Handle = fepdf_model::reader::DictHandle;
    /// A dictionary read out of the arena.
    pub type Map = std::collections::BTreeMap<
        fepdf_model::handle::Handle<fepdf_model::object::PdfName>,
        fepdf_model::object::Object,
    >;
}
