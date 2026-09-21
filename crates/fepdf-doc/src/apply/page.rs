#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
use crate::operation::{
    ContentScale, PageArrangement, PageDivision, PageLabelSpec, PageLabelStyle, PageResize,
    PageSelection, PdfStandard, RotateMode, WhatFallsOutside,
};
use bytes::Bytes;
use fepdf_model::{Document, Object, PdfError, PdfResult};
use std::collections::BTreeMap;

/// Applies page rotation to selected pages.
pub fn apply_rotate(doc: &mut Document, pages: &PageSelection, mode: &RotateMode) -> PdfResult<()> {
    let count = doc.page_count()?;
    let indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    for idx in indices {
        if idx < count {
            let page = doc.get_page(idx)?;
            let current = if let Some(Object::Integer(angle)) = page.resolve_attribute("Rotate") {
                let normalized = (angle % 360) as i32;
                normalized.rem_euclid(360)
            } else {
                0
            };
            let target = match mode {
                RotateMode::Absolute(q) => q.to_degrees(),
                RotateMode::Relative(q) => (current + q.to_degrees()).rem_euclid(360),
            };
            let page_dh = doc.resolve_to_dict(page.obj_handle())?;
            let arena = doc.arena();
            let mut dict = arena.get_dict(page_dh).unwrap_or_default();
            dict.insert(arena.name("Rotate"), Object::Integer(i64::from(target)));
            arena.set_dict(page_dh, dict);
        }
    }
    Ok(())
}

/// Moves a page from one index to another.
pub fn apply_reorder(doc: &mut Document, from: usize, to: usize) -> PdfResult<()> {
    let count = doc.page_count()?;
    if from >= count || to >= count {
        return Err(fepdf_model::PdfError::Arena("Page index out of bounds".into()));
    }
    doc.reorder_page(from, to)
}

fn prune_struct_tree_pages(doc: &Document, removed: &[fepdf_model::Handle<Object>]) {
    if removed.is_empty() {
        return;
    }
    let Ok(Some(root_h)) = doc.get_structure_root() else {
        return;
    };
    let removed_set: std::collections::BTreeSet<_> = removed.iter().copied().collect();
    let mut visitor = crate::structure::StructureVisitor::new(doc.arena(), root_h);
    let pg_key = doc.arena().name("Pg");

    while let Some(elem_h) = visitor.next_element() {
        if let Some(dh) = doc.arena().get_object(elem_h).and_then(|o| o.as_dict_handle())
            && let Some(mut dict) = doc.arena().get_dict(dh)
            && let Some(Object::Reference(h)) = dict.get(&pg_key)
            && removed_set.contains(h)
        {
            dict.remove(&pg_key);
            doc.arena().set_dict(dh, dict);
        }
    }
}

/// Removes selected pages from the document.
pub fn apply_remove_pages(doc: &mut Document, pages: &PageSelection) -> PdfResult<()> {
    let count = doc.page_count()?;
    let mut indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    indices.sort_unstable();
    indices.dedup();
    let mut removed_handles = Vec::new();
    for &idx in &indices {
        if let Some(h) = doc.get_page_handle(idx) {
            removed_handles.push(h);
        }
    }
    for idx in indices.into_iter().rev() {
        if idx < count {
            doc.remove_page(idx)?;
        }
    }
    prune_struct_tree_pages(doc, &removed_handles);
    Ok(())
}

/// Resolves a selection against the current page count.
fn indices_of(pages: &PageSelection, count: usize) -> Vec<usize> {
    match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    }
}

/// Moves several pages to one position, as a single movement.
pub fn apply_reorder_batch(
    doc: &mut Document,
    sources: &[usize],
    target: usize,
) -> PdfResult<std::ops::Range<usize>> {
    doc.reorder_pages_batch(sources, target)
}

/// Duplicates pages, each clone placed immediately after its original.
///
/// Moved out of the facade, where it had been reachable as `PdfDocument::duplicate_page`
/// beside the vocabulary rather than through it (ARCHITECTURE §4.1, Rule D). Nothing about
/// it belonged above `fepdf-doc`: the cloner it needs lives here.
pub fn apply_duplicate_pages(doc: &mut Document, pages: &PageSelection) -> PdfResult<()> {
    let count = doc.page_count()?;
    let mut indices = indices_of(pages, count);
    indices.sort_unstable();
    indices.dedup();
    if let Some(&worst) = indices.last()
        && worst >= count
    {
        return Err(fepdf_model::PdfError::Arena(
            format!("Page index {worst} out of bounds").into(),
        ));
    }

    // Descending, so each insertion leaves the indices still to be handled where they
    // were. Ascending does not merely mis-order: measured on three pages selected
    // together, it clones page 0 three times, because after the first insertion indices 1
    // and 2 name the clones rather than the originals the caller chose. That arithmetic
    // is the reason page selections are resolved here and not in a frontend loop —
    // `fepdf-gui` was running one of its own until Rule D was enforced.
    for idx in indices.into_iter().rev() {
        let source_page = doc.get_page(idx)?;
        let source_dh = doc.resolve_to_dict(source_page.obj_handle())?;
        let arena = doc.arena();
        let cloned = {
            let mut cloner = crate::cloning::ObjectCloner::new(arena, arena);
            cloner.clone_complete(&Object::Dictionary(source_dh))?
        };
        if let Object::Dictionary(dh) = cloned {
            let handle = doc.arena().alloc_object(Object::Dictionary(dh));
            doc.pages.insert(idx + 1, handle);
        }
    }
    doc.rebuild_page_tree_in_arena()
}

