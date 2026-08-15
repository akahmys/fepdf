//! Surveys what a file's structure actually contains, across a whole corpus.
//!
//! Written before `inspect structure` so the command is designed against measured
//! contents rather than an imagined file. Each row is one input; the columns are the
//! things `inspect structure` would report, so a column that is always the same number
//! is a column not worth printing.

use fepdf_model::arena::PdfArena;
use fepdf_model::interpretation::{DecisionLog, Severity};

use fepdf_model::reader;
use fepdf_syntax::xref::{self, XrefRecord};

struct Survey {
    name: String,
    version: String,
    header_at: usize,
    sections: usize,
    entries_newest: usize,
    in_use: usize,
    free: usize,
    in_stream: usize,
    containers: usize,
    scanned: bool,
    decisions: (usize, usize, usize),
    encrypted: bool,
    objects: usize,
}

fn survey(path: &str) -> Result<Survey, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let name = std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());

    let header = xref::find_header(&data);
    let arena = PdfArena::new();
    let mut log = DecisionLog::default();

    // The chain as the reader walks it, not as the trailer claims it.
    let chain =
        xref::find_startxref(&data).map_or_else(Vec::new, |s| xref::section_chain(&data, s));

    let mut newest = std::collections::BTreeMap::new();
    for at in &chain {
        let Ok(offset) = usize::try_from(*at) else { continue };
        if let Ok(section) = reader::read_xref_section(&data, offset, &arena, &mut log) {
            for (num, rec) in section.entries {
                newest.entry(num).or_insert(rec);
            }
        }
    }

    let mut in_use = 0;
    let mut free = 0;
    let mut in_stream = 0;
    let mut containers = std::collections::BTreeSet::new();
    for rec in newest.values() {
        match rec {
            XrefRecord::InFile { .. } => in_use += 1,
            XrefRecord::Free { .. } => free += 1,
            XrefRecord::InObjectStream { container, .. } => {
                in_stream += 1;
                containers.insert(*container);
            }
        }
    }

    let doc = reader::load_document(&data)?;
    let entries = doc.decisions.entries();
    let decisions = (
        entries.iter().filter(|d| d.severity == Severity::Ambiguity).count(),
        entries.iter().filter(|d| d.severity == Severity::Repaired).count(),
        entries.iter().filter(|d| d.severity == Severity::Violation).count(),
    );
    // The reader records the substitution when it falls back to scanning.
    let scanned = entries.iter().any(|d| d.action.contains("scan") || d.found.contains("scan"));

    let encrypted = doc.trailer.is_some_and(|t| {
        doc.arena.get_dict(t).is_some_and(|d| d.contains_key(&doc.arena.name("Encrypt")))
    });

    let objects = doc.arena.object_count() as usize;

    Ok(Survey {
        name,
        version: doc.version,
        header_at: header.map_or(usize::MAX, |h| h.offset),
        sections: chain.len(),
        entries_newest: newest.len(),
        in_use,
        free,
        in_stream,
        containers: containers.len(),
        scanned,
        decisions,
        encrypted,
        objects,
    })
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: structure_survey <pdf>...");
        std::process::exit(2);
    }

    println!(
        "{:<26} {:>4} {:>4} {:>5} {:>7} {:>7} {:>5} {:>7} {:>5} {:>4} {:>9} {:>4}",
        "file",
        "ver",
        "hdr",
        "sects",
        "entries",
        "in-use",
        "free",
        "in-strm",
        "ctnr",
        "scan",
        "A/R/V",
        "objs"
    );
    for path in &paths {
        match survey(path) {
            Ok(s) => println!(
                "{:<26} {:>4} {:>4} {:>5} {:>7} {:>7} {:>5} {:>7} {:>5} {:>4} {:>9} {:>4}",
                s.name,
                s.version,
                if s.header_at == usize::MAX { "-".into() } else { s.header_at.to_string() },
                s.sections,
                s.entries_newest,
                s.in_use,
                s.free,
                s.in_stream,
                s.containers,
                if s.scanned { "yes" } else { "" },
                format!("{}/{}/{}", s.decisions.0, s.decisions.1, s.decisions.2),
                s.objects,
            ),
            Err(e) => println!("{:<26} FAILED: {e}", s_name(path)),
        }
    }
    println!("\nencrypted:");
    for path in &paths {
        if let Ok(s) = survey(path)
            && s.encrypted
        {
            println!("  {}", s.name);
        }
    }
}

fn s_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}
