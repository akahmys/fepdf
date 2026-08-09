//! Compliance & Conformance Artifacts (ISO 32000 / 19005 / 15930 / 14289)

use serde::{Deserialize, Serialize};

/// High-level compliance status of a PDF document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComplianceInfo {
    /// Basic metadata claims (PDF/A, PDF/X, PDF/UA).
    pub metadata: StandardMetadata,
    /// Whether the document contains a /StructTreeRoot (Logical Structure).
    pub has_struct_tree: bool,
    /// Whether the document is marked (MarkInfo /Marked true).
    pub is_marked: bool,
    /// Found OutputIntents (ICC profile identifiers).
    pub output_intents: Vec<String>,
    /// Any immediate compliance issues found during ingestion.
    pub issues: Vec<ComplianceIssue>,
}

/// Metadata claims extracted from XMP or Info dictionary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StandardMetadata {
    /// PDF/A version (e.g., Some(4)).
    pub pdf_a_part: Option<u32>,
    /// PDF/X version string (e.g., Some("PDF/X-6")).
    pub pdf_x_version: Option<String>,
    /// PDF/UA version (e.g., Some(2) for UA-2).
    pub pdf_ua_part: Option<u32>,
}

/// A specific compliance violation or warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceIssue {
    pub severity: Severity,
    pub standard: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}
