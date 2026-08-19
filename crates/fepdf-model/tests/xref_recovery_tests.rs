//! A cross-reference section that cannot be read (7.5.8), and what is left of the file.

use fepdf_model::reader;

/// A file whose *newest* cross-reference section is a stream with a filter no reader
/// implements, chained by `/Prev` to a table that covers only some of the objects.
///
/// This is `UnknownFilter-Linearized.pdf` from `pdf-association/pdf-differences` in
/// miniature: a linearized file whose first cross-reference stream is
/// `/Filter /XXXDecode`, so its trailing section reads fine and its leading one does not.
fn file_with_an_undecodable_section() -> Vec<u8> {
    let bodies: [(u32, &str); 3] = [
        (1, "<< /Type /Catalog /Pages 2 0 R >>"),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
        (3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>"),
    ];
    let mut out = b"%PDF-1.5\n".to_vec();
    let mut third = 0;
    for (number, body) in bodies {
        if number == 3 {
            third = out.len();
        }
        out.extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    // A classic table covering object 3 only. Objects 1 and 2 — including the catalogue
    // — are left to the section that will not decode.
    let table_at = out.len();
    out.extend_from_slice(
        format!("xref\n0 1\n0000000000 65535 f \n3 1\n{third:010} 00000 n \n").as_bytes(),
    );
    out.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");

    let stream_at = out.len();
    let payload = [0u8; 12];
    out.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /XRef /Filter /XXXDecode /W [1 2 1] /Index [1 2] /Size 5 \
             /Root 1 0 R /Prev {table_at} /Length {} >>\nstream\n",
            payload.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&payload);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out.extend_from_slice(format!("startxref\n{stream_at}\n%%EOF\n").as_bytes());
    out
}

/// The objects a lost section indexed are recovered, and the loss is recorded.
///
/// Before this, a section that failed to read was dropped by an `if let Ok(..)` with
/// nothing said, and the fallback scan only ran when the records were *empty*. A file
/// with one good section and one bad one therefore lost every object the bad one covered
/// — its catalogue included — and reported "read without departing from the standard".
#[test]
fn an_unreadable_section_is_recorded_and_its_objects_are_recovered() {
    let raw = reader::load_document(&file_with_an_undecodable_section())
        .expect("the readable section and a scan are between them enough");

    let log = format!("{:?}", raw.decisions.entries());
    assert!(log.contains("7.5.8"), "the lost section is not recorded: {log}");
    assert!(log.contains("XXXDecode"), "the reason is not recorded: {log}");
    assert!(log.contains("7.5.4"), "the recovery is not recorded: {log}");

    // The catalogue was indexed only by the section that could not be read.
    let root = raw
        .trailer
        .and_then(|t| raw.arena.get_dict(t))
        .and_then(|d| d.get(&raw.arena.name("Root")).cloned())
        .expect("the trailer names a /Root");
    let dict = raw
        .arena
        .get_dict(root.resolve(&raw.arena).as_dict_handle().expect("/Root resolves"))
        .expect("and is a dictionary");
    assert!(dict.contains_key(&raw.arena.name("Pages")), "the catalogue came back whole");
}

/// A file with no unreadable section is not scanned, and says nothing about scanning.
///
/// The gap filling is safe only because it never overrides a section that *was* read: a
/// scan cannot tell a current object from a superseded one lying earlier in the file. It
/// must therefore stay off entirely when nothing was lost.
#[test]
fn a_file_whose_sections_all_read_is_never_scanned() {
    let raw = reader::load_document(&well_formed_file()).expect("a well-formed file");
    let log = format!("{:?}", raw.decisions.entries());
    assert!(!log.contains("7.5.8"), "reported an unreadable section: {log}");
    assert!(!log.contains("could not be read"), "scanned a file that lost nothing: {log}");
}

/// The same three objects with one cross-reference table covering all of them, and
/// nothing wrong. Built rather than doctored: an earlier version of this test blanked the
/// filter name out of the file above, which ate into `/W` and produced a *differently*
/// unreadable section — so it failed for the reason it was written to rule out.
fn well_formed_file() -> Vec<u8> {
    let bodies: [&str; 3] = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ];
    let mut out = b"%PDF-1.5\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    out.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n").as_bytes(),
    );
    out
}

/// The scan fills holes and never overrides a section that was read.
///
/// That is the whole safety argument for running it, and nothing checked it: removing the
/// filter that enforces it left both tests above passing. A scan cannot tell a current
/// object from a superseded one lying elsewhere in the file, so where a readable section
/// has an answer, that answer stands.
///
/// The file below carries object 3 twice — an old version first, a newer one after — and
/// a readable table that points at the **old** one, which is what a rolled-back
/// incremental update looks like (ADR-0006). A scan that overrode the table would hand
/// back the newer bytes and be confidently wrong.
#[test]
fn the_scan_does_not_override_a_section_that_was_read() {
    let mut out = b"%PDF-1.5\n".to_vec();
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    let old_three = out.len();
    out.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 111 111] >>\nendobj\n",
    );
    out.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 999 999] >>\nendobj\n",
    );

    // A table naming the *older* object 3, and covering nothing else.
    let table_at = out.len();
    out.extend_from_slice(
        format!("xref\n0 1\n0000000000 65535 f \n3 1\n{old_three:010} 00000 n \n").as_bytes(),
    );
    out.extend_from_slice(b"trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");

    // And an unreadable newest section, so the gap filling runs at all.
    let stream_at = out.len();
    let payload = [0u8; 12];
    out.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /XRef /Filter /XXXDecode /W [1 2 1] /Index [1 2] /Size 5 \
             /Root 1 0 R /Prev {table_at} /Length {} >>\nstream\n",
            payload.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&payload);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out.extend_from_slice(format!("startxref\n{stream_at}\n%%EOF\n").as_bytes());

    let raw = reader::load_document(&out).expect("readable");
    let page = raw.arena.get_object(fepdf_model::Handle::new(3)).expect("object 3 is present");
    let dict = raw
        .arena
        .get_dict(page.resolve(&raw.arena).as_dict_handle().expect("a dictionary"))
        .expect("a page");
    let media = dict.get(&raw.arena.name("MediaBox")).expect("/MediaBox");
    let values = raw.arena.get_array(media.resolve(&raw.arena).as_array().expect("an array"));
    let width = format!("{:?}", values.expect("read"));
    assert!(
        width.contains("111"),
        "the table named the object at {old_three} and the scan overrode it: {width}"
    );
}
