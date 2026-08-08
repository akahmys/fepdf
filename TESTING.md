# 🧪 Ferruginous Testing & Validation Strategy

This document details the testing methodology, test suites, visual regression framework, and quality assurance processes for Ferruginous.

---

## 🎯 1. Test Pyramid

Ferruginous employs a 3-tier testing hierarchy:

```
                  ┌──────────────────────────────┐
                  │ 1. Visual Regression Tests   │ (Python + Vello Render)
                  ├──────────────────────────────┤
                  │ 2. Crate Integration Tests   │ (fepdf CLI, SDK, MCP)
                  ├──────────────────────────────┤
                  │ 3. Workspace Unit Tests      │ (ferruginous-core, etc.)
                  └──────────────────────────────┘
```

---

## 🧪 2. Workspace Unit Tests

All crates in the workspace MUST maintain high test coverage for core data structures, object sublimation, and font handling.

### Run All Unit Tests
```bash
cargo test --workspace
```

### Key Subsystem Tests
- **`ferruginous-core`**:
  - `font::cmap`: Hex parsing, AGL lookups, CMap ranges.
  - `font::reconstruction`: CFF2 wrapping, TTCF disassembly, format detection.
  - `object`: Macro expansions and arena handle stability.
- **`ferruginous-render`**:
  - `path_tests`: Curves, path bounds, transformation matrix application.
  - `text_tests`: Text positioning, matrix initialization.
- **`ferruginous-sdk`**:
  - `cielab`: CIELAB to sRGB color space conversions.
  - `security`: R5 key derivation and multi-stage decryption.
  - `writer`: Document save, object stream packing, heuristic re-tagging.

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
- [ ] `cargo test --workspace` passes with 0 failures.
- [ ] `cargo deny check licenses` returns `licenses ok`.
- [ ] `.git/hooks/pre-commit` passes secret scanning without leaks.
