//! Whether writing a document drops what nothing points at.
//!
//! `SaveOptions` carried a `vacuum` flag and the export wizard carried a checkbox for it
//! — "Vacuum Pass (Remove orphan/unreachable objects)". Nothing in the engine ever read
//! the flag, and `Writer::set_vacuum` was an empty function with no callers, under a
//! comment claiming the pass was "inherent in our ID remapping logic". This is that
//! claim, measured.

use bytes::Bytes;
use fepdf::PdfDocument;

/// The string only the unreachable object carries.
const ORPHAN: &str = "NothingPointsAtThis";

/// Four objects, one of which the catalogue reaches only when `catalogue_extra` says so.
fn with_an_orphan(catalogue_extra: &str) -> Bytes {
    use std::fmt::Write as _;
    let bodies: [String; 4] = [
        format!("<< /Type /Catalog /Pages 2 0 R{catalogue_extra} >>"),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
        // Referenced by nothing: not the catalogue, not the page tree, not the trailer.
        format!("<< /Type /Whatever /Marker ({ORPHAN}) >>"),
    ];
    let mut out = String::from("%PDF-2.0\n");
    let mut offsets = Vec::with_capacity(bodies.len());
    for (n, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", n + 1);
    }
    let start_xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", bodies.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start_xref}\n%%EOF\n",
        bodies.len() + 1
    );
    Bytes::from(out.into_bytes())
}

/// A directory to write into, removed by the caller.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fepdf-vacuum-{name}"));
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    dir
}

/// Written so that what is in the file can be read out of it.
///
/// **The first version of this test passed for the wrong reason.** `SaveOptions` defaults
/// to `obj_stm: true` and `compress: true`, so every object goes into a deflated object
/// stream and no object's text appears in the file as bytes — which made "the marker is
/// not in the output" true whether the object was dropped or written. Making the orphan
/// reachable did not fail the test, which is how that was found.
fn readable() -> fepdf::SaveOptions {
    fepdf::SaveOptions { obj_stm: false, compress: false, ..fepdf::SaveOptions::default() }
}

/// Saves `source` and answers the bytes written.
fn written(source: Bytes, name: &str, options: &fepdf::SaveOptions) -> String {
    let doc = PdfDocument::open(source).expect("it opens");
    let dir = scratch(name);
    let path = dir.join("out.pdf");
    let _ = doc.save_with_options(&path, "2.0", options).expect("it writes");
    let bytes = std::fs::read(&path).expect("it is on disk");
    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The orphan is in the file that was opened, and not in the one that was written.
///
/// **Nothing asked for this.** [`readable`] leaves `vacuum` at its default of `false`,
/// which is the whole point: the flag the export wizard offered was never the reason
/// unreachable objects went.
#[test]
fn an_object_nothing_points_at_is_not_written_out() {
    assert!(
        String::from_utf8_lossy(&with_an_orphan("")).contains(ORPHAN),
        "the fixture has no orphan in it, so this test would pass for the wrong reason"
    );
    assert!(!written(with_an_orphan(""), "orphan", &readable()).contains(ORPHAN));
}

/// And the same object, once something points at it, is written — which is what makes
/// the test above a measurement rather than a sentence about deflate.
#[test]
fn the_same_object_survives_once_the_catalogue_reaches_it() {
    let reached = written(with_an_orphan(" /Reached 4 0 R"), "reached", &readable());
    assert!(
        reached.contains(ORPHAN),
        "a reachable object was dropped, so the trace drops more than orphans"
    );
}
