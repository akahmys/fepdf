//! fepdf MCP Server Binary.
//!
//! This is the entry point for the stdio-based MCP server.

use fepdf_mcp::run_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_server().await
}
