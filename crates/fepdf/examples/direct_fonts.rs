//! How many files write a font dictionary direct, over the samples and the external corpus.
//!
//! ROADMAP W-E2c: 7.3.10 lets a `/Font` entry hold the dictionary itself rather than a
//! reference to it. Opening a document gives each such font an object of its own, so this
//! reads the file as written — the reader's arena, before ingestion — to count what the
//! file does rather than what the engine made of it.
use fepdf_model::Object;
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
    let (mut read, mut with_fonts, mut with_direct, mut direct_fonts) = (0, 0, 0, 0);
    for f in &files {
        let Ok(bytes) = std::fs::read(f) else { continue };
        let Ok(raw) = fepdf_model::reader::load_document(&bytes.into()) else { continue };
        read += 1;
        let arena = &raw.arena;
        let font_key = arena.name("Font");
        let (mut any, mut direct) = (false, 0usize);
        for holder in arena.all_dict_handles() {
            let Some(Object::Dictionary(fonts)) =
                arena.dict_entry(holder, font_key).map(|entry| entry.resolve(arena))
            else {
                continue;
            };
            for entry in arena.get_dict(fonts).unwrap_or_default().values() {
                any = true;
                direct += usize::from(matches!(entry, Object::Dictionary(_)));
            }
        }
        with_fonts += usize::from(any);
        if direct > 0 {
            with_direct += 1;
            direct_fonts += direct;
            println!("{}: {direct}", f.strip_prefix(&root).unwrap_or(f).display());
        }
    }
    println!(
        "{} files, {read} read, {with_fonts} with a font resource, {with_direct} writing one \
         direct ({direct_fonts} entries)",
        files.len()
    );
}
