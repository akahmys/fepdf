//! What interactive features (clause 12) the corpus actually carries.
//!
//! Third of these, for the same reason as the first two: `inspect interactive` should
//! report things that exist. `/AcroForm` appears in one of nine samples, so the survey
//! has to say whether annotations, actions and outlines are any better represented
//! before a command is built around all four.

use fepdf_model::arena::PdfArena;
use fepdf_model::object::Object;
use fepdf_model::reader;
use std::collections::BTreeMap;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let mut subtypes: BTreeMap<String, usize> = BTreeMap::new();
    let mut actions: BTreeMap<String, usize> = BTreeMap::new();

    println!(
        "{:<22} {:>6} {:>7} {:>7} {:>7} {:>8}",
        "file", "pages", "annots", "fields", "outline", "actions"
    );
    for path in &paths {
        let Ok(data) = std::fs::read(path) else { continue };
        let Ok(raw) = reader::load_document(&data) else {
            println!("{:<22} unreadable", short(path));
            continue;
        };
        let arena = &raw.arena;
        let Some(catalog) = raw
            .trailer
            .and_then(|t| arena.get_dict(t))
            .and_then(|d| d.get(&arena.name("Root")).cloned())
            .and_then(|r| dict_of(arena, &r))
        else {
            println!("{:<22} no catalogue", short(path));
            continue;
        };

        let pages = collect_pages(arena, &catalog);
        let mut annots = 0;
        for page in &pages {
            for a in array_of(arena, page.get(&arena.name("Annots"))).unwrap_or_default() {
                annots += 1;
                if let Some(d) = dict_of(arena, &a) {
                    let sub = d
                        .get(&arena.name("Subtype"))
                        .and_then(|s| name_of(arena, s))
                        .unwrap_or_else(|| "(none)".into());
                    *subtypes.entry(sub).or_default() += 1;
                }
            }
        }

        let fields = catalog
            .get(&arena.name("AcroForm"))
            .and_then(|a| dict_of(arena, a))
            .and_then(|f| array_of(arena, f.get(&arena.name("Fields"))))
            .map_or(0, |v| v.len());

        let outline = catalog
            .get(&arena.name("Outlines"))
            .and_then(|o| dict_of(arena, o))
            .and_then(|o| o.get(&arena.name("Count")).cloned())
            .and_then(|c| match c {
                Object::Integer(n) => Some(n.abs()),
                _ => None,
            })
            .unwrap_or(0);

        // Per file, then folded into the corpus total: counting into the shared map
        // and summing it made every later file report every earlier file's actions.
        let mut here: BTreeMap<String, usize> = BTreeMap::new();
        count_action(arena, &catalog, &mut here);
        for page in &pages {
            for a in array_of(arena, page.get(&arena.name("Annots"))).unwrap_or_default() {
                if let Some(d) = dict_of(arena, &a) {
                    count_action(arena, &d, &mut here);
                }
            }
        }
        let total_actions: usize = here.values().sum();
        for (k, n) in here {
            *actions.entry(k).or_default() += n;
        }

        println!(
            "{:<22} {:>6} {:>7} {:>7} {:>7} {:>8}",
            short(path),
            pages.len(),
            annots,
            fields,
            outline,
            total_actions
        );
    }

    println!("\n=== annotation subtypes across the corpus (12.5.6) ===");
    if subtypes.is_empty() {
        println!("  none");
    }
    for (k, n) in &subtypes {
        println!("  {k:<20} {n}");
    }
    println!("\n=== action types seen (12.6.4) ===");
    if actions.is_empty() {
        println!("  none");
    }
    for (k, n) in &actions {
        println!("  {k:<20} {n}");
    }
}

/// Records `/A` and `/AA` action subtypes found on a dictionary.
fn count_action(arena: &PdfArena, dict: &Dict, out: &mut BTreeMap<String, usize>) {
    for key in ["A", "OpenAction"] {
        if let Some(a) = dict.get(&arena.name(key))
            && let Some(d) = dict_of(arena, &a.clone())
        {
            let s = d
                .get(&arena.name("S"))
                .and_then(|s| name_of(arena, s))
                .unwrap_or_else(|| "(untyped)".into());
            *out.entry(s).or_default() += 1;
        }
    }
    if let Some(aa) = dict.get(&arena.name("AA"))
        && let Some(d) = dict_of(arena, &aa.clone())
    {
        for v in d.values() {
            if let Some(inner) = dict_of(arena, v) {
                let s = inner
                    .get(&arena.name("S"))
                    .and_then(|s| name_of(arena, s))
                    .unwrap_or_else(|| "(untyped)".into());
                *out.entry(format!("AA/{s}")).or_default() += 1;
            }
        }
    }
}

type Dict = BTreeMap<fepdf_model::Handle<fepdf_model::PdfName>, Object>;

/// Every page dictionary, walking `/Kids` depth-first with a depth bound.
fn collect_pages(arena: &PdfArena, catalog: &Dict) -> Vec<Dict> {
    let mut out = Vec::new();
    let Some(root) = catalog.get(&arena.name("Pages")).and_then(|p| dict_of(arena, p)) else {
        return out;
    };
    let mut stack = vec![(root, 0_u32)];
    while let Some((node, depth)) = stack.pop() {
        if depth > 64 {
            continue;
        }
        match array_of(arena, node.get(&arena.name("Kids"))) {
            Some(kids) => {
                for k in kids.into_iter().rev() {
                    if let Some(d) = dict_of(arena, &k) {
                        stack.push((d, depth + 1));
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

fn short(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}
