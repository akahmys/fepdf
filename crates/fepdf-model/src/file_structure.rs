//! What a file's layout is, as ISO 32000-2 clause 7.5 describes it.
//!
//! This reports the *file*, not the document: revisions, cross-reference form, where
//! objects are stored, and every [`Decision`] the reader took getting there. A
//! normalised [`crate::document::Document`] has already resolved all of that away,
//! which is why this reads the bytes itself rather than inspecting a loaded document.
//!
//! Every field here was chosen because it varies across the sample corpus. A figure
//! that reads the same for every file tells the caller nothing, and this codebase has
//! a habit of building containers before their contents exist — so the survey came
//! first (`examples/structure_survey.rs`) and the type second.

use crate::arena::PdfArena;
use crate::error::PdfResult;
use crate::interpretation::{Decision, DecisionLog, Severity};
use crate::object::Object;
use crate::reader::{self, XrefForm};
use fepdf_syntax::xref::{self, XrefRecord};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One cross-reference section, which is to say one revision of the file (7.5.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Revision {
    /// 1 is the original file; higher numbers are incremental updates.
    pub index: usize,
    /// Byte offset of the section, as `startxref` and `/Prev` give it.
    pub offset: u64,
    /// How many objects this section alone defines.
    pub entries: usize,
    /// Whether this section carries a trailer dictionary.
    pub has_trailer: bool,
    /// Which of the two forms it is written in.
    pub form: String,
}

/// Where the objects of the newest revision live.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectCensus {
    /// Highest object number the cross-reference names.
    pub highest_number: u32,
    /// Objects written directly in the file.
    pub in_file: usize,
    /// Slots marked free — deleted by some revision, or never used.
    pub free: usize,
    /// Objects carried inside an object stream (7.5.7).
    pub in_object_stream: usize,
    /// Whether these counts come from scanning the file rather than from its
    /// cross-reference. Without this the census read as all-zero for a file the reader
    /// had in fact recovered 77 objects from, which is the sort of confident nonsense
    /// this command exists to prevent.
    pub from_scan: bool,
}

/// One `/Type /ObjStm` and how much of the file it carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectStream {
    /// Object number of the container.
    pub container: u32,
    /// How many objects the newest cross-reference still places inside it.
    pub carries: usize,
}

/// One stream filter the file names (7.4), and where it sits.
///
/// Counted by walking the arena rather than by searching the bytes, because searching
/// the bytes cannot see it: `grep -l CCITTFaxDecode` over both corpora finds **zero**
/// files where this census finds two, since the name is inside a `/FlateDecode`d object
/// stream. A judgement about which codecs are worth building rests on this count, so
/// the instrument had to be one that can read what it is counting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterUse {
    /// The filter's name exactly as the file writes it, abbreviations included — Table 6
    /// lets `/AHx` mean `/ASCIIHexDecode`, and a census that folded them together would
    /// hide which spelling a producer used.
    pub name: String,
    /// How many streams name it.
    pub streams: usize,
    /// How many of those are image XObjects. An image carries no text, so a filter that
    /// only ever appears here cannot be what stops a page yielding its words — the
    /// measurement that moved the three image codecs out of the way of clause 7.4.
    pub on_images: usize,
    /// Whether this engine decodes it.
    pub decoded: bool,
}

/// A file's layout, and what had to be decided to read it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStructure {
    /// Version from the `%PDF-` header.
    pub version: String,
    /// Where the header was found. Anything but 0 means bytes precede it (7.5.2).
    pub header_offset: usize,
    /// Total size of the file in bytes.
    pub size: usize,
    /// Cross-reference sections, oldest first.
    pub revisions: Vec<Revision>,
    /// Where the objects of the merged cross-reference live.
    pub objects: ObjectCensus,
    /// Object streams, largest first.
    pub object_streams: Vec<ObjectStream>,
    /// Whether the trailer declares `/Encrypt` (7.6).
    pub encrypted: bool,
    /// Whether the file names a `/Root`, which is how the catalogue is reached.
    pub declares_root: bool,
    /// Object numbers defined by more than one revision — what the updates rewrote.
    pub superseded: usize,
    /// Every decision the reader took, in the order taken.
    pub decisions: Vec<Decision>,
    /// Which filters the file's streams name, most used first.
    pub filters: Vec<FilterUse>,
}

