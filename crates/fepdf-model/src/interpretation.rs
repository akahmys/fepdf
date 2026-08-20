//! What the engine decided when the input was not what the standard describes.
//!
//! Files written before PDF 2.0 are frequently non-conforming, and parts of the older
//! specifications are genuinely ambiguous. Reading them means *deciding* — how to
//! delimit a stream whose `/Length` is wrong, what a font dictionary with no
//! `/Subtype` is, whether a byte sequence terminates an inline image.
//!
//! Those decisions are the substance of "read 1.7, write 2.0", so they are recorded
//! rather than logged. A caller must be able to tell "this loaded" from "this was
//! conforming", and `fepdf inspect audit` reports the difference.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// How the engine treats input that departs from the standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Strictness {
    /// Accept what can be understood, recording each decision. The default, because
    /// refusing real-world files is not useful.
    #[default]
    Lenient,
    /// Refuse the document when a [`Severity::Violation`] is found. For validating a
    /// producer, or for gating files entering an archive.
    Strict,
}

/// How far the input departed from the standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// The standard permits more than one reading and the engine picked one.
    Ambiguity,
    /// The input is technically wrong but the intent is unmistakable, so it was
    /// repaired.
    Repaired,
    /// The input contradicts a requirement and no repair was possible; something was
    /// dropped or substituted.
    Violation,
}

/// One decision the engine made about non-conforming or ambiguous input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// How far the input departed from the standard.
    pub severity: Severity,
    /// The clause that governs it, such as `7.3.8.2`. Empty when the standard is
    /// silent and the decision follows established reader behaviour instead.
    pub clause: Cow<'static, str>,
    /// What was found in the input.
    pub found: String,
    /// What the engine did about it, in terms the caller can act on.
    pub action: String,
}

impl Decision {
    /// Records a reading chosen where the standard permits several.
    pub fn ambiguity(
        clause: impl Into<Cow<'static, str>>,
        found: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Ambiguity,
            clause: clause.into(),
            found: found.into(),
            action: action.into(),
        }
    }

    /// Records wrong input whose intent was clear enough to repair.
    pub fn repaired(
        clause: impl Into<Cow<'static, str>>,
        found: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Repaired,
            clause: clause.into(),
            found: found.into(),
            action: action.into(),
        }
    }

    /// Records a requirement the input contradicts, with what was lost.
    pub fn violation(
        clause: impl Into<Cow<'static, str>>,
        found: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Violation,
            clause: clause.into(),
            found: found.into(),
            action: action.into(),
        }
    }
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag = match self.severity {
            Severity::Ambiguity => "AMBIGUITY",
            Severity::Repaired => "REPAIRED",
            Severity::Violation => "VIOLATION",
        };
        if self.clause.is_empty() {
            write!(f, "[{tag}] {} -> {}", self.found, self.action)
        } else {
            write!(f, "[{tag}] ISO {} : {} -> {}", self.clause, self.found, self.action)
        }
    }
}

/// Every decision taken while reading one document.
///
/// **Behind a lock, so that a decision taken through `&Document` can still be
/// recorded.** Reading a file is not the only moment the engine departs from the
/// standard: interpreting a page skips an image whose filter it cannot decode, and the
/// interpreter holds a shared reference. Until this lock existed that departure reached
/// `log::debug!` and nothing else — §5.3 says the engine records rather than logs, and
/// one place in it could not. The alternative was to return the decisions from
/// `render_page` and `extract_text`, which puts a departure somewhere `inspect
/// structure` will not print and changes every caller's signature to carry a note about
/// a picture (ADR-0018).
///
/// The consequence is that the log **grows as the document is used**, so
/// [`DecisionLog::is_conforming`] answers "no departure in what has been examined"
/// rather than "no departure". That was always true — a file whose pages are never
/// interpreted has never been fully read — and the lock makes it visible rather than
/// introducing it.
#[derive(Default)]
pub struct DecisionLog {
    entries: parking_lot::Mutex<Vec<Decision>>,
}

impl DecisionLog {
    /// Records a decision. Takes `&self`, so a shared reference is enough.
    pub fn push(&self, decision: Decision) {
        log::debug!("{decision}");
        self.entries.lock().push(decision);
    }

    /// Consumes the log, yielding the decisions it recorded.
    #[must_use]
    pub fn into_entries(self) -> Vec<Decision> {
        self.entries.into_inner()
    }

    /// The decisions recorded so far, in the order they were taken.
    ///
    /// A snapshot rather than a borrow: the log is behind a lock, and a caller holding
    /// a guard while interpreting a page would deadlock against the interpreter that
    /// records into it. Logs are small — 11 decisions across 251 files of both corpora
    /// — so copying one is not a cost worth designing around.
    #[must_use]
    pub fn entries(&self) -> Vec<Decision> {
        self.entries.lock().clone()
    }

    /// How many decisions have been recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Whether nothing has been recorded. The same question as
    /// [`DecisionLog::is_conforming`], under the name a reader of a collection expects.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }

    /// Whether the document was read without any departure from the standard **in what
    /// has been examined so far**. See the type's documentation.
    #[must_use]
    pub fn is_conforming(&self) -> bool {
        self.entries.lock().is_empty()
    }

    /// Decisions at or above `severity`.
    #[must_use]
    pub fn at_least(&self, severity: Severity) -> Vec<Decision> {
        self.entries.lock().iter().filter(|d| d.severity >= severity).cloned().collect()
    }

