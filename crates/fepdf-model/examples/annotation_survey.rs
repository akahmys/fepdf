//! Which *entries* the corpus's annotations and form fields actually carry.
//!
//! `interactive_survey.rs` counted annotations by subtype and stopped there, which was
//! the right question when the answer was "29,973, all `/Link`". The external corpus
//! carries 16 subtypes, so the next question is what is inside them — a subtype-specific
//! reader for a key no file writes is a container before its contents (ADR-0017, and
//! `ARCHITECTURE.md` §4.2).
//!
//! Prints, per subtype, how many annotations carry each key, and the same for the
//! terminal fields of every `/AcroForm`. Run it over both corpora:
//!
//! ```text
//! cargo run --example annotation_survey -- samples/*.pdf target/external/*/*.pdf
//! ```

use fepdf_model::arena::PdfArena;
use fepdf_model::object::Object;
use fepdf_model::reader;
use std::collections::BTreeMap;

type Dict = BTreeMap<fepdf_model::Handle<fepdf_model::PdfName>, Object>;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    // subtype -> key -> how many annotations of that subtype carry it.
    let mut by_subtype: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut subtype_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut field_keys: BTreeMap<String, usize> = BTreeMap::new();
    let mut acroform_keys: BTreeMap<String, usize> = BTreeMap::new();
    let mut fields_seen = 0usize;

    for path in &paths {
        let Ok(data) = std::fs::read(path) else { continue };
        let Ok(raw) = reader::load_document(&data) else { continue };
        let arena = &raw.arena;
        let Some(catalog) = raw
            .trailer
            .and_then(|t| arena.get_dict(t))
            .and_then(|d| d.get(&arena.name("Root")).cloned())
            .and_then(|r| dict_of(arena, &r))
        else {
            continue;
        };

        for page in collect_pages(arena, &catalog) {
            for a in array_of(arena, page.get(&arena.name("Annots"))).unwrap_or_default() {
                let Some(d) = dict_of(arena, &a) else { continue };
                let sub = d
                    .get(&arena.name("Subtype"))
                    .and_then(|s| name_of(arena, s))
                    .unwrap_or_else(|| "(none)".into());
                *subtype_counts.entry(sub.clone()).or_default() += 1;
                let entry = by_subtype.entry(sub).or_default();
                for key in d.keys() {
                    if let Some(k) = arena.get_name_str(*key) {
                        *entry.entry(k).or_default() += 1;
                    }
                }
            }
        }

        // The form: what `/AcroForm` itself declares, and what its terminal fields hold.
        if let Some(acro) = catalog.get(&arena.name("AcroForm")).and_then(|a| dict_of(arena, a)) {
            for key in acro.keys() {
                if let Some(k) = arena.get_name_str(*key) {
                    *acroform_keys.entry(k).or_default() += 1;
                }
            }
            let mut queue: Vec<(Object, u32)> = array_of(arena, acro.get(&arena.name("Fields")))
                .unwrap_or_default()
                .into_iter()
                .map(|f| (f, 0))
                .collect();
            while let Some((node, depth)) = queue.pop() {
                if depth > 64 {
                    continue;
                }
                let Some(d) = dict_of(arena, &node) else { continue };
                match array_of(arena, d.get(&arena.name("Kids"))) {
                    Some(kids) if !kids.is_empty() => {
                        queue.extend(kids.into_iter().map(|k| (k, depth + 1)));
                    }
                    _ => {
                        fields_seen += 1;
                        for key in d.keys() {
                            if let Some(k) = arena.get_name_str(*key) {
                                *field_keys.entry(k).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    println!("=== annotation entries, by subtype (12.5) ===");
    let mut order: Vec<(&String, &usize)> = subtype_counts.iter().collect();
    order.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (sub, n) in order {
        println!("\n  /{sub}  ({n} annotations)");
        let mut keys: Vec<(&String, &usize)> = by_subtype[sub].iter().collect();
        keys.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (k, c) in keys {
            println!("      /{k:<16} {c}");
        }
    }

    println!("\n=== /AcroForm entries ===");
    for (k, n) in &acroform_keys {
        println!("  /{k:<16} {n}");
    }
    println!("\n=== terminal field entries ({fields_seen} fields) ===");
    for (k, n) in &field_keys {
        println!("  /{k:<16} {n}");
    }
}

fn collect_pages(arena: &PdfArena, catalog: &Dict) -> Vec<Dict> {
    let mut out = Vec::new();
    let Some(root) = catalog.get(&arena.name("Pages")).and_then(|p| dict_of(arena, p)) else {
        return out;
    };
    let mut queue = vec![(root, 0u32)];
    while let Some((node, depth)) = queue.pop() {
        if depth > 64 {
            continue;
        }
        match array_of(arena, node.get(&arena.name("Kids"))) {
            Some(kids) => {
                for kid in kids {
                    if let Some(d) = dict_of(arena, &kid) {
                        queue.push((d, depth + 1));
                    }
                }
            }
            None => out.push(node),
        }
    }
    out
}

fn dict_of(arena: &PdfArena, object: &Object) -> Option<Dict> {
    match object {
        Object::Dictionary(h) => arena.get_dict(*h),
        Object::Stream(h, _) => arena.get_dict(*h),
        Object::Reference(h) => match arena.get_object(*h)? {
            Object::Dictionary(d) | Object::Stream(d, _) => arena.get_dict(d),
            _ => None,
        },
        _ => None,
    }
}

fn array_of(arena: &PdfArena, object: Option<&Object>) -> Option<Vec<Object>> {
    match object? {
        Object::Array(h) => arena.get_array(*h),
        Object::Reference(h) => match arena.get_object(*h)? {
            Object::Array(a) => arena.get_array(a),
            _ => None,
        },
        _ => None,
    }
}

fn name_of(arena: &PdfArena, object: &Object) -> Option<String> {
    match object {
        Object::Name(h) => arena.get_name_str(*h),
        _ => None,
    }
}