/// Walks the cross-reference chain once, yielding the per-revision facts, the merged
/// cross-reference, and the object numbers more than one revision defines.
///
/// One walk rather than one per question: `samples/intel_sdm.pdf` has 332,386 entries
/// in its first section alone, and three walks cost a minute.
fn walk_sections(
    bytes: &[u8],
    header_offset: usize,
) -> (Vec<Revision>, BTreeMap<u32, XrefRecord>, BTreeSet<u32>) {
    let scratch = PdfArena::new();
    let mut scratch_log = DecisionLog::default();
    let chain = xref::find_startxref(bytes).map_or_else(Vec::new, |start| {
        xref::section_chain(bytes, start.saturating_add(header_offset as u64))
    });

    let mut revisions = Vec::new();
    let mut merged: BTreeMap<u32, XrefRecord> = BTreeMap::new();
    let mut superseded: BTreeSet<u32> = BTreeSet::new();
    for (i, at) in chain.iter().enumerate() {
        let Ok(offset) = usize::try_from(*at) else { continue };
        let Ok(section) = reader::read_xref_section(bytes, offset, &scratch, &mut scratch_log)
        else {
            continue;
        };
        revisions.push(Revision {
            index: i + 1,
            offset: *at,
            entries: section.entries.len(),
            has_trailer: section.trailer.is_some(),
            form: match section.form {
                XrefForm::Table => "table".into(),
                XrefForm::Stream => "stream".into(),
            },
        });
        for (num, rec) in section.entries {
            // The chain runs oldest first and `insert` overwrites, so the newest
            // definition of a number wins — matching `reader::locate_objects`, which
            // `extend`s in the same order. Keeping the *oldest* instead is the error
            // ADR-0006 records: it resurrects a superseded object.
            //
            // Object 0 is excluded from `superseded`: 7.5.4 makes it the head of the
            // free list, so every table section carries it and counting it would add a
            // constant 1 to every updated file without naming anything the update
            // actually rewrote.
            if merged.insert(num, rec).is_some() && num != 0 {
                superseded.insert(num);
            }
        }
    }
    (revisions, merged, superseded)
}

/// Counts where the merged cross-reference places each object, and how much each
/// object stream still carries.
fn take_census(
    merged: &BTreeMap<u32, XrefRecord>,
    from_scan: bool,
) -> (ObjectCensus, Vec<ObjectStream>) {
    let mut census =
        ObjectCensus { highest_number: 0, in_file: 0, free: 0, in_object_stream: 0, from_scan };
    let mut containers: BTreeMap<u32, usize> = BTreeMap::new();
    for (&num, rec) in merged {
        census.highest_number = census.highest_number.max(num);
        match rec {
            XrefRecord::InFile { .. } => census.in_file += 1,
            XrefRecord::Free { .. } => census.free += 1,
            XrefRecord::InObjectStream { container, .. } => {
                census.in_object_stream += 1;
                *containers.entry(*container).or_default() += 1;
            }
        }
    }

    let mut object_streams: Vec<ObjectStream> = containers
        .into_iter()
        .map(|(container, carries)| ObjectStream { container, carries })
        .collect();
    object_streams.sort_by(|a, b| b.carries.cmp(&a.carries).then(a.container.cmp(&b.container)));
    (census, object_streams)
}

