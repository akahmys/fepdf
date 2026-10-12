//! FDF annotations (12.7.8): a document's comments written to a file of their own, and
//! such a file's comments put onto a document (ROADMAP AA-3).
//!
//! **Its form values are `/Fields`**, read and written here and set by `form_data`
//! (AA-3b, ADR-0125).
//!
//! **An FDF annotation is the document's annotation without the document.** `/P` names
//! a page object of the file it came from, `/Popup` a window of that file's viewer, and
//! `/StructParent` a key into that file's structure tree. None of them means anything in
//! another file, so the export leaves them out and says which page with `/Page`
//! (Table 254), and the import puts back what the target document gives.
//!
//! **What answers what survives**: `/IRT` is a reference, written into the FDF as a
//! reference to the exported annotation it answers, and read back as one.

use crate::cloning::ObjectCloner;
use fepdf_model::interpretation::Decision;
use fepdf_model::{DictHandle, Document, Handle, Object, PdfArena, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// What Table 246 keeps out of an FDF's `/Annots`.
const NOT_IN_FDF: &[&str] = &["Link", "Movie", "Widget", "PrinterMark", "Screen", "TrapNet"];

/// Entries that tie an annotation to the file it is in. Left out of an export, and not
/// taken from an import; `/IRT` is carried across separately, as a reference remapped.
const OF_THE_FILE: &[&str] = &["P", "Popup", "Parent", "StructParent", "IRT", "Page"];

/// Every markup annotation of `doc` as an FDF file (12.7.8).
///
/// Each carries `/Page`, which Table 254 requires, and an `/NM`: its own, or
/// `fepdf-p<page>-<index>` where it has none, so that a document it is imported into can
/// match it the next time ([ADR-0117]). Pop-ups are not exported; they are the viewer's.
///
/// [ADR-0117]: ../../../docs/adr/0117-an-fdf-import-replaces-an-annotation-of-the-same-name.md
///
/// # Errors
/// When a page will not read, an annotation will not clone, or the file will not write.
pub fn export(doc: &Document) -> PdfResult<Vec<u8>> {
    let source = doc.arena();
    let chosen = exportable(doc)?;
    let target = PdfArena::new();
    // Every exported annotation's place in the FDF is known before any is filled, so that
    // `/IRT` can name one that comes later.
    let placed: BTreeMap<Handle<Object>, Handle<Object>> =
        chosen.iter().map(|c| (c.handle, target.alloc_object(Object::Null))).collect();
    let mut cloner = ObjectCloner::new(source, &target);
    for c in &chosen {
        let dict = exported(source, &mut cloner, c, &placed)?;
        if let Some(slot) = placed.get(&c.handle) {
            target.set_object(*slot, Object::Dictionary(target.alloc_dict(dict)));
        }
    }
    let annots: Vec<Object> = chosen
        .iter()
        .filter_map(|c| placed.get(&c.handle))
        .map(|h| Object::Reference(*h))
        .collect();
    let mut fdf = Dict::new();
    fdf.insert(target.name("Annots"), Object::Array(target.alloc_array(annots)));
    let mut catalog = Dict::new();
    catalog.insert(target.name("FDF"), Object::Dictionary(target.alloc_dict(fdf)));
    // The version of the specification it conforms to, which the header cannot say
    // (Table 245): the annotations are this engine's, and it writes 2.0.
    catalog.insert(target.name("Version"), Object::Name(target.name("2.0")));
    let root = target.alloc_object(Object::Dictionary(target.alloc_dict(catalog)));
    write(&target, root)
}

/// An annotation chosen for export.
pub(crate) struct Chosen {
    pub(crate) page: usize,
    pub(crate) index: usize,
    pub(crate) handle: Handle<Object>,
}

/// The markup annotations of every page, in page and `/Annots` order.
pub(crate) fn exportable(doc: &Document) -> PdfResult<Vec<Chosen>> {
    let mut chosen = Vec::new();
    for page in 0..doc.page_count()? {
        let annots = crate::apply::redact_annots::annotations_on(doc, page)?;
        for comment in crate::comments::on_page(doc, page)? {
            if !comment.markup || NOT_IN_FDF.contains(&comment.subtype.as_str()) {
                continue;
            }
            if let Some(handle) = annots.get(comment.at.index).and_then(Object::as_reference) {
                chosen.push(Chosen { page, index: comment.at.index, handle });
            }
        }
    }
    Ok(chosen)
}

/// The FDF dictionary of the annotation `c` names.
fn exported(
    source: &PdfArena,
    cloner: &mut ObjectCloner<'_>,
    c: &Chosen,
    placed: &BTreeMap<Handle<Object>, Handle<Object>>,
) -> PdfResult<Dict> {
    let target = cloner.target();
    let entries = dict_of(source, c.handle).and_then(|d| source.get_dict(d)).unwrap_or_default();
    let mut dict = Dict::new();
    for (key, value) in &entries {
        let name = source.get_name_str(*key).unwrap_or_default();
        if OF_THE_FILE.contains(&name.as_str()) {
            continue;
        }
        dict.insert(target.name(&name), cloner.clone_complete(value)?);
    }
    let answered = entries.get(&source.name("IRT")).and_then(Object::as_reference);
    if let Some(to) = answered.and_then(|h| placed.get(&h)) {
        dict.insert(target.name("IRT"), Object::Reference(*to));
    }
    dict.entry(target.name("NM"))
        .or_insert_with(|| Object::Text(format!("fepdf-p{}-{}", c.page, c.index)));
    let page = i64::try_from(c.page).map_err(|_| PdfError::internal("a page number past i64"))?;
    dict.insert(target.name("Page"), Object::Integer(page));
    Ok(dict)
}

/// `arena` written as an FDF file whose catalogue is `root`.
fn write(arena: &PdfArena, root: Handle<Object>) -> PdfResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut writer = fepdf_model::PdfWriter::new(&mut out, arena);
    // A classic cross-reference table: 12.7.8.1 makes it optional, and FDF readers older
    // than object streams read this one.
    writer.set_pack_objects(false);
    writer.write_fdf_header()?;
    writer.finish(root, None)?;
    drop(writer);
    Ok(out)
}

