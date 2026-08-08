# Core Library Team

## Crate Scope
`ferruginous-core`, `ferruginous-render`, `ferruginous-sdk`, `ferruginous-macros`.

## Core Directives
- Focuses on parsing, memory allocation (`PdfArena`), CJK font loading, Vello GPU rasterization, and SDK bindings.
- Must strictly isolate code changes to core/library crates. Never modify bridge or interface crates.

## Sub-Roles

### Core PM
- Coordinates core engine milestones, parses ISO 32000-2 clauses, and defines strict API contracts for integration layers.

### Core Logic Engineer
- Writes memory-safe, ultra-fast Rust algorithms (MSRV 1.94, RR-15 compliant).

### Core Spec Auditor
- Conducts static/logical compliance audits, checks memory layouts, and designs core regression testing.

## Cross-Team Protocols
- The Core team publishes type-safe contracts via `ferruginous-sdk`.
- Any API contract adjustments require multi-PM signature in the session plan.
