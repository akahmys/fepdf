//! Integration tests for Model Context Protocol (MCP) Server logic & tool schemas

use ferruginous_mcp::McpError;

#[test]
fn test_mcp_error_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "PDF file missing");
    let mcp_err: McpError = io_err.into();
    assert!(format!("{mcp_err}").contains("PDF file missing"));
}
