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
    use Seen::{Absent, Array, Name, Present};
    &[
        Row { number: 1, from: From::Info, path: &["Title"], open: Absent, saved: Absent },
        Row { number: 1, from: From::Info, path: &["Trapped"], open: Absent, saved: Absent },
        row(5, &["NeedsRendering"], Present, Present),
        row(5, &["AcroForm", "XFA"], Present, Present),
        row(6, &["SpiderInfo"], Present, Absent),
        row(6, &["Pages", "Kids", PAGE, "ID"], Present, Absent),
        row(6, &["Pages", "Kids", PAGE, "Resources", "XObject", "Im1", "ID"], Present, Absent),
        row(6, &["Names", "IDS"], Present, Absent),
        row(6, &["Names", "URLS"], Present, Absent),
        row(7, &["Names", "AlternatePresentations"], Present, Absent),
        row(8, &["Pages", "Kids", PAGE, "Resources", "ProcSet"], Absent, Absent),
        row(
            9,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "F", "DOS"],
            Present,
            Absent,
        ),
        row(
            9,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "F", "Mac"],
            Present,
            Absent,
        ),
        row(
            9,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "F", "Unix"],
            Present,
            Absent,
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
        row(14, &["Pages", "Kids", PAGE, "Resources", "XObject", "Im1", "Name"], Absent, Absent),
        row(14, &["Pages", "Kids", PAGE, "Resources", "XObject", "Fm1", "Name"], Absent, Absent),
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
        row(
            16,
            &[
                "Pages",
                "Kids",
                PAGE,
                "Resources",
                "Font",
                "F3",
                "DescendantFonts",
                "0",
                "CIDSystemInfo",
                "Ordering",
            ],
            Present,
            Present,
        ),
        row(
            16,
            &[
                "Pages",
                "Kids",
                PAGE,
                "Resources",
                "Font",
                "F4",
                "DescendantFonts",
                "0",
                "CIDSystemInfo",
                "Ordering",
            ],
            Present,
            Present,
        ),
        row(18, &["Pages", "Kids", PAGE, "Annots", "Subtype=Movie"], Present, Present),
        row(18, &["OpenAction", "Sound"], Present, Present),
        row(18, &["OpenAction", "Next", "Annotation"], Present, Present),
        row(17, &["ViewerPreferences", "ViewArea"], Present, Absent),
        row(17, &["ViewerPreferences", "PrintClip"], Present, Absent),
        row(18, &["Pages", "Kids", PAGE, "Annots", "Subtype=Sound"], Present, Present),
        row(19, &["Pages", "Kids", PAGE, "Annots", "Subtype=TrapNet"], Present, Absent),
        row(
            19,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=TrapNet", "AP", "N", "TrapRegions"],
            Present,
            Absent,
        ),
        row(20, &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "Win"], Present, Present),
        row(20, &["Pages", "Kids", PAGE, "Annots", "Subtype=Link", "A", "Unix"], Present, Present),
        row(21, &["AcroForm", "NeedAppearances"], Absent, Absent),
        row(23, &["AcroForm", "Fields", "0", "V", "SubFilter"], Present, Present),
        row(23, &["Perms", "UR3", "SubFilter"], Present, Present),
        row(
            24,
            &["AcroForm", "Fields", "0", "V", "Reference", "0", "DigestMethod"],
            Present,
            Present,
        ),
        row(25, &["AcroForm", "Fields", "0", "V", "R"], Present, Present),
        row(26, &["Perms", "UR3"], Present, Present),
        row(26, &["Perms", "UR3", "Reference", "0", "TransformMethod"], Present, Present),
        row(27, &["Legal", "MovieActions"], Present, Present),
        row(27, &["Legal", "SoundActions"], Present, Present),
        row(27, &["Legal", "ExternalOPIdicts"], Present, Present),
        row(
            28,
            &["Pages", "Kids", PAGE, "Annots", "Subtype=3D", "3DD", "Resources"],
            Present,
            Absent,
        ),
        row(29, &["MarkInfo", "Suspects"], Present, Absent),
        row(30, &["StructTreeRoot", "K", "0", "R"], Absent, Absent),
        row(30, &["StructTreeRoot", "K", "0", "C"], Array, Array),
        row(30, &["StructTreeRoot", "K", "0", "A"], Present, Present),
        row(31, &["StructTreeRoot", "K", "0", "A", "O=PrintField", "checked"], Absent, Absent),
        row(31, &["StructTreeRoot", "K", "0", "A", "O=PrintField", "Checked"], Present, Present),
        row(32, &["Pages", "Kids", PAGE, "SeparationInfo"], Present, Absent),
        row(33, &["Pages", "Kids", PAGE, "Resources", "XObject", "Im1", "OPI"], Present, Absent),
        row(33, &["Pages", "Kids", PAGE, "Resources", "XObject", "Fm1", "OPI"], Present, Absent),
        row(34, &["StructTreeRoot", "K", "1", "K", "1", "Obj"], Present, Present),
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

