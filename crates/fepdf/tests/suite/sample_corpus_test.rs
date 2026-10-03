//! The sample corpus holds each document once.
//!
//! **`samples/sample.pdf` and `samples/constitution.pdf` were byte-identical** — same
//! length, same MD5 — so every figure taken "over the samples" counted that document
//! twice, including the ones ROADMAP W-E1a quotes about embedded font programs. None was
//! wrong as stated, and each was one file less varied than it sounded.
//!
//! **Removing the file here does not remove it anywhere else.** `samples/` is untracked —
//! `.gitignore` excludes it — so the corpus is whatever each working copy happens to hold,
//! and there is no list in the repository saying what that is. What a commit *can* carry
//! is this: on any working copy where two samples are the same bytes, the gate fails and
//! names them. Fifteen places walk `samples/*.pdf`, and one check here covers all of them
//! rather than each learning to deduplicate.

use std::path::{Path, PathBuf};

fn samples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        // A working copy with no samples has nothing to count twice. Every other test
        // that reads them says the same thing its own way.
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .collect();
    files.sort();
    files
}

/// **No two samples are the same file.**
///
/// Compared by content and not by name, because the duplicate had a different name — the
/// thing that let it stand unnoticed. Grouped by length first so the byte comparison runs
/// only where it could possibly match.
#[test]
fn no_two_samples_are_the_same_document() {
    let files = samples();
    let mut by_length: std::collections::BTreeMap<u64, Vec<&PathBuf>> =
        std::collections::BTreeMap::new();
    for file in &files {
        let length = std::fs::metadata(file).map_or(0, |m| m.len());
        by_length.entry(length).or_default().push(file);
    }

    let mut twins = Vec::new();
    for group in by_length.values().filter(|g| g.len() > 1) {
        for (i, a) in group.iter().enumerate() {
            for b in &group[i + 1..] {
                if std::fs::read(a).ok() == std::fs::read(b).ok() {
                    twins.push(format!("{} and {}", a.display(), b.display()));
                }
            }
        }
    }
    assert!(
        twins.is_empty(),
        "these samples are the same bytes, so every figure measured over the corpus \
         counts one document twice — remove one of each pair: {twins:?}"
    );
}
