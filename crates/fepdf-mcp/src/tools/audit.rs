use crate::{McpError, McpResult};
use bytes::Bytes;
use fepdf_sdk::{IssueSeverity, PdfDocument};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

/// Arguments for the structural audit tool.
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct AuditArgs {
    /// Path to the PDF document to audit.
    pub path: String,
}

/// A summary report of the structural audit.
#[derive(Serialize, Deserialize, Debug)]
pub struct AuditReport {
    /// Overall audit status (PASSED, FAILED, CRITICAL_FAILURE).
    pub status: String,
    /// Detailed list of findings during the audit.
    pub findings: Vec<Finding>,
}

/// A specific observation or error found during the audit.
#[derive(Serialize, Deserialize, Debug)]
pub struct Finding {
    /// Severity level (Info, Warning, Error).
    pub severity: String,
    /// The functional area where the issue was found (Structural, Compliance).
    pub category: String,
    /// Detailed description of the finding.
    pub message: String,
}

/// Implementation of the structural audit logic.
pub async fn audit_document_impl(args: AuditArgs) -> Result<String, String> {
    audit_document_internal(args).map_err(|e| e.to_string())
}

fn audit_document_internal(args: AuditArgs) -> McpResult<String> {
    let mut findings = Vec::new();

    // 1. Load File
    let data = Bytes::from(
        fs::read(&args.path).map_err(|e| McpError::Other(format!("Failed to read file: {e}")))?,
    );

    // 2. Open Document (Structural Check)
    let doc = match PdfDocument::open(data) {
        Ok(d) => d,
        Err(e) => {
            findings.push(Finding {
                severity: "Error".into(),
                category: "Structural".into(),
                message: format!("Document failed to open: {e}"),
            });
            return Ok(serde_json::to_string_pretty(&AuditReport {
                status: "CRITICAL_FAILURE".into(),
                findings,
            })?);
        }
    };

    findings.push(Finding {
        severity: "Info".into(),
        category: "Structural".into(),
        message: "XRef chain and trailer resolved successfully.".into(),
    });

    // 3. Use SDK Summary for Audit
    let summary = doc.get_summary().map_err(|e| McpError::Pdf(e.to_string()))?;

    for issue in summary.compliance.issues {
        findings.push(Finding {
            severity: match issue.severity {
                IssueSeverity::Error | IssueSeverity::Critical => "Error".into(),
                IssueSeverity::Warning => "Warning".into(),
                IssueSeverity::Info => "Info".into(),
            },
            category: "Compliance".into(),
            message: issue.message,
        });
    }

    let status = if findings.iter().any(|f| f.severity == "Error") { "FAILED" } else { "PASSED" };

    Ok(serde_json::to_string_pretty(&AuditReport { status: status.into(), findings })?)
}