/// Inserts every page of another document, given as that document's bytes.
///
/// Returns the number of pages inserted. Opening the source here rather than taking an
/// already-open document is what lets this be an `Operation` at all — a value that
/// serialises, which `fepdf-mcp` needs and the GUI already had in hand.
pub fn apply_insert_from(doc: &mut Document, source: &[u8], at: usize) -> PdfResult<usize> {
    let options = fepdf_model::ingest::IngestionOptions::default();
    let source_doc = Document::open(Bytes::copy_from_slice(source), &options)?;
    let source_count = source_doc.page_count()?;
    if source_count == 0 {
        return Ok(0);
    }

    let clamped = at.min(doc.pages.len());
    let mut cloner = crate::cloning::ObjectCloner::new(source_doc.arena(), doc.arena());
    let mut handles = Vec::with_capacity(source_count);
    for i in 0..source_count {
        let page = source_doc.get_page(i)?;
        let dh = source_doc.resolve_to_dict(page.obj_handle())?;
        if let Object::Dictionary(cloned) = cloner.clone_complete(&Object::Dictionary(dh))? {
            handles.push(doc.arena().alloc_object(Object::Dictionary(cloned)));
        }
    }

    let inserted = handles.len();
    for (i, h) in handles.into_iter().enumerate() {
        doc.pages.insert(clamped + i, h);
    }
    doc.rebuild_page_tree_in_arena()?;
    Ok(inserted)
}

/// Declares a target standard in the catalogue, and sets the version to 2.0.
///
/// Every branch writes 2.0 because output is always 2.0 (ROADMAP, "the subsets this
/// processor has chosen"); what differs is the key each standard reads.
pub fn apply_upgrade(doc: &mut Document, standard: PdfStandard) -> PdfResult<()> {
    use fepdf_model::PdfName;
    let arena = doc.arena();
    arena.set_version(2.0);

    let (key, value) = match standard {
        PdfStandard::ISO32000_2 => return Ok(()),
        PdfStandard::A4 => ("GTS_PDFA14", Object::Name(arena.intern_name(PdfName::new("Yes")))),
        PdfStandard::UA2 => ("PdfUA", Object::Integer(2)),
        PdfStandard::X6 => ("GTS_PDFX", Object::Name(arena.intern_name(PdfName::new("PDFX6")))),
    };

    if let Some(catalog_handle) = doc.catalog_handle()
        && let Ok(dh) = doc.resolve_to_dict(catalog_handle)
    {
        let mut catalog = arena.get_dict(dh).unwrap_or_default();
        catalog.insert(arena.intern_name(PdfName::new(key)), value);
        arena.set_dict(dh, catalog);
    }
    Ok(())
}

/// Applies page label ranges (Table 159).
pub fn apply_set_page_labels(doc: &Document, labels: Vec<PageLabelSpec>) -> PdfResult<()> {
    let arena = doc.arena();
    let mut nums_items = Vec::new();

    for spec in labels {
        let mut label_dict = BTreeMap::new();
        label_dict.insert(arena.name("Type"), Object::Name(arena.name("PageLabel")));
        let style_name = match spec.style {
            PageLabelStyle::Decimal => "D",
            PageLabelStyle::UpperRoman => "R",
            PageLabelStyle::LowerRoman => "r",
            PageLabelStyle::UpperAlpha => "A",
            PageLabelStyle::LowerAlpha => "a",
        };
        label_dict.insert(arena.name("S"), Object::Name(arena.name(style_name)));
        if let Some(prefix) = spec.prefix
            && !prefix.is_empty()
        {
            // A text string (Table 161): the prefix is shown beside the page number in a
            // reader's UI, so `付録-` has to survive being written.
            label_dict.insert(arena.name("P"), Object::Text(prefix));
        }
        if spec.start_number != 1 {
            label_dict.insert(arena.name("St"), Object::Integer(i64::from(spec.start_number)));
        }
        let label_dh = arena.alloc_dict(label_dict);
        let label_h = arena.alloc_object(Object::Dictionary(label_dh));

        nums_items.push(Object::Integer(spec.start_page as i64));
        nums_items.push(Object::Reference(label_h));
    }

    let nums_arr_h = arena.alloc_array(nums_items);
    let mut num_tree_dict = BTreeMap::new();
    num_tree_dict.insert(arena.name("Nums"), Object::Array(nums_arr_h));
    let num_tree_dh = arena.alloc_dict(num_tree_dict);
    let num_tree_h = arena.alloc_object(Object::Dictionary(num_tree_dh));

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        cdict.insert(arena.name("PageLabels"), Object::Reference(num_tree_h));
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}