/// Puts the annotations of the FDF file `fdf` onto `doc` (12.7.8.3.4).
///
/// One whose `/NM` names an annotation already on its page replaces that one in place,
/// keeping its `/P`, `/Popup` and `/StructParent`; any other is added at the end of its
/// page's `/Annots` ([ADR-0117]). One whose `/Page` the document does not have, or whose
/// subtype Table 246 keeps out, is skipped with a `Decision`.
///
/// **Read whole before anything changes** ([ADR-0110]): a file that does not read
/// changes nothing.
///
/// [ADR-0117]: ../../../docs/adr/0117-an-fdf-import-replaces-an-annotation-of-the-same-name.md
/// [ADR-0110]: ../../../docs/adr/0110-an-operation-that-fails-changes-nothing.md
///
/// # Errors
/// When the file is not an FDF file with an `/FDF` dictionary, or a page will not read.
pub fn apply_import(doc: &Document, fdf: &[u8]) -> PdfResult<()> {
    let raw = fepdf_model::reader::load_document(&bytes::Bytes::copy_from_slice(fdf))?;
    let source = &raw.arena;
    let catalog = catalog(&raw)?;
    let fdf = source
        .dict_entry(catalog, source.name("FDF"))
        .and_then(|f| f.resolve(source).as_dict_handle());
    let annots: Vec<Handle<Object>> = fdf
        .and_then(|f| source.dict_entry(f, source.name("Annots")))
        .and_then(|a| a.resolve(source).as_array())
        .and_then(|a| source.get_array(a))
        .unwrap_or_default()
        .iter()
        .filter_map(Object::as_reference)
        .collect();
    let fields = fdf.map(|f| incoming_fields(source, f)).transpose()?.unwrap_or_default();
    if let Some(fdf) = fdf {
        report_embedded(doc, source, fdf);
    }
    import_annotations(doc, source, &annots)?;
    crate::form_data::put(doc, &fields, "12.7.8.3.2")
}

