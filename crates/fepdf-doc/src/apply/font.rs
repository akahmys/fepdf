//! Putting a font program into a document, as a Type 0 font keyed by glyph.
//!
//! **Identity-H, because this engine chooses the codes.** Text it sets is written as glyph
//! ids, so a CMap that maps a two-byte code to the CID of the same value is the whole of
//! the encoding, and `/CIDToGIDMap /Identity` carries it the rest of the way. Nothing here
//! has to agree with a producer's private encoding, because there is no producer but this.
//!
//! What may be embedded at all is not decided here
//! ([ADR-0090](../../../../docs/adr/0090-the-face-a-document-embeds-is-not-a-licence-to-set-new-text.md)):
//! the caller has established that the program permits it before reaching this module.

use bytes::Bytes;
use fepdf_model::arena::PdfArena;
use fepdf_model::interpretation::Decision;
use fepdf_model::object::{PdfName, SublimatedData};
use fepdf_model::{Document, Handle, Object, PdfError, PdfResult};

/// A dictionary as the arena holds it.
type Dict = BTreeMap<Handle<PdfName>, Object>;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Glyph space is a thousandth of an em, whatever grid the program is drawn on (9.2.4).
const GLYPH_SPACE: f64 = 1000.0;

/// What a caller asks to be embedded: a program, its name, and what each glyph says.
pub struct EmbeddedFace<'a> {
    /// The whole font program. It is subsetted here to the glyphs below.
    pub program: &'a [u8],
    /// The `/BaseFont` name to carry, before the subset tag is put on it.
    pub base_font: &'a str,
    /// Every glyph to keep, and the text it stands for — which is what `/ToUnicode`
    /// carries and what extraction gives back.
    pub glyphs: &'a BTreeMap<u16, String>,
}

/// A glyph, the code a content stream shows it by, and the text it stands for.
///
/// **The two numbers are not always the same.** Through `Identity-H` a code is a CID, and
/// a CID-keyed CFF gives its glyphs the identifiers its collection assigned: on the
/// Japanese face this machine carries, 20,316 of 20,326 glyphs have a CID equal to their
/// id and **ten do not**. Taking the two for one draws those ten as other characters, and
/// only those ten, which is the kind of wrong that is never noticed.
pub struct Coded {
    /// What the content stream writes.
    pub code: u16,
    /// What the font program draws.
    pub gid: u16,
    /// What it says, for `/ToUnicode`.
    pub text: String,
}

/// A face put into a document, and how to show its glyphs.
pub struct Embedded {
    /// The font dictionary to name in a resource dictionary.
    pub font: Handle<Object>,
    /// The code to write for each glyph.
    pub code_of: BTreeMap<u16, u16>,
}

/// Embeds `face` as whichever kind of font its program is, and says how to show it.
///
/// # Errors
/// Fails when the program is neither a TrueType nor a CFF this engine can subset, or
/// states no metrics.
pub fn embed(doc: &Document, face: &EmbeddedFace<'_>) -> PdfResult<Embedded> {
    let cid_of = fepdf_font::cff::glyph_to_cid(face.program);
    let coded: Vec<Coded> = face
        .glyphs
        .iter()
        .map(|(gid, text)| Coded {
            code: cid_of.as_ref().and_then(|map| map.get(gid).copied()).unwrap_or(*gid),
            gid: *gid,
            text: text.clone(),
        })
        .collect();
    let code_of = coded.iter().map(|c| (c.gid, c.code)).collect();

    let font = if fepdf_font::subset::cff_table(face.program).is_some() {
        embed_cff(doc, face, &coded)?
    } else {
        embed_truetype_coded(doc, face, &coded)?
    };
    Ok(Embedded { font, code_of })
}

/// Embeds `face`, and answers the handle of the font dictionary to name in a resource
/// dictionary.
///
/// # Errors
/// Fails when the program is not a TrueType this engine can subset, or states no metrics.
pub fn embed_truetype(doc: &Document, face: &EmbeddedFace<'_>) -> PdfResult<Handle<Object>> {
    let coded: Vec<Coded> = face
        .glyphs
        .iter()
        .map(|(gid, text)| Coded { code: *gid, gid: *gid, text: text.clone() })
        .collect();
    embed_truetype_coded(doc, face, &coded)
}

