# 🏛️ `fepdf` Architecture & System Design

The authoritative architectural blueprint for **fepdf**: crate topology, layering
rules, the Sublimation Pipeline, and memory invariants.

> **Status.** This describes the *target* topology. Several crates below do not exist
> yet — their code lives in `fepdf-core` or `fepdf-sdk` today. Every entry in
> [§3](#-3-crate-responsibilities) carries a status marker, and [§6](#-6-migration)
> records the order of work. Nothing here is aspirational hand-waving: the shape is
> derived from measured coupling in the current tree, recorded in
> [§4](#-4-why-this-shape).

---

## 📐 1. Design Principles

Three rules decide where code goes. They are what keeps the topology from eroding;
the layer diagram is a consequence of them, not the other way round.

### Rule A — Storage abstractions stop at the facade

`PdfArena` and `Handle<T>` are how the object graph is *stored*. They are not part of
the user's vocabulary. They may appear anywhere below `fepdf`, and **never above it**.

A frontend that traverses arenas has taken on domain logic it cannot test and the
engine cannot protect. When that happens the defect surfaces as "the UI is wrong"
long after the real cause.

### Rule B — A crate that defines a contract does not depend on its implementations

Traits and their data types live with the code that *calls* them, not with any one
implementor. `Backend` belongs beside the interpreter that drives it; the GPU
rasteriser is one implementation among several.

Violating this drags an implementation's dependency tree into every consumer of the
contract — the mechanism by which a JSON-over-stdio server ends up linking a GPU
stack.

### Rule C — Read and write live together

PDF work is *read → amend → write*. Parsing and serialisation are two halves of one
round trip and belong at the same level in the same crate. Splitting them across
layers produces an engine that can read but not write, and forces callers to reach
across the seam.

---

## 🗺️ 2. Target Topology

```
┌─ Frontends ─────────────────────────────────────────────────────────┐
│   fepdf-cli      fepdf-gui       fepdf-mcp       fepdf-wasm         │
└─────────────────────────┬───────────────────────────────────────────┘
                          │   ◄── Rule A boundary: no Arena / Handle above here
┌─────────────────────────▼───────────────────────────────────────────┐
│  fepdf            Public facade: Document, Page, SaveOptions        │
└─────────────────────────┬───────────────────────────────────────────┘
          ┌───────────────┼───────────────────┐
          ▼               ▼                   ▼
   ┌────────────┐  ┌──────────────┐   ┌────────────────┐
   │ fepdf-doc  │  │ fepdf-content│   │  fepdf-render  │
   │ operations │  │ interpreter  │   │  Vello / wgpu  │
   │ conformance│  │ + Backend    │◄──┤  implements    │
   │ remediation│  │   contract   │   │  Backend       │
   └─────┬──────┘  └──────┬───────┘   └────────────────┘
         │                │                  ▲
         │                │      Rule B: the arrow points this way
         ▼                ▼
   ┌──────────────────────────────┐
   │ fepdf-resource               │  PDF dictionaries → usable resources
   │ font dicts · colour · images │
   └───────┬──────────────┬───────┘
           ▼              ▼
   ┌───────────────┐  ┌──────────────────┐
   │ fepdf-model   │  │ fepdf-font       │
   │ Arena/Object  │  │ CFF · TrueType   │
   │ read ⇄ write  │  │ CMap · AGL       │
   │ normalisation │  │ (knows no PDF)   │
   └───────┬───────┘  └──────────────────┘
           ▼
   ┌───────────────┐
   │ fepdf-syntax  │  lexer · parser · filters · crypto
   └───────────────┘
```

Dependencies flow strictly downward. `fepdf-render` is the one arrow that points *up*
into `fepdf-content`, because it implements a contract defined there — that is Rule B
working as intended, not a cycle.

---

## 🧩 3. Crate Responsibilities

Status: **✅** exists as-is · **🔄** code exists, lives elsewhere today · **🆕** new.

| Crate | Status | ~Lines | Responsibility |
| :--- | :---: | ---: | :--- |
| **`fepdf-syntax`** | 🔄 in core | 1,200 | Bytes ⇄ raw objects. Lexing, parsing, stream filters, encryption/decryption. Knows nothing of documents. |
| **`fepdf-font`** | 🔄 in core | 3,500 | Font *programs*: CFF, TrueType, CMap, Adobe Glyph List, subsetting, reconstruction. **Contains no PDF concepts** — verified, see §4. |
| **`fepdf-model`** | 🔄 in core+sdk | 8,600 | The document graph: `PdfArena`, `Handle<T>`, `Object`, page tree, metadata. Owns **both** ingestion and serialisation (Rule C), plus the normalisation passes. |
| **`fepdf-resource`** | 🔄 in core | 3,600 | Turns PDF resource dictionaries into usable resources: font dict → `FontResource`, colour spaces, images. The bridge between `fepdf-model` and `fepdf-font`. |
| **`fepdf-content`** | 🔄 in sdk+render | 2,300 | Content-stream interpreter, and the **`Backend` contract** it drives (`TextGlyph`, `TextState`, `SMaskData`, path geometry). No GPU dependency. |
| **`fepdf-doc`** | 🔄 in sdk | 2,200 | Document-level operations (merge, split, rotate, tag, redact, upgrade), structure-tree handling, conformance auditing, remediation. |
| **`fepdf-render`** | ✅ | 1,100 | A `Backend` implementation on **Vello** + **wgpu**. Nothing else depends on it. |
| **`fepdf`** | 🆕 | — | The public facade. `Document`, `Page`, `SaveOptions`. The Rule A boundary. |
| **`fepdf-cli`** | ✅ | 1,400 | Command-line binary (`fepdf`). |
| **`fepdf-gui`** | ✅ | 8,000 | Desktop application on **egui** + **eframe** + **wgpu**. |
| **`fepdf-mcp`** | ✅ | 340 | Model Context Protocol server for AI assistants. |
| **`fepdf-wasm`** | ✅ | 40 | WebAssembly bindings. Currently a stub — `render_page` is unimplemented. |
| **`fepdf-macros`** | ✅ | 160 | Compile-time procedural macros. |

Two `Backend` implementations besides the GPU one — text extraction and geometry
collection — live in `fepdf-doc`, which is exactly what Rule B makes possible.

---

## 🔬 4. Why This Shape

The layering is not a taxonomy exercise. Each boundary was placed where the current
tree already shows a seam or a defect.

**The font split is measured, not assumed.** Of 6,590 lines under `font/`, **3,547
reference no PDF type at all** — `agl`, `cff_standard`, `cmap`, `reconstruction`,
`rescue`, `subset` are pure font-format work. The remaining 3,043 exist solely to
read font dictionaries, which is why they become `fepdf-resource` rather than moving
with the rest.

**The contract/implementation inversion has a concrete cost.** `Backend` is defined
in `fepdf-render`, yet two of its three implementations live in `fepdf-sdk`. The SDK
therefore depends on the GPU crate to obtain a trait definition, and every SDK
consumer inherits `vello` + `wgpu` transitively — including the MCP server, which
speaks JSON over stdio, and the WASM build. Rule B removes that edge.

**Rule A exists because it was already broken.** `PdfArena` currently reaches
`fepdf-gui` (9 references) and `fepdf-cli` (2). The GUI worker holds struct-tree
traversal, `/BBox` interpretation and `/Pg` inheritance — PDF semantics living in the
presentation layer, outside the reach of engine tests. A page-mapping defect survived
there precisely because of that.

**Rule C exists because the round trip is currently split.** Ingestion sits in
`fepdf-core`; `writer.rs` — the single largest file in the workspace at 2,536 lines —
sits in `fepdf-sdk`. The engine can read but not write.

---

## 🛡️ 5. Cross-Cutting Concerns

### 5.1 The Sublimation Pipeline: normalisation-at-load

Every byte passes three normalisation stages before application code sees it. The
pipeline spans `fepdf-syntax` → `fepdf-model`, which is why normalisation is a
concern of the model rather than a crate of its own.

```
Raw bytes ─► Pass 0: Physical ─► Pass 1: Arena ─► Pass 2: Semantic ─► Document
```

- **Pass 0 — Physical normalisation.** Recursive stack-based decryption and
  cross-reference repair. Strips residual `/Encrypt` dictionaries for deterministic
  reader compatibility.
- **Pass 1 — Arena ingestion.** Expands object streams (`/ObjStm`), stores objects in
  `PdfArena` under deterministic `Handle<Object>` (id + generation), and indexes
  resource dictionaries.
- **Pass 2 — Semantic sublimation.** Re-encodes character mappings to eliminate legacy
  CJK mojibake, preserves exact path endpoints (`EndPath n`), harmonises graphics
  state, and normalises colour.

### 5.2 Safety invariants

- **Handles, not pointers.** Objects are reached only through `Handle<Object>`,
  eliminating use-after-free and dangling references by construction.
- **Deterministic traversal.** `PdfArena` uses `BTreeMap` and indexed handle arrays
  throughout, so iteration order — and therefore produced bytes — is reproducible.
  RR-15 Rule 10 forbids `HashMap`/`HashSet` in the crates that decide output.
- **Zero unsafe.** `unsafe_code = "forbid"` across the workspace.

### 5.3 Rendering

`fepdf-content` walks the content stream and issues calls against `Backend`.
`fepdf-render` answers them with **Vello** compute shaders on **wgpu**. Path snapping
keeps double-precision `kurbo` geometry until rasterisation; `skrifa` and `read-fonts`
handle glyph mapping, Japanese fallback fonts, and Type 3 precipitation.

Because the contract is separate, the same interpreter drives text extraction and
geometry collection without a GPU present.

---

## 🚧 6. Migration

Ordered by value against risk. Steps 1–2 relocate code without changing logic, so a
green test run is sufficient evidence of correctness.

| # | Step | Effect | Risk |
| :-: | :--- | :--- | :---: |
| 1 | Move the `Backend` contract and its types from `fepdf-render` into `fepdf-content` | Drops `vello`/`wgpu` from MCP and WASM | Low |
| 2 | Extract the PDF-free half of `font/` into `fepdf-font` | 3,500 lines become independently testable | Low |
| 3 | Move struct-tree handling out of `fepdf-gui` into `fepdf-doc` | Domain logic returns to the engine; closes the Rule A leak | Medium |
| 4 | Move `writer` into `fepdf-model` | Restores the read/write round trip | Medium |
| 5 | Introduce the `fepdf` facade | Rule A becomes enforceable; touches all four frontends | High |

Step 5 delivers most of the usability gain and should follow 1–4, not precede them.
The current API cannot hide its internals — reaching a catalogue requires
`doc.inner().catalog_handle()`, which is the symptom the facade removes.

**Deliberately not planned.** Splitting `fepdf-doc` into separate operation and
verification crates: auditing and remediation act on the same document surface, so
module boundaries suffice until that changes. Treating `fepdf-wasm` as a peer
frontend: at 40 lines with an unimplemented renderer, whether to build it is a product
decision, not an architectural one.

---

## 🔍 7. Enforcement

Architecture rules that are not checked become comments. These are:

- **Rules A–C**: crate dependency direction is enforced by Cargo itself once the split
  lands — a violation fails to compile.
- **RR-15 protocol**: [`CODING.md`](CODING.md), checked by
  [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh).
- **Lints**: `cargo clippy --workspace --all-targets -- -D warnings`. `--all-targets`
  is required — without it tests, examples and benches go unlinted.
- **Licences**: `cargo deny check licenses` ([`deny.toml`](deny.toml)).
- **Secrets and PII**: `betterleaks` pre-commit hook ([`.betterleaks.toml`](.betterleaks.toml)).

Governance sits in [`AGENTS.md`](AGENTS.md), [`CODING.md`](CODING.md),
[`AUDITING.md`](AUDITING.md), and [`TESTING.md`](TESTING.md).
