# fepdf Workspace Directory Layout (WS-01)

This document defines the canonical directory structure of fepdf. Adherence to this hierarchy is mandatory for both human developers and autonomous agents to ensure discovery efficiency and data isolation.

---

## 1. Directory Hierarchy

| Path | Purpose | Ownership |
| :--- | :--- | :--- |
| `assets/` | Static, Read-only Resources (Fonts, Models) | Project |
| `crates/` | Modular Rust Logic Layer | Engineering |
| `docs/` | Technical Specs & Architectural History | Architecture |
| `external/` | Submodules & Third-party Compliance Data | Engineering |
| `crates/*/examples/` | Rust Usage Examples & Demonstrations. Must live under the crate they exercise — a root `examples/` directory is never compiled, because the workspace root has no `[package]`. | Engineering |
| `out/` | Ephemeral & Persistent Outputs (Ignored by Git) | Pipeline |
| `out/artifacts/`| Test results, renders, and temporary PDFs | CI/CD |
| `out/exports/` | Extracted document assets (Fonts, Images) | Refinery |
| `samples/` | Test Input Corpus (PDFs) — **files this project chose**, 9 of them | QA |
| `scripts/` | Automation & CI/CD Scripts | DevOps |
| `target/` | Cargo's build directory, **and the external corpora**: 515 files this project did not choose, under `target/external/`, plus `encrypted/`, `malformed/`, `scans/`, `layers/` and `colour/`. Git-ignored, fetched by `scripts/test/fetch_external_corpus.sh` | QA |

## 2. Organization Rules

1.  **Consolidation**: All static resources MUST reside within `assets/`. Prohibit root-level resource directories (e.g., `resources/`).
2.  **Output Isolation**: All dynamically generated files MUST reside within `out/`.
    (Checked 2026-08-22: **not held, in two different ways.** Three writers use
    `out/artifacts/` — `scripts/dev/render_pages.sh`, `scripts/test/batch_process.sh` —
    and three use a root-level `artifacts/`: `crates/fepdf/examples/render_all_samples.rs`,
    `render_japanese_samples.rs` and `scripts/test/hiragana_render_test.sh`. Both are
    git-ignored, so nothing was ever going to notice.

    The second was worse and is fixed: `fepdf debug extract-font` wrote to a root-level
    **`exports/`**, which is unregistered, **not git-ignored**, and never created — so the
    write failed unless the user had made the directory by hand, and left untracked files
    in the repository root when they had. It writes to the registered `out/exports/` now
    and creates it. A `#[ignore]`d test in `fepdf-font` read a file from that same path,
    which meant it could not have run even without the attribute; it was removed.

    Registering `target/` above is the same omission from the other direction: §4 says
    every root directory must be registered, and the one holding 515 corpus files was
    not.)
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

- Every new directory added to the root MUST be registered in this document. **Nothing
  checks this**, which is how `target/` — the largest and most load-bearing of them —
  went unregistered through five phases that measured against its contents.
- Root-level stray files are prohibited except for core configuration (`Cargo.toml`, `Makefile`, `LICENSE`).
- Tool-owned dotfile directories (`.git/`, `.github/`, `.cargo/`, `.claude/`, `.gemini/`)
  are not registered and do not need to be. This document governs what the project puts
  in the tree, not what its tools do.