/// A TrueType face, shown by glyph id.
fn embed_truetype_coded(
    doc: &Document,
    face: &EmbeddedFace<'_>,
    coded: &[Coded],
) -> PdfResult<Handle<Object>> {
    let arena = doc.arena();
    let wanted = face.glyphs.keys().copied().collect();
    let subsetted = fepdf_font::subset::subset_truetype(face.program, &wanted)
        .map_err(|e| PdfError::Other(format!("the program will not subset: {e}").into()))?;
    let metrics = fepdf_font::metrics::read_metrics(face.program)
        .ok_or_else(|| PdfError::Other("the program states no metrics".into()))?;

    let name = arena.name(&format!("{}+{}", subset_tag(face), face.base_font));
    let descriptor = write_descriptor(doc, name, &metrics, &subsetted, "FontFile2", None)?;
    let descendant = write_cid_font(doc, name, &metrics, face, coded, descriptor, Kind::TrueType);
    let to_unicode = write_to_unicode(doc, coded);

    let mut font = BTreeMap::new();
    font.insert(arena.name("Type"), Object::Name(arena.name("Font")));
    font.insert(arena.name("Subtype"), Object::Name(arena.name("Type0")));
    font.insert(arena.name("BaseFont"), Object::Name(name));
    font.insert(arena.name("Encoding"), Object::Name(arena.name("Identity-H")));
    font.insert(arena.name("DescendantFonts"), Object::Array(arena.alloc_array(vec![descendant])));
    font.insert(arena.name("ToUnicode"), to_unicode);
    Ok(arena.alloc_object(Object::Dictionary(arena.alloc_dict(font))))
}

/// Which of the two shapes 9.7.4 gives a CIDFont this face takes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Outlines in `glyf`, shown by glyph id, with `/CIDToGIDMap`.
    TrueType,
    /// Outlines in a charstring index, shown by the identifier the collection gave them.
    Cff,
}

/// A CFF face, shown by the identifiers its own collection assigned.
///
/// **A bare `/FontFile3` out of another document has no `head` or `hhea`**, so it states
/// no metrics this engine can read and is refused here rather than embedded with invented
/// ones. The faces the ladder reaches are OpenType and carry both.
fn embed_cff(
    doc: &Document,
    face: &EmbeddedFace<'_>,
    coded: &[Coded],
) -> PdfResult<Handle<Object>> {
    let arena = doc.arena();
    let wanted = face.glyphs.keys().copied().collect();
    let subsetted = fepdf_font::cff::subset_cff(face.program, &wanted)
        .map_err(|e| PdfError::Other(format!("the program will not subset: {e}").into()))?;
    let metrics = fepdf_font::metrics::read_metrics(face.program)
        .ok_or_else(|| PdfError::Other("the program states no metrics".into()))?;

    let name = arena.name(&format!("{}+{}", subset_tag(face), face.base_font));
    let descriptor =
        write_descriptor(doc, name, &metrics, &subsetted, "FontFile3", Some("CIDFontType0C"))?;
    let descendant = write_cid_font(doc, name, &metrics, face, coded, descriptor, Kind::Cff);
    let to_unicode = write_to_unicode(doc, coded);

    let mut font = BTreeMap::new();
    font.insert(arena.name("Type"), Object::Name(arena.name("Font")));
    font.insert(arena.name("Subtype"), Object::Name(arena.name("Type0")));
    font.insert(arena.name("BaseFont"), Object::Name(name));
    font.insert(arena.name("Encoding"), Object::Name(arena.name("Identity-H")));
    font.insert(arena.name("DescendantFonts"), Object::Array(arena.alloc_array(vec![descendant])));
    font.insert(arena.name("ToUnicode"), to_unicode);
    Ok(arena.alloc_object(Object::Dictionary(arena.alloc_dict(font))))
}