/// Says that the FDF file's `/EmbeddedFDFs` (Table 245) are not imported, and how many of
/// them are encrypted, which 2.0 deprecates (12.7.8.3.1; ROADMAP AB-9). An embedded FDF
/// file is not opened here, encrypted or not, so what it carries is left out — and said,
/// rather than left out unseen.
fn report_embedded(doc: &Document, source: &PdfArena, fdf: DictHandle) {
    let embedded = source
        .dict_entry(fdf, source.name("EmbeddedFDFs"))
        .and_then(|e| e.resolve(source).as_array())
        .and_then(|e| source.get_array(e))
        .unwrap_or_default();
    if embedded.is_empty() {
        return;
    }
    let encrypted = embedded
        .iter()
        .filter(|spec| {
            use fepdf_model::access::entry;
            entry(source, spec, "EF")
                .and_then(|ef| entry(source, &ef, "F"))
                .and_then(|file| entry(source, &file, "EncryptionRevision"))
                .is_some()
        })
        .count();
    doc.record(Decision::violation(
        "12.7.8.3.1",
        format!(
            "the FDF file embeds {} FDF files in /EmbeddedFDFs, {encrypted} of them encrypted, \
             which 2.0 deprecates",
            embedded.len()
        ),
        "left them out of the import: this engine does not open an embedded FDF file",
    ));
}

/// The document's form values as an FDF file: `/Fields`, each a root field with its kids,
/// a terminal field's value in `/V` (12.7.8.3.2). A signature's value is not exported.
///
/// # Errors
/// When the file will not write.
pub fn export_form(doc: &Document) -> PdfResult<Vec<u8>> {
    let target = PdfArena::new();
    let roots = crate::form_data::tree(crate::form_data::values(doc));
    let fields: Vec<Object> = roots.iter().map(|node| field_object(node, &target, 0)).collect();
    let mut fdf = Dict::new();
    fdf.insert(target.name("Fields"), Object::Array(target.alloc_array(fields)));
    let mut catalog = Dict::new();
    catalog.insert(target.name("FDF"), Object::Dictionary(target.alloc_dict(fdf)));
    catalog.insert(target.name("Version"), Object::Name(target.name("2.0")));
    let root = target.alloc_object(Object::Dictionary(target.alloc_dict(catalog)));
    write(&target, root)
}

/// A field as an FDF field dictionary.
///
/// Its kids are direct, which Table 249 allows, and go as deep as a field tree is read
/// (Rule 6).
fn field_object(node: &crate::form_data::FieldNode, arena: &PdfArena, depth: usize) -> Object {
    use crate::form_data::Given;
    let mut dict = Dict::new();
    dict.insert(arena.name("T"), Object::Text(node.name.clone()));
    match &node.value {
        Some(Given::Text(text)) => {
            dict.insert(arena.name("V"), Object::Text(text.clone()));
        }
        Some(Given::Name(state)) => {
            dict.insert(arena.name("V"), Object::Name(arena.name(state)));
        }
        Some(Given::Several(all)) => {
            let items = all.iter().map(|t| Object::Text(t.clone())).collect();
            dict.insert(arena.name("V"), Object::Array(arena.alloc_array(items)));
        }
        None => {}
    }
    if !node.kids.is_empty() && depth < 64 {
        let kids = node.kids.iter().map(|kid| field_object(kid, arena, depth + 1)).collect();
        dict.insert(arena.name("Kids"), Object::Array(arena.alloc_array(kids)));
    }
    Object::Dictionary(arena.alloc_dict(dict))
}

/// Entries of Table 249 this does not import.
const NOT_IMPORTED: [&str; 7] = ["AP", "APRef", "IF", "Opt", "A", "AA", "RV"];