/// **What is dropped or rewritten is said**: at load `/ProcSet`, a font's, an image's and a
/// form's `/Name`, the two deprecated forms of `/BM`, `checked` and the revision numbers; on
/// save a descriptor's subset claims and the nine kinds `drop_for_output` leaves out — each
/// once, by its clause. `/Trapped` moves into the
/// packet and loses nothing, so it is not said (`ARCHITECTURE.md` §4.3).
#[test]
fn each_drop_is_recorded_by_its_clause() {
    let doc = opened();
    let at_load: Vec<String> = doc.decisions().into_iter().map(|d| d.clause.into_owned()).collect();
    for clause in ["14.2", "9.6.2.1", "11.6.3", "11.3.5", "8.9.5", "8.10.2", "14.8.5.6", "14.7.6.3"]
    {
        assert_eq!(
            at_load.iter().filter(|c| *c == clause).count(),
            1,
            "{clause} at load: {at_load:?}"
        );
    }
    let (_, on_save) = saved(&doc);
    let on_save: Vec<String> = on_save.into_iter().map(|d| d.clause.into_owned()).collect();
    for clause in [
        "9.8.1", "14.10", "13.5", "12.2", "14.7.1", "14.11.4", "14.11.7", "14.11.6", "7.11.3",
        "13.6.3",
    ] {
        assert_eq!(
            on_save.iter().filter(|c| *c == clause).count(),
            1,
            "{clause} on save: {on_save:?}"
        );
    }
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

/// **#1: `/Trapped` leaves `/Info` for the packet**, as `pdf:Trapped`, which a reader of
/// the saved file finds with the value `/Info` gave.
#[test]
fn trapped_moves_into_the_packet() {
    let (bytes, _) = saved(&opened());
    let reread = PdfDocument::open(bytes.into()).expect("the save reads");
    // The saved `/Info` has no `/Trapped` (row #1), so this can only be the packet's.
    assert_eq!(reread.metadata().trapped.as_deref(), Some("False"));
}

/// **#19: the trap network annotation is not written at all** — not only gone from
/// `/Annots`, which the row checks, but absent from every object of the saved file, so
/// nothing is left that no page reaches.
#[test]
fn no_trap_network_is_written() {
    let (bytes, _) = saved(&opened());
    let raw = fepdf_model::reader::load_document(&Bytes::from(bytes)).expect("the save reads");
    let trap_networks = raw
        .arena
        .all_dict_handles()
        .into_iter()
        .filter(|h| {
            raw.arena
                .dict_entry(*h, raw.arena.name("Subtype"))
                .and_then(|s| s.as_name())
                .and_then(|n| raw.arena.get_name_str(n))
                .is_some_and(|n| n == "TrapNet")
        })
        .count();
    assert_eq!(trap_networks, 0);
}

/// The `/P` an encrypted save of the probe writes, given `permissions`.
fn written_p(permissions: Option<&str>) -> i64 {
    let options = SaveOptions {
        password: Some("probe".to_string()),
        permissions: permissions.map(str::to_string),
        ..SaveOptions::default()
    };
    let path = std::env::temp_dir()
        .join(format!("fepdf-deprecations-p-{}-{permissions:?}.pdf", std::process::id()));
    let _ = opened().save_with_options(&path, "2.0", &options).expect("the probe saves");
    let bytes = std::fs::read(&path).expect("the save was written");
    let _ = std::fs::remove_file(&path);
    let raw = fepdf_model::reader::load_document(&Bytes::from(bytes)).expect("the save reads");
    let trailer = raw.trailer.map(Object::Dictionary).expect("a trailer");
    let encrypt = entry(&raw.arena, &trailer, "Encrypt").expect("an encryption dictionary");
    entry(&raw.arena, &encrypt, "P").and_then(|p| p.as_integer()).expect("a /P")
}

/// **#4: bit 10 of `/P` is always set**, which is what 2.0 asks of a writer once it
/// deprecates the restriction: "PDF writers shall always set this bit to 1" (Table 22).
/// With permissions named and `accessibility` not among them, it was cleared.
#[test]
fn permission_bit_10_is_always_set() {
    let bit_10 = |p: i64| p & (1 << 9) != 0;
    assert!(bit_10(written_p(None)), "with every permission granted");
    assert!(bit_10(written_p(Some("print"))), "with print named and accessibility not");
    assert!(bit_10(written_p(Some(""))), "with nothing granted");
}

/// A form saying `/NeedAppearances true` over a text field whose appearance shows the
/// value it held before (`Old`) and a check box whose `/AS` is not its `/V`, with
/// `extra` added to the page's annotations.
fn stale_form(extra: &str) -> PdfDocument {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 6 0 R] /NeedAppearances true \
           /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv << /Type /Font /Subtype /Type1 \
           /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> >> >> >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [4 0 R 6 0 R{extra}] >>"
        ),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (New) /Rect [10 10 110 30] \
           /P 3 0 R /AP << /N 5 0 R >> >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 100 20] /Resources << >> /Length 33 >>\n\
         stream\nBT /Helv 12 Tf 2 5 Td (Old) Tj ET\nendstream",
        "<< /Type /Annot /Subtype /Widget /FT /Btn /T (box) /V /Yes /AS /Off \
           /Rect [10 40 30 60] /P 3 0 R /AP << /N << /Yes 7 0 R /Off 7 0 R >> >> >>",
        "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Resources << >> /Length 0 >>\n\
         stream\n\nendstream",
        "<< /Type /Annot /Subtype /Widget /FT /Sig /T (unsigned) /Rect [10 70 110 90] /P 3 0 R >>",
    ]);
    PdfDocument::open(bytes.into()).expect("the form opens")
}

