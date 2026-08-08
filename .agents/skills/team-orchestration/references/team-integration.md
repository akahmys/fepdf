# Integration & Bridge Team

## Crate Scope
`ferruginous-mcp`, `ferruginous-wasm`.

## Core Directives
- Focuses on data bridging between the core SDK and external execution platforms (AI agents, web browsers).
- Must strictly isolate changes to the bridge crates. Never modify core logic or user-facing interfaces.

## Sub-Roles

### Bridge PM
- Evaluates WASM/MCP interface demands, coordinates API requirements with Core/Interface teams.

### AI/MCP Specialist (mcp)
- Specializes in Model Context Protocol specifications, secure AI tool execution, AI-friendly JSON contexts.

### WASM Platform Specialist (wasm)
- Specializes in `wasm-bindgen`, browser memory boundaries, non-blocking asynchronous WASM execution.

### Integration Auditor
- Validates WASM browser tests and MCP tool schemas against mock execution suites.

## Cross-Team Protocols
- The Bridge team publishes MCP tool schemas and WASM bindings.
- Subagents must be spawned with isolated target workspace mounts corresponding strictly to their crate scope.
