//! How much of what a corpus presents this engine reads the *contents* of.
//!
//! `ROADMAP.md` opens with "an engine that understands ISO 32000-2 semantically". That
//! is not a predicate: no run of anything can report it true or false, and
//! `status.sh` closed by saying so while every phase beneath it had a completion
//! condition that could be checked. This is the nearest thing that can be measured.
//!
//! **The denominator is what the files contain, not what the standard defines.** A
//! construct no file writes can neither raise the figure nor lower it, which is the
//! property that matters: this project has twice built containers before their contents
//! and had the count read as progress ([ADR-0017](../../docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md)).
//! Typing `/DPartRoot`, which occurs in none of the 251 files of both corpora, moves
//! nothing here.
//!
//! **The numerator is reading contents, not reaching them.** A catalogue entry counts
//! when [`crate::catalog::Support::Modelled`] — a field typed `Option<Object>` hands
//! back what the arena already held. An annotation entry counts when the subtype's
//! readers name it. A filter counts when [`crate::filters::is_decoded`] is true of it.
//!
//! What this is **not** is stated in
//! [ADR-0019](../../docs/adr/0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md),
//! and belongs here too: a coverage figure over what a corpus happened to contain is a
//! *proxy* for understanding. It says nothing about whether what was read was read
//! correctly, and a corpus that presents little will flatter an engine that does little.
//! Three axes have a denominator worth having today; adding a fourth can only lower the
//! figure, which is the direction that keeps it honest.

use crate::catalog::{CatalogReport, Support};
use crate::error::PdfResult;
use crate::file_structure::FileStructure;
use crate::interactive::InteractiveReport;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The axes measured, with the clause each is drawn from.
///
/// Three, and each is here because its denominator is a set the engine can enumerate
/// from a file without guessing. Actions (12.6) are the obvious fourth and are absent
/// on purpose: "reads an action" has no agreed meaning yet — a `/GoTo`'s destination
/// resolves through the name tree while a `/URI`'s target is never looked at — and an
/// axis whose numerator is a judgement call is one the figure can be argued into.
pub const AXES: &[(&str, &str)] =
    &[("catalogue entries", "7.7.2"), ("annotation entries", "12.5"), ("stream filters", "7.4")];

/// What one or more files present, and how much of it is read.
///
/// Sets rather than counts, so that merging two files does not count the same construct
/// twice — 224 files write `/FlateDecode` and that is one filter, not 224.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Coverage {
    presented: BTreeMap<String, BTreeSet<String>>,
    read: BTreeMap<String, BTreeSet<String>>,
}

/// One axis's figure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AxisCoverage {
    /// The axis, as [`AXES`] names it.
    pub axis: &'static str,
    /// The clause it is drawn from.
    pub clause: &'static str,
    /// Distinct constructs the files present.
    pub presented: usize,
    /// How many of those the engine reads the contents of.
    pub read: usize,
}

impl AxisCoverage {
    /// The proportion read, or `None` when the corpus presents nothing on this axis —
    /// which is not the same as reading none of it, and must not print as 0%.
    #[must_use]
    pub fn fraction(&self) -> Option<f64> {
        (self.presented > 0).then(|| self.read as f64 / self.presented as f64)
    }
}

impl Coverage {
    /// Measures one file.
    ///
    /// # Errors
    /// Fails when the file cannot be read far enough to survey — a file that opens but
    /// carries nothing on an axis contributes an empty set, which is different.
    pub fn of(bytes: &[u8]) -> PdfResult<Self> {
        let mut c = Self::default();

        // 7.7.2 — a key counts when the file carries it, and is read when its field
        // says what it holds rather than handing back the arena's own object.
        let catalog = CatalogReport::survey(bytes)?;
        for entry in &catalog.entries {
            c.present("catalogue entries", &entry.key);
            if entry.support == Support::Modelled {
                c.mark_read("catalogue entries", &entry.key);
            }
        }

        // 12.5 — per subtype, because `/Parent` is read on a `/Popup` and on nothing
        // else. `/Circle /BS` and `/Movie /BS` are two constructs, and folding them
        // into one would let a reader for either claim both.
        if let Ok(interactive) = InteractiveReport::survey(bytes) {
            for subtype in &interactive.annotations.subtypes {
                for entry in &subtype.entries {
                    let name = format!("/{}  {}", subtype.subtype, entry.key);
                    c.present("annotation entries", &name);
                    if entry.read {
                        c.mark_read("annotation entries", &name);
                    }
                }
            }
        }

        // 7.4 — the census walks streams, so a filter named inside a compressed object
        // stream counts. Searching the bytes for one finds nothing (`file_structure.rs`).
        let structure = FileStructure::survey(bytes)?;
        for filter in &structure.filters {
            c.present("stream filters", &filter.name);
            if filter.decoded {
                c.mark_read("stream filters", &filter.name);
            }
        }

        Ok(c)
    }

    /// Folds another file's coverage into this one.
    pub fn merge(&mut self, other: &Self) {
        for (axis, set) in &other.presented {
            self.presented.entry(axis.clone()).or_default().extend(set.iter().cloned());
        }
        for (axis, set) in &other.read {
            self.read.entry(axis.clone()).or_default().extend(set.iter().cloned());
        }
    }

