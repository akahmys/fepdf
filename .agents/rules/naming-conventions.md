# Naming Convention Protocol (RFC 0430)

This document defines the official naming standards for Ferruginous, adhering to [Rust RFC 0430](https://github.com/rust-lang/rfcs/blob/master/text/0430-finalizing-naming-conventions.md).

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

## 5. Multi-Pass Refinement Naming
- **Standard**: Methods performing transformation phases MUST follow the `perform_pass_N_<action>` pattern.
    - **Pass 0**: Physical Normalization.
    - **Pass 1**: Structural Ingestion.
    - **Pass 2**: Content Refinement.

## 6. Error Enumeration
- **Standard**: `PdfError` variants must follow a "Result-of-Action" pattern.
    - `Parse(...)`: Lexical failure.
    - `Ingestion(...)`: Semantic mapping failure.
    - `ClauseViolation(...)`: ISO 32000-2 non-compliance.
