//! FDF annotations (12.7.8): a document's comments written to a file of their own, and
//! such a file's comments put onto a document (ROADMAP AA-3).
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
    import_annotations(doc, source, &annots)
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
