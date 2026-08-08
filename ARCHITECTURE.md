# 🏛️ Ferruginous Architecture & System Design

This document serves as the authoritative architectural blueprint for **Ferruginous**, detailing crate topology, data flow, the Sublimation Pipeline, memory invariants, and GPU rendering pipelines.

---

## 🗺️ 1. Workspace Topology & Crate Hierarchy

Ferruginous is architected as a modular Rust Cargo Workspace. The component dependencies flow strictly from high-level interface applications down to core physical engines.

```
                   ┌─────────────────────────────────────────┐
                   │  Desktop GUI (crates/ferruginous)      │
                   │  Universal CLI (crates/fepdf)            │
                   │  AI MCP Server (crates/ferruginous-mcp) │
                   │  WebAssembly   (crates/ferruginous-wasm)│
                   └────────────────────┬────────────────────┘
                                        │
                                        ▼
                   ┌─────────────────────────────────────────┐
                   │  Public SDK (crates/ferruginous-sdk)    │
                   └───────────┬─────────────────┬───────────┘
                               │                 │
                               ▼                 ▼
     ┌───────────────────────────────┐ ┌───────────────────────────────┐
     │ GPU Compute Renderer          │ │ PDF Core & Ingestion Engine   │
     │ (crates/ferruginous-render)   │ │ (crates/ferruginous-core)     │
     └───────────────┬───────────────┘ └───────────────┬───────────────┘
                     │                                 │
                     └────────────────┬────────────────┘
                                      │
                                      ▼
                     ┌─────────────────────────────────┐
                     │ Internal Procedural Macros      │
                     │ (crates/ferruginous-macros)     │
                     └─────────────────────────────────┘
```

### Module Responsibilities

| Crate | Responsibilities |
| :--- | :--- |
| **`ferruginous`** | Flagship desktop GUI application built on **egui**, **eframe**, and **wgpu**. Provides CAD measurement tools, Japanese/CJK text selection, atomic redaction, and UI localization. |
| **`fepdf`** | Command-line interface for PDF auditing, repair, structural inspection, and PDF 2.0 re-production. |
| **`ferruginous-mcp`** | **Model Context Protocol (MCP)** server bridge enabling AI assistants to run direct structural diagnostics and inspection tools on PDF documents. |
| **`ferruginous-wasm`** | WebAssembly bindings for running the Ferruginous engine inside modern browser runtimes. |
| **`ferruginous-sdk`** | High-level, handle-based public API for document manipulation, object stream packing, and PDF 2.0 conversion. |
| **`ferruginous-render`** | GPU-accelerated rendering engine using **Vello** (WGPU compute shaders) for CAD-grade vector path rasterization and CJK typography. |
| **`ferruginous-core`** | PDF physical engine, `PdfArena` handle storage, Pass 0 physical normalization, XRef repair, and cryptography. |
| **`ferruginous-macros`** | Compile-time procedural macros enforcing RR-15 compile-time checks. |

---

## 🛡️ 2. The Sublimation Pipeline: Normalization-at-Load

To guarantee absolute **ISO 32000-2:2020** compliance and eliminate malformed PDF vulnerabilities, Ferruginous employs a 3-stage normalization pipeline during ingestion:

```
Raw Bytes ──► [ Pass 0: Physical Normalization ] ──► [ Pass 1: Arena Ingestion ] ──► [ Pass 2: Semantic Sublimation ] ──► PdfDocument
```

### Pass 0: Physical Normalization
- **Decryption & XRef Repair**: Recursive, stack-based decryption and cross-reference stream repair.
- **Sanitization**: Mandatory removal of legacy `/Encrypt` dictionary residuals to ensure deterministic Acrobat reader compatibility.

### Pass 1: Arena Ingestion
- **Stream Expansion**: Decompression and expansion of object streams (`/ObjStm`).
- **Handle Allocation**: Storing all objects inside `PdfArena` and assigning deterministic `Handle<Object>` (ID + Generation).
- **Resource Indexing**: Structural deduplication and indexing of resource dictionaries.

### Pass 2: Semantic Sublimation
- **Unicode Re-encoding**: Re-encodes character mapping strings to eliminate legacy CJK mojibake.
- **Path Integrity Preservation**: Preserves exact path endpoints (`EndPath n`) and harmonizes graphics state.
- **PDF/UA-2 Tagging**: Remediation of accessibility tags for **ISO 14289-2** standards.

---

## 🔒 3. Safety Invariants: `PdfArena`

- **Handles over Raw Pointers**: Objects are accessed strictly via `Handle<Object>`, eliminating use-after-free and dangling pointer risks.
- **Deterministic Traversal**: Collection traversal inside `PdfArena` is 100% deterministic (using `BTreeMap` and indexed handle arrays), guaranteeing bit-perfect reproduciability.
- **Zero Unsafe**: Entire pipeline executes under `workspace.lints.rust.unsafe_code = "forbid"`.

---

## 🎨 4. GPU Compute Rendering Architecture

Rendering vector graphics and complex typography with high fidelity requires compute-shader rasterization:

- **Compute Shaders**: Powered by **Vello** and **wgpu**, offloading path stroke/fill calculations directly to the GPU.
- **CAD Precision**: Sub-pixel path snapping uses double-precision (`f64`) `kurbo` Bezier curve math prior to GPU rasterization.
- **Typography & CJK**: Integrates `skrifa` and `read-fonts` for precise glyph mapping, fallback font matching (Japanese system fonts), and Type 3 font precipitation.

---

## 🛡️ 5. Security, Governance & Verification

Architecture invariants are enforced by automated CI/CD and pre-commit checks:

- **Governance Constitution**: Enforced via [`AGENTS.md`](AGENTS.md), [`CODING.md`](CODING.md), [`AUDITING.md`](AUDITING.md).
- **License Integrity**: Audited with `cargo-deny` ([`deny.toml`](deny.toml)).
- **PII & Secret Protection**: Scanned with `betterleaks` pre-commit hooks ([`.betterleaks.toml`](.betterleaks.toml)).
