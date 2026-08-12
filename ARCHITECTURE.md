# 🏛️ `fepdf` Architecture & System Design

The authoritative architectural blueprint for **fepdf**: crate topology, layering
rules, the Sublimation Pipeline, and memory invariants.

> **Status.** This describes the *target* topology. Several crates below do not exist
> yet — their code lives in `fepdf-model` or `fepdf-sdk` today. Every entry in
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

### Rule D — Frontends translate; they never decide

Every mutation of a document is a value in **one vocabulary**, owned by `fepdf-doc`
(see [§5.1](#51-the-operation-vocabulary)). A frontend's job is to turn argv, a button
press, an MCP call or a JS call into that value and hand it over. It never implements
the operation itself.

Where two frontends each implement "the same" operation, the two implementations
drift, silently, because nothing compares them. That has already happened here — see
[§4](#-4-why-this-shape).

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
   ┌───────────────────────────────┐  ┌──────────────────┐
   │ fepdf-model                   │  │ fepdf-font       │
   │ Arena/Object · read ⇄ write   │─►│ CFF · TrueType   │
   │ normalisation                 │  │ CMap · AGL       │
   │ resource resolution (see §4)  │  │ (knows no PDF)   │
   └───────┬───────────────────────┘  └──────────────────┘
           ▼
   ┌───────────────┐
   │ fepdf-syntax  │  lexer · crypto (no model types)
   └───────────────┘
```

Dependencies flow strictly downward. `fepdf-render` is the one arrow that points *up*
into `fepdf-content`, because it implements a contract defined there — that is Rule B
working as intended, not a cycle.

---

## 🧩 3. Crate Responsibilities

Status: **✅** exists as-is · **⚠️** partially landed · **🔄** code exists, lives elsewhere today · **🆕** new.

| Crate | Status | ~Lines | Responsibility |
| :--- | :---: | ---: | :--- |
| **`fepdf-syntax`** | ✅ | 850 | The byte layer: lexing and encryption/decryption. Depends on no model type, which is what lets the cryptography be reviewed on its own. Parsing and stream filters are *not* here — see §4. |
| **`fepdf-font`** | 🔄 in core (Audited ✅) | 3,500 | Font *programs*: CFF, TrueType, CMap, Adobe Glyph List, subsetting, reconstruction. Hardened against W/W2 out-of-bounds, CMap underflows (`e_val >= s_val`), and CID byte truncations. |
| **`fepdf-model`** | ✅ | 8,600 | The document graph: `PdfArena`, `Handle<T>`, `Object`, page tree, metadata. Hardened with pool overflow guards, cyclic `resolve` limits (`64`), and safe `Null` reference fallbacks. |
| **`fepdf-content`** | ⚠️ partial | 2,300 | Content-stream interpreter, and the **`RenderBackend` contract** it drives (`TextGlyph`, `TextState`, `SMaskData`, path geometry). No GPU dependency. *The contract has landed; the interpreter still lives in `fepdf-sdk`.* |
| **`fepdf-doc`** | 🔄 in sdk | 2,200 | Owns the **`Operation` vocabulary** (§5.1) and is its only interpreter: merge, split, rotate, tag, redact, upgrade. Also structure-tree handling, conformance auditing, remediation. |
| **`fepdf-render`** | ✅ | 1,100 | A `RenderBackend` implementation on **Vello** + **wgpu**. Reached only through the SDK's optional `render` feature. |
| **`fepdf`** | ⚠️ lives in `fepdf-sdk` | — | The public facade: `PdfDocument`, `SaveOptions`, `Operation`. It is the Rule A boundary in fact — frontends depend on it and on nothing below — but it is not yet a crate of its own. |
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
read font dictionaries, which is why they stay in `fepdf-model` rather than moving
with the rest.

**Resource resolution is part of the model, not a layer above it.** An earlier
revision of this document placed a `fepdf-resource` crate between the model and its
consumers, on the assumption that font dictionaries are resolved lazily when content
is interpreted. They are not: this engine resolves them eagerly during ingestion.
`Document` owns the font cache, `ingest::discover_fonts` populates it, and the
refinery normalises it, with `document.rs` alone referring to the font module in 29
places. A crate above the model therefore cannot own that work without inverting a
dependency ingestion genuinely needs.

The crate was created before this was noticed and never adopted; it sat unused for
long enough to accumulate a partial, divergent copy of `font/`, and has been removed.
What it was meant to separate — font *programs* from PDF *dictionaries* — is already
separated, as `fepdf-font` versus `fepdf-model`'s `font/`.

**The contract/implementation inversion had a concrete cost.** `RenderBackend` was
defined in `fepdf-render`, yet two of its three implementations lived in `fepdf-sdk`.
The SDK therefore depended on the GPU crate to obtain a trait definition, and every
SDK consumer inherited `vello` + `wgpu` transitively.

Rule B does not make that dependency *disappear* — it makes it **opt-in**. `fepdf-cli`
and `fepdf-mcp` both call `render_page_to_file` and genuinely rasterise, so they
enable the SDK's `render` feature and still link the GPU stack, correctly. What
changes is that the choice is now explicit: `fepdf-wasm`, which never rasterises,
went from three transitive GPU dependencies to none.

**Rule A exists because it was already broken.** `PdfArena` currently reaches
`fepdf-gui` (9 references) and `fepdf-cli` (2). The GUI worker holds struct-tree
traversal, `/BBox` interpretation and `/Pg` inheritance — PDF semantics living in the
presentation layer, outside the reach of engine tests. A page-mapping defect survived
there precisely because of that.

**Rule C exists because the round trip is currently split.** Ingestion sits in
`fepdf-model`; `writer.rs` — the single largest file in the workspace at 2,536 lines —
sits in `fepdf-sdk`. The engine can read but not write.

**Rule D exists because the vocabularies have already diverged.** "Rotate" is defined
twice — once as a clap subcommand, once as a `WorkerRequest` variant — and the two
disagree:

```rust
// fepdf-cli  handle_rotate()            — absolute assignment
doc.set_page_rotation(idx, angle)

// fepdf-gui  WorkerRequest::RotatePages — relative delta, normalised
doc.set_page_rotation(idx, (current + delta).rem_euclid(360))
```

On a page already at 90°, `fepdf edit rotate --angle 90` leaves it at 90° while the
GUI's 90° button takes it to 180°. The CLI's own progress message says "rotating **by**
N degrees", describing the behaviour it does not have. Neither path normalises, so
`--angle 45` reaches `/Rotate`, which ISO 32000-2 requires to be a multiple of 90.

Nothing detected this, because there is no place where the two definitions meet.

---

## 🛡️ 5. Cross-Cutting Concerns

### 5.1 The operation vocabulary

Every document mutation is a value of one type, defined in `fepdf-doc` and re-exported
through the facade. Frontends construct it; only `fepdf-doc` interprets it.

```
   fepdf-cli    argv          ─┐
   fepdf-gui    button press  ─┤
   fepdf-mcp    tool call     ─┼─►  Operation  ─►  fepdf-doc::apply
   fepdf-wasm   JS call       ─┘     (a value)      (the only implementation)
```

Ambiguity that used to live in prose becomes a type. The rotate divergence above is
not fixable by convention; it is fixable by making the choice unrepresentable:

```rust
pub enum Operation {
    // --- Core Page & Structure Operations ---
    Rotate { pages: PageSelection, mode: RotateMode },
    Reorder { from: usize, to: usize },
    RemovePages(PageSelection),
    InsertFrom { source: DocumentId, at: usize },
    Retag { .. },
    Redact { zones: Vec<RedactionZone> },
    Upgrade { standard: PdfStandard },

    // --- Metadata & Structure Domain ---
    CreatePortfolio { items: Vec<PortfolioInputItem>, cover_page: Option<CoverPageSpec> },
    UpdateOutlines { items: Vec<OutlineNodeSpec> },
    CreateLayer { name: String, visible_by_default: bool, printable: bool },
    SetLayerVisibility { layer_id: String, visible: bool },
    AttachAssociatedFile { target: TargetObjectRef, file_name: String, mime_type: String, bytes: Vec<u8> },
    SetOutputIntent { subtype: String, identifier: String, icc_profile_bytes: Option<Vec<u8>> },
    SetPronunciationLexicon { lexicon_xml_bytes: Vec<u8> },

    // --- Security & Provenance Domain ---
    VerifyDigitalSignature { field_name: String },

    // --- Decorations & Annotations Domain ---
    AddHyperlink { page: usize, rect: [f32; 4], destination: LinkDestination },
    AddPageDecorations { header: Option<String>, footer: Option<String>, watermark: Option<WatermarkSpec> },
    ApplyBatesNumbering { prefix: String, start_number: u64, digits: usize, position: PagePosition },
    AddAnnotation { page: usize, annotation: AnnotationSpec },
    AddStamp { page: usize, rect: [f32; 4], stamp_image_bytes: Vec<u8> },
    SetMeasurementScale { page: usize, scale_ratio: f32, unit_label: String },

    // --- Interactive Forms Domain ---
    SetFormFieldValue { field_name: String, value: FormValue },
}

pub enum RotateMode {
    /// Set `/Rotate` to this angle. Rejected unless a multiple of 90.
    Absolute(Quarter),
    /// Add to the current angle, normalised into [0, 360).
    Relative(Quarter),
}
```

A caller must now say which it means, and `Quarter` makes 45° unconstructible.

**Three consequences fall out rather than being designed in:**

- **Undo/redo.** Operations are values, so they can be recorded, inverted and
  replayed. The GUI gets history without a parallel mechanism.
- **MCP tool surface.** A tool becomes the serialised form of an `Operation`. New
  operations reach AI assistants without new bridging code.
- **Testability.** An operation sequence can be applied and asserted without starting
  a GUI or spawning a process.

**What this is not.** The GUI keeps its worker thread: `WorkerRequest` remains, but as
a thin envelope (`Execute(Operation)`, plus genuinely GUI-only messages such as
`RenderPage`). Off-thread execution is a GUI concern; the *meaning* of an operation is
not. Equally, this is not "the GUI drives the CLI as a subprocess" — the GUI is a
stateful editor holding an arena, retained scenes and per-page spans in memory, and
re-ingesting a 5,057-page document per interaction is not viable. Shared vocabulary,
not a shared process.

### 5.2 The Sublimation Pipeline: normalisation-at-load

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
  resource dictionaries, `/Collection` portfolios, `/OCProperties` layers, `/AF` associated files, and `/Outlines`.
- **Pass 2 — Semantic sublimation.** Re-encodes character mappings to eliminate legacy
  CJK mojibake, preserves exact path endpoints (`EndPath n`), harmonises graphics
  state, normalises colour, and validates PDF 2.0 structure integrity.

### 5.3 Unified Extension Architecture (Anti-Ad-Hoc Policy)

To prevent codebase drift, ad-hoc struct additions, or uncoordinated writer logic, all new backend capabilities MUST fit into one of four orthogonal domain namespaces owned by `fepdf-model` / `fepdf-doc`:

1. **Metadata & Structure**: Portfolio (`/Collection`), Outlines (`/Outlines`), Optional Content (`/OCProperties`), Associated Files (`/AF`), Output Intents (`/OutputIntents`), Pronunciation (`/PL`).
2. **Security & Provenance**: Crypt Revision 6 (AES-256-GCM), PKI PAdES Digital Signatures, Sublimation/Redaction.
3. **Decorations & Annotations**: Watermarks, Bates Numbering, Hyperlinks (`/Link`), Stamps, Measurements (`/Measure`).
4. **Interactive Forms**: AcroForms, FDF/XFDF static data models.

No feature is permitted to bypass the `Operation` vocabulary or inject un-audited dictionary mutations directly into frontends or serialisers.

#### 5.3.1 Multi-Format Provider Architecture
When introducing support for external document formats (e.g., Word `.docx`, Excel `.xlsx`, SVG, HTML), each format MUST follow Rule C by encapsulating its ingestion (reading) and emission (writing) within a dedicated format provider module/crate (e.g., `fepdf-import-docx`). Providers translate external formats into the `Operation` vocabulary or intermediate layout structures without exposing format-specific dependencies to `fepdf-model`.

### 5.4 Safety invariants

- **Handles, not pointers.** Objects are reached only through `Handle<Object>`,
  eliminating use-after-free and dangling references by construction.
- **Deterministic traversal.** `PdfArena` uses `BTreeMap` and indexed handle arrays
  throughout, so iteration order — and therefore produced bytes — is reproducible.
  RR-15 Rule 10 forbids `HashMap`/`HashSet` in the crates that decide output.
- **Zero unsafe.** `unsafe_code = "forbid"` across the workspace.

### 5.4 Rendering

`fepdf-content` walks the content stream and issues calls against `Backend`.
`fepdf-render` answers them with **Vello** compute shaders on **wgpu**. Path snapping
keeps double-precision `kurbo` geometry until rasterisation; `skrifa` and `read-fonts`
handle glyph mapping, Japanese fallback fonts, and Type 3 precipitation.

Because the contract is separate, the same interpreter drives text extraction and
geometry collection without a GPU present.

---

## 🚧 6. Migration

Ordered by value against risk. Steps 1–3 and 5 relocate code without changing logic,
so a green test run is sufficient evidence of correctness. Steps 0, 4 and 6 change
behaviour or API and need their own tests.

| # | Step | Effect | Risk |
| :-: | :--- | :--- | :---: |
| 0 | Reconcile the two `rotate` implementations | Fixes a live behavioural divergence; independent of everything below | Low |
| 1 | Move the `RenderBackend` contract and its types from `fepdf-render` into `fepdf-content` | ✅ **Done.** GPU became opt-in; WASM dropped to zero GPU dependencies | Low |
| 2 | Extract the PDF-free half of `font/` into `fepdf-font` | 3,500 lines become independently testable | Low |
| 3 | Move struct-tree handling out of `fepdf-gui` into `fepdf-doc` | Domain logic returns to the engine; closes the Rule A leak | Medium |
| 4 | Introduce `Operation` in `fepdf-doc`; reduce the CLI subcommands and `WorkerRequest` to adapters over it | Rule D becomes structural; divergence stops being possible | Medium |
| 5 | Move `writer` into `fepdf-model` (core) | ✅ **Done.** Restores the read/write round trip in `fepdf-model` (Rule C); `fepdf-sdk` re-exports for compatibility | Low |
| 6 | Introduce the `fepdf` facade | Rule A becomes enforceable; touches all four frontends | High |

Step 0 is a bug fix and needs no restructuring — but do it as part of step 4, not
before it, or the same divergence simply recurs the next time an operation is added.

Steps 3 and 4 belong together: both pull domain decisions out of the presentation
layer, and step 4 is what stops them leaking back.

Step 6 delivers most of the usability gain and should follow 1–5, not precede them.
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

- **Rules A–C**: enforced by Cargo. No frontend declares `fepdf-model`, so a model type
  cannot be named from `fepdf-cli`, `fepdf-gui`, `fepdf-mcp` or `fepdf-wasm` at all —
  reaching for one is a compile error, not a review finding. The facade re-exports
  what frontends legitimately need.
- **Rule D**: enforced by construction, not by review. Once mutations exist only as
  `Operation` values and `fepdf-doc` holds the only `apply`, a frontend has nothing to
  re-implement. The rule is worth stating because that property is easy to give away:
  the moment a frontend calls a mutating method directly instead of building an
  `Operation`, drift becomes possible again.
- **RR-15 protocol**: [`CODING.md`](CODING.md), checked by
  [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh).
- **Lints**: `cargo clippy --workspace --all-targets -- -D warnings`. `--all-targets`
  is required — without it tests, examples and benches go unlinted.
- **Licences**: `cargo deny check licenses` ([`deny.toml`](deny.toml)).
- **Secrets and PII**: `betterleaks` pre-commit hook ([`.betterleaks.toml`](.betterleaks.toml)).

Governance sits in [`AGENTS.md`](AGENTS.md), [`CODING.md`](CODING.md),
[`AUDITING.md`](AUDITING.md), and [`TESTING.md`](TESTING.md).