/// The affine a [`PageResize`] asks for, given the sheet being left and the one arrived at.
///
/// Returned as PDF's six numbers in the order a `cm` operator takes them (8.3.3): the
/// matrix is `[a b c d e f]` and only the diagonal and the translation are ever set,
/// because none of this rotates or skews.
///
/// **Two things: a scale, and where the result sits.** They were one four-valued enum
/// whose values were four pairs, and then briefly three things — a placement beside an
/// offset, which said the same thing twice. Zero is centred; everything else is a
/// distance from there, and `PageResize::offset_to` names the ones worth naming.
///
/// Pure, and tested on its own: a sentence about where a drawing ends up is the kind that
/// reads as obviously right and is off by a factor of two.
#[must_use]
pub fn fit_matrix(from: (f64, f64), to: (f64, f64), how: &PageResize) -> [f64; 6] {
    let scale = match how.scale {
        ContentScale::Keep => 1.0,
        // The smaller ratio, so the whole of it lands on the sheet.
        ContentScale::Fit => (to.0 / from.0).min(to.1 / from.1),
        // The larger, so the sheet is covered and the rest hangs over.
        ContentScale::Fill => (to.0 / from.0).max(to.1 / from.1),
        ContentScale::By(by) => by,
    };
    // Centred, then moved by the offset — which is what an offset of zero meaning
    // "centred" comes to.
    let centred = (from.0.mul_add(-scale, to.0) / 2.0, from.1.mul_add(-scale, to.1) / 2.0);
    [scale, 0.0, 0.0, scale, centred.0 + how.offset.0, centred.1 + how.offset.1]
}

/// A rectangle with `m` applied to it, as the four numbers a page box is written as.
fn moved_box(rect: [f64; 4], m: [f64; 6]) -> [f64; 4] {
    let x = |v: f64| m[0].mul_add(v, m[4]);
    let y = |v: f64| m[3].mul_add(v, m[5]);
    [x(rect[0]), y(rect[1]), x(rect[2]), y(rect[3])]
}

/// The four numbers of a page box the page itself declares, if it declares one.
///
/// **What the page declares, not what it inherits.** `/MediaBox` is inheritable (7.7.3.3)
/// and a resize writes one onto the page, which overrides whatever the tree said; the
/// other boxes are rewritten only where the page carries them, because writing one onto a
/// page that had none would be inventing a crop nobody asked for.
fn declared_box(
    arena: &fepdf_model::arena::PdfArena,
    dict: &BTreeMap<fepdf_model::Handle<fepdf_model::object::PdfName>, Object>,
    name: &str,
) -> Option<[f64; 4]> {
    let array = arena.get_array(dict.get(&arena.name(name))?.resolve(arena).as_array()?)?;
    if array.len() < 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (slot, entry) in out.iter_mut().zip(array.iter()) {
        *slot = entry.resolve(arena).as_f64()?;
    }
    Some(out)
}

/// The boxes that describe the *content* rather than the sheet, and so move with it
/// (14.11.2).
///
/// `/MediaBox` and `/CropBox` are not here. The first is the new sheet. The second is
/// what a viewer actually displays, and moving it with the content would undo the whole
/// operation: a page resized to A3 whose crop still hugged the old drawing would show the
/// old page, and a `Scale(0.9)` would look like nothing had happened. It becomes the new
/// sheet too.
///
/// **A crop that was hiding something stops hiding it.** That is the cost, and it is the
/// smaller one: the alternative is an operation a reader cannot see the result of. Note
/// that this is not a rare path — ingestion normalises a `/CropBox` onto a page that
/// declared none, so every page reaching here has one.
const CONTENT_BOXES: [&str; 3] = ["BleedBox", "TrimBox", "ArtBox"];

/// Puts the named pages on a different sheet (14.11.2).
///
/// The content is wrapped rather than rewritten: `/Contents` is an array that a reader
/// concatenates into one stream (7.8.2), so a `q <cm>` in front of it and a `Q` behind it
/// transform everything already there without this having to understand any of it.
///
/// # Errors
/// Fails when the new sheet has no area, and when a [`ContentFit::Scale`] factor is not a
/// positive finite number — either would produce a page nothing can be drawn on, and a
/// zero in a `cm` matrix is not an error any viewer reports.
pub fn apply_resize_pages(doc: &Document, pages: &PageSelection, to: &PageResize) -> PdfResult<()> {
    if let Some(size) = to.sheet
        && (!(size.0.is_finite() && size.1.is_finite()) || size.0 <= 0.0 || size.1 <= 0.0)
    {
        return Err(PdfError::Other(
            format!("a sheet of {} by {} points has no area", size.0, size.1).into(),
        ));
    }
    if let ContentScale::By(by) = to.scale
        && (!by.is_finite() || by <= 0.0)
    {
        return Err(PdfError::Other(format!("a scale of {by} draws nothing").into()));
    }
    if !(to.offset.0.is_finite() && to.offset.1.is_finite()) {
        return Err(PdfError::Other(format!("an offset of {:?} is nowhere", to.offset).into()));
    }
    let count = doc.page_count()?;
    let indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    for idx in indices {
        if idx < count {
            resize_one_page(doc, idx, to)?;
        }
    }
    Ok(())
}

