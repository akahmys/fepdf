//! What 2.0 deprecates, measured row by row against `docs/specs/pdf-2-0-deprecations.md`
//! (ROADMAP AB-1).
//!
//! **The table's Now column is this test's expectations.** A row whose treatment changes
//! changes here and there together; a mismatch prints every row as it was observed, so
//! the column can be re-derived from one run instead of from reading the code.
//!
//! The open document is read through the engine; the saved file through the byte layer
//! (`reader::load_document`), which normalises nothing — reading it back through
//! `PdfDocument::open` would hide a key the writer wrote and the next load dropped.

use bytes::Bytes;
use fepdf::{PdfDocument, SaveOptions};
use fepdf_content::BlendMode;
use fepdf_fixtures::recorder::{Event, Recorder};
use fepdf_model::access::entry;
use fepdf_model::{Object, PdfArena};
use kurbo::Affine;

/// Where a row's object hangs from.
#[derive(Clone, Copy)]
enum From {
    Catalog,
    Info,
}

/// What a row expects to find.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Seen {
    Absent,
    Present,
    /// A name: the value, without its solidus.
    Name(&'static str),
    /// An array holding an integer — a revision number among attribute objects or classes.
    ArrayWithInteger,
    /// An array of names.
    Array,
}

/// One key of one row of the table.
struct Row {
    /// The table's row number.
    number: u8,
    from: From,
    /// Keys from `from`; a number indexes an array, and `Key=Value` picks the dictionary
    /// of an array whose `/Key` is the name `Value`.
    path: &'static [&'static str],
    open: Seen,
    saved: Seen,
}

const fn row(number: u8, path: &'static [&'static str], open: Seen, saved: Seen) -> Row {
    Row { number, from: From::Catalog, path, open, saved }
}

const PAGE: &str = "0";

/// Every row, with what the engine does now. Kept in the table's order.
const ROWS: &[Row] = {
    use Seen::{Absent, ArrayWithInteger, Name, Present};
    &[
        Row { number: 1, from: From::Info, path: &["Title"], open: Absent, saved: Absent },
        Row { number: 1, from: From::Info, path: &["Trapped"], open: Present, saved: Present },
        row(5, &["NeedsRendering"], Present, Present),
        row(5, &["AcroForm", "XFA"], Present, Present),
        row(6, &["SpiderInfo"], Present, Present),
        row(6, &["Pages", "Kids", PAGE, "ID"], Present, Present),
        row(6, &["Pages", "Kids", PAGE, "Resources", "XObject", "Im1", "ID"], Present, Present),
        row(6, &["Names", "IDS"], Present, Present),
        row(6, &["Names", "URLS"], Present, Present),
        row(7, &["Names", "AlternatePresentations"], Present, Present),
        row(8, &["Pages", "Kids", PAGE, "Resources", "ProcSet"], Absent, Absent),
        row(
            9,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "F", "DOS"],
            Present,
            Present,
        ),
        row(
            9,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "F", "Mac"],
            Present,
            Present,
        ),
        row(
            9,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "F", "Unix"],
            Present,
            Present,
        ),
        row(
            10,
            &["Pages", "Kids", PAGE, "Resources", "ExtGState", "G1", "BM"],
            Name("Multiply"),
            Name("Multiply"),
        ),
        row(
            11,
            &["Pages", "Kids", PAGE, "Resources", "ExtGState", "G2", "BM"],
            Name("Normal"),
            Name("Normal"),
        ),
        row(12, &["Pages", "Kids", PAGE, "Resources", "ExtGState", "G1", "TR"], Present, Present),
        row(12, &["Pages", "Kids", PAGE, "Resources", "ExtGState", "G1", "TR2"], Present, Present),
        row(14, &["Pages", "Kids", PAGE, "Resources", "Font", "F1", "Name"], Absent, Absent),
        row(14, &["Pages", "Kids", PAGE, "Resources", "XObject", "Im1", "Name"], Present, Present),
        row(14, &["Pages", "Kids", PAGE, "Resources", "XObject", "Fm1", "Name"], Present, Present),
        row(
            15,
            &["Pages", "Kids", PAGE, "Resources", "Font", "F1", "FontDescriptor", "CharSet"],
            Present,
            Absent,
        ),
        row(
            15,
            &[
                "Pages",
                "Kids",
                PAGE,
                "Resources",
                "Font",
                "F2",
                "DescendantFonts",
                "0",
                "FontDescriptor",
                "CIDSet",
            ],
            Present,
            Absent,
        ),
        row(17, &["ViewerPreferences", "ViewArea"], Present, Present),
        row(17, &["ViewerPreferences", "PrintClip"], Present, Present),
        row(18, &["Pages", "Kids", PAGE, "Annots", "Subtype=Sound"], Present, Present),
        row(19, &["Pages", "Kids", PAGE, "Annots", "Subtype=TrapNet"], Present, Present),
        row(
            19,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=TrapNet", "AP", "N", "TrapRegions"],
            Present,
            Present,
        ),
        row(20, &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "Win"], Present, Present),
        row(20, &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "Unix"], Present, Present),
        row(21, &["AcroForm", "NeedAppearances"], Present, Present),
        row(
            28,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=3D", "3DD", "Resources"],
            Present,
            Present,
        ),
        row(29, &["MarkInfo", "Suspects"], Present, Present),
        row(30, &["StructTreeRoot", "K", "R"], Present, Present),
        row(30, &["StructTreeRoot", "K", "C"], ArrayWithInteger, ArrayWithInteger),
        row(30, &["StructTreeRoot", "K", "A"], ArrayWithInteger, ArrayWithInteger),
        row(31, &["StructTreeRoot", "K", "A", "O=PrintField", "checked"], Present, Present),
        row(32, &["Pages", "Kids", PAGE, "SeparationInfo"], Present, Present),
        row(33, &["Pages", "Kids", PAGE, "Resources", "XObject", "Im1", "OPI"], Present, Present),
        row(33, &["Pages", "Kids", PAGE, "Resources", "XObject", "Fm1", "OPI"], Present, Present),
    ]
};

