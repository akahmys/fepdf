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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecisionLog {
    entries: Vec<Decision>,
}

impl DecisionLog {
    /// Records a decision.
    pub fn push(&mut self, decision: Decision) {
        log::debug!("{decision}");
        self.entries.push(decision);
    }

    /// Every decision, in the order they were taken.
    #[must_use]
    /// Consumes the log, yielding the decisions it recorded.
    pub fn into_entries(self) -> Vec<Decision> {
        self.entries
    }

    /// The decisions recorded so far, in the order they were taken.
    pub fn entries(&self) -> &[Decision] {
        &self.entries
    }

    /// Whether the document was read without any departure from the standard.
    #[must_use]
    pub fn is_conforming(&self) -> bool {
        self.entries.is_empty()
    }

    /// Decisions at or above `severity`.
    pub fn at_least(&self, severity: Severity) -> impl Iterator<Item = &Decision> + '_ {
        self.entries.iter().filter(move |d| d.severity >= severity)
    }

    /// Whether `strictness` should reject a document carrying these decisions.
    #[must_use]
    pub fn rejects_under(&self, strictness: Strictness) -> bool {
        strictness == Strictness::Strict && self.at_least(Severity::Violation).next().is_some()
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
        let mut log = DecisionLog::default();
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
        let mut log = DecisionLog::default();
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
        let mut log = DecisionLog::default();
        log.push(Decision::ambiguity("", "text string without BOM", "decoded as PDFDocEncoding"));
        log.push(Decision::violation("9.6.2", "missing /Subtype", "treated as Type1"));
        assert_eq!(log.at_least(Severity::Ambiguity).count(), 2);
        assert_eq!(log.at_least(Severity::Violation).count(), 1);
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