/// One page: the sheet, then the boxes on it, then the drawing.
fn resize_one_page(doc: &Document, index: usize, to: &PageResize) -> PdfResult<()> {
    let page = doc.get_page(index)?;
    let page_h = page.obj_handle();
    // The box as it is written, which is the space the content was drawn in.
    let drawn_in = doc_page_size(doc, index);
    // **A sheet is asked for as it is seen, and `/Rotate` is why those differ.** A page
    // box of 595 by 842 with `/Rotate 90` is a landscape page to everyone looking at it,
    // and this window reports it as one. Asking for A4 on such a page and writing 595 by
    // 842 into the box would hand back a landscape A4 — so the sheet asked for is turned
    // to match, and the content is transformed in the space the box is written in.
    //
    // Scanned documents are where this lives: a scanner writes the page one way up and
    // `/Rotate` the other. No page in this corpus carries a rotation, so the fixture in
    // `tests/rotated_resize_test.rs` is a hand-built one.
    let quarter_turned = matches!(page_turn(doc, index), 90 | 270);
    let seen = if quarter_turned { (drawn_in.1, drawn_in.0) } else { drawn_in };
    let asked = to.sheet.unwrap_or(seen);
    let size = if quarter_turned { (asked.1, asked.0) } else { asked };
    let matrix = fit_matrix(drawn_in, size, to);

    let page_dh = doc.resolve_to_dict(page_h)?;
    let arena = doc.arena();
    let mut dict = arena.get_dict(page_dh).unwrap_or_default();

    let media = [0.0, 0.0, size.0, size.1];
    for sheet in ["MediaBox", "CropBox"] {
        dict.insert(arena.name(sheet), Object::Array(arena.alloc_array(numbers(media))));
    }
    for name in CONTENT_BOXES {
        let Some(existing) = declared_box(arena, &dict, name) else { continue };
        let moved = clamp_into(moved_box(existing, matrix), media);
        dict.insert(arena.name(name), Object::Array(arena.alloc_array(numbers(moved))));
    }
    arena.set_dict(page_dh, dict);

    wrap_contents(doc, page_h, matrix)
}

/// `rect` brought inside `bounds`, which is what 14.11.2 requires of every box but the
/// media box: one that fell outside would be a crop a viewer either ignores or obeys, and
/// neither is a thing to leave to the viewer.
fn clamp_into(rect: [f64; 4], bounds: [f64; 4]) -> [f64; 4] {
    [
        rect[0].clamp(bounds[0], bounds[2]),
        rect[1].clamp(bounds[1], bounds[3]),
        rect[2].clamp(bounds[0], bounds[2]),
        rect[3].clamp(bounds[1], bounds[3]),
    ]
}

fn numbers(rect: [f64; 4]) -> Vec<Object> {
    rect.iter().map(|v| Object::Real(*v)).collect()
}

/// What `/Rotate` the page is under, normalised to `0`, `90`, `180` or `270` (7.7.3.3).
fn page_turn(doc: &Document, index: usize) -> i64 {
    let Ok(page) = doc.get_page(index) else { return 0 };
    match page.resolve_attribute("Rotate") {
        Some(Object::Integer(angle)) => (angle % 360).rem_euclid(360),
        _ => 0,
    }
}

/// The sheet a page is currently on, in points, from whatever declares it.
fn doc_page_size(doc: &Document, index: usize) -> (f64, f64) {
    let Ok(page) = doc.get_page(index) else { return (612.0, 792.0) };
    let arena = doc.arena();
    let found = page
        .resolve_attribute("MediaBox")
        .and_then(|entry| entry.as_array())
        .and_then(|handle| arena.get_array(handle))
        .filter(|array| array.len() >= 4)
        .map(|array| {
            let at = |i: usize| array[i].resolve(arena).as_f64().unwrap_or(0.0);
            ((at(2) - at(0)).abs(), (at(3) - at(1)).abs())
        });
    // Letter, which is what `PdfDocument::create_empty` writes and what a page declaring
    // no box at all is being guessed at as.
    found.filter(|(w, h)| *w > 0.0 && *h > 0.0).unwrap_or((612.0, 792.0))
}

