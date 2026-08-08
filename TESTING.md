# 🧪 Ferruginous Testing & Validation Strategy

This document details the testing methodology, test suites, visual regression framework, and quality assurance processes for Ferruginous.

---

## 🎯 1. Test Pyramid & Separation Policy

Ferruginous employs a 3-tier testing hierarchy with strict **Test Code Separation**:

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
   - Multi-file tests, scenario tests, schema expansions, and end-to-end integration tests MUST be located in the crate's root `tests/` directory (e.g., `crates/ferruginous-core/tests/parser_tests.rs`).
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
- **`ferruginous-core`**:
  - `tests/parser_tests.rs`: Lexer primitives, tokenizing, and object parser.
  - `tests/security_tests.rs`: R4 AES-128 and R5 AES-256 security handlers.
  - `tests/object_tests.rs`: Object reference resolution, name deduplication, dictionary traversal.
  - `tests/schema_tests.rs`: Font & ExtGState ISO schema expansion.
  - `tests/mapping_tests.rs`: Unicode/CID mapping & encoding reconciliation.
- **`ferruginous-render`**:
  - `tests/path_tests.rs`: Bezier curves, path bounds, transformation matrices.
  - `tests/text_tests.rs`: Text positioning and text matrix initialization.
- **`ferruginous-sdk`**:
  - `src/tests.rs`: Color space conversions, object stream packing, R5 key derivation, document upgrade & retagging.
- **`ferruginous-mcp`**:
  - `tests/mcp_server_tests.rs`: Model Context Protocol server error display and schema validation.

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
- [ ] `cargo test --workspace` passes with 0 failures across all 48+ test suites.
- [ ] All integration tests are placed in `crates/*/tests/` following the Test Separation Policy.
- [ ] `cargo deny check licenses` returns `licenses ok`.
- [ ] `.git/hooks/pre-commit` passes secret scanning without leaks.