    /// Whether `strictness` should reject a document carrying these decisions.
    #[must_use]
    pub fn rejects_under(&self, strictness: Strictness) -> bool {
        strictness == Strictness::Strict
            && self.entries.lock().iter().any(|d| d.severity >= Severity::Violation)
    }
}

impl std::fmt::Debug for DecisionLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DecisionLog").field(&*self.entries.lock()).finish()
    }
}

impl Clone for DecisionLog {
    fn clone(&self) -> Self {
        Self { entries: parking_lot::Mutex::new(self.entries.lock().clone()) }
    }
}

impl From<Vec<Decision>> for DecisionLog {
    fn from(entries: Vec<Decision>) -> Self {
        Self { entries: parking_lot::Mutex::new(entries) }
    }
}

impl Serialize for DecisionLog {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.entries.lock().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DecisionLog {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<Decision>::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_read_records_nothing() {
        let log = DecisionLog::default();
        assert!(log.is_conforming());
        assert!(!log.rejects_under(Strictness::Strict));
    }

    #[test]
    fn lenient_accepts_what_strict_refuses() {
        let log = DecisionLog::default();
        log.push(Decision::violation(
            "9.6.2",
            "font dictionary with no /Subtype",
            "treated as Type1",
        ));
        assert!(log.rejects_under(Strictness::Strict));
        assert!(!log.rejects_under(Strictness::Lenient));
    }

    #[test]
    fn repairs_alone_do_not_fail_a_strict_read() {
        // A wrong /Length is repairable without losing anything, so it must not
        // reject a document that a producer would otherwise consider valid output.
        let log = DecisionLog::default();
        log.push(Decision::repaired(
            "7.3.8.2",
            "/Length 5, stream ran 4096 bytes",
            "scanned to endstream",
        ));
        assert!(!log.rejects_under(Strictness::Strict));
        assert!(!log.is_conforming());
    }

    #[test]
    fn severity_filtering_is_ordered() {
        let log = DecisionLog::default();
        log.push(Decision::ambiguity("", "text string without BOM", "decoded as PDFDocEncoding"));
        log.push(Decision::violation("9.6.2", "missing /Subtype", "treated as Type1"));
        assert_eq!(log.at_least(Severity::Ambiguity).len(), 2);
        assert_eq!(log.at_least(Severity::Violation).len(), 1);
    }

    #[test]
    fn display_names_the_clause_when_there_is_one() {
        let d = Decision::repaired("7.3.8.2", "wrong /Length", "scanned to endstream");
        assert_eq!(
            format!("{d}"),
            "[REPAIRED] ISO 7.3.8.2 : wrong /Length -> scanned to endstream"
        );

        let d = Decision::ambiguity("", "no BOM", "PDFDocEncoding");
        assert_eq!(format!("{d}"), "[AMBIGUITY] no BOM -> PDFDocEncoding");
    }
}

#[cfg(test)]
mod carried_everywhere {
    //! Every report type carries the decision log.
    //!
    //! Phase B's last item was to surface it beyond the audit. Without a check this is
    //! a claim in a roadmap: a report added later would omit it and nothing would say
    //! so. Asserting on the reports rather than on the CLI keeps this fast; the
    //! renderers all read the same field.

    use crate::catalog::CatalogReport;
    use crate::file_structure::FileStructure;
    use crate::interactive::InteractiveReport;

    /// A file with 300 bytes before `%PDF-`, which is a `Repaired` under 7.5.2.
    fn prefixed() -> Vec<u8> {
        let body = "%PDF-2.0\n\
             1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
             2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
             3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n";
        let offsets: Vec<usize> =
            (1..=3).map(|n| body.find(&format!("\n{n} 0 obj")).map_or(0, |p| p + 1)).collect();
        let mut out = String::from(body);
        let xref_at = out.len();
        out.push_str("xref\n0 4\n0000000000 65535 f \n");
        for off in &offsets {
            out.push_str(&format!("{off:010} 00000 n \n"));
        }
        out.push_str(&format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n"));

        let mut prefixed = vec![b'X'; 300];
        prefixed.extend_from_slice(out.as_bytes());
        prefixed
    }

    #[test]
    fn every_report_carries_the_decision_log() {
        let bytes = prefixed();

        let structure = FileStructure::survey(&bytes).expect("structure");
        let catalog = CatalogReport::survey(&bytes).expect("catalog");
        let interactive = InteractiveReport::survey(&bytes).expect("interactive");

        for (name, decisions) in [
            ("structure", &structure.decisions),
            ("catalog", &catalog.decisions),
            ("interactive", &interactive.decisions),
        ] {
            assert!(
                decisions.iter().any(|d| d.clause == "7.5.2"),
                "{name} does not carry the 7.5.2 repair: {decisions:?}"
            );
        }
    }

    #[test]
    fn a_conforming_file_leaves_every_report_silent() {
        // The other half: a log that is never empty is a log nobody reads. Strip the
        // prefix and the same file must record nothing anywhere.
        let bytes = prefixed()[300..].to_vec();

        assert!(FileStructure::survey(&bytes).expect("structure").decisions.is_empty());
        assert!(CatalogReport::survey(&bytes).expect("catalog").decisions.is_empty());
        assert!(InteractiveReport::survey(&bytes).expect("interactive").decisions.is_empty());
    }
}