/// Puts `q <cm>` in front of a page's content and `Q` behind it.
///
/// **Wrapped, not rewritten.** 7.8.2 makes a `/Contents` array one stream once it is
/// concatenated, so a transform in front of it applies to everything in it — including
/// the operators this crate has never had to parse. A page whose content leaves the
/// graphics stack unbalanced was already malformed by that clause; this does not make it
/// worse, and it does not have to read it to find out.
fn wrap_contents(
    doc: &Document,
    page_h: fepdf_model::Handle<Object>,
    matrix: [f64; 6],
) -> PdfResult<()> {
    let page_dh = doc.resolve_to_dict(page_h)?;
    let arena = doc.arena();
    let mut dict = arena.get_dict(page_dh).unwrap_or_default();
    let contents_key = arena.name("Contents");
    let Some(existing) = dict.get(&contents_key) else { return Ok(()) };

    let mut items = match existing {
        Object::Array(handle) => arena.get_array(*handle).unwrap_or_default(),
        Object::Reference(handle) => vec![Object::Reference(*handle)],
        _ => return Ok(()),
    };
    if items.is_empty() {
        return Ok(());
    }

    // The six numbers `cm` takes, in 8.3.3's order.
    let cm = matrix.map(|v| v.to_string()).join(" ");
    items.insert(0, stream_of(arena, format!("q\n{cm} cm\n")));
    // A newline in front of the `Q`: the page's last operator may have no delimiter after
    // it, and `...ETQ` is one token that means nothing.
    items.push(stream_of(arena, "\nQ\n".to_string()));

    dict.insert(contents_key, Object::Array(arena.alloc_array(items)));
    arena.set_dict(page_dh, dict);
    Ok(())
}

/// A content stream holding `text`, as an indirect object.
fn stream_of(arena: &fepdf_model::arena::PdfArena, text: String) -> Object {
    let stream = Object::Stream(
        arena.alloc_dict(BTreeMap::new()),
        std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(Bytes::from(
            text.into_bytes(),
        ))),
    );
    Object::Reference(arena.alloc_object(stream))
}

/// Cuts the named pages down to `keep`, in the space their boxes are written in.
///
/// **Two things are called cropping and only one of them cuts.** `/CropBox` names the
/// region a viewer displays (14.11.2) and leaves everything else in the file, which is a
/// view: any reader can move it back and see what it hid. Taking the content out is a
/// different act with a different consequence, and
/// [ADR-0088](../../../../docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)
/// keeps both rather than choosing for the reader — so the caller says which.
///
/// The kept rectangle becomes the new sheet, with its lower-left corner at the origin.
/// The content is moved rather than rewritten, the way a resize moves it: a `q <cm>` in
/// front and a `Q` behind transform everything already there without this having to
/// understand any of it (7.8.2).
///
/// # Errors
/// Fails when the rectangle has no area, when it is not made of finite numbers, or when a
/// page cannot be read.
pub fn apply_crop_pages(
    doc: &Document,
    pages: &PageSelection,
    keep: (f64, f64, f64, f64),
    outside: WhatFallsOutside,
) -> PdfResult<()> {
    let (width, height) = (keep.2 - keep.0, keep.3 - keep.1);
    if ![keep.0, keep.1, keep.2, keep.3].iter().all(|edge| edge.is_finite())
        || width <= 0.0
        || height <= 0.0
    {
        return Err(PdfError::Other(
            format!("a crop to {keep:?} keeps a region with no area").into(),
        ));
    }
    let count = doc.page_count()?;
    let indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    for index in indices {
        if index < count {
            crop_one_page(doc, index, keep, outside)?;
        }
    }
    Ok(())
}

/// One page: what goes out of the file, then the sheet, then the drawing moved onto it.
///
/// The order matters. Removing works in the space the page is drawn in, so it happens
/// before anything moves; the sheet and the shift then put what is left at the origin.
fn crop_one_page(
    doc: &Document,
    index: usize,
    keep: (f64, f64, f64, f64),
    outside: WhatFallsOutside,
) -> PdfResult<()> {
    if outside == WhatFallsOutside::Goes {
        crate::apply::text::apply_remove_outside(doc, index, keep)?;
    }
    let page_h = doc.get_page(index)?.obj_handle();
    let page_dh = doc.resolve_to_dict(page_h)?;
    let arena = doc.arena();
    let mut dict = arena.get_dict(page_dh).unwrap_or_default();

    let media = [0.0, 0.0, keep.2 - keep.0, keep.3 - keep.1];
    for sheet in ["MediaBox", "CropBox"] {
        dict.insert(arena.name(sheet), Object::Array(arena.alloc_array(numbers(media))));
    }
    // The content boxes describe the drawing and move with it, then are brought inside
    // the new sheet — one that fell outside would be a box a viewer either ignores or
    // obeys, and neither is what was asked for (14.11.2).
    let shift = [1.0, 0.0, 0.0, 1.0, -keep.0, -keep.1];
    for name in CONTENT_BOXES {
        let Some(existing) = declared_box(arena, &dict, name) else { continue };
        let moved = clamp_into(moved_box(existing, shift), media);
        dict.insert(arena.name(name), Object::Array(arena.alloc_array(numbers(moved))));
    }
    arena.set_dict(page_dh, dict);

    wrap_contents(doc, page_h, shift)
}

