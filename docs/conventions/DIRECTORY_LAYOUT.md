# fepdf Workspace Directory Layout (WS-01)

This document defines the canonical directory structure of fepdf. Adherence to this hierarchy is mandatory for both human developers and autonomous agents to ensure discovery efficiency and data isolation.

---

## 1. Directory Hierarchy

| Path | Purpose | Ownership |
| :--- | :--- | :--- |
| `.agents/` | Governance & Agent Protocols | Antigravity IDE |
| `assets/` | Static, Read-only Resources (Fonts, Models) | Project |
| `crates/` | Modular Rust Logic Layer | Engineering |
| `docs/` | Technical Specs & Architectural History | Architecture |
| `external/` | Submodules & Third-party Compliance Data | Engineering |
| `crates/*/examples/` | Rust Usage Examples & Demonstrations. Must live under the crate they exercise — a root `examples/` directory is never compiled, because the workspace root has no `[package]`. | Engineering |
| `out/` | Ephemeral & Persistent Outputs (Ignored by Git) | Pipeline |
| `out/artifacts/`| Test results, renders, and temporary PDFs | CI/CD |
| `out/exports/` | Extracted document assets (Fonts, Images) | Refinery |
| `samples/` | Test Input Corpus (PDFs) | QA |
| `scripts/` | Automation & CI/CD Scripts | DevOps |

## 2. Organization Rules

1.  **Consolidation**: All static resources MUST reside within `assets/`. Prohibit root-level resource directories (e.g., `resources/`).
2.  **Output Isolation**: All dynamically generated files MUST reside within `out/`.
3.  **Script Categorization**:
    *   `scripts/audit/`: Compliance, security, and static analysis.
    *   `scripts/dev/`: Developer productivity and UI utilities.
    *   `scripts/test/`: Integration and functional testing.
4.  **Documentation Locality**: All technical specifications and architectural history MUST reside within `docs/`. High-level vision documents (`README.md`, `ROADMAP.md`, `AGENTS.md`) are permitted at the root for maximum visibility.
5.  **Scratch & Utility Binaries**:
    *   Prototyping debug scripts in `src/bin/` are permitted for initial verification.
    *   Once stabilized, their logic MUST be integrated into standard product CLI subcommands (e.g., `fepdf debug <cmd>`) or standardized as formal regression tests.
    *   Redundant or obsolete prototyping files MUST be purged during milestone stabilization to prevent codebase rot.
    *   Infrastructure binaries (e.g., `verify_render.rs` for visual regressions, `bypass_decrypt.rs` for emergency recovery) are exempt but MUST be clean of hardcoded values and compile warning-free under RR-15.

## 3. Governance Rules

1.  **No Redundancy**: Do not copy files from `external/` to `assets/`. Point the engine directly to the unified `external/` paths.
2.  **Script Placement**: Always place new automation in the appropriate `scripts/` subdirectory (`audit`, `dev`, or `test`).
3.  **Clean Root**: Keep the project root clean. Only core project metadata (`README`, `ROADMAP`, `VISION`, `LICENSE`) and workspace Cargo files should reside here.

## 4. Maintenance

- Every new directory added to the root MUST be registered in this document.
- Root-level stray files are prohibited except for core configuration (`Cargo.toml`, `Makefile`, `LICENSE`).
