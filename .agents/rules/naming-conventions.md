# Naming Convention Protocol (RFC 0430)

This document defines the official naming standards for fepdf, adhering to [Rust RFC 0430](https://github.com/rust-lang/rfcs/blob/master/text/0430-finalizing-naming-conventions.md).

> [!IMPORTANT]
> **Priority**: When the PDF specification (ISO 32000) conflicts with Rust conventions, **Rust Naming Context shall prevail** for internal implementation to ensure language idiomatics.

---

## 1. Casing Strategy
- **Types & Traits**: `UpperCamelCase`
- **Functions & Variables**: `snake_case`
- **Enum Variants**: `UpperCamelCase`
- **Constants**: `SCREAMING_SNAKE_CASE`

## 2. Ownership-Aware API
- **Conversions**:
    - `as_foo()`: Immutable reference return.
    - `to_foo()`: New object creation (expensive).
    - `into_foo()`: Value consumption (transfer of ownership).
- **Getter Policy**: Avoid the `get_` prefix for simple field access. Use the raw field name or a descriptive noun.

## 3. Handle Stability Protocol
To distinguish between stable document-level references and volatile arena indices, the following terminology is mandatory:

| Term | Type Alias | Stability | Context |
| :--- | :--- | :--- | :--- |
| **ObjHandle** | `Handle<Object>` | **Stable** | Indirect object ID. Surpasses refinery passes. |
| **DictHandle** | `Handle<BTreeMap>` | **Volatile** | Internal dictionary index. Subject to change. |
| **ArrayHandle**| `Handle<Vec>` | **Volatile** | Internal array index. Subject to change. |
| **NameHandle** | `Handle<PdfName>` | **Stable** | Deduplicated Atom handle. |

- **Storage Rule**: Persistent structures (Page, Catalog) MUST ONLY store `ObjHandle`.
- **Transience Rule**: `DictHandle` and `ArrayHandle` are reserved for stack-based execution (Interpreter) or immediate resolution.

## 4. PDF Domain Integration
- **Terminology**: Retain specification terms (e.g., MediaBox) but adapt to Rust casing (`media_box`).
- **Acronyms**: Treat as normal words (`PdfError` instead of `PDFError`).

## 5. Pipeline Stage Naming
- **Standard**: A stage of the load pipeline is named for what it does, not for its
  position. The stages are described in `ARCHITECTURE.md` §5.4; the functions are
  `reader::load_document`, `decrypt::unlock`, `Ingestor::perform_active_refinement` and
  `metadata::settle`.
- This section previously mandated `perform_pass_N_<action>` and listed a **Pass 1
  (Structural Ingestion)** that [ADR-0003] removed. No method in the workspace has ever
  matched the pattern, so it was a rule nothing followed describing a stage that did not
  exist. Numbering a name fixes it to a position in a sequence that changes; `Pass 0`
  survives in prose because the standard's clause order gives it a meaning.

[ADR-0003]: ../../docs/adr/0003-lopdf-was-not-providing-robustness.md

## 6. Error Enumeration
- **Standard**: `PdfError` variants must follow a "Result-of-Action" pattern.
    - `Parse(...)`: Lexical failure.
    - `Ingestion(...)`: Semantic mapping failure.
    - `ClauseViolation(...)`: ISO 32000-2 non-compliance.