    /// Each axis, in the order [`AXES`] declares.
    #[must_use]
    pub fn axes(&self) -> Vec<AxisCoverage> {
        AXES.iter()
            .map(|(axis, clause)| AxisCoverage {
                axis,
                clause,
                presented: self.presented.get(*axis).map_or(0, BTreeSet::len),
                read: self.read.get(*axis).map_or(0, BTreeSet::len),
            })
            .collect()
    }

    /// Every axis at once: constructs read, constructs presented.
    ///
    /// One number, and it weights an axis by how many distinct constructs it presents
    /// rather than by importance — 20 catalogue keys count for more than 7 filters
    /// because there are more of them, not because they matter more. Averaging the
    /// three percentages instead would let an axis with two constructs swing the total
    /// as far as one with two hundred.
    #[must_use]
    pub fn total(&self) -> (usize, usize) {
        self.axes().iter().fold((0, 0), |(r, p), a| (r + a.read, p + a.presented))
    }

    /// The constructs an axis presents that are not read, which is what a caller acts on.
    #[must_use]
    pub fn unread(&self, axis: &str) -> Vec<String> {
        let read = self.read.get(axis);
        self.presented
            .get(axis)
            .map(|set| {
                set.iter().filter(|k| !read.is_some_and(|r| r.contains(*k))).cloned().collect()
            })
            .unwrap_or_default()
    }

    fn present(&mut self, axis: &str, construct: &str) {
        self.presented.entry(axis.to_string()).or_default().insert(construct.to_string());
    }

    fn mark_read(&mut self, axis: &str, construct: &str) {
        self.read.entry(axis.to_string()).or_default().insert(construct.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file carrying two catalogue keys, one annotation and one filtered stream.
    fn small_document() -> Vec<u8> {
        let content = "BT ET";
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R /PageMode /UseNone >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Contents 4 0 R \
              /Annots [5 0 R] >>"
                .to_string(),
            format!(
                "<< /Length {} /Filter /ASCIIHexDecode >>\nstream\n{content}\nendstream",
                content.len()
            ),
            "<< /Type /Annot /Subtype /Link /Rect [0 0 1 1] /Dest [3 0 R /Fit] >>".to_string(),
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

    /// **The property the whole measurement rests on**: a construct the file does not
    /// carry is in neither the numerator nor the denominator.
    ///
    /// Without it the figure could be raised by adding a type for something nothing
    /// reaches, which is how "32 of Table 29's 32" came to coexist with six entries
    /// whose contents the engine could read (ADR-0017). `/DPartRoot` is the case in
    /// point: it has a field, occurs in none of the 251 files of both corpora, and must
    /// not appear here.
    #[test]
    fn a_construct_no_file_carries_counts_in_neither_direction() {
        let c = Coverage::of(&small_document()).expect("measures");
        let catalogue: Vec<String> = c
            .presented
            .get("catalogue entries")
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        assert!(catalogue.contains(&"PageMode".to_string()));
        assert!(
            !catalogue.contains(&"DPartRoot".to_string()),
            "the file does not carry it, so neither does the denominator: {catalogue:?}"
        );
        assert!(!catalogue.contains(&"AcroForm".to_string()), "{catalogue:?}");
    }

    /// Reaching an entry is not reading it.
    ///
    /// `/Type` is the example because Phase K left it deliberately unread — 7.7.2 fixes
    /// its value at `/Catalog`, so a reader for it is an assertion, not a type. It was
    /// `/Pages` until that phase gave the page tree root a reader.
    #[test]
    fn a_declared_entry_is_presented_and_not_read() {
        let c = Coverage::of(&small_document()).expect("measures");
        let unread = c.unread("catalogue entries");
        assert!(unread.contains(&"Type".to_string()), "typed `Handle<PdfName>`: {unread:?}");
        assert!(!unread.contains(&"PageMode".to_string()), "typed `Option<PageMode>`");
        assert!(!unread.contains(&"Pages".to_string()), "Phase K: `Located<PageTreeRoot>`");
    }

    /// A filter the engine decodes counts; the axis is per name, not per stream.
    #[test]
    fn a_decoded_filter_counts_once_however_many_streams_use_it() {
        let c = Coverage::of(&small_document()).expect("measures");
        let filters = c.axes().into_iter().find(|a| a.axis == "stream filters").expect("axis");
        assert_eq!(filters.presented, 1);
        assert_eq!(filters.read, 1, "/ASCIIHexDecode decodes");
    }

    /// Merging two files counts a shared construct once.
    #[test]
    fn merging_two_files_does_not_count_a_construct_twice() {
        let mut a = Coverage::of(&small_document()).expect("measures");
        let b = Coverage::of(&small_document()).expect("measures");
        let before = a.total();
        a.merge(&b);
        assert_eq!(a.total(), before, "the same file twice presents the same constructs");
    }

    /// An axis a corpus says nothing about has no fraction, and must not read as 0%.
    #[test]
    fn an_axis_with_nothing_presented_has_no_fraction() {
        let empty = AxisCoverage { axis: "x", clause: "y", presented: 0, read: 0 };
        assert!(empty.fraction().is_none());
    }
}
