//! Ferruginous MCP: Model Context Protocol server for PDF intelligence.
//!
//! This crate implements an MCP server that exposes PDF rendering and
//! auditing capabilities to AI agents and LLM clients.

use thiserror::Error;

/// Error type for MCP operations.
#[derive(Error, Debug)]
pub enum McpError {
    /// Internal PDF processing error.
    #[error("PDF error: {0}")]
    Pdf(String),
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Other errors.
    #[error("Error: {0}")]
    Other(String),
}

/// Result type for MCP operations.
pub type McpResult<T> = Result<T, McpError>;

/// The core server implementation logic.
pub mod server;
/// The library of tools available to the MCP server.
pub mod tools;

pub use server::run_server;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_error_display() {
        let err = McpError::Pdf("Invalid PDF header".to_string());
        assert_eq!(format!("{err}"), "PDF error: Invalid PDF header");
    }
}