/// Cuts one page into several, each carrying one region of it.
///
/// **A split always removes what belongs to the other sheets.** Half a drawing, still
/// searchable, on a page that shows the other half is a leak dressed as a feature: a
/// reader who cuts an A3 assembly drawing into two A4 sheets to send one of them has sent
/// both ([ADR-0088](../../../../docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)).
/// So this is the crop that cuts, and there is no option to make it the one that hides.
///
/// The new pages are in the order the regions are given, and a grid is given in reading
/// order: across a row first, then down. They take the place of the page they came from.
///
/// # Errors
/// Fails when the page is not there, when the division names no region, or when a region
/// has no area.
pub fn apply_split_page(doc: &mut Document, page: usize, division: &PageDivision) -> PdfResult<()> {
    let count = doc.page_count()?;
    if page >= count {
        return Err(PdfError::Other(
            format!("this document has {count} pages and no page {page}").into(),
        ));
    }
    let regions = division.regions(doc_page_size(doc, page));
    if regions.is_empty() {
        return Err(PdfError::Other("a split into no regions leaves nothing".into()));
    }

    // One copy per region, all of them in place of the page they came from. Duplicating
    // first and cropping after is what keeps each region's arithmetic in the space the
    // original was drawn in: cropping one copy moves its content, and a second region
    // measured against the moved page would be measured against the wrong thing.
    for _ in 1..regions.len() {
        apply_duplicate_pages(doc, &PageSelection::Single(page))?;
    }
    for (nth, region) in regions.iter().enumerate() {
        apply_crop_pages(doc, &PageSelection::Single(page + nth), *region, WhatFallsOutside::Goes)?;
    }
    Ok(())
}

/// Puts several pages onto one sheet, in a grid.
///
/// **Each source page becomes a form XObject and is drawn into a cell.** A form XObject
/// carries its own resources (8.10), so a page brought onto another sheet keeps the fonts
/// and images it names without those having to be merged into anything — which is what
/// makes this an arrangement rather than a rewrite.
///
/// The pages fill the grid in reading order, across a row and then down, which is the
/// order [`PageDivision::Grid`] cuts one up in. Each is scaled to fit its cell whole and
/// centred in it, so a portrait page in a landscape cell keeps its shape.
///
/// **What is not content does not come.** Annotations, and anything else a page carries
/// beside what it draws, belong to the page they were on; this draws pages and says so
/// rather than carrying half of them across.
///
/// # Errors
/// Fails when the grid has no cells, when the sheet has no area, or when a page cannot be
/// read.
pub fn apply_combine_pages(
    doc: &mut Document,
    pages: &PageSelection,
    onto: &PageArrangement,
) -> PdfResult<()> {
    let count = doc.page_count()?;
    let per_sheet = onto.columns.checked_mul(onto.rows).unwrap_or(0);
    if per_sheet == 0 {
        return Err(PdfError::Other(
            format!("a grid of {} by {} has no cells", onto.columns, onto.rows).into(),
        ));
    }
    let mut indices = indices_of(pages, count);
    indices.sort_unstable();
    indices.dedup();
    indices.retain(|index| *index < count);
    if indices.is_empty() {
        return Ok(());
    }

    let sheet = onto.sheet.unwrap_or_else(|| doc_page_size(doc, indices[0]));
    if !(sheet.0.is_finite() && sheet.1.is_finite()) || sheet.0 <= 0.0 || sheet.1 <= 0.0 {
        return Err(PdfError::Other(
            format!("a sheet of {} by {} points has no area", sheet.0, sheet.1).into(),
        ));
    }

    let at = indices[0];
    let mut built = Vec::new();
    for group in indices.chunks(per_sheet) {
        built.push(combined_sheet(doc, group, sheet, onto)?);
    }
    // The new sheets go in where the first of the sources was, and the sources come out
    // afterwards — removing first would move the place they are meant to go to. Every
    // source is at or after `at`, so each of them has moved along by however many sheets
    // went in.
    let sheets = built.len();
    for (nth, page) in built.into_iter().enumerate() {
        doc.pages.insert(at + nth, page);
    }
    let moved: Vec<usize> = indices.iter().map(|index| index + sheets).collect();
    apply_remove_pages(doc, &PageSelection::Indices(moved))
}