/// The six uppercase letters that mark a subset, derived from what is in it.
///
/// **Derived rather than random**, so that embedding the same glyphs of the same face
/// twice produces the same name — two documents merged do not then carry two faces
/// claiming to be different subsets of one program. 9.9's tag is six uppercase letters
/// and a `+`; `fepdf_font::subset_tag` is the reader of the same form.
fn subset_tag(face: &EmbeddedFace<'_>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in face.base_font.bytes().chain(face.glyphs.keys().flat_map(|g| g.to_be_bytes())) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    (0..6).map(|i| char::from(b'A' + u8::try_from((hash >> (i * 5)) % 26).unwrap_or(0))).collect()
}

/// The descriptor, and the stream holding the program itself.
fn write_descriptor(
    doc: &Document,
    name: Handle<PdfName>,
    metrics: &fepdf_font::metrics::ProgramMetrics,
    subsetted: &[u8],
    key: &str,
    stream_subtype: Option<&str>,
) -> PdfResult<Object> {
    let arena = doc.arena();
    let file = write_font_file(arena, subsetted, stream_subtype);
    let scale = |v: i16| Object::Real(f64::from(v) * GLYPH_SPACE / f64::from(metrics.units_per_em));

    let mut descriptor = BTreeMap::new();
    descriptor.insert(arena.name("Type"), Object::Name(arena.name("FontDescriptor")));
    descriptor.insert(arena.name("FontName"), Object::Name(name));
    descriptor.insert(arena.name("Flags"), Object::Integer(i64::from(flags(metrics))));
    let bbox = metrics.bbox.map(scale).to_vec();
    descriptor.insert(arena.name("FontBBox"), Object::Array(arena.alloc_array(bbox)));
    // Required, and absent from a program with no `post`. Zero is upright, which is a
    // claim the program did not make, so it is recorded as one rather than assumed.
    descriptor.insert(
        arena.name("ItalicAngle"),
        Object::Real(f64::from(metrics.italic_angle.unwrap_or_else(|| {
            doc.record(Decision::ambiguity(
                "9.8.1",
                "the font program carries no post table, and ItalicAngle is required",
                "wrote 0, which states upright",
            ));
            0.0
        }))),
    );
    descriptor.insert(arena.name("Ascent"), scale(metrics.ascent));
    descriptor.insert(arena.name("Descent"), scale(metrics.descent));
    // **Zero is the standard's own word for "unknown"** (9.8.1), and this engine cannot
    // measure a stem without rasterising the outlines. Every other tool estimates here;
    // an estimate is a number nobody measured.
    descriptor.insert(arena.name("StemV"), Object::Integer(0));
    if let Some(cap) = metrics.cap_height {
        descriptor.insert(arena.name("CapHeight"), scale(cap));
    }
    descriptor.insert(arena.name(key), file);
    Ok(Object::Reference(arena.alloc_object(Object::Dictionary(arena.alloc_dict(descriptor)))))
}

/// Table 121's bits, for a face reached by glyph id.
///
/// **Symbolic, and it is not a judgement about the glyphs.** Bit 6 says the character set
/// is the Standard Latin one *and that the standard names are used for it*; text set by
/// glyph id through Identity-H is neither. The two flags are one binary choice and 9.8.2
/// forbids setting or clearing both, so bit 3 goes on and bit 6 stays off.
///
/// Serif and Script are left clear because nothing here measures them: `OS/2` carries a
/// family class and a PANOSE number that could be guessed from, and a guess in a flag is
/// indistinguishable from a reading.
fn flags(metrics: &fepdf_font::metrics::ProgramMetrics) -> u32 {
    let mut flags = 1 << 2; // Symbolic, bit 3
    if metrics.fixed_pitch {
        flags |= 1; // FixedPitch, bit 1
    }
    if metrics.italic_angle.is_some_and(|angle| angle != 0.0) {
        flags |= 1 << 6; // Italic, bit 7
    }
    flags
}

