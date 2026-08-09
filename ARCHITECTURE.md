# 🏛️ `fepdf` Architecture & System Design

This document serves as the authoritative architectural blueprint for **fepdf**, detailing crate topology, data flow, the Sublimation Pipeline, memory invariants, and GPU rendering pipelines.

---

## 🗺️ 1. Workspace Topology & 4-Layer Crate Hierarchy

`fepdf` is architected as a modular Rust Cargo Workspace divided into 4 clear logical layers:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Layer 4: Frontends & Integrations                                           │
│ ┌─────────────────────────┬─────────────────────────┬─────────────────────┐ │
│ │ Desktop GUI             │ AI MCP Server           │ WebAssembly         │ │
│ │ (crates/fepdf-gui)      │ (crates/fepdf-mcp)      │ (crates/fepdf-wasm) │ │
│ └────────────┬────────────┴────────────┬────────────┴──────────┬──────────┘ │
└──────────────┼─────────────────────────┼───────────────────────┼────────────┘
               │                         │                       │
┌──────────────┴─────────────────────────┴───────────────────────┴────────────┐
│ Layer 3: CLI Application                                                    │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │ Universal CLI (crates/fepdf-cli -> binary: `fepdf`)                     │ │
│ └────────────────────────────────────┬────────────────────────────────────┘ │
└──────────────────────────────────────┼──────────────────────────────────────┘
                                       │
┌──────────────────────────────────────┴──────────────────────────────────────┐
│ Layer 2: PDF Operators & Transformation Engine                              │
│ ┌───────────────────────┬───────────────────────┬─────────────────────────┐ │
│ │ Ingestion & Sublimation│ Remediation & Edit    │ Writer & Serialization  │ │
│ │ (fepdf-core::ingest)  │ (fepdf-sdk::ops)      │ (fepdf-sdk::writer)     │ │
│ ├───────────────────────┼───────────────────────┼─────────────────────────┤ │
│ │ GPU Render Operator   │ Compliance Audit      │ Security Operator       │ │
│ │ (fepdf-render)        │ (MatterhornAuditor)   │ (Sign, Encrypt, DSS)    │ │
│ └───────────────────────┴───────────────────────┴─────────────────────────┘ │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
┌──────────────────────────────────────┴──────────────────────────────────────┐
│ Layer 1: PDF 2.0 Rust Data Types (AST & Arena)                              │
│ ┌───────────────────────┬───────────────────────┬─────────────────────────┐ │
│ │ Low-Level PDF Objects │ Arena & Handle Model  │ High-Level Document AST │ │
│ │ (Object, PdfName, etc)│ (PdfArena, Handle<T>) │ (Document, Page, etc)   │ │
│ └───────────────────────┴───────────────────────┴─────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Module Responsibilities

| Crate | Layer | Responsibilities |
| :--- | :--- | :--- |
| **`fepdf-gui`** | Layer 4 | Desktop GUI application built on **egui**, **eframe**, and **wgpu**. |
| **`fepdf-cli`** | Layer 3 | Universal command-line interface binary (`fepdf`) for PDF auditing, repair, inspection, and manipulation. |
| **`fepdf-mcp`** | Layer 4 | **Model Context Protocol (MCP)** server enabling AI assistants to execute PDF diagnostic tools natively. |
| **`fepdf-wasm`** | Layer 4 | WebAssembly bindings for running the fepdf engine inside web browsers. |
| **`fepdf-sdk`** | Layer 2 | High-level operations engine (PageOperator, RedactionOperator, Writer, Decoders/Encoders). |
| **`fepdf-render`** | Layer 2 | GPU-accelerated rendering engine using **Vello** (WGPU compute shaders). |
| **`fepdf-core`** | Layer 1 | PDF 2.0 data models (`PdfArena`, `Document`, `Object`, `Page`), Pass 0/1/2 physical normalization, and cryptography. |
| **`fepdf-macros`** | Layer 1/2 | Compile-time procedural macros enforcing compile-time invariants. |

---

## 🛡️ 2. The Sublimation Pipeline: Normalization-at-Load

To guarantee absolute **ISO 32000-2:2020** compliance and eliminate malformed PDF vulnerabilities, fepdf employs a 3-stage normalization pipeline during ingestion:

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
