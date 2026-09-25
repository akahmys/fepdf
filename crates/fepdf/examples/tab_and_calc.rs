//! How the corpus orders its fields: each page's `/Tabs`, and the form's `/CO` against the
//! fields that carry a calculation (ROADMAP W-F2-b).
//!
//! Reads the file as written — the reader's arena, before ingestion — so it counts what
//! the file says rather than what the engine made of it.
use fepdf_model::Object;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn pdfs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            pdfs(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            out.push(path);
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    pdfs(&root.join("samples"), &mut files);
    pdfs(&root.join("target/external"), &mut files);
    files.sort();
    let mut tabs: BTreeMap<String, usize> = BTreeMap::new();
    let (mut files_with_annots, mut calc_files, mut co_mismatch) = (0, 0, 0);
    for f in &files {
        let Ok(bytes) = std::fs::read(f) else { continue };
        let Ok(raw) = fepdf_model::reader::load_document(&bytes.into()) else { continue };
        let arena = &raw.arena;
        let (ty, page, annots, tabs_key) =
            (arena.name("Type"), arena.name("Page"), arena.name("Annots"), arena.name("Tabs"));
        let (aa, c, co, fields) =
            (arena.name("AA"), arena.name("C"), arena.name("CO"), arena.name("Fields"));
        let mut any_annots = false;
        let mut calculated = 0usize;
        let mut declared = None;
        for i in 0..arena.object_count() {
            let Some(Object::Dictionary(dh)) = arena.get_object(fepdf_model::Handle::new(i)) else {
                continue;
            };
            if arena.dict_entry(dh, ty).and_then(|t| t.as_name()) == Some(page)
                && arena.dict_entry(dh, annots).is_some()
            {
                any_annots = true;
                let value = arena
                    .dict_entry(dh, tabs_key)
                    .and_then(|t| t.as_name())
                    .and_then(|n| arena.get_name_str(n))
                    .unwrap_or_else(|| "(absent)".to_string());
                *tabs.entry(value).or_default() += 1;
            }
            if let Some(actions) = arena.dict_entry(dh, aa).map(|a| a.resolve(arena))
                && let Some(ah) = actions.as_dict_handle()
                && arena.dict_entry(ah, c).is_some()
            {
                calculated += 1;
            }
            if arena.dict_entry(dh, fields).is_some() {
                declared = arena
                    .dict_entry(dh, co)
                    .map(|o| o.resolve(arena))
                    .and_then(|o| if let Object::Array(a) = o { arena.get_array(a) } else { None })
                    .map(|a| a.len());
            }
        }
        files_with_annots += usize::from(any_annots);
        if calculated > 0 || declared.is_some() {
            calc_files += 1;
            let name = f.strip_prefix(&root).unwrap_or(f).display();
            if declared != Some(calculated) {
                co_mismatch += 1;
            }
            println!("{name}: {calculated} fields calculate, /CO holds {declared:?}");
        }
    }
    println!("{} files, {files_with_annots} with annotations on a page", files.len());
    println!("pages with annotations by /Tabs: {tabs:?}");
    println!("{calc_files} files calculate or declare /CO; {co_mismatch} where the two disagree");
}