/// The font file stream: `/Length1` for a TrueType one, `/Subtype` for a CFF one, as
/// Table 124 asks of each.
fn write_font_file(arena: &PdfArena, subsetted: &[u8], stream_subtype: Option<&str>) -> Object {
    let mut dict = BTreeMap::new();
    match stream_subtype {
        Some(subtype) => {
            dict.insert(arena.name("Subtype"), Object::Name(arena.name(subtype)));
        }
        None => {
            dict.insert(
                arena.name("Length1"),
                Object::Integer(i64::try_from(subsetted.len()).unwrap_or(i64::MAX)),
            );
        }
    }
    let stream = Object::Stream(
        arena.alloc_dict(dict),
        Arc::new(SublimatedData::Raw(Bytes::copy_from_slice(subsetted))),
    );
    Object::Reference(arena.alloc_object(stream))
}

/// The CIDFont that carries the glyphs, and the widths that shall agree with the program.
fn write_cid_font(
    doc: &Document,
    name: Handle<PdfName>,
    metrics: &fepdf_font::metrics::ProgramMetrics,
    face: &EmbeddedFace<'_>,
    coded: &[Coded],
    descriptor: Object,
    kind: Kind,
) -> Object {
    let arena = doc.arena();
    // **The collection has to be the program's own.** A CID-keyed CFF's codes are
    // identifiers its collection assigned, so saying `Identity` over one of those would
    // give a reader two answers to the same question; a TrueType shown by glyph id has no
    // collection, and `Identity` is what that is called.
    let (registry, ordering, supplement) = match kind {
        Kind::Cff => fepdf_font::cff::registry_ordering_supplement(face.program)
            .unwrap_or_else(|| ("Adobe".to_string(), "Identity".to_string(), 0)),
        Kind::TrueType => ("Adobe".to_string(), "Identity".to_string(), 0),
    };
    let mut info = BTreeMap::new();
    info.insert(arena.name("Registry"), Object::String(Bytes::from(registry.into_bytes())));
    info.insert(arena.name("Ordering"), Object::String(Bytes::from(ordering.into_bytes())));
    info.insert(arena.name("Supplement"), Object::Integer(i64::from(supplement)));

    let mut cid = BTreeMap::new();
    cid.insert(arena.name("Type"), Object::Name(arena.name("Font")));
    let subtype = match kind {
        Kind::TrueType => "CIDFontType2",
        Kind::Cff => "CIDFontType0",
    };
    cid.insert(arena.name("Subtype"), Object::Name(arena.name(subtype)));
    cid.insert(arena.name("BaseFont"), Object::Name(name));
    cid.insert(arena.name("CIDSystemInfo"), Object::Dictionary(arena.alloc_dict(info)));
    cid.insert(arena.name("FontDescriptor"), descriptor);
    if kind == Kind::TrueType {
        // 9.7.4.2 gives this to a CIDFontType2 and not to a CIDFontType0, where the
        // charstring index is reached through the program's own charset instead.
        cid.insert(arena.name("CIDToGIDMap"), Object::Name(arena.name("Identity")));
    }
    cid.insert(arena.name("DW"), Object::Integer(1000));
    cid.insert(
        arena.name("W"),
        Object::Array(arena.alloc_array(widths(arena, metrics, face.program, coded))),
    );
    Object::Reference(arena.alloc_object(Object::Dictionary(arena.alloc_dict(cid))))
}

/// `/W`, in the `c [w1 … wn]` form of 9.7.4.3, one group per run of consecutive glyphs.
///
/// **These widths shall be consistent with the program's** — 9.7.4.3 says so, and it is
/// the reason they are read from `hmtx` here rather than taken from a caller.
fn widths(
    arena: &PdfArena,
    metrics: &fepdf_font::metrics::ProgramMetrics,
    program: &[u8],
    coded: &[Coded],
) -> Vec<Object> {
    let mut by_code: Vec<&Coded> = coded.iter().collect();
    by_code.sort_by_key(|c| c.code);

    let mut out: Vec<Object> = Vec::new();
    let mut run: Vec<Object> = Vec::new();
    let mut run_start: Option<u16> = None;
    let mut previous: Option<u16> = None;

    for glyph in by_code {
        // **The width is the glyph's and the key is the code's.** Reading `hmtx` by the
        // code would answer with whatever glyph happens to have that id, which on a
        // CID-keyed face is a different letter.
        let advance = fepdf_font::metrics::advance_width(program, glyph.gid).unwrap_or(0);
        let width = f64::from(advance) * GLYPH_SPACE / f64::from(metrics.units_per_em);
        if previous.is_some_and(|p| glyph.code != p.saturating_add(1)) {
            flush(arena, &mut out, &mut run, run_start.take());
        }
        run_start.get_or_insert(glyph.code);
        run.push(Object::Real(width));
        previous = Some(glyph.code);
    }
    flush(arena, &mut out, &mut run, run_start);
    out
}

