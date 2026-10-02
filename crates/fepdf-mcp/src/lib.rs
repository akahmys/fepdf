//! fepdf MCP: Model Context Protocol server for PDF intelligence.
//!
//! This crate implements an MCP server that exposes PDF rendering and
//! auditing capabilities to AI agents and LLM clients.

#![allow(unknown_lints)]
#![allow(clippy::unused_async_trait_impl)]

use thiserror::Error;

/// Error type for MCP operations.
///
/// **An engine error is carried, not printed**
/// ([ADR-0102](../../../docs/adr/0102-an-error-says-whose-it-is.md)). This was
/// `Pdf(String)`, so an argument the model got wrong and a defect in the engine reached
/// the client as the same text; [`IntoCallToolResult`] below answers the first as an
/// error in the call and the second as an error of the server.
#[derive(Error, Debug)]
pub enum McpError {
    /// The engine refused or failed, while doing `during`.
    #[error("{during}: {error}")]
    Pdf {
        /// What the tool was doing, as a person would say it.
        during: std::borrow::Cow<'static, str>,
        /// What the engine answered.
        error: fepdf::PdfError,
    },
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The call's arguments cannot be carried out as given.
    #[error("{0}")]
    Other(String),
}

impl McpError {
    /// The engine's `error`, met while doing `during`.
    pub fn pdf(during: impl Into<std::borrow::Cow<'static, str>>, error: fepdf::PdfError) -> Self {
        Self::Pdf { during: during.into(), error }
    }
}

impl From<String> for McpError {
    fn from(why: String) -> Self {
        Self::Other(why)
    }
}

/// Whether `error` is this engine's fault rather than something the call can change.
///
/// Every variant is named, so a new one is a decision here and not a default.
const fn engine_fault(error: &fepdf::PdfError) -> bool {
    use fepdf::PdfError;
    match error {
        PdfError::Internal(_)
        | PdfError::Arena(_)
        | PdfError::HintStreamOverflow { .. }
        | PdfError::LinearizationSyncError { .. } => true,
        PdfError::Io(_)
        | PdfError::Parse { .. }
        | PdfError::Ingestion { .. }
        | PdfError::Filter { .. }
        | PdfError::DepthLimitExceeded(_)
        | PdfError::ClauseViolation { .. }
        | PdfError::Crypto(_)
        | PdfError::Syntax(_)
        | PdfError::NotImplemented(_)
        | PdfError::NotFound(_)
        | PdfError::Refused { .. } => false,
    }
}

impl rmcp::handler::server::tool::IntoCallToolResult for McpError {
    /// The model's error comes back as the call's result, marked as an error, so it can
    /// read why and try again; the engine's goes back as the server's.
    fn into_call_tool_result(self) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let theirs = match &self {
            Self::Pdf { error, .. } => !engine_fault(error),
            Self::Io(_) | Self::Other(_) => true,
            Self::Serialization(_) => false,
        };
        if theirs {
            Ok(rmcp::model::CallToolResult::error(vec![rmcp::model::Content::text(
                self.to_string(),
            )]))
        } else {
            Err(rmcp::ErrorData::internal_error(self.to_string(), None))
        }
    }
}

/// Result type for MCP operations.
pub type McpResult<T> = Result<T, McpError>;

/// MCP prompts for accessibility audit and remediation.
pub mod prompts;
/// MCP resources for live inspection of PDF structures.
pub mod resources;
/// The core server implementation logic.
// Private: nothing outside this crate names it. `run_server` is re-exported below and
// is the whole of what the binary uses.
mod server;
/// The library of tools available to the MCP server.
pub mod tools;

pub use server::{FepdfServer, run_server};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_error_display() {
        let err = McpError::pdf("opening the PDF", fepdf::PdfError::no_page(4, 2));
        assert_eq!(format!("{err}"), "opening the PDF: this document has 2 pages and no page 4");
    }

    /// **The model's error is the call's; the engine's is the server's.** Both read the
    /// same through `Pdf(String)`, so a client could not tell an argument to change from a
    /// defect to report.
    #[test]
    fn a_refusal_answers_the_call_and_an_engine_fault_answers_as_the_server() {
        use rmcp::handler::server::tool::IntoCallToolResult;
        let refused = McpError::pdf("joining runs", fepdf::PdfError::refused("MergeRuns", "no"));
        let answered = refused.into_call_tool_result().expect("a refusal is the call's result");
        assert_eq!(answered.is_error, Some(true), "a refusal was reported as a success");

        let broken = McpError::pdf("saving", fepdf::PdfError::internal("a handle was dangling"));
        assert!(
            broken.into_call_tool_result().is_err(),
            "an engine fault was handed to the model as something it could fix"
        );
    }
}