/// The fields of the FDF dictionary `fdf`, by fully qualified name: each that carries a
/// value or a flag, read down `/Kids` as far as a field tree is read (Rule 6).
///
/// # Errors
/// Refused when `/Encoding` names one Table 246 does not: its names and values would be
/// read as the wrong characters.
fn incoming_fields(
    source: &PdfArena,
    fdf: DictHandle,
) -> PdfResult<Vec<crate::form_data::Incoming>> {
    let encoding = source
        .dict_entry(fdf, source.name("Encoding"))
        .and_then(|e| e.as_name())
        .and_then(|e| source.get_name_str(e));
    let strings = Strings::declared(encoding.as_deref()).ok_or_else(|| {
        PdfError::refused(
            "ImportFdf",
            format!("its strings are in {encoding:?}, which Table 246 does not name"),
        )
    })?;
    let roots = source
        .dict_entry(fdf, source.name("Fields"))
        .and_then(|f| f.resolve(source).as_array())
        .and_then(|f| source.get_array(f))
        .unwrap_or_default();
    let mut found = Vec::new();
    let mut pending: Vec<(Object, Option<String>, usize)> =
        roots.into_iter().rev().map(|f| (f, None, 0)).collect();
    while let Some((field, parent, depth)) = pending.pop() {
        let Some(dict) = field.resolve(source).as_dict_handle() else { continue };
        let own =
            source.dict_entry(dict, source.name("T")).and_then(|t| fdf_text(source, &t, strings));
        let name = match (parent, own) {
            (Some(parent), Some(own)) => format!("{parent}.{own}"),
            (Some(parent), None) => parent,
            (None, Some(own)) => own,
            (None, None) => continue,
        };
        if let Some(field) = incoming_field(source, dict, &name, strings) {
            found.push(field);
        }
        let kids = source
            .dict_entry(dict, source.name("Kids"))
            .and_then(|k| k.resolve(source).as_array())
            .and_then(|k| source.get_array(k))
            .unwrap_or_default();
        if depth < 64 {
            pending.extend(kids.into_iter().rev().map(|k| (k, Some(name.clone()), depth + 1)));
        }
    }
    Ok(found)
}

/// One FDF field dictionary, where it carries a value or a flag.
fn incoming_field(
    source: &PdfArena,
    dict: DictHandle,
    name: &str,
    strings: Strings,
) -> Option<crate::form_data::Incoming> {
    use crate::form_data::{Flags, Given, Incoming};
    let integer =
        |key: &str| source.dict_entry(dict, source.name(key)).and_then(|v| v.as_integer());
    let flags = Flags {
        ff: integer("Ff"),
        set_ff: integer("SetFf"),
        clr_ff: integer("ClrFf"),
        f: integer("F"),
        set_f: integer("SetF"),
        clr_f: integer("ClrF"),
    };
    let value = source.dict_entry(dict, source.name("V")).map(|v| v.resolve(source)).and_then(
        |v| match v {
            Object::Name(n) => source.get_name_str(n).map(Given::Name),
            Object::Array(items) => Some(Given::Several(
                source
                    .get_array(items)?
                    .iter()
                    .filter_map(|i| fdf_text(source, i, strings))
                    .collect(),
            )),
            other => fdf_text(source, &other, strings).map(Given::Text),
        },
    );
    if value.is_none() && flags == Flags::default() {
        return None;
    }
    let unread = NOT_IMPORTED
        .iter()
        .filter(|key| source.dict_entry(dict, source.name(key)).is_some())
        .map(|key| format!("/{key}"))
        .collect();
    Some(Incoming { name: name.to_owned(), value, flags, unread })
}

