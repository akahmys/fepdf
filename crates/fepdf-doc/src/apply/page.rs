#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
use crate::operation::{
    ContentFit, PageLabelSpec, PageLabelStyle, PageResize, PageSelection, PdfStandard, RotateMode,
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
            label_dict.insert(arena.name("P"), Object::String(Bytes::from(prefix)));
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

/// The affine a [`ContentFit`] asks for, given the sheet being left and the one arrived at.
///
/// Returned as PDF's six numbers in the order a `cm` operator takes them (8.3.3): the
/// matrix is `[a b c d e f]` and only the diagonal and the translation are ever set,
/// because none of the fits rotate or skew.
///
/// **Pure, and tested on its own.** Every one of the four fits is a sentence about where
/// a drawing ends up, and a sentence about geometry is the kind that reads as obviously
/// right and is off by a factor of two.
#[must_use]
pub fn fit_matrix(from: (f64, f64), to: (f64, f64), fit: ContentFit) -> [f64; 6] {
    let scale = match fit {
        ContentFit::Anchor | ContentFit::Centre => 1.0,
        // The smaller ratio, so the whole of it lands on the sheet.
        ContentFit::Fit => (to.0 / from.0).min(to.1 / from.1),
        ContentFit::Scale(by) => by,
    };
    let (dx, dy) = match fit {
        // The origin is the bottom-left corner, and that is where it stays.
        ContentFit::Anchor => (0.0, 0.0),
        ContentFit::Centre | ContentFit::Fit | ContentFit::Scale(_) => {
            (from.0.mul_add(-scale, to.0) / 2.0, from.1.mul_add(-scale, to.1) / 2.0)
        }
    };
    [scale, 0.0, 0.0, scale, dx, dy]
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
    let (size, fit) = (to.size, to.content);
    if !(size.0.is_finite() && size.1.is_finite()) || size.0 <= 0.0 || size.1 <= 0.0 {
        return Err(PdfError::Other(
            format!("a sheet of {} by {} points has no area", size.0, size.1).into(),
        ));
    }
    if let ContentFit::Scale(by) = fit
        && (!by.is_finite() || by <= 0.0)
    {
        return Err(PdfError::Other(format!("a scale of {by} draws nothing").into()));
    }
    let count = doc.page_count()?;
    let indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    for idx in indices {
        if idx < count {
            resize_one_page(doc, idx, size, fit)?;
        }
    }
    Ok(())
}

/// One page: the sheet, then the boxes on it, then the drawing.
fn resize_one_page(
    doc: &Document,
    index: usize,
    size: (f64, f64),
    fit: ContentFit,
) -> PdfResult<()> {
    let page = doc.get_page(index)?;
    let page_h = page.obj_handle();
    // The sheet being left, inherited if the page declares none, which is what the
    // content was drawn against.
    let (width, height) = doc_page_size(doc, index);
    let matrix = fit_matrix((width, height), size, fit);

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

#[cfg(test)]
mod resize_geometry {
    use super::{ContentFit, fit_matrix, moved_box};

    /// Two matrices equal to within what a `cm` operator can express.
    fn near(left: &[f64; 6], right: &[f64; 6]) -> bool {
        left.iter().zip(right).all(|(a, b)| (a - b).abs() < 1e-9)
    }

    const A4: (f64, f64) = (595.0, 842.0);
    const A3: (f64, f64) = (842.0, 1191.0);

    /// The corner a PDF measures from is the one `Anchor` keeps still.
    #[test]
    fn anchoring_moves_nothing_and_scales_nothing() {
        let m = fit_matrix(A4, A3, ContentFit::Anchor);
        assert!(near(&m, &[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]), "{m:?}");
        // The bottom-left corner of the drawing is still the bottom-left of the sheet.
        assert!(moved_box([0.0, 0.0, 595.0, 842.0], m)[1].abs() < 1e-9);
    }

    /// Centring keeps the size and splits the difference on each axis.
    #[test]
    fn centring_leaves_equal_margins() {
        let m = fit_matrix(A4, A3, ContentFit::Centre);
        assert!((m[0] - 1.0).abs() < 1e-9 && (m[3] - 1.0).abs() < 1e-9, "centring is a scale");
        assert!((m[4] - (842.0 - 595.0) / 2.0).abs() < 1e-9);
        assert!((m[5] - (1191.0 - 842.0) / 2.0).abs() < 1e-9);
        // What was in the middle is still in the middle.
        let moved = moved_box([0.0, 0.0, 595.0, 842.0], m);
        assert!((f64::midpoint(moved[0], moved[2]) - 842.0 / 2.0).abs() < 1e-9);
    }

    /// **Fitting is uniform.** Scaling each axis by its own ratio fills the sheet and
    /// leaves nothing on it the shape it was drawn as.
    #[test]
    fn fitting_uses_one_ratio_for_both_axes() {
        let m = fit_matrix(A4, A3, ContentFit::Fit);
        assert!((m[0] - m[3]).abs() < 1e-12, "the axes were scaled apart");
        let by_width: f64 = 842.0 / 595.0;
        let by_height: f64 = 1191.0 / 842.0;
        assert!((m[0] - by_width.min(by_height)).abs() < 1e-9);

        // The whole of it lands on the sheet, and touches an edge.
        let moved = moved_box([0.0, 0.0, 595.0, 842.0], m);
        assert!(moved[0] >= -1e-9 && moved[1] >= -1e-9);
        assert!(moved[2] <= 842.0 + 1e-9 && moved[3] <= 1191.0 + 1e-9);
        let snug = (moved[2] - moved[0] - 842.0).abs() < 1e-6
            || (moved[3] - moved[1] - 1191.0).abs() < 1e-6;
        assert!(snug, "it fits with room to spare on both axes: {moved:?}");
    }

    /// A factor on the sheet it is already on is the other thing "scale" means.
    #[test]
    fn scaling_in_place_grows_the_margins() {
        let m = fit_matrix(A4, A4, ContentFit::Scale(0.9));
        assert!((m[0] - 0.9).abs() < 1e-9 && (m[3] - 0.9).abs() < 1e-9);
        let moved = moved_box([0.0, 0.0, 595.0, 842.0], m);
        let left = moved[0];
        let right = 595.0 - moved[2];
        assert!((left - right).abs() < 1e-9, "the margins are uneven: {left} and {right}");
        assert!(left > 0.0, "a 90% drawing has no margin at all");
    }

    /// Shrinking a sheet fits onto it as readily as growing one.
    #[test]
    fn fitting_onto_a_smaller_sheet_shrinks() {
        let m = fit_matrix(A3, A4, ContentFit::Fit);
        assert!(m[0] < 1.0, "it grew going from A3 to A4: {}", m[0]);
        let moved = moved_box([0.0, 0.0, 842.0, 1191.0], m);
        assert!(moved[2] <= 595.0 + 1e-9 && moved[3] <= 842.0 + 1e-9, "{moved:?}");
    }
}