/// A widget's entry `key` on the open document's page.
fn widget_entry(doc: &PdfDocument, index: usize, key: &str) -> Option<Object> {
    let arena = doc.inner().arena();
    let page = Object::Reference(doc.inner().get_page_handle(0).expect("a page"));
    let annots = entry(arena, &page, "Annots")?;
    let widget = walk(arena, annots, &[&index.to_string()])?;
    entry(arena, &widget, key)
}

/// **#21: `/NeedAppearances true` is done and then dropped.** The text field's appearance
/// is rebuilt from `/V` and no longer shows `Old`; the check box shows the state `/V`
/// names; the entry goes, and that is said.
#[test]
fn need_appearances_rebuilds_from_the_values_and_goes() {
    let doc = stale_form("");
    let arena = doc.inner().arena();
    let ap = widget_entry(&doc, 0, "AP").expect("the text field has an appearance");
    let normal = entry(arena, &ap, "N").expect("a normal appearance");
    let Object::Stream(_, data) = normal else { panic!("a stream") };
    let content =
        String::from_utf8_lossy(&arena.get_stream_bytes(&data).expect("it reads")).into_owned();
    assert!(!content.contains("Old"), "the stale appearance stayed: {content}");
    let state = widget_entry(&doc, 1, "AS").and_then(|s| s.as_name());
    assert_eq!(state.and_then(|n| arena.get_name_str(n)).as_deref(), Some("Yes"));
    let catalog = Object::Reference(*doc.inner().root_handle());
    let form = entry(arena, &catalog, "AcroForm").expect("the form");
    assert!(entry(arena, &form, "NeedAppearances").is_none(), "the entry stayed");
    assert!(doc.decisions().iter().any(|d| d.clause == "12.7.3"), "{:?}", doc.decisions());
}