/// Every filter the file's streams name, and how many of them are on images.
///
/// Walks **streams**, not dictionaries. The first version of this walked every
/// dictionary holding a `/Filter` key on the reasoning that only a stream may have one,
/// and the corpus refuted it immediately: two files reported a filter called
/// `/Standard`, which is the *security handler* named by the encryption dictionary
/// (Table 20), and a signature dictionary names `/Adobe.PPKLite` the same way. Asking
/// the arena for its streams cannot make that mistake.
///
/// It does find every stream, which is the point — including the cross-reference
/// streams that carry the file's own index. Not hypothetical:
/// `UnknownFilter-Linearized.pdf` names `/XXXDecode` on two of them, and nothing that
/// reads only page content would ever see it.
///
/// **Inline images are not counted** (7.8.6). Their filters are written inside a
/// content stream, in Table 6's abbreviated form, and reaching them means interpreting
/// the page rather than walking the arena. So a `/AHx` that occurs only inline is
/// absent from this table, and the table says streams rather than filters for that
/// reason.
fn take_filter_census(arena: &PdfArena) -> Vec<FilterUse> {
    let filter_key = arena.name("Filter");
    let subtype_key = arena.name("Subtype");
    let image = arena.name("Image");

    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for i in 0..arena.object_count() {
        let Some(Object::Stream(dh, _)) = arena.get_object(crate::handle::Handle::new(i)) else {
            continue;
        };
        let Some(dict) = arena.get_dict(dh) else { continue };
        let Some(filter) = dict.get(&filter_key) else { continue };
        let on_image = dict
            .get(&subtype_key)
            .and_then(|o| o.resolve(arena).as_name())
            .is_some_and(|n| n == image);

        let mut names = Vec::new();
        match filter.resolve(arena) {
            Object::Name(h) => names.extend(arena.get_name_str(h)),
            Object::Array(ah) => {
                for item in arena.get_array(ah).unwrap_or_default() {
                    if let Some(h) = item.resolve(arena).as_name() {
                        names.extend(arena.get_name_str(h));
                    }
                }
            }
            _ => {}
        }
        for name in names {
            let entry = counts.entry(name).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += usize::from(on_image);
        }
    }

    let mut uses: Vec<FilterUse> = counts
        .into_iter()
        .map(|(name, (streams, on_images))| FilterUse {
            decoded: crate::filters::is_decoded(&name),
            name,
            streams,
            on_images,
        })
        .collect();
    uses.sort_by(|a, b| b.streams.cmp(&a.streams).then(a.name.cmp(&b.name)));
    uses
}

impl FileStructure {
    /// Reads `bytes` and reports its layout.
    ///
    /// # Errors
    /// Propagates a read failure only when the file cannot be opened at all; a file
    /// that needs repair is reported, with the repairs in [`Self::decisions`].
    pub fn survey(bytes: &[u8]) -> PdfResult<Self> {
        let header_offset = xref::find_header(bytes).map_or(0, |h| h.offset);
        let (revisions, mut merged, superseded) = walk_sections(bytes, header_offset);

        // The document as the reader actually assembles it, so the decisions reported
        // are the ones a caller would get from `Document::open` and not a re-derivation.
        let raw = reader::load_document(bytes)?;

        // The reader falls back to scanning when the cross-reference yields nothing
        // (7.5.4); the census has to follow it there, or it reports an empty file.
        let from_scan = merged.is_empty();
        if from_scan {
            merged.extend(
                xref::scan_indirect_objects(bytes)
                    .into_iter()
                    .map(|(n, o)| (n, XrefRecord::InFile { offset: o, generation: 0 })),
            );
        }
        let (census, object_streams) = take_census(&merged, from_scan);

        let (encrypted, declares_root) = raw.trailer.map_or((false, false), |t| {
            raw.arena.get_dict(t).map_or((false, false), |d| {
                (
                    d.contains_key(&raw.arena.name("Encrypt")),
                    d.contains_key(&raw.arena.name("Root")),
                )
            })
        });

        Ok(Self {
            version: raw.version.clone(),
            header_offset,
            size: bytes.len(),
            revisions,
            objects: census,
            object_streams,
            encrypted,
            declares_root,
            superseded: superseded.len(),
            decisions: raw.decisions.entries(),
            filters: take_filter_census(&raw.arena),
        })
    }

