//! What the sample catalogues actually contain, against what the engine can name.
//!
//! Written before `inspect catalog` for the same reason `structure_survey` was written
//! before `inspect structure`: the command's job is to make gaps visible, so the gaps
//! have to be measured before the command is designed around them.
//!
//! ROADMAP.md claims "10 of ~30 catalogue entries typed". This checks it.

use fepdf_model::object::Object;
use fepdf_model::reader;
use std::collections::{BTreeMap, BTreeSet};

/// Every key ISO 32000-2 Table 29 defines for the document catalogue (7.7.2).
const CATALOG_KEYS: &[&str] = &[
    "Type",
    "Version",
    "Extensions",
    "Pages",
    "PageLabels",
    "Names",
    "Dests",
    "ViewerPreferences",
    "PageLayout",
    "PageMode",
    "Outlines",
    "Threads",
    "OpenAction",
    "AA",
    "URI",
    "AcroForm",
    "Metadata",
    "StructTreeRoot",
    "MarkInfo",
    "Lang",
    "SpiderInfo",
    "OutputIntents",
    "PieceInfo",
    "OCProperties",
    "Perms",
    "Legal",
    "Requirements",
    "Collection",
    "NeedsRendering",
    "DSS",
    "AF",
    "DPartRoot",
];

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut unknown: BTreeSet<String> = BTreeSet::new();

    for path in &paths {
        let Ok(data) = std::fs::read(path) else { continue };
        let Ok(raw) = reader::load_document(&data) else {
            println!("{:<26} unreadable", short(path));
            continue;
        };

        let Some(root) = raw
            .trailer
            .and_then(|t| raw.arena.get_dict(t))
            .and_then(|d| d.get(&raw.arena.name("Root")).cloned())
        else {
            println!("{:<26} no /Root", short(path));
            continue;
        };
        let Some(dict) = resolve_dict(&raw.arena, &root) else {
            println!("{:<26} /Root does not resolve to a dictionary", short(path));
            continue;
        };

        let mut keys: Vec<String> = Vec::new();
        for name in dict.keys() {
            let Some(k) = raw.arena.get_name_str(*name) else { continue };
            *seen.entry(k.clone()).or_default() += 1;
            if !CATALOG_KEYS.contains(&k.as_str()) {
                unknown.insert(k.clone());
            }
            keys.push(k);
        }
        keys.sort();
        println!("{:<26} {:>2} entries: {}", short(path), keys.len(), keys.join(" "));
    }

    println!("\n=== how often each catalogue key appears across {} files ===", paths.len());
    for key in CATALOG_KEYS {
        let n = seen.get(*key).copied().unwrap_or(0);
        let named = names_it(key);
        println!(
            "  {:<18} in {:>2} files   engine names the key: {}",
            key,
            n,
            if named { "yes" } else { "NO" }
        );
    }
    if !unknown.is_empty() {
        println!("\n=== keys outside Table 29 ===");
        for k in &unknown {
            println!("  {k}");
        }
    }
}

/// Whether any engine source names this key as a string literal. A key the engine
/// never writes is one it can neither read nor round-trip deliberately, whatever
/// spec types exist beside it.
fn names_it(key: &str) -> bool {
    let needle = format!("\"{key}\"");
    walk("crates").into_iter().any(|p| {
        std::fs::read_to_string(&p)
            .is_ok_and(|s| s.contains(&needle) && !p.to_string_lossy().contains("catalog_survey"))
    })
}

fn walk(dir: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            out.extend(walk(&p.to_string_lossy()));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}

fn resolve_dict(
    arena: &fepdf_model::arena::PdfArena,
    object: &Object,
) -> Option<std::collections::BTreeMap<fepdf_model::Handle<fepdf_model::PdfName>, Object>> {
    match object {
        Object::Dictionary(h) => arena.get_dict(*h),
        Object::Reference(h) => {
            let inner = arena.get_object(*h)?;
            match inner {
                Object::Dictionary(d) => arena.get_dict(d),
                _ => None,
            }
        }
        _ => None,
    }
}

fn short(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}