/// How an FDF file's field names, values and options are decoded: its `/Encoding`
/// (Table 246), for every string that does not begin with U+FEFF.
#[derive(Debug, Clone, Copy)]
enum Strings {
    /// PDFDocEncoding, the default, or `utf_16`: a PDF text string either way.
    Pdf,
    /// `utf_8`.
    Utf8,
    /// `Shift_JIS`, `BigFive`, `GBK` or `UHC`, decoded as the WHATWG Encoding Standard
    /// decodes them — Shift_JIS with Windows' extensions, as Windows writes it, and UHC as
    /// the superset of EUC-KR that the standard calls EUC-KR.
    Legacy(&'static encoding_rs::Encoding),
}

impl Strings {
    /// The decoding `/Encoding` names; none where Table 246 does not name it.
    fn declared(name: Option<&str>) -> Option<Self> {
        Some(match name {
            None | Some("PDFDocEncoding" | "utf_16") => Self::Pdf,
            Some("utf_8") => Self::Utf8,
            Some("Shift_JIS") => Self::Legacy(encoding_rs::SHIFT_JIS),
            Some("BigFive") => Self::Legacy(encoding_rs::BIG5),
            Some("GBK") => Self::Legacy(encoding_rs::GBK),
            Some("UHC") => Self::Legacy(encoding_rs::EUC_KR),
            Some(_) => return None,
        })
    }
}

/// A string of an FDF file as text, decoded as `/Encoding` says, or as a PDF text string
/// where it begins with U+FEFF or the encoding is PDFDocEncoding. A string not valid in
/// its encoding is no text, rather than one with replacement characters in it.
fn fdf_text(source: &PdfArena, object: &Object, strings: Strings) -> Option<String> {
    let object = object.resolve(source);
    let bytes = match &object {
        Object::String(bytes) | Object::Hex(bytes) if !bytes.starts_with(&[0xFE, 0xFF]) => bytes,
        _ => return crate::apply::fields::text_of(source, &object),
    };
    match strings {
        Strings::Pdf => crate::apply::fields::text_of(source, &object),
        Strings::Utf8 => String::from_utf8(bytes.to_vec()).ok(),
        Strings::Legacy(encoding) => encoding
            .decode_without_bom_handling_and_without_replacement(bytes)
            .map(std::borrow::Cow::into_owned),
    }
}

/// Puts the annotations `annots` of `source` onto `doc`: an FDF file's, or an XFDF file's
/// built into an arena of their own. One whose `/NM` matches an annotation on its page
/// replaces it in place, and any other is added (ADR-0117).
///
/// `/IRT` is a reference to another of `annots`, or a text string naming an annotation
/// already on the page, which is how XFDF's `inreplyto` names one outside its file.
///
/// # Errors
/// When a page will not read or an annotation will not clone.
pub(crate) fn import_annotations(
    doc: &Document,
    source: &PdfArena,
    annots: &[Handle<Object>],
) -> PdfResult<()> {
    let incoming = incoming(doc, source, annots)?;
    let target = doc.arena();
    // Where each incoming annotation will be: the annotation it replaces, or a new object.
    let mut placed: BTreeMap<Handle<Object>, Handle<Object>> = BTreeMap::new();
    for i in &incoming {
        let slot = i.replaces.unwrap_or_else(|| target.alloc_object(Object::Null));
        placed.insert(i.handle, slot);
    }
    let mut cloner = ObjectCloner::new(source, target);
    let mut built = Vec::new();
    for i in &incoming {
        built.push((i, imported(doc, (source, &mut cloner), i, &placed)?));
    }
    for (i, dict) in built {
        let Some(slot) = placed.get(&i.handle).copied() else { continue };
        match i.replaces.and_then(|h| dict_of(target, h)) {
            Some(existing) => target.set_dict(existing, dict),
            None => {
                target.set_object(slot, Object::Dictionary(target.alloc_dict(dict)));
                crate::apply::annotations::append_to_page(doc, doc.page_handle(i.page)?, slot)?;
            }
        }
    }
    Ok(())
}

/// The catalogue of an FDF file: the trailer's `/Root`, or else the dictionary that holds
/// `/FDF`. 12.7.8.1 makes the cross-reference optional, and the trailer is found through
/// it, so a file without one is read by what its catalogue must contain.
fn catalog(raw: &fepdf_model::reader::RawDocument) -> PdfResult<DictHandle> {
    let arena = &raw.arena;
    let fdf = arena.name("FDF");
    let from_trailer = raw
        .trailer
        .and_then(|t| arena.dict_entry(t, arena.name("Root")))
        .and_then(|r| r.resolve(arena).as_dict_handle());
    from_trailer
        .filter(|d| arena.dict_entry(*d, fdf).is_some())
        .or_else(|| {
            arena.all_dict_handles().into_iter().find(|d| arena.dict_entry(*d, fdf).is_some())
        })
        .ok_or_else(|| {
            PdfError::refused("ImportFdf", "this is not an FDF file: no /FDF dictionary")
        })
}

/// An annotation of the file that will be put on the document.
struct Incoming {
    handle: Handle<Object>,
    page: usize,
    /// The annotation on that page with the same `/NM`, which this replaces.
    replaces: Option<Handle<Object>>,
}

/// The annotations of the file that the document can take, each with its page and what
/// it replaces. What it cannot take is recorded and left out.
fn incoming(
    doc: &Document,
    source: &PdfArena,
    annots: &[Handle<Object>],
) -> PdfResult<Vec<Incoming>> {
    let pages = doc.page_count()?;
    let mut taken = Vec::new();
    for handle in annots.iter().copied() {
        let page = entry(source, handle, "Page")
            .and_then(|p| p.as_integer())
            .and_then(|p| usize::try_from(p).ok());
        let subtype = entry(source, handle, "Subtype")
            .and_then(|s| s.as_name())
            .and_then(|n| source.get_name_str(n));
        let Some(page) = page.filter(|p| *p < pages) else {
            doc.record(Decision::violation(
                "12.7.8.3.4",
                format!("an FDF annotation names page {page:?}, and the document has {pages}"),
                "left it out of the import",
            ));
            continue;
        };
        if subtype.as_deref().is_none_or(|s| NOT_IN_FDF.contains(&s)) {
            doc.record(Decision::violation(
                "12.7.8.3.1",
                format!("an FDF annotation of subtype {subtype:?}, which Table 246 keeps out"),
                "left it out of the import",
            ));
            continue;
        }
        let replaces = text(source, handle, "NM").and_then(|nm| named_on(doc, page, &nm));
        taken.push(Incoming { handle, page, replaces });
    }
    Ok(taken)
}

/// The annotation on `page` named `name`.
fn named_on(doc: &Document, page: usize, name: &str) -> Option<Handle<Object>> {
    let arena = doc.arena();
    crate::apply::redact_annots::annotations_on(doc, page)
        .ok()?
        .iter()
        .filter_map(Object::as_reference)
        .find(|h| text(arena, *h, "NM").as_deref() == Some(name))
}

/// The document's dictionary for the incoming annotation `i`.
fn imported(
    doc: &Document,
    (source, cloner): (&PdfArena, &mut ObjectCloner<'_>),
    i: &Incoming,
    placed: &BTreeMap<Handle<Object>, Handle<Object>>,
) -> PdfResult<Dict> {
    let target = doc.arena();
    let entries = dict_of(source, i.handle).and_then(|d| source.get_dict(d)).unwrap_or_default();
    let mut dict = Dict::new();
    for (key, value) in &entries {
        let name = source.get_name_str(*key).unwrap_or_default();
        if !OF_THE_FILE.contains(&name.as_str()) {
            dict.insert(target.name(&name), cloner.clone_complete(value)?);
        }
    }
    let answered = match entries.get(&source.name("IRT")) {
        Some(Object::Reference(h)) => placed.get(h).copied(),
        // A name, as XFDF's `inreplyto` gives one: the annotation of that name on the page.
        Some(name) => {
            crate::apply::fields::text_of(source, name).and_then(|n| named_on(doc, i.page, &n))
        }
        None => None,
    };
    if let Some(to) = answered {
        dict.insert(target.name("IRT"), Object::Reference(to));
    }
    dict.insert(target.name("P"), Object::Reference(doc.page_handle(i.page)?));
    // An FDF annotation can come without its appearance, and Table 166 requires one
    // (ADR-0119).
    if !dict.contains_key(&target.name("AP"))
        && let Some(appearance) = crate::apply::drawn::appearance_for(doc, &dict)?
    {
        dict.insert(target.name("AP"), appearance);
    }
    // What ties the annotation it replaces to this document stays: its pop-up, and its
    // key in the structure tree.
    for key in ["Popup", "StructParent"] {
        if let Some(kept) = i.replaces.and_then(|h| entry(target, h, key)) {
            dict.insert(target.name(key), kept);
        }
    }
    Ok(dict)
}

fn dict_of(arena: &PdfArena, handle: Handle<Object>) -> Option<DictHandle> {
    arena.get_object(handle)?.as_dict_handle()
}

fn entry(arena: &PdfArena, handle: Handle<Object>, key: &str) -> Option<Object> {
    arena.dict_entry(dict_of(arena, handle)?, arena.name(key))
}

fn text(arena: &PdfArena, handle: Handle<Object>, key: &str) -> Option<String> {
    crate::apply::fields::text_of(arena, &entry(arena, handle, key)?)
}
