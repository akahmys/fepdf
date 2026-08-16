# 🧪 fepdf Testing & Validation Strategy

This document details the testing methodology, test suites, visual regression framework, and quality assurance processes for fepdf.

---

## 🎯 1. Test Pyramid & Separation Policy

fepdf employs a 3-tier testing hierarchy with strict **Test Code Separation**:

```
                  ┌──────────────────────────────┐
                  │ 1. Visual Regression Tests   │ (Python + Vello Render)
                  ├──────────────────────────────┤
                  │ 2. Crate Integration Tests   │ (crates/*/tests/*.rs)
                  ├──────────────────────────────┤
                  │ 3. Workspace Unit Tests      │ (#[cfg(test)] in src/)
                  └──────────────────────────────┘
```

### 📁 Test Code Separation Guidelines
1. **Integration & Large Test Suites (`crates/*/tests/`)**:
   - Multi-file tests, scenario tests, schema expansions, and end-to-end integration tests MUST be located in the crate's root `tests/` directory (e.g., `crates/fepdf-model/tests/parser_tests.rs`).
   - Do NOT place standalone test files inside `src/` (e.g., `src/schema_tests.rs` is forbidden).
2. **Inline Unit Tests (`src/`)**:
   - Small, private helper unit tests may reside alongside production code inside `#[cfg(test)] mod tests { ... }` blocks at the bottom of `src/` files.
   - Test utilities or mock helpers must NOT pollute production module exports.

---

## 🧪 2. Workspace Unit & Integration Tests

All crates in the workspace MUST maintain high test coverage for core data structures, object sublimation, and font handling.

### Run All Unit & Integration Tests
```bash
cargo test --workspace
```

### Key Subsystem Test Suites
- **`fepdf-model`**:
  - `tests/parser_tests.rs`: Lexer primitives, tokenizing, and object parser.
  - `tests/security_tests.rs`: R4 AES-128 and R5 AES-256 security handlers.
  - `tests/object_tests.rs`: Object reference resolution, name deduplication, dictionary traversal.
  - `tests/schema_tests.rs`: Font & ExtGState ISO schema expansion.
  - `tests/mapping_tests.rs`: Unicode/CID mapping & encoding reconciliation.
- **`fepdf-render`**:
  - `tests/path_tests.rs`: Bezier curves, path bounds, transformation matrices.
  - `tests/text_tests.rs`: Text positioning and text matrix initialization.
- **`fepdf-sdk`**:
  - `src/tests.rs`: Color space conversions, object stream packing, R5 key derivation, document upgrade & retagging.
- **`fepdf-mcp`**:
  - `tests/mcp_server_tests.rs`: Model Context Protocol server error display and schema validation.

### The malformed corpus

Six files, each damaging one part of ISO 32000-2 clause 7.5, are the reader's
acceptance test; `docs/adr/0003` and `ROADMAP.md` both quote results measured against
them. They are generated rather than committed, from `samples/sample.pdf`:

```bash
python3 scripts/test/make_malformed.py
cargo run -q -p fepdf-model --example read_probe -- target/malformed/*.pdf
```

Reading recovers 111 objects from five of them — the count of the undamaged file — and
77 from the truncated one, being all that survives. The truncated file has no
`/Type /Catalog` at all, so it reads but cannot be *opened* as a document; that is the
expected result, not a gap.

---

## 🖼️ 3. Visual Regression Testing

GPU compute rendering fidelity via **Vello** is verified against canonical baseline reference images.

### Run Visual Regression Tests
```bash
python3 scripts/visual_regression.py
```

### Update Baseline Reference Images
```bash
python3 scripts/visual_regression.py --update
```

---

## 🦀 4. Minimum Supported Rust Version (MSRV) Verification

The project requires **Rust 1.94+ (Edition 2024)**. MSRV compatibility is verified via:

```bash
cargo check --workspace
```

---

## 🛠️ 5. Pre-Merge Quality Checklist

Before submitting a Pull Request or completing a task:

- [ ] `./scripts/audit/verify_compliance.sh` completes with `=== AUDIT PASSED ===`.
- [ ] `cargo test --workspace` passes with 0 failures (220 tests across 31 suites, 2026-08-16).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean. `--all-targets` matters: without it tests, examples and benches are never linted.
- [ ] `cargo fmt --all --check` reports no diff. Enforced as Rule 19 by the audit, so `make audit` covers it.
- [ ] All integration tests are placed in `crates/*/tests/` following the Test Separation Policy. Binary crates (`fepdf`) are the exception: an integration test cannot reach into a binary, so their unit tests live in inline `#[cfg(test)] mod tests` blocks, which the rule permits.
- [ ] `cargo deny check licenses` returns `licenses ok`.
- [ ] `.git/hooks/pre-commit` passes secret scanning without leaks.
- [ ] `./scripts/test/cli_smoke.sh` — **a debug build**. Every other check in this list
      runs `--release`, and clap's duplicate-argument check is a `debug_assert`: a
      collision between two argument definitions once panicked seven subcommands while
      the whole verification suite stayed green.
- [ ] `./scripts/test/crosscheck_roundtrip.sh` — text preserved through a save, compared
      against a second implementation. Internal measurement cannot find a page the
      engine never built; this is what caught [ADR-0006] and [ADR-0010].
- [ ] `./scripts/dev/status.sh` — the figures the documents quote, re-derived. A number
      that has gone stale shows up as a disagreement rather than reading as current.

[ADR-0006]: docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md
[ADR-0010]: docs/adr/0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md
