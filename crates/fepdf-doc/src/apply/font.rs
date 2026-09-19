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

/// Embeds `face`, and answers the handle of the font dictionary to name in a resource
/// dictionary.
///
/// # Errors
/// Fails when the program is not a TrueType this engine can subset, or states no metrics.
pub fn embed_truetype(doc: &Document, face: &EmbeddedFace<'_>) -> PdfResult<Handle<Object>> {
    let arena = doc.arena();
    let wanted = face.glyphs.keys().copied().collect();
    let subsetted = fepdf_font::subset::subset_truetype(face.program, &wanted)
        .map_err(|e| PdfError::Other(format!("the program will not subset: {e}").into()))?;
    let metrics = fepdf_font::metrics::read_metrics(face.program)
        .ok_or_else(|| PdfError::Other("the program states no metrics".into()))?;

    let name = arena.name(&format!("{}+{}", subset_tag(face), face.base_font));
    let descriptor = write_descriptor(doc, name, &metrics, &subsetted)?;
    let descendant = write_cid_font(doc, name, &metrics, face, descriptor);
    let to_unicode = write_to_unicode(doc, face);

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
) -> PdfResult<Object> {
    let arena = doc.arena();
    let file = write_font_file(arena, subsetted);
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
    descriptor.insert(arena.name("FontFile2"), file);
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

/// The font file stream, with the `/Length1` Table 124 requires of a TrueType one.
fn write_font_file(arena: &PdfArena, subsetted: &[u8]) -> Object {
    let mut dict = BTreeMap::new();
    dict.insert(
        arena.name("Length1"),
        Object::Integer(i64::try_from(subsetted.len()).unwrap_or(i64::MAX)),
    );
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
    descriptor: Object,
) -> Object {
    let arena = doc.arena();
    let mut info = BTreeMap::new();
    info.insert(arena.name("Registry"), Object::String(Bytes::from_static(b"Adobe")));
    info.insert(arena.name("Ordering"), Object::String(Bytes::from_static(b"Identity")));
    info.insert(arena.name("Supplement"), Object::Integer(0));

    let mut cid = BTreeMap::new();
    cid.insert(arena.name("Type"), Object::Name(arena.name("Font")));
    cid.insert(arena.name("Subtype"), Object::Name(arena.name("CIDFontType2")));
    cid.insert(arena.name("BaseFont"), Object::Name(name));
    cid.insert(arena.name("CIDSystemInfo"), Object::Dictionary(arena.alloc_dict(info)));
    cid.insert(arena.name("FontDescriptor"), descriptor);
    cid.insert(arena.name("CIDToGIDMap"), Object::Name(arena.name("Identity")));
    cid.insert(arena.name("DW"), Object::Integer(1000));
    cid.insert(arena.name("W"), Object::Array(arena.alloc_array(widths(arena, metrics, face))));
    Object::Reference(arena.alloc_object(Object::Dictionary(arena.alloc_dict(cid))))
}

/// `/W`, in the `c [w1 … wn]` form of 9.7.4.3, one group per run of consecutive glyphs.
///
/// **These widths shall be consistent with the program's** — 9.7.4.3 says so, and it is
/// the reason they are read from `hmtx` here rather than taken from a caller.
fn widths(
    arena: &PdfArena,
    metrics: &fepdf_font::metrics::ProgramMetrics,
    face: &EmbeddedFace<'_>,
) -> Vec<Object> {
    let mut out: Vec<Object> = Vec::new();
    let mut run: Vec<Object> = Vec::new();
    let mut run_start: Option<u16> = None;
    let mut previous: Option<u16> = None;

    for &gid in face.glyphs.keys() {
        let advance = fepdf_font::metrics::advance_width(face.program, gid).unwrap_or(0);
        let width = f64::from(advance) * GLYPH_SPACE / f64::from(metrics.units_per_em);
        if previous.is_some_and(|p| gid != p.saturating_add(1)) {
            flush(arena, &mut out, &mut run, run_start.take());
        }
        run_start.get_or_insert(gid);
        run.push(Object::Real(width));
        previous = Some(gid);
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
fn write_to_unicode(doc: &Document, face: &EmbeddedFace<'_>) -> Object {
    let arena = doc.arena();
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    // A `bfchar` section takes at most 100 entries, which is the one limit of the form.
    for chunk in face.glyphs.iter().collect::<Vec<_>>().chunks(100) {
        use std::fmt::Write as _;
        let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
        for (gid, text) in chunk {
            let mut utf16 = String::new();
            for unit in text.encode_utf16() {
                let _ = write!(utf16, "{unit:04X}");
            }
            let _ = writeln!(cmap, "<{gid:04X}> <{utf16}>");
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