/// A count of cells as a number the arithmetic can use.
///
/// Beyond what a `u32` holds there is no sheet to speak of, and a grid that fine is one
/// `apply_combine_pages` has already refused or would divide into nothing.
fn cells(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// One sheet with `group`'s pages drawn into its cells.
fn combined_sheet(
    doc: &Document,
    group: &[usize],
    sheet: (f64, f64),
    onto: &PageArrangement,
) -> PdfResult<fepdf_model::Handle<Object>> {
    let arena = doc.arena();
    // Counted as `u32`, which every `f64` holds exactly. `apply_combine_pages` has
    // already refused a grid with no cells, and one finer than four thousand million
    // across divides a sheet into cells no unit of this format can express.
    let across = cells(onto.columns);
    let down = cells(onto.rows);
    let (wide, tall) = (sheet.0 / across, sheet.1 / down);
    let mut xobjects = BTreeMap::new();
    let mut drawing = String::new();

    for (cell, index) in group.iter().enumerate() {
        let name = format!("P{cell}");
        let source = doc_page_size(doc, *index);
        let Some(form) = page_as_form(doc, *index)? else { continue };
        xobjects.insert(arena.name(&name), Object::Reference(form));

        let column = cells(cell % onto.columns);
        let row = cells(cell / onto.columns);
        // Whole and centred: the smaller of the two ratios, so a portrait page in a
        // landscape cell keeps its shape rather than being stretched to fill it.
        let scale = (wide / source.0).min(tall / source.1);
        let left = column.mul_add(wide, source.0.mul_add(-scale, wide) / 2.0);
        let bottom = (row + 1.0).mul_add(-tall, sheet.1) + source.1.mul_add(-scale, tall) / 2.0;
        use std::fmt::Write as _;
        let _ =
            writeln!(drawing, "q {scale:.6} 0 0 {scale:.6} {left:.4} {bottom:.4} cm /{name} Do Q");
    }

    let mut resources = BTreeMap::new();
    resources.insert(arena.name("XObject"), Object::Dictionary(arena.alloc_dict(xobjects)));
    let contents = arena.alloc_object(Object::Stream(
        arena.alloc_dict(BTreeMap::new()),
        std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(Bytes::from(drawing))),
    ));

    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Page")));
    let media = [0.0, 0.0, sheet.0, sheet.1];
    for name in ["MediaBox", "CropBox"] {
        dict.insert(arena.name(name), Object::Array(arena.alloc_array(numbers(media))));
    }
    dict.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(resources)));
    dict.insert(arena.name("Contents"), Object::Reference(contents));
    let dict_h = arena.alloc_dict(dict);
    Ok(arena.alloc_object(Object::Dictionary(dict_h)))
}

/// A page as a form XObject drawing the same thing (8.10).
///
/// Its `/BBox` is the page's own box, so the caller places it with a `cm` and nothing has
/// to know what is inside. Its `/Resources` are the page's, which is what lets the drawing
/// keep naming the fonts and images it always named.
fn page_as_form(doc: &Document, index: usize) -> PdfResult<Option<fepdf_model::Handle<Object>>> {
    let page = doc.get_page(index)?;
    let page_dh = doc.resolve_to_dict(page.obj_handle())?;
    let arena = doc.arena();
    let dict = arena.get_dict(page_dh).unwrap_or_default();
    let Some(contents) = dict.get(&arena.name("Contents")).cloned() else { return Ok(None) };
    let data = crate::remediation::decode_page_contents(doc, &contents)?;
    let size = doc_page_size(doc, index);

    let mut form = BTreeMap::new();
    form.insert(arena.name("Type"), Object::Name(arena.name("XObject")));
    form.insert(arena.name("Subtype"), Object::Name(arena.name("Form")));
    let box_ = [0.0, 0.0, size.0, size.1];
    form.insert(arena.name("BBox"), Object::Array(arena.alloc_array(numbers(box_))));
    let resources = arena.get_dict(page.resources_handle()).unwrap_or_default();
    form.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(resources)));
    let form_dh = arena.alloc_dict(form);
    Ok(Some(arena.alloc_object(Object::Stream(
        form_dh,
        std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(data)),
    ))))
}

#[cfg(test)]
mod resize_geometry {
    use super::{ContentScale, PageResize, fit_matrix, moved_box};
    use crate::operation::Align;

    const A4: (f64, f64) = (595.0, 842.0);
    const A3: (f64, f64) = (842.0, 1191.0);
    /// The whole of an A4 page, as a box to watch move.
    const DRAWING: [f64; 4] = [0.0, 0.0, 595.0, 842.0];

    fn resize(sheet: Option<(f64, f64)>, scale: ContentScale, offset: (f64, f64)) -> PageResize {
        PageResize { sheet, scale, offset }
    }

