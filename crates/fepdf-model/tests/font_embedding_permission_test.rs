//! What the corpus's embedded faces say about being embedded.
//!
//! `OS/2.fsType` is how a font states whether it may be put inside a document
//! (ISO 14496-22). Nothing in this engine read it until `fepdf-font::embedding`, and the
//! ladder that chooses a face to embed rests on two properties of the corpus rather than
//! on an assumption: that faces refusing an editable embedding **exist**, and that most
//! embedded programs say **nothing at all**, so that silence cannot be read as consent.
//!
//! Measured over the nine samples on 2026-09-19: 235 embedded programs, 64 carrying an
//! `OS/2` table — 34 editable, 23 installable, 7 preview-and-print, 0 restricted — and
//! 171 saying nothing, of which 153 are CFF-based `FontFile3`, a format with no `OS/2`
//! table at all, and 18 are TrueType subsets whose producer dropped it.
//!
//! **The first run of this measurement was wrong and said 1 of 235.** It read the stream
//! as the arena holds it, which is still `/FlateDecode`d; a zlib header is not an SFNT
//! and every table tag came out as noise. The check that caught it was printing the tags.

use fepdf_font::embedding::{Embedding, embedding_permission};
use fepdf_model::{Handle, Object, PdfArena, document::Document, ingest::IngestionOptions};

/// Every embedded font program in `path`, decoded.
fn font_programs(path: &std::path::Path) -> Vec<Vec<u8>> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let Ok(doc) = Document::open(bytes.into(), &IngestionOptions::default()) else {
        return Vec::new();
    };
    let arena = doc.arena();
    let mut out = Vec::new();
    for i in 0..arena.object_count() {
        let Some(Object::Dictionary(dh)) = arena.get_object(Handle::new(i)) else {
            continue;
        };
        let Some(dict) = arena.get_dict(dh) else { continue };
        for (k, v) in &dict {
            let Some(name) = arena.get_name(*k) else { continue };
            if !matches!(name.as_str(), "FontFile" | "FontFile2" | "FontFile3") {
                continue;
            }
            if let Some(sh) = v.as_reference()
                && let Some(program) = decoded_stream(arena, sh)
            {
                out.push(program);
            }
        }
    }
    out
}

/// The stream at `handle`, with its `/Filter` undone.
///
/// `get_stream_bytes` undoes the arena's own in-memory compression and not the file's
/// filter, which is the distinction the first version of this measurement missed.
fn decoded_stream(arena: &PdfArena, handle: Handle<Object>) -> Option<Vec<u8>> {
    let Some(Object::Stream(sdh, data)) = arena.get_object(handle) else { return None };
    let raw = arena.get_stream_bytes(&data).ok()?;
    let dict = arena.get_dict(sdh)?;
    let filter = dict.iter().find_map(|(k, v)| {
        let name = arena.get_name(*k)?;
        (name.as_str() == "Filter").then(|| v.resolve(arena))
    });
    let Some(filter_name) = filter.and_then(|o| o.as_name()).and_then(|n| arena.get_name(n)) else {
        return Some(raw.to_vec());
    };
    match fepdf_model::filters::decode_stream(filter_name.as_str(), &raw, None, arena) {
        Ok(decoded) => Some(decoded.to_vec()),
        Err(_) => Some(raw.to_vec()),
    }
}

/// The nine samples, or an empty list when `samples/` is not in the tree.
fn samples() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("samples/ is in the tree")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .collect();
    files.sort();
    files
}

/// **A face that refuses an editable embedding is not hypothetical.**
///
/// The ladder refuses rather than substituting when no permitted face carries a glyph, and
/// a refusal path nothing reaches is a refusal path nobody has tested. Seven programs of
/// the nine samples permit preview and print only.
#[test]
fn the_corpus_carries_faces_that_refuse_an_editable_embedding() {
    let refusing = samples()
        .iter()
        .flat_map(|p| font_programs(p))
        .filter_map(|program| embedding_permission(&program))
        .filter(|p| !p.allows_embedding_for_editing())
        .count();
    assert!(
        refusing > 0,
        "no embedded face in the samples refuses an editable embedding, so the ladder's \
         refusal is untested against a real file"
    );
}

/// **Most embedded programs say nothing, and silence is not consent.**
///
/// A CFF-based `FontFile3` has no `OS/2` table by format, and a TrueType subset's
/// producer may drop it. Reading "no table" as "no restriction" would embed, by default,
/// exactly the faces whose terms are unknown.
#[test]
fn most_embedded_programs_state_no_permission_at_all() {
    let programs: Vec<Vec<u8>> = samples().iter().flat_map(|p| font_programs(p)).collect();
    let stated = programs.iter().filter(|p| embedding_permission(p).is_some()).count();
    assert!(!programs.is_empty(), "the samples embed font programs");
    assert!(
        stated * 2 < programs.len(),
        "{stated} of {} programs state a permission; if most now do, the ladder's \
         treatment of silence is worth revisiting",
        programs.len()
    );
}

/// The permissions the samples actually carry, as a tally rather than an assertion.
///
/// `cargo test -p fepdf-model --test font_embedding_permission_test -- --nocapture`
#[test]
fn the_tally_is_printable() {
    let mut installable = 0;
    let mut editable = 0;
    let mut preview = 0;
    let mut restricted = 0;
    let mut silent = 0;
    let mut total = 0;
    for program in samples().iter().flat_map(|p| font_programs(p)) {
        total += 1;
        match embedding_permission(&program).map(|p| p.usage) {
            Some(Embedding::Installable) => installable += 1,
            Some(Embedding::Editable) => editable += 1,
            Some(Embedding::PreviewAndPrint) => preview += 1,
            Some(Embedding::Restricted) => restricted += 1,
            None => silent += 1,
        }
    }
    println!(
        "{total} embedded programs: {installable} installable, {editable} editable, \
         {preview} preview-and-print, {restricted} restricted, {silent} saying nothing"
    );
}
