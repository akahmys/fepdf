# 💻 Ferruginous Coding Standards & Hardening Protocol

This document defines the coding conventions, safety standards (**RR-15 Protocol**), and architectural patterns required across all crates in the Ferruginous workspace.

---

## 🛡️ 1. The RR-15 Hardening Rules

Derived from aerospace safety principles, the **RR-15 (Reliable Rust-15)** rules guarantee determinism, memory safety, and absolute runtime reliability.

### Rule Summary Matrix

| Rule | Area | Requirement | Enforcement |
| :--- | :--- | :--- | :--- |
| **Rule 1** | Function Length | Max 50 lines for standard functions.<br>Max 200 lines for `// RR-15 Limit: GUI`.<br>Max 500 lines for `// RR-15 Limit: Dispatcher`. | `./scripts/audit/verify_compliance.sh` |
| **Rule 2** | Panic Prevention | `unwrap()` and `expect()` are forbidden in production code. Use `?` or `unwrap_or()`. | Automated grep check |
| **Rule 3** | Unsafe Ban | `unsafe` blocks are forbidden (`workspace.lints.rust.unsafe_code = "forbid"`). | Rustc lint |
| **Rule 4** | Control Flow | Avoid deep nesting (`if let` / `match`). Prefer early return with `?`. | Code review / Clippy |
| **Rule 5** | Match Exhaustiveness | Wildcard patterns (`=> _`) in `match` are forbidden. | Automated grep check |
| **Rule 6** | Stack Safety | Unbounded recursion is forbidden. Use heap-based loops with `Vec`. | Code review |
| **Rule 7** | Global State | `static mut` and global mutable state are forbidden. | Automated grep check |
| **Rule 8** | Invalid State | Use type-safe `enum` states instead of boolean flags or nested `Option`s. | Architecture review |
| **Rule 10** | Determinism | `HashMap` and `HashSet` are forbidden in core pipelines. Use `BTreeMap`, `BTreeSet`, or `PdfArena`. | Automated grep check |
| **Rule 11** | Error Transparency | Return typed `thiserror` enums. String-based errors (`Result<T, String>`) are forbidden in core APIs. | Automated grep check |
| **Rule 13** | Error Swallowing | `filter_map(Result::ok)` and silent error swallowing are forbidden. | Automated grep check |
| **Rule 14** | Test Code Separation | Standalone/Integration tests MUST be placed in `crates/*/tests/`. Do NOT pollute `src/` with dedicated test files. | Directory structure check |
| **Rule 15** | Clone Optimization | Avoid excessive `.clone()`. Use `Arc` or handle references where appropriate. | Code review / Density warning |
| **Rule 17** | Type Explicitly | Explicitly specify floating-point types (`1.0_f32`, `2.5_f32`) to prevent Edition 2024 inference fallbacks. | Clippy / Compiler |

---

## 🏛️ 2. ISO 32000-2 PDF 2.0 Engine Architecture

### Normalization-at-Load (The Sublimation Pipeline)
All physical bytes MUST pass through 3 normalization stages before application processing:

1. **Pass 0 (Physical Normalization)**: Recursive stack-based decryption and XRef table repair. Strips residual `/Encrypt` dictionaries for Acrobat compatibility.
2. **Pass 1 (Arena Ingestion)**: Expands object streams and stores objects in `PdfArena`. Generates stable handles (`Handle<Object>`).
3. **Pass 2 (Semantic Sublimation)**: Re-encodes Unicode strings (eliminating legacy mojibake), restores path integrity (preserves EndPath `n`), and normalizes color states.

### Safety Invariant: `PdfArena`
- Use `Handle<Object>` (ID + Generation) instead of direct pointers or raw indices.
- Traverse object trees deterministically.

---

## 🎨 3. GPU Rendering & GUI Conventions

- **Compute Rasterization**: Render PDF page streams using **Vello** compute shaders (`ferruginous-render`).
- **UI Architecture**: Desktop GUI uses **egui** + **wgpu**.
- **CAD Precision**: Path snapping and measurement tools must preserve sub-pixel double-precision (`f64`) coordinates before canvas rasterization.
- **Localization**: User interface strings must support `egui` CJK font loading and English/Japanese localization strings.