/// The value at `path` from `start`.
fn walk(arena: &PdfArena, start: Object, path: &[&str]) -> Option<Object> {
    let mut at = start.resolve(arena);
    for step in path {
        at = if let Ok(index) = step.parse::<usize>() {
            match at {
                Object::Array(array) => arena.get_array(array)?.get(index)?.resolve(arena),
                _ => return None,
            }
        } else if let Some((key, value)) = step.split_once('=') {
            let Object::Array(array) = at else { return None };
            arena.get_array(array)?.into_iter().map(|i| i.resolve(arena)).find(|item| {
                entry(arena, item, key)
                    .and_then(|n| n.as_name())
                    .and_then(|n| arena.get_name_str(n))
                    .is_some_and(|n| n == value)
            })?
        } else {
            entry(arena, &at, step)?
        };
    }
    Some(at)
}

/// The names a row distinguishes; any other reads as [`Seen::Present`].
const NAMES: &[&str] = &["Compatible", "Normal", "Multiply"];

/// What is at `path`, as a row says it.
fn seen(arena: &PdfArena, start: Option<Object>, path: &[&str]) -> Seen {
    let Some(value) = start.and_then(|s| walk(arena, s, path)) else { return Seen::Absent };
    match value {
        Object::Name(n) => {
            let spelled = arena.get_name_str(n);
            NAMES
                .iter()
                .find(|k| Some(**k) == spelled.as_deref())
                .map_or(Seen::Present, |k| Seen::Name(k))
        }
        Object::Array(array) => {
            let items = arena.get_array(array).unwrap_or_default();
            if items.iter().any(|i| matches!(i, Object::Integer(_))) {
                Seen::ArrayWithInteger
            } else if items.iter().all(|i| matches!(i, Object::Name(_))) && !items.is_empty() {
                Seen::Array
            } else {
                Seen::Present
            }
        }
        _ => Seen::Present,
    }
}

/// The probe, opened.
fn opened() -> PdfDocument {
    PdfDocument::open(fepdf_fixtures::deprecated_in_2_0().into()).expect("the probe opens")
}

/// The probe saved loose and uncompressed, as the bytes and what the save decided.
fn saved(doc: &PdfDocument) -> (Vec<u8>, Vec<fepdf_model::interpretation::Decision>) {
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    // One path a save: the tests of this file run in parallel in one process, and a path
    // from the process id alone let one test remove the file another was about to read.
    static SAVES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SAVES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("fepdf-deprecations-{}-{n}.pdf", std::process::id()));
    let decisions = doc.save_with_options(&path, "2.0", &options).expect("the probe saves");
    let bytes = std::fs::read(&path).expect("the save was written");
    let _ = std::fs::remove_file(&path);
    (bytes, decisions)
}