/// **The entry stays while a widget still lacks the appearance it asks for** — here an
/// unsigned signature field with a page-sized `/Rect`, which nothing here draws.
#[test]
fn need_appearances_stays_while_a_widget_lacks_one() {
    let doc = stale_form(" 8 0 R");
    let arena = doc.inner().arena();
    let catalog = Object::Reference(*doc.inner().root_handle());
    let form = entry(arena, &catalog, "AcroForm").expect("the form");
    assert!(entry(arena, &form, "NeedAppearances").is_some(), "the entry went");
}

/// **#16: a font of a collection this engine cannot read is said once**, not once for the
/// Type 0 font and again for its descendant CIDFont loaded as a font of its own.
#[test]
fn a_declined_collection_is_said_once() {
    let declined = opened().decisions().into_iter().filter(|d| d.clause == "9.7.3").count();
    assert_eq!(declined, 1);
}

/// **#16: Adobe-Korea1 text is read through its own table**, which is fetched, so CID 34
/// comes out as `A` and the font is not declined. Adobe-Japan2, whose table is not
/// fetched, is the one declined.
#[test]
fn korea1_text_is_read_through_its_own_table() {
    let doc = opened();
    let text = doc.extract_text(0).expect("the page reads");
    assert!(text.contains('A'), "{text:?}");
    let declined: Vec<String> =
        doc.decisions().into_iter().filter(|d| d.clause == "9.7.3").map(|d| d.found).collect();
    assert!(declined.iter().all(|f| !f.contains("Korea1")), "{declined:?}");
}

/// The colour a page's one fill is drawn in, with `gs` naming a graphics state that sets
/// `/TR` to `f(x) = 1 - x` when `with_transfer`.
fn fill_colour(with_transfer: bool) -> Vec<fepdf_content::Color> {
    let gs = if with_transfer { "/G1 gs " } else { "" };
    let content = format!("{gs}0.25 g 0 0 10 10 re f");
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 20 20] /Contents 4 0 R /Resources \
           << /ExtGState << /G1 << /TR << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >> \
           >> >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the page opens");
    let mut drawn = Recorder::new();
    doc.render_page(0, &mut drawn, Affine::IDENTITY).expect("the page draws");
    drawn
        .events
        .iter()
        .filter_map(|e| if let Event::Fill { color, .. } = e { Some(*color) } else { None })
        .collect()
}

/// **#12: a graphics state's `/TR` is kept and not applied** — the fill is drawn in the
/// colour it names, as though there were no transfer function (ADR-0029 declines them).
#[test]
fn a_transfer_function_is_not_applied() {
    let (with, without) = (fill_colour(true), fill_colour(false));
    assert_eq!(with.len(), 1);
    assert_eq!(format!("{with:?}"), format!("{without:?}"));
}

