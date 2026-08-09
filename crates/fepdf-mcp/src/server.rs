use crate::tools::audit::{AuditArgs, audit_document_impl};
use crate::tools::render::{RenderArgs, render_page_impl};
use crate::tools::signature::{VerifySignaturesArgs, verify_signatures_impl};
use rmcp::{
    ServiceExt,
    handler::server::{ServerHandler, router::Router, wrapper::Parameters},
    tool, tool_handler, tool_router,
};

/// The fepdf MCP Server implementation.
///
/// It provides tools for PDF rendering and structural auditing via the
/// Model Context Protocol.
pub struct FepdfServer;

#[tool_handler]
impl ServerHandler for FepdfServer {}

#[tool_router]
impl FepdfServer {
    /// Creates a new instance of the fepdf MCP server.
    pub fn new() -> Self {
        Self
    }

    #[tool(
        name = "render_page",
        description = "Renders a specific page of a PDF document to a PNG image for visual inspection."
    )]
    /// MCP tool: render_page
    pub async fn render_page(
        &self,
        Parameters(args): Parameters<RenderArgs>,
    ) -> Result<String, String> {
        render_page_impl(args).await
    }

    #[tool(
        name = "audit_document",
        description = "Performs a structural compliance audit of a PDF document, checking Catalog, XRef, and Page Tree integrity."
    )]
    /// MCP tool: audit_document
    pub async fn audit_document(
        &self,
        Parameters(args): Parameters<AuditArgs>,
    ) -> Result<String, String> {
        audit_document_impl(args).await
    }

    #[tool(
        name = "verify_signatures",
        description = "Analyzes and verifies all digital signatures in a PDF, including integrity checks (MD5/SHA) and signer certificate validation."
    )]
    /// MCP tool: verify_signatures
    pub async fn verify_signatures(
        &self,
        Parameters(args): Parameters<VerifySignaturesArgs>,
    ) -> Result<String, String> {
        verify_signatures_impl(args).await
    }
}

impl Default for FepdfServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Entry point for running the fepdf MCP server over stdio.
pub async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = FepdfServer::new();
    let router = Router::new(server).with_tools(FepdfServer::tool_router());

    let transport = rmcp::transport::stdio();

    println!("fepdf MCP Server starting on stdio...");
    router.serve(transport).await.map_err(|e| format!("Server error: {e}"))?;

    Ok(())
}