/// One `c [w1 … wn]` group, where there is one to write.
fn flush(arena: &PdfArena, out: &mut Vec<Object>, run: &mut Vec<Object>, start: Option<u16>) {
    if let Some(start) = start
        && !run.is_empty()
    {
        out.push(Object::Integer(i64::from(start)));
        out.push(Object::Array(arena.alloc_array(std::mem::take(run))));
    }
    run.clear();
}

/// The `/ToUnicode` CMap: what each glyph of this subset says, for extraction (9.10.3).
///
/// **Without it the text this engine writes cannot be read back**, and that is the defect
/// this whole phase started from — a Bates prefix that drew as Latin noise and extracted
/// as nothing.
fn write_to_unicode(doc: &Document, coded: &[Coded]) -> Object {
    let arena = doc.arena();
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    // A `bfchar` section takes at most 100 entries, which is the one limit of the form.
    let mut by_code: Vec<&Coded> = coded.iter().collect();
    by_code.sort_by_key(|c| c.code);
    for chunk in by_code.chunks(100) {
        use std::fmt::Write as _;
        let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
        for glyph in chunk {
            let mut utf16 = String::new();
            for unit in glyph.text.encode_utf16() {
                let _ = write!(utf16, "{unit:04X}");
            }
            let code = glyph.code;
            let _ = writeln!(cmap, "<{code:04X}> <{utf16}>");
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");

    let stream = Object::Stream(
        arena.alloc_dict(BTreeMap::new()),
        Arc::new(SublimatedData::Raw(Bytes::from(cmap.into_bytes()))),
    );
    Object::Reference(arena.alloc_object(stream))
}

/// Where a run of text goes on a page, and in what face.
pub struct ShownText<'a> {
    /// The font program to embed. It is subsetted to the glyphs this text needs.
    pub program: &'a [u8],
    /// The `/BaseFont` name to carry.
    pub base_font: &'a str,
    /// The text itself.
    pub text: &'a str,
    /// Where its first glyph sits, in default user space.
    pub at: (f64, f64),
    /// The size to set it at, in points.
    pub size: f64,
}

/// Draws `shown` on page `page` in a face embedded for the purpose.
///
/// **The codes written are glyph ids**, because the font is `Identity-H`: a two-byte code
/// is the glyph, and `/ToUnicode` is what tells a reader back what it stood for. That is
/// the whole of the difference from `overlay_text_on_page`, which escapes a Rust string
/// into a literal and shows it through a font nobody embedded — and which loses every
/// character outside WinAnsi on the way.
///
/// # Errors
/// Fails when the program has no glyph for a character of `text`, naming it; when the
/// program cannot be subsetted; or when the page is not there.
pub fn show_text(doc: &Document, page: usize, shown: &ShownText<'_>) -> PdfResult<()> {
    let glyph_ids = fepdf_font::subset::glyphs_for(shown.program, shown.text)
        .map_err(|c| PdfError::Other(format!("this face draws no {c:?}").into()))?;

    // **One glyph stands for one character here, and a glyph drawn twice is still one
    // glyph.** Appending instead of inserting made `/ToUnicode` say that the glyph for
    // `0` stood for `000`, so `0001` came back out of the file as `0000000001`: every
    // occurrence extracted as every occurrence. Where a face draws two characters with
    // one glyph, the first is what the entry says, which is all a single entry can say.
    let mut glyphs: BTreeMap<u16, String> = BTreeMap::new();
    for (gid, c) in glyph_ids.iter().zip(shown.text.chars()) {
        glyphs.entry(*gid).or_insert_with(|| c.to_string());
    }
    let embedded = embed(
        doc,
        &EmbeddedFace { program: shown.program, base_font: shown.base_font, glyphs: &glyphs },
    )?;

    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let name = name_font_in_page(doc, page_h, &mut page_dict, embedded.font);

    // **What goes in the string is the code, not the glyph.** On a CID-keyed face they
    // differ for some glyphs and not others, so writing the id draws the right letter
    // almost always.
    let mut codes = String::with_capacity(glyph_ids.len() * 4);
    for gid in &glyph_ids {
        use std::fmt::Write as _;
        let code = embedded.code_of.get(gid).copied().unwrap_or(*gid);
        let _ = write!(codes, "{code:04X}");
    }
    let (x, y) = shown.at;
    let drawing = format!(
        "q\nBT\n/{name} {:.2} Tf\n1 0 0 1 {x:.2} {y:.2} Tm\n<{codes}> Tj\nET\nQ\n",
        shown.size
    );
    append_content(doc, page_dh, &mut page_dict, drawing.into_bytes());
    arena.set_dict(page_dh, page_dict);
    Ok(())
}

/// Names `font` in the page's resources, under a name nothing else there uses.
fn name_font_in_page(
    doc: &Document,
    page_h: Handle<Object>,
    page_dict: &mut BTreeMap<Handle<PdfName>, Object>,
    font: Handle<Object>,
) -> String {
    let arena = doc.arena();
    let res_dh = super::annotations::ensure_page_resources(doc, page_h, page_dict);
    let mut resources = arena.get_dict(res_dh).unwrap_or_default();
    let font_key = arena.name("Font");

    let fonts_dh = match resources.get(&font_key).and_then(|o| o.resolve(arena).as_dict_handle()) {
        Some(dh) => dh,
        None => {
            let dh = arena.alloc_dict(BTreeMap::new());
            resources.insert(font_key, Object::Dictionary(dh));
            dh
        }
    };
    let mut fonts = arena.get_dict(fonts_dh).unwrap_or_default();

    // A name nothing else in this dictionary answers to. The resources of a page this
    // engine did not write hold whatever its producer chose, so a fixed `/F1` would take
    // a name that already draws something.
    let taken: Vec<String> =
        fonts.keys().filter_map(|k| arena.get_name(*k).map(|n| n.as_str().to_string())).collect();
    // Bounded, because an unbounded search for a free name is a loop that a page with
    // enough fonts on it would not leave. A page naming a thousand of this engine's faces
    // is not a case that has happened; falling back to the last one if it ever does is a
    // collision rather than a hang.
    let name = (1..1000)
        .map(|i| format!("FE{i}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| "FE1000".to_string());

    fonts.insert(arena.name(&name), Object::Reference(font));
    arena.set_dict(fonts_dh, fonts);
    arena.set_dict(res_dh, resources);
    name
}

/// Adds `drawing` to the page's content, after whatever is already there.
fn append_content(
    doc: &Document,
    page_dh: Handle<Dict>,
    page_dict: &mut BTreeMap<Handle<PdfName>, Object>,
    drawing: Vec<u8>,
) {
    let arena = doc.arena();
    let stream = Object::Stream(
        arena.alloc_dict(BTreeMap::new()),
        Arc::new(SublimatedData::Raw(Bytes::from(drawing))),
    );
    let stream_h = arena.alloc_object(stream);

    let contents_key = arena.name("Contents");
    let mut items = match page_dict.get(&contents_key).map(|o| o.resolve(arena)) {
        Some(Object::Array(ah)) => arena.get_array(ah).unwrap_or_default(),
        Some(_) => match page_dict.get(&contents_key) {
            Some(Object::Reference(h)) => vec![Object::Reference(*h)],
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    items.push(Object::Reference(stream_h));
    page_dict.insert(contents_key, Object::Array(arena.alloc_array(items)));
    let _ = page_dh;
}