    /// Two numbers equal to within what a `cm` operator can express.
    fn near(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    /// **Zero is centred**, which is what makes a separate placement unnecessary.
    #[test]
    fn an_offset_of_zero_is_the_middle_of_the_sheet() {
        let m = fit_matrix(A4, A3, &resize(Some(A3), ContentScale::Keep, (0.0, 0.0)));
        assert!(near(m[0], 1.0) && near(m[3], 1.0), "it scaled: {m:?}");
        let moved = moved_box(DRAWING, m);
        assert!(near(f64::midpoint(moved[0], moved[2]), 842.0 / 2.0), "{moved:?}");
        assert!(near(f64::midpoint(moved[1], moved[3]), 1191.0 / 2.0), "{moved:?}");
    }

    /// Fitting is uniform: scaling each axis by its own ratio leaves nothing the shape it
    /// was drawn as.
    #[test]
    fn fitting_uses_one_ratio_for_both_axes() {
        let m = fit_matrix(A4, A3, &resize(Some(A3), ContentScale::Fit, (0.0, 0.0)));
        assert!(near(m[0], m[3]), "the axes were scaled apart");
        let by_width: f64 = 842.0 / 595.0;
        let by_height: f64 = 1191.0 / 842.0;
        assert!(near(m[0], by_width.min(by_height)));

        let moved = moved_box(DRAWING, m);
        assert!(moved[0] >= -1e-9 && moved[1] >= -1e-9, "{moved:?}");
        assert!(moved[2] <= 842.0 + 1e-9 && moved[3] <= 1191.0 + 1e-9, "{moved:?}");
    }

    /// **The offset a named position works out to puts the drawing against that edge.**
    ///
    /// This is what the form's nine buttons fill in, so the arithmetic behind them is the
    /// arithmetic here — one place, not two.
    #[test]
    fn the_offset_a_named_corner_gives_lands_on_that_corner() {
        let scale = 1.0;
        let corner = PageResize::offset_to((Align::Start, Align::End), A4, A3, scale);
        let m = fit_matrix(A4, A3, &resize(Some(A3), ContentScale::Keep, corner));
        let moved = moved_box(DRAWING, m);
        assert!(near(moved[0], 0.0), "not against the left edge: {moved:?}");
        assert!(near(moved[3], 1191.0), "not against the top: {moved:?}");
    }

    /// **Filling covers the sheet and hangs over one axis**, which is the other half of
    /// the same question fitting answers — and the aspect is kept by both.
    ///
    /// `Letter` is chosen over A3 on purpose: A4 and A3 differ in aspect by a quarter of
    /// a percent, so fit and fill are the same scale on them to three decimals and a test
    /// using them would pass whichever this computed.
    #[test]
    fn filling_covers_the_sheet_and_fitting_stays_inside_it() {
        const LETTER: (f64, f64) = (612.0, 792.0);
        let fitted = fit_matrix(A4, LETTER, &resize(Some(LETTER), ContentScale::Fit, (0.0, 0.0)));
        let filled = fit_matrix(A4, LETTER, &resize(Some(LETTER), ContentScale::Fill, (0.0, 0.0)));
        assert!(filled[0] > fitted[0], "filling did not scale up more: {filled:?} {fitted:?}");
        assert!(near(filled[0], filled[3]) && near(fitted[0], fitted[3]), "an axis was stretched");

        let inside = moved_box(DRAWING, fitted);
        assert!(inside[0] >= -1e-9 && inside[2] <= 612.0 + 1e-9, "fitting ran off: {inside:?}");
        assert!(inside[1] >= -1e-9 && inside[3] <= 792.0 + 1e-9, "fitting ran off: {inside:?}");

        let over = moved_box(DRAWING, filled);
        let covers_x = over[0] <= 1e-9 && over[2] >= 612.0 - 1e-9;
        let covers_y = over[1] <= 1e-9 && over[3] >= 792.0 - 1e-9;
        assert!(covers_x && covers_y, "filling left a gap: {over:?}");
        let hangs =
            over[0] < -1e-9 || over[1] < -1e-9 || over[2] > 612.0 + 1e-9 || over[3] > 792.0 + 1e-9;
        assert!(hangs, "filling a sheet of a different aspect hung over nothing: {over:?}");
    }

    /// A factor with no sheet named keeps the sheet and shrinks the drawing.
    #[test]
    fn scaling_with_no_sheet_named_grows_the_margins() {
        let m = fit_matrix(A4, A4, &resize(None, ContentScale::By(0.9), (0.0, 0.0)));
        assert!(near(m[0], 0.9) && near(m[3], 0.9));
        let moved = moved_box(DRAWING, m);
        assert!(near(moved[0], 595.0 - moved[2]), "uneven: {} and {}", moved[0], 595.0 - moved[2]);
        assert!(moved[0] > 0.0, "a 90% drawing has no margin at all");
    }

    /// The offset moves it on from the middle, which is what a binding margin is.
    #[test]
    fn the_offset_moves_it_on_from_the_middle() {
        let centred = fit_matrix(A4, A3, &resize(Some(A3), ContentScale::Keep, (0.0, 0.0)));
        let nudged = fit_matrix(A4, A3, &resize(Some(A3), ContentScale::Keep, (20.0, -10.0)));
        assert!(near(nudged[4] - centred[4], 20.0), "across: {} to {}", centred[4], nudged[4]);
        assert!(near(nudged[5] - centred[5], -10.0), "up: {} to {}", centred[5], nudged[5]);
    }

    /// Shrinking a sheet fits onto it as readily as growing one.
    #[test]
    fn fitting_onto_a_smaller_sheet_shrinks() {
        let m = fit_matrix(A3, A4, &resize(Some(A4), ContentScale::Fit, (0.0, 0.0)));
        assert!(m[0] < 1.0, "it grew going from A3 to A4: {}", m[0]);
        let moved = moved_box([0.0, 0.0, 842.0, 1191.0], m);
        assert!(moved[2] <= 595.0 + 1e-9 && moved[3] <= 842.0 + 1e-9, "{moved:?}");
    }
}