/// **#2, #3: an encrypted save is AES-256 under the standard handler's revision 6**, the
/// one scheme 2.0 does not deprecate (ADR-0015).
#[test]
fn an_encrypted_save_is_v5_r6() {
    let options = SaveOptions { password: Some("probe".to_string()), ..SaveOptions::default() };
    let path =
        std::env::temp_dir().join(format!("fepdf-deprecations-v5-{}.pdf", std::process::id()));
    let _ = opened().save_with_options(&path, "2.0", &options).expect("the probe saves");
    let bytes = std::fs::read(&path).expect("the save was written");
    let _ = std::fs::remove_file(&path);
    let raw = fepdf_model::reader::load_document(&Bytes::from(bytes)).expect("the save reads");
    let trailer = raw.trailer.map(Object::Dictionary).expect("a trailer");
    let encrypt = entry(&raw.arena, &trailer, "Encrypt").expect("an encryption dictionary");
    let number = |key: &str| entry(&raw.arena, &encrypt, key).and_then(|v| v.as_integer());
    assert_eq!((number("V"), number("R")), (Some(5), Some(6)));
    let cfm = walk(&raw.arena, encrypt, &["CF", "StdCF", "CFM"])
        .and_then(|n| n.as_name())
        .and_then(|n| raw.arena.get_name_str(n));
    assert_eq!(cfm.as_deref(), Some("AESV3"));
}

/// **What 2.0 removed is kept through a save**, as a feature with no successor that the
/// engine reads and does not make: a PostScript XObject and its `Do`, a marked clipping
/// sequence tagged `Clip`, and one tagged `TagSuspect` (1.7 8.8.2, 14.6.3, 14.8.2.3).
#[test]
fn removed_features_through_a_save() {
    let (bytes, _) = saved(&opened());
    let raw = fepdf_model::reader::load_document(&Bytes::from(bytes)).expect("the save reads");
    let trailer = raw.trailer.map(Object::Dictionary).expect("a trailer");
    let root = entry(&raw.arena, &trailer, "Root").expect("a catalogue");
    let page = walk(&raw.arena, root, &["Pages", "Kids", PAGE]).expect("the page");
    let ps = walk(&raw.arena, page.clone(), &["Resources", "XObject", "PS1"]);
    let Some(Object::Stream(_, data)) = page_contents(&raw.arena, &page) else {
        panic!("the page has one content stream")
    };
    let content =
        String::from_utf8_lossy(&raw.arena.get_stream_bytes(&data).expect("it reads")).into_owned();
    assert!(ps.is_some(), "the PostScript XObject went");
    for kept in ["/Clip BMC", "/TagSuspect << /TagSuspect /Ordering >> BDC", "/PS1 Do"] {
        assert!(content.contains(kept), "{kept} went: {content}");
    }
}

/// **#22: an FDF export writes no encryption**, of the comments or of the form values.
#[test]
fn an_fdf_export_writes_no_encryption() {
    let doc = opened();
    for (what, bytes) in [
        ("comments", doc.export_fdf().expect("comments export")),
        ("form", doc.export_fdf_form().expect("form values export")),
    ] {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("/Encrypt"), "{what}: an /Encrypt was written");
        assert!(!text.contains("EncryptionRevision"), "{what}: an encrypted FDF was written");
    }
}

/// **#22: an FDF import says it leaves out the FDF files it embeds**, and how many are
/// encrypted, rather than leaving them out unseen; the rest of the import goes ahead.
#[test]
fn an_fdf_import_says_its_embedded_fdfs_are_left_out() {
    let fdf = fepdf_fixtures::Pdf::new().version("1.2").assemble(&[
        "<< /FDF << /Annots [] /EmbeddedFDFs [2 0 R 4 0 R] >> >>",
        "<< /Type /Filespec /F (locked.fdf) /EF << /F 3 0 R >> >>",
        "<< /Type /EmbeddedFile /EncryptionRevision 1 /Length 4 >>\nstream\nabcd\nendstream",
        "<< /Type /Filespec /F (plain.fdf) /EF << /F 5 0 R >> >>",
        "<< /Type /EmbeddedFile /Length 4 >>\nstream\nabcd\nendstream",
    ]);
    let mut doc = opened();
    doc.apply(fepdf::Operation::ImportFdf { fdf }).expect("the import goes ahead");
    let said: Vec<String> =
        doc.decisions().into_iter().filter(|d| d.clause == "12.7.8.3.1").map(|d| d.found).collect();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("embeds 2") && said[0].contains("1 of them encrypted"), "{said:?}");
}