/// **Every row is what the table says the engine does now**, in the open document and in
/// what a save writes.
#[test]
fn every_row_is_as_the_table_says() {
    let doc = opened();
    let inner = doc.inner();
    let open_arena = inner.arena();
    let open_start = |from: From| match from {
        From::Catalog => Some(Object::Reference(*inner.root_handle())),
        From::Info => inner.info_handle().map(Object::Reference),
    };

    let (bytes, _) = saved(&doc);
    let raw = fepdf_model::reader::load_document(&Bytes::from(bytes)).expect("the save reads");
    let trailer = raw.trailer.map(Object::Dictionary);
    let saved_start = |from: From| {
        let key = match from {
            From::Catalog => "Root",
            From::Info => "Info",
        };
        trailer.as_ref().and_then(|t| entry(&raw.arena, t, key))
    };

    let mut table = Vec::new();
    let mut wrong = false;
    for row in ROWS {
        let open = seen(open_arena, open_start(row.from), row.path);
        let saved = seen(&raw.arena, saved_start(row.from), row.path);
        let ok = open == row.open && saved == row.saved;
        wrong |= !ok;
        table.push(format!(
            "{} #{:<2} {:<60} open {:<20} saved {:?}",
            if ok { "  " } else { "✗ " },
            row.number,
            row.path.join("/"),
            format!("{open:?}"),
            saved
        ));
    }
    assert!(!wrong, "rows differ from the table (✗):\n{}", table.join("\n"));
}

/// **#13: a saved content stream paints with `f`, never `F`.** The operator is read as
/// `f` and written as the content stream's commands are, so `F` does not survive a save.
#[test]
fn the_operator_capital_f_is_written_as_f() {
    let (bytes, _) = saved(&opened());
    let raw = fepdf_model::reader::load_document(&Bytes::from(bytes)).expect("the save reads");
    let trailer = raw.trailer.map(Object::Dictionary).expect("a trailer");
    let root = entry(&raw.arena, &trailer, "Root").expect("a catalogue");
    let page = walk(&raw.arena, root, &["Pages", "Kids", PAGE]).expect("the page");
    let Some(Object::Stream(_, data)) = page_contents(&raw.arena, &page) else {
        panic!("the page has one content stream")
    };
    let content = raw.arena.get_stream_bytes(&data).expect("the stream reads");
    let operators: Vec<&[u8]> = content.split(u8::is_ascii_whitespace).collect();
    assert!(operators.contains(&b"f".as_slice()), "{}", String::from_utf8_lossy(&content));
    assert!(!operators.contains(&b"F".as_slice()), "{}", String::from_utf8_lossy(&content));
}

/// The page's `/Contents`, as the stream object it refers to.
fn page_contents(arena: &PdfArena, page: &Object) -> Option<Object> {
    let handle = page.as_dict_handle()?;
    let contents = arena.dict_entry(handle, arena.name("Contents"))?;
    arena.get_object(contents.as_reference()?)
}

/// **What is dropped or rewritten is said**: `/ProcSet`, a font's `/Name` and the two
/// deprecated forms of `/BM` at load, a descriptor's subset claims on save — each once, by
/// its clause.
#[test]
fn each_drop_is_recorded_by_its_clause() {
    let doc = opened();
    let at_load: Vec<String> = doc.decisions().into_iter().map(|d| d.clause.into_owned()).collect();
    for clause in ["14.2", "9.6.2.1", "11.6.3", "11.3.5"] {
        assert_eq!(
            at_load.iter().filter(|c| *c == clause).count(),
            1,
            "{clause} at load: {at_load:?}"
        );
    }
    let (_, on_save) = saved(&doc);
    let on_save: Vec<String> = on_save.into_iter().map(|d| d.clause.into_owned()).collect();
    assert_eq!(on_save.iter().filter(|c| *c == "9.8.1").count(), 1, "on save: {on_save:?}");
}

/// **#10, #11: the page draws with Multiply, then Normal** — the first mode of
/// `[/Foo /Multiply]` a reader recognises, and `Compatible` as Table 134 has it (11.6.3).
#[test]
fn a_blend_mode_array_draws_with_its_first_recognised_mode() {
    let mut drawn = Recorder::new();
    opened().render_page(0, &mut drawn, Affine::IDENTITY).expect("the page draws");
    let blends: Vec<BlendMode> = drawn
        .events
        .iter()
        .filter_map(|e| if let Event::Blend(m) = e { Some(*m) } else { None })
        .collect();
    assert_eq!(blends, vec![BlendMode::Multiply, BlendMode::Normal]);
}
