//! What the corpora actually put in an `ExtGState` (Table 58).
//!
//! Written for one question — does any file carry `/HT`, and therefore does 10.6 matter?
//! — and made to answer the neighbouring ones at the same time, because the expensive
//! part is opening 524 files and the tally is free. `/TR` and `/TR2` are 10.5's transfer
//! functions and sit on the same table; `/SMask`, `/BM` and `/ca` are what a file
//! *actually* uses one of these dictionaries for.
//!
//! **Not `grep`.** An `ExtGState` is usually inside an object stream and always inside a
//! compressed one, so searching the bytes finds a fraction of them and reports the
//! fraction as the answer. This walks every dictionary the arena holds after the file has
//! been read, which is the only place the question can be asked honestly.
//!
//! **An arena walk double-counts, and the dictionary totals below say so.**
//! `commit_to_arena` allocates a *new* dictionary for every refined one and the parsed
//! original stays where it was, so a live dictionary is typically held twice. Measured on
//! a control file built with exactly one `ExtGState` and one halftone dictionary: this
//! reports two of each. The totals are therefore an **upper bound** — useful for the
//! ratios between keys, not as a count of what a page reaches.
//!
//! The **file** counts are unaffected, and they are what this survey exists for: the walk
//! is a superset of what any page can reach, so a key found in no file is genuinely in no
//! file. Verified against a control carrying `/HT`, `/TR` and `/TR2` before the corpora
//! were run, because "no file carries it" and "the survey is broken" print identically.
//!
//! ```text
//! cargo run --release -p fepdf --example survey_extgstate -- samples/*.pdf target/external/*/*.pdf
//! ```

use fepdf::PdfDocument;
use fepdf_model::{Object, PdfName};
use std::collections::BTreeMap;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: survey_extgstate <file.pdf>...");
        return;
    }

    let mut keys: BTreeMap<String, usize> = BTreeMap::new();
    let mut carriers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut opened, mut refused, mut states, mut halftone_dicts) = (0_usize, 0_usize, 0, 0);

    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else {
            refused += 1;
            continue;
        };
        let Ok(doc) = PdfDocument::open(bytes.into()) else {
            refused += 1;
            continue;
        };
        opened += 1;
        let found = survey_one(&doc, &mut keys, &mut states, &mut halftone_dicts);
        let name = std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        for key in found {
            carriers.entry(key).or_default().push(name.clone());
        }
    }

    report(opened, refused, states, halftone_dicts, &keys, &carriers);
}

/// Tallies one document, returning the keys of interest it carries at all.
fn survey_one(
    doc: &PdfDocument,
    keys: &mut BTreeMap<String, usize>,
    states: &mut usize,
    halftone_dicts: &mut usize,
) -> Vec<String> {
    let arena = doc.inner().arena();
    let mut carried = Vec::new();
    for handle in arena.all_dict_handles() {
        let Some(dict) = arena.get_dict(handle) else { continue };
        let names: Vec<String> = dict.keys().filter_map(|k| arena.get_name_str(*k)).collect();

        // A halftone dictionary is identified by its own required entry (Table 128),
        // not by sitting under an `/HT`, so it is counted wherever it is.
        if names.iter().any(|n| n == "HalftoneType") {
            *halftone_dicts += 1;
        }

        if !is_ext_gstate(&dict, arena, &names) {
            continue;
        }
        *states += 1;
        for name in names {
            *keys.entry(name.clone()).or_default() += 1;
            if matches!(name.as_str(), "HT" | "TR" | "TR2") && !carried.contains(&name) {
                carried.push(name);
            }
        }
    }
    carried
}

/// Whether a dictionary is an `ExtGState`.
///
/// `/Type` is optional on one (Table 58), so a survey that required it would under-count
/// by however many producers leave it out — and under-counting is the one answer this
/// survey must not give, since it exists to decide whether a clause can be left unbuilt.
///
/// **The fallback has to be keys that appear on nothing else, which `/SMask` and `/CA`
/// are not.** Written with those two in it first, this matched 834 image XObjects — every
/// one has `/SMask` — and a run of annotations, which carry `/CA` for opacity. The tell
/// was `/Width`, `/Height`, `/BitsPerComponent` and `/Filter` all appearing exactly 834
/// times in a tally of *graphics state* keys. The list below is confined to entries
/// Table 58 does not share with any other dictionary.
fn is_ext_gstate(
    dict: &BTreeMap<fepdf_model::Handle<PdfName>, Object>,
    arena: &fepdf_model::PdfArena,
    names: &[String],
) -> bool {
    let type_key = arena.intern_name(PdfName::new("Type"));
    if let Some(Object::Name(h)) = dict.get(&type_key).map(|o| o.resolve(arena))
        && arena.get_name_str(h).as_deref() == Some("ExtGState")
    {
        return true;
    }
    names.iter().any(|n| {
        matches!(n.as_str(), "ca" | "OPM" | "SA" | "AIS" | "TK" | "BG2" | "UCR2" | "HT" | "TR2")
    })
}

fn report(
    opened: usize,
    refused: usize,
    states: usize,
    halftone_dicts: usize,
    keys: &BTreeMap<String, usize>,
    carriers: &BTreeMap<String, Vec<String>>,
) {
    println!("files opened            {opened}");
    println!("files that would not open {refused}");
    println!("ExtGState dictionaries  {states}   (upper bound: refinement holds each twice)");
    println!("halftone dictionaries   {halftone_dicts}   (a /HalftoneType anywhere, same caveat)");
    println!("\nExtGState keys, by arena occurrences — ratios are meaningful, totals are not:");
    let mut sorted: Vec<(&String, &usize)> = keys.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (key, count) in sorted {
        let clause = match key.as_str() {
            "HT" => "  <- 10.6 halftones",
            "TR" | "TR2" => "  <- 10.5 transfer functions",
            _ => "",
        };
        println!("  /{key:<12} {count:>6}{clause}");
    }
    for key in ["HT", "TR", "TR2"] {
        let files = carriers.get(key).map(Vec::as_slice).unwrap_or_default();
        println!("\nfiles carrying /{key}: {}", files.len());
        for f in files.iter().take(12) {
            println!("    {f}");
        }
        if files.len() > 12 {
            println!("    … and {} more", files.len() - 12);
        }
    }
}