    /// Whether the file was read without departing from the standard.
    #[must_use]
    pub fn is_conforming(&self) -> bool {
        self.decisions.is_empty()
    }

    /// How many decisions fall at each severity, for a one-line summary.
    #[must_use]
    pub fn decision_counts(&self) -> (usize, usize, usize) {
        let count = |s: Severity| self.decisions.iter().filter(|d| d.severity == s).count();
        (count(Severity::Ambiguity), count(Severity::Repaired), count(Severity::Violation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two revisions: object 2 starts in an object stream and is rewritten directly,
    /// object 3 starts there and is freed. The same shape as the file behind ADR-0006.
    fn incrementally_updated() -> Vec<u8> {
        let mut out = Vec::new();
        let push = |out: &mut Vec<u8>, s: &str| out.extend_from_slice(s.as_bytes());

        push(&mut out, "%PDF-2.0\n");
        let payload = "2 0 3 12 << /V (old) >> << /V (three) >>";
        let stm_at = out.len();
        push(
            &mut out,
            &format!(
                "4 0 obj\n<< /Type /ObjStm /N 2 /First 8 /Length {} >>\nstream\n{payload}\nendstream\nendobj\n",
                payload.len()
            ),
        );
        let first_xref = out.len();
        push(&mut out, "xref\n0 5\n0000000000 65535 f \n0000000000 65535 f \n");
        push(&mut out, "0000000000 00000 n \n0000000000 00000 n \n");
        push(&mut out, &format!("{stm_at:010} 00000 n \n"));
        push(&mut out, &format!("trailer\n<< /Size 5 >>\nstartxref\n{first_xref}\n%%EOF\n"));

        let new_two = out.len();
        push(&mut out, "2 0 obj\n<< /V (new) >>\nendobj\n");
        let second_xref = out.len();
        push(&mut out, "xref\n0 1\n0000000000 65535 f \n2 2\n");
        push(&mut out, &format!("{new_two:010} 00000 n \n0000000000 00001 f \n"));
        push(
            &mut out,
            &format!(
                "trailer\n<< /Size 5 /Prev {first_xref} >>\nstartxref\n{second_xref}\n%%EOF\n"
            ),
        );
        out
    }

    /// Two revisions written with **cross-reference streams**, which is the only form
    /// that has a type-2 entry — an object living inside an object stream.
    ///
    /// Object 2 starts inside container 4 and is rewritten as a direct object in
    /// revision 2; object 3 starts there and is freed. A classic `xref` table cannot
    /// express this, which is why the first version of these tests passed with the
    /// ADR-0006 defect injected: `in_object_stream` was 0 either way.
    fn updated_with_xref_streams() -> Vec<u8> {
        fn entry(kind: u8, field2: u32, field3: u16) -> Vec<u8> {
            let mut e = vec![kind];
            e.extend_from_slice(&field2.to_be_bytes());
            e.extend_from_slice(&field3.to_be_bytes());
            e
        }

        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"%PDF-2.0\n");

        // Revision 1: container 4 carries objects 2 and 3.
        let payload = "2 0 3 12 << /V (old) >> << /V (three) >>";
        let stm_at = out.len();
        out.extend_from_slice(
            format!(
                "4 0 obj\n<< /Type /ObjStm /N 2 /First 8 /Length {} >>\nstream\n{payload}\nendstream\nendobj\n",
                payload.len()
            )
            .as_bytes(),
        );

        let xref1_at = out.len();
        let mut table1 = Vec::new();
        table1.extend(entry(0, 0, 65535)); // 0: head of the free list
        table1.extend(entry(0, 0, 0)); // 1: free
        table1.extend(entry(2, 4, 0)); // 2: inside container 4
        table1.extend(entry(2, 4, 1)); // 3: inside container 4
        table1.extend(entry(1, u32::try_from(stm_at).unwrap(), 0)); // 4: the container
        table1.extend(entry(1, u32::try_from(xref1_at).unwrap(), 0)); // 5: this section
        out.extend_from_slice(
            format!(
                "5 0 obj\n<< /Type /XRef /Size 6 /W [1 4 2] /Index [0 6] /Root 9 0 R /Length {} >>\nstream\n",
                table1.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&table1);
        out.extend_from_slice(b"\nendstream\nendobj\n");
        out.extend_from_slice(format!("startxref\n{xref1_at}\n%%EOF\n").as_bytes());

        // Revision 2: object 2 rewritten directly, object 3 freed.
        let new_two = out.len();
        out.extend_from_slice(b"2 0 obj\n<< /V (new) >>\nendobj\n");

        let xref2_at = out.len();
        let mut table2 = Vec::new();
        table2.extend(entry(1, u32::try_from(new_two).unwrap(), 0)); // 2: now direct
        table2.extend(entry(0, 0, 1)); // 3: freed
        table2.extend(entry(1, u32::try_from(xref2_at).unwrap(), 0)); // 6: this section
        out.extend_from_slice(
            format!(
                "6 0 obj\n<< /Type /XRef /Size 7 /W [1 4 2] /Index [2 2 6 1] /Prev {xref1_at} /Root 9 0 R /Length {} >>\nstream\n",
                table2.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&table2);
        out.extend_from_slice(b"\nendstream\nendobj\n");
        out.extend_from_slice(format!("startxref\n{xref2_at}\n%%EOF\n").as_bytes());
        out
    }

    #[test]
    fn every_revision_is_reported_oldest_first() {
        let s = FileStructure::survey(&incrementally_updated()).expect("reads");
        assert_eq!(s.revisions.len(), 2);
        assert_eq!(s.revisions[0].index, 1);
        assert!(s.revisions[0].offset < s.revisions[1].offset);
        assert!(s.revisions.iter().all(|r| r.form == "table"));
    }

    #[test]
    fn the_census_follows_the_newest_revision_not_the_oldest() {
        // Revision 1 puts objects 2 and 3 inside container 4; revision 2 rewrites 2 as
        // a direct object and frees 3. Reading the oldest definition instead reports
        // both as still living in the container — the defect ADR-0006 records, arising
        // in a report rather than in the arena.
        //
        // This assertion is the one that has to bite. Injecting oldest-wins must turn
        // `in_object_stream` from 0 into 2.
        let s = FileStructure::survey(&updated_with_xref_streams()).expect("reads");
        assert_eq!(s.revisions.len(), 2);
        assert!(s.revisions.iter().all(|r| r.form == "stream"));
        assert_eq!(s.superseded, 2, "objects 2 and 3 are defined by both revisions");
        assert_eq!(
            s.objects.in_object_stream, 0,
            "both objects moved out of container 4; the newest revision says so"
        );
        assert_eq!(s.objects.free, 3, "objects 0 and 1 were always free; 3 was freed");
        assert_eq!(s.object_streams.len(), 0, "no container still carries anything");
    }

    #[test]
    fn a_clean_file_reports_no_decisions() {
        let s = FileStructure::survey(&incrementally_updated()).expect("reads");
        assert!(s.is_conforming(), "unexpected decisions: {:?}", s.decisions);
        assert_eq!(s.decision_counts(), (0, 0, 0));
    }

    #[test]
    fn a_prefixed_file_reports_the_header_offset_and_a_repair() {
        let mut bytes = vec![b'X'; 300];
        bytes.extend_from_slice(&incrementally_updated());
        let s = FileStructure::survey(&bytes).expect("reads");
        assert_eq!(s.header_offset, 300);
        assert!(!s.is_conforming(), "300 bytes before %PDF- is a departure (7.5.2)");
        assert!(s.decisions.iter().any(|d| d.clause == "7.5.2"));
    }

    #[test]
    fn a_census_from_scanning_says_so() {
        // `startxref` points nowhere, so the cross-reference yields nothing and the
        // reader falls back to scanning (7.5.4). The census has to follow it there:
        // reporting the file as having zero objects when 77 were recovered is what this
        // flag exists to prevent, and it is what the command did before it was measured.
        let base = incrementally_updated();
        let broken = String::from_utf8_lossy(&base)
            .replace(&format!("startxref\n{}", find_last_startxref(&base)), "startxref\n999999")
            .into_bytes();

        let s = FileStructure::survey(&broken).expect("reads");
        assert!(s.revisions.is_empty(), "the damaged chain must yield no revision");
        assert!(s.objects.from_scan, "the census came from a scan and must say so");
        assert!(s.objects.in_file > 0, "the scan found objects the cross-reference did not");
        assert!(s.decisions.iter().any(|d| d.clause == "7.5.4"));
    }

    /// The offset the last `startxref` names, so a test can invalidate exactly it.
    fn find_last_startxref(bytes: &[u8]) -> u64 {
        let text = String::from_utf8_lossy(bytes);
        text.rmatch_indices("startxref")
            .next()
            .and_then(|(at, _)| text[at + 9..].split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    }

    /// A file with one image XObject, one content stream, and a dictionary that carries
    /// `/Filter` without being a stream — the shape that broke the first census.
    fn file_with_an_undecodable_image() -> Vec<u8> {
        let content = "q 10 0 0 10 0 0 cm /ImgX Do Q";
        let image = "\x01\x02\x03\x04";
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] \
              /Resources << /XObject << /ImgX 5 0 R >> >> /Contents 4 0 R >>"
                .to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            format!(
                "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray \
                  /BitsPerComponent 8 /Filter /XXXDecode /Length {} >>\nstream\n{image}\nendstream",
                image.len()
            ),
            // Not a stream, and `/Filter` here names a security handler (Table 20).
            "<< /Filter /Standard /V 2 /R 3 >>".to_string(),
        ];
        let mut out = b"%PDF-2.0\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let table_at = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", bodies.len() + 1).as_bytes(),
        );
        for offset in &offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n",
                bodies.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    /// The census names the filter, says it is on an image, and says it is not decoded.
    ///
    /// All three matter separately: the name is what a codec would have to implement,
    /// "on an image" is why not implementing it costs no text, and "not decoded" is the
    /// engine reporting its own gap rather than the roadmap asserting it.
    #[test]
    fn an_undecodable_image_filter_is_counted_as_one() {
        let s = FileStructure::survey(&file_with_an_undecodable_image()).expect("surveys");
        let unknown = s
            .filters
            .iter()
            .find(|f| f.name == "XXXDecode")
            .unwrap_or_else(|| panic!("the census missed it: {:?}", s.filters));
        assert_eq!(unknown.streams, 1);
        assert_eq!(unknown.on_images, 1, "it is on an /XObject /Subtype /Image");
        // `/CCITTFaxDecode` stood here, then `/JPXDecode`, and each in turn gained a
        // decoder — which is the check working rather than failing. What is left is a
        // filter the test suites invented, which no engine will ever decode, and that is
        // now the only thing the census's "not decoded" column can honestly name.
        assert!(!unknown.decoded, "no engine decodes a filter the standard does not define");
    }

    /// `/Filter` in an encryption dictionary is a security handler, not a stream filter.
    ///
    /// The first census walked every dictionary holding the key, on the reasoning that
    /// only a stream may carry one. Two files of the external corpus reported a filter
    /// called `/Standard` within minutes, which is the handler named by Table 20.
    #[test]
    fn a_security_handler_is_not_counted_as_a_stream_filter() {
        let s = FileStructure::survey(&file_with_an_undecodable_image()).expect("surveys");
        assert!(
            !s.filters.iter().any(|f| f.name == "Standard"),
            "the encryption dictionary is not a stream: {:?}",
            s.filters
        );
    }
}
