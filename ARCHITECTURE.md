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

`~Lines` is the *target* crate's size: for a crate that exists, what it holds today;
for one that does not, the code that would move into it. Measured 2026-08-18 with
`find crates/<name>/src -name '*.rs' | xargs cat | wc -l`, so any figure here can be
checked in one command.

| Crate | Status | ~Lines | Responsibility |
| :--- | :---: | ---: | :--- |
| **`fepdf-syntax`** | ✅ | 3,380 | The byte layer: lexing and encryption/decryption. Depends on no model type, which is what lets the cryptography be reviewed on its own. Parsing and stream filters are *not* here — see §4. |
| **`fepdf-font`** | ✅ (Audited ✅) | 3,710 | Font *programs*: CFF, TrueType, CMap, Adobe Glyph List, subsetting, reconstruction. Hardened against W/W2 out-of-bounds, CMap underflows (`e_val >= s_val`), and CID byte truncations. |
| **`fepdf-model`** | ✅ | 20,700 | The document graph: `PdfArena`, `Handle<T>`, `Object`, page tree, metadata — and, since Phase A, the reader (7.5) and `writer.rs`. Hardened with pool overflow guards, cyclic `resolve` limits (`64`), and safe `Null` reference fallbacks. |
| **`fepdf-content`** | ✅ | 2,380 | Content-stream interpreter, and the **`RenderBackend` contract** it drives (`TextGlyph`, `TextState`, `SMaskData`, path geometry). No GPU dependency. |
| **`fepdf-doc`** | ✅ | 2,960 | Owns the **`Operation` vocabulary** (§5.1) and is its only interpreter: 24 canonical mutation operations across 5 domains. Also structure-tree handling, conformance auditing, remediation. |
| **`fepdf-render`** | ✅ | 1,240 | A `RenderBackend` implementation on **Vello** + **wgpu**. Reached only through the SDK's optional `render` feature. |
| **`fepdf`** | ✅ | 1,610 | The public facade: `PdfDocument`, `SaveOptions`, `Operation`. It is the Rule A boundary in fact — frontends depend on it and on nothing below. |
| **`fepdf-cli`** | ✅ | 2,510 | Command-line binary (`fepdf`). |
| **`fepdf-gui`** | ✅ | 8,020 | Desktop application on **egui** + **eframe** + **wgpu**. |
| **`fepdf-mcp`** | ✅ | 330 | Model Context Protocol server for AI assistants. |
| **`fepdf-wasm`** | ✅ | 40 | WebAssembly bindings. Currently a stub — `render_page` is unimplemented. |
| **`fepdf-macros`** | ✅ | 170 | Compile-time procedural macros. |

Two `RenderBackend` implementations besides the GPU one — `TextExtractionBackend` and
`CollectorBackend` — sit alongside the operations, in `fepdf-doc`. Neither pulls in a GPU,
which is exactly what Rule B makes possible.

---

## 🔬 4. Why This Shape

The layering is not a taxonomy exercise. Each boundary was placed where the current
tree already shows a seam or a defect.

**The font split was measured, not assumed.** Of the 6,590 lines then under `font/`,
**3,547 referenced no PDF type at all** — `agl`, `cff_standard`, `cmap`,
`reconstruction`, `rescue`, `subset` are pure font-format work. The remaining 3,043
existed solely to read font dictionaries, which is why they stayed in `fepdf-model`
rather than moving with the rest. The split has since been made: `fepdf-font` is 3,700
lines and `fepdf-model/src/font/` 3,100.

**Resource resolution is part of the model, not a layer above it.** This engine
resolves font dictionaries eagerly during ingestion rather than lazily at interpretation
time, so a crate above the model cannot own that work. An earlier revision placed one
there; see [ADR-0001](docs/adr/0001-resource-resolution-stays-in-the-model.md).

**The contract/implementation inversion had a concrete cost.** `RenderBackend` was
defined in `fepdf-render`, yet two of its three implementations lived in `fepdf-sdk`.
The SDK therefore depended on the GPU crate to obtain a trait definition, and every
SDK consumer inherited `vello` + `wgpu` transitively.

Rule B does not make that dependency *disappear* — it makes it **opt-in**
([ADR-0004](docs/adr/0004-rule-b-makes-the-gpu-dependency-optional.md)). `fepdf-cli`
and `fepdf-mcp` both call `render_page_to_file` and genuinely rasterise, so they
enable the SDK's `render` feature and still link the GPU stack, correctly. What
changes is that the choice is now explicit: `fepdf-wasm`, which never rasterises,
went from three transitive GPU dependencies to none.

**Rule A exists because it had already been broken once.** `PdfArena` reached
`fepdf-gui` (9 references) and `fepdf-cli` (2), and the GUI worker held struct-tree
traversal, `/BBox` interpretation and `/Pg` inheritance — PDF semantics living in the
presentation layer, outside the reach of engine tests. A page-mapping defect survived
there precisely because of that.

That leak is closed: no frontend declares `fepdf-model`, so the count is now zero on
both, and the GUI asks the engine (`doc.extract_struct_tree()`) instead of walking the
tree itself. The rule is stated because Cargo enforces it (§7) — not because it is
currently violated.

**Rule C exists because the round trip had been split.** Ingestion sat in
`fepdf-model` while `writer.rs` sat in `fepdf-sdk`, so the engine could read but not
write. `writer.rs` now lives in `fepdf-model` (2,548 lines) and `fepdf-sdk` keeps a
six-line re-export for compatibility (§6, step 5).

**Rule D exists because the vocabularies had already diverged.** "Rotate" was defined
twice — once as a clap subcommand, once as a `WorkerRequest` variant — and the two
disagreed:

```rust
// fepdf-cli  handle_rotate()            — absolute assignment
doc.set_page_rotation(idx, angle)

// fepdf-gui  WorkerRequest::RotatePages — relative delta, normalised
doc.set_page_rotation(idx, (current + delta).rem_euclid(360))
```

On a page already at 90°, `fepdf edit rotate --angle 90` left it at 90° while the
GUI's 90° button took it to 180°. Neither path normalised, so `--angle 45` reached
`/Rotate`, which ISO 32000-2 requires to be a multiple of 90.

Nothing detected this, because there was no place where the two definitions met. The
fix was to make the choice unrepresentable rather than to align the two call sites —
`RotateMode` and `Quarter` below (§5.1). Both frontends now construct an `Operation`:
`fepdf-cli` an `Absolute`, `fepdf-gui` a `Relative`.

**Where the reasoning lives.** This section says why the architecture has its present
shape. Decisions that were *reversed*, and the measurements that reversed them, are in
[`docs/adr/`](docs/adr/README.md) — including the scope of `fepdf-syntax`
([ADR-0002](docs/adr/0002-the-syntax-layer-is-lexer-and-crypto-only.md)), the reader's
independence from `lopdf`
([ADR-0003](docs/adr/0003-lopdf-was-not-providing-robustness.md)), how Rules A–C came
to be enforced by the build rather than by review
([ADR-0005](docs/adr/0005-layering-rules-are-enforced-by-cargo.md)), and why an object
stream may not overwrite a newer revision of what it carries
([ADR-0006](docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md)).

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

Ambiguity that used to live in prose becomes a type. The rotate divergence in §4 was
not fixable by convention; it was fixed by making the choice unrepresentable:

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

### 5.2 Document sources

`DocumentSource` is the boundary between "read some bytes" and "have a normalised
`Document`". `PdfSource` is the only implementation; the trait exists so that
file-format knowledge stays on one side of that line rather than inside `Document`.

Its options are an associated type, not a shared struct: `password` and
`force_fallback` mean something to PDF and nothing to a word-processor format.

**Deliberately minimal.** There is no registry, no dynamic dispatch and no
per-format feature flags. An interface designed against one implementation is
almost always wrong for the second, and this codebase keeps paying for building
a container before its contents existed — `fepdf-resource`, an `Operation`
vocabulary of which 19 of 24 are stubs, and ingestion options nothing reads
([ADR-0007](docs/adr/0007-an-option-that-is-not-read-is-hidden.md); one of the two
became live with [ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md), and
`color_policy` is the one that remains). When a second source exists, its real
requirements reshape this.

**What a second source owes.** A source hands back a `Document` whose arena already
holds a catalogue, page tree, content streams and font resources. For a format such
as DOCX that means resolving styles, breaking lines, paginating and generating
content streams — a layout engine. Implementing the trait is the small part; apart
from font handling, almost nothing is shared with reading PDF. Naming the boundary
makes such a converter easy to *place*, not easy to *write*.

### 5.3 Interpretation policy

"Read 1.7, write 2.0" is mostly a *decision* problem. Files written before PDF 2.0
are frequently non-conforming, and parts of the older specifications are genuinely
ambiguous — how to delimit a stream whose `/Length` is wrong, whether a byte sequence
inside an inline image terminates it, how to read a text string with no BOM. Reading
such a file means choosing, and the choice determines the output.

Those choices are therefore **recorded, not logged**. `Document::decisions` carries a
[`DecisionLog`], and every `inspect` command prints it in every output format — text,
JSON and Markdown — with `inspect text` writing to stderr so its piped output stays
clean. A caller must be able to distinguish *this loaded* from *this was conforming*,
which a `log::warn!` on stderr cannot do.

Structured, too. The audit alone used to show them, by stringifying each decision into
a compliance issue at `IssueSeverity::Warning`: a JSON consumer was told "Warning"
about something the engine had classified `Repaired`, and a `Violation` was
indistinguishable from an `Ambiguity`. `DocumentSummary::decisions` now carries the log
with its severities, and the audit no longer launders them.

Each decision carries the clause that governs it and what was done:

| Severity | Meaning |
| :--- | :--- |
| `Ambiguity` | The standard permits several readings; the engine picked one. |
| `Repaired` | The input is wrong but its intent is unmistakable, and nothing was lost. |
| `Violation` | The input contradicts a requirement; something was dropped or substituted. |

`Strictness::Lenient` is the default, because refusing real-world files is not useful.
`Strictness::Strict` rejects a document carrying a `Violation` — for validating a
producer, or gating an archive. Repairs alone never fail a strict read.

**Rules for adding a decision point.** Where the engine departs from a literal reading
of the standard, it records why, at the point of decision, with the clause. A silent
acceptance is a defect even when the output is right, because the next reader of the
code cannot tell a deliberate choice from an oversight.

**Current coverage is 69 sites**, up from one: `reader.rs` 19, `refine/color.rs` 12,
`font/mod.rs` 8, `interpreter/ops/xobject.rs` 7, `decrypt.rs` 6, `document.rs` 4,
`object/sublimation/parser.rs` 3, `optional_content.rs` 3, `ingest/mod.rs` 2,
`interpreter/ops/marked.rs` 2, `metadata.rs` 2, `refine/mod.rs` 1 (2026-08-22). The three in `fepdf-content` are the newest and the reason the
count moved: an image whose filter this engine cannot decode is skipped so that the
page's text survives, and that skip is now recorded rather than logged
([ADR-0018](docs/adr/0018-interpreting-a-page-can-add-to-the-decision-log.md)). The
`status.sh` row had to learn to search that crate before it could see them. The five
newest are optional content (8.11): three where `/OCProperties` or its default
configuration will not read, and two where a `/OC` entry names no group this engine can
find — each of which draws the content rather than hiding it, so the log is the only place
the doubt is visible ([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md)).
This paragraph read 39 while
`status.sh` reported 53 — the drift the row exists to surface, surfaced and then not
folded back, and the larger half of it is a whole file the list never had:
`refine/color.rs` records clause 8.6 twice per defect, the `Violation` under
`ColorPolicy::Strict` and the `Repaired` substitution of `/DeviceRGB` under
`Relaxed`. `decrypt.rs` gained four with clause 7.6: three
report a public-key document that could not be opened and why, and one an object stream
that would not expand after decryption. The eleven places that previously detected
non-conformance and merely `log::warn!` — missing font `/Subtype`, empty
`/DescendantFonts`, undecodable CMap streams, unknown content-stream operators — were
converted while the reader was replaced, which is where most of them sat.

**A site is not a firing, and which sites can fire depends on the command.** Over the
251 files of both corpora, `inspect structure` reports 11 decisions in total, from five
clauses and all of them the reader's: 7.5.4 four, 7.5.8 four, and one each of 7.3.8.2,
7.5.2 and 7.5.7. `inspect info` adds a twelfth on one file — a page tree whose root the
file does not contain (7.7.3.2), which `find_all_pages` used to drop in silence. The font, decryption and colour sites contribute nothing to that
figure, because inspecting the structure does not run those paths — not because they
are dead. `isartor-6-3-2-t01-fail-b.pdf` makes the difference visible: `inspect
structure` reports no decision on it, and `inspect text` on the same file reports a
`Violation` of 9.9, a font program in no recognised format, skipped for a system font.
So `is_conforming` answers "no departure **in what has been examined**", and the log is
only as complete as the work the caller asked for. That is now the documented meaning
rather than an accident of which command was run: the log is behind a lock, a page being
interpreted can add to it, and `inspect text` prints what reading decided before the
text and what interpreting decided after it (ADR-0018).

One `log::warn!` remains in the engine, in `font/mod.rs`, and is deliberate: it reports
which fonts *this machine* has, which is a property of the host and not of the
document, so it is not a decision about the input.

**A decision that fires on conforming input is worse than none**, because it makes the
log a constant rather than a signal. Reading an indirect `/Length` — which 7.3.8.2
permits — was recorded as an `Ambiguity`, so `samples/sample.pdf` reported 31
departures and `is_conforming` returned `false` for a clean file
([ADR-0008](docs/adr/0008-an-indirect-length-is-not-an-ambiguity.md)).

The rule has caught a second one since. Settling `/Info` against the metadata stream
(§5.4) began by recording the move of the entries 14.3.3 deprecates — which every one
of the nine samples carries, so every one of them grew a `Repaired` line. Carrying a
deprecated entry is not non-conformance and moving it loses nothing, so it is not a
decision; the disagreement that *does* lose something is, and that fires on one file.
Eight samples record nothing and `samples/fy05.pdf` records its one real ambiguity.
`metadata.rs` holds a test asserting exactly that, because the property is easy to
break from a distance.

When adding a decision point, check it against a conforming file as well as a broken
one. `./scripts/dev/status.sh` re-derives the site count above, so a figure that has
gone stale shows up as a disagreement rather than reading as current.

### 5.4 The Sublimation Pipeline: normalisation-at-load

Every byte passes three normalisation stages before application code sees it. The
pipeline spans `fepdf-syntax` → `fepdf-model`, which is why normalisation is a
concern of the model rather than a crate of its own.

```
Raw bytes ─► Reading ─► Pass 0: Decryption ─► Pass 2: Semantic ─► Settling ─► Document
```

- **Reading** (`reader::load_document`). Locates the header, walks the cross-reference
  sections oldest-first so a later revision overrides an earlier one, expands object
  streams, and places each object in the arena **at the slot matching its number**.
  There is no remapping table: the parser builds `Object::Reference(Handle::new(n))`
  straight from `n 0 R`, so references are correct as written. Every tolerance applied
  is recorded as a `Decision` (§5.3). When the cross-reference is unusable the file is
  scanned for `N G obj` instead, and that substitution is recorded too.
- **Pass 0 — Decryption** (`decrypt::unlock`). Walks the populated arena decrypting
  strings and streams with each object's own number, skipping the `/Encrypt`
  dictionary, which is never encrypted. Removes `/Encrypt` afterwards: Acrobat reports
  error 135 for a file whose objects are plain but whose trailer still claims
  encryption.
- **Pass 2 — Semantic sublimation.** Re-encodes character mappings to eliminate legacy
  CJK mojibake, preserves exact path endpoints (`EndPath n`), harmonises graphics
  state, normalises colour, and validates PDF 2.0 structure integrity.
- **Settling** (`metadata::settle`). Reconciles the two places a PDF may keep document
  metadata — the `/Info` dictionary and the catalogue's metadata stream — into one,
  recording where they disagreed. Runs after Pass 2 because Pass 2 rewrites the stream.

Pass 1 no longer exists. It converted another library's object model into ours; the
reader now produces the arena directly
([ADR-0003](docs/adr/0003-lopdf-was-not-providing-robustness.md)).

**A `Document` is therefore one normalised state, not the file.** Everything above
happens before application code sees anything, so by the time a `Document` exists the
revision chain has been merged, the ciphertext is gone, and the metadata has one
answer. Nothing later can put those back
([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)).

That leaves the question of how to see the file as written, and the answer is a second
entry point rather than a mode of this one:

| | Entry point | Reports | Commands |
| :--- | :--- | :--- | :--- |
| **Byte layer** | `reader::load_document` + Pass 0 | the file as written | `inspect structure`, `catalog`, `encryption`, `interactive` |
| **Document layer** | `Document::open` | the document the engine made | `inspect info`, `text`, `tree`; all `edit` and `publish` |

`FileStructure`, `CatalogReport`, `InteractiveReport` and `EncryptionReport` each take
`&[u8]` and never see a refined arena. The two layers can disagree about the same file,
and that is the design: they answer different questions. Before ADR-0013 named them,
which one a command answered was an accident of how it had been written.

**What the model cannot hold is lost here, with no later stage to recover it.** That is
the price of normalising at load, and it is not hypothetical: the text decoder corrupted
a conforming `/Title` at this point, and the only reason output was ever right was that
the save path happened to overwrite the value from XMP. Changes to reading carry more
weight than their size suggests.

### 5.5 Unified Extension Architecture (Anti-Ad-Hoc Policy)

To prevent drift, ad-hoc struct additions and uncoordinated writer logic, a new backend
capability belongs in one of four domain namespaces owned by `fepdf-model` (and, once it
exists, `fepdf-doc` — see [§6](#-6-migration)):

1. **Metadata & Structure**: Portfolio (`/Collection`), Outlines (`/Outlines`), Optional
   Content (`/OCProperties`), Associated Files (`/AF`), Output Intents
   (`/OutputIntents`), Pronunciation (`/PL`).
2. **Security & Provenance**: public-key security handlers (7.6.5), PAdES digital
   signatures, redaction.
3. **Decorations & Annotations**: watermarks, Bates numbering, hyperlinks (`/Link`),
   stamps, measurements (`/Measure`).
4. **Interactive Forms**: AcroForms, FDF/XFDF static data models.

No feature may bypass the `Operation` vocabulary or inject un-audited dictionary
mutations directly into frontends or serialisers.

This list named "Crypt Revision 6 (AES-256-GCM)" until it was checked against the
standard: revision 6 is implemented, and the string `GCM` does not occur anywhere in ISO
32000-2. AES in this standard is CBC — "If using the AES algorithm, the Cipher Block
Chaining (CBC) mode, which requires an initialization vector, is used." A namespace list
is exactly where an invented detail survives longest, because nothing compiles against
it.

#### 5.5.1 Multi-Format Provider Architecture

When introducing support for external document formats (Word `.docx`, Excel `.xlsx`,
SVG, HTML), each format follows Rule C by keeping its ingestion and emission in one
provider crate (`fepdf-import-docx`). Providers translate into the `Operation`
vocabulary or intermediate layout structures without exposing format-specific
dependencies to `fepdf-model`. See [§5.2](#52-document-sources) for what such a provider
actually owes.

### 5.6 Safety invariants

- **Handles, not pointers.** Objects are reached only through `Handle<Object>`,
  eliminating use-after-free and dangling references by construction.
- **Deterministic traversal.** `PdfArena` uses `BTreeMap` and indexed handle arrays
  throughout, so iteration order — and therefore produced bytes — is reproducible.
  RR-15 Rule 10 forbids `HashMap`/`HashSet` in the crates that decide output.
- **Zero unsafe.** `unsafe_code = "forbid"` across the workspace.

### 5.7 Rendering

`fepdf-content` walks the content stream and issues calls against `Backend`.
`fepdf-render` answers them with **Vello** compute shaders on **wgpu**. Path snapping
keeps double-precision `kurbo` geometry until rasterisation; `skrifa` and `read-fonts`
handle glyph mapping, Japanese fallback fonts, and Type 3 precipitation.

Because the contract is separate, the same interpreter drives text extraction and
geometry collection without a GPU present.

One thing sits between the interpreter and whichever backend answers: `canvas` withholds
the five calls that put marks on a page while an optional-content group is off (8.11),
and forwards everything else, so the state a hidden section leaves behind is the state the
operators after it inherit. It is a wrapper rather than a check at each painting site
because the symptom of forgetting one such check is a layer that should be off appearing
on the page, with nothing to fail
([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md)).

---

## 🚧 6. Migration

Ordered by value against risk. Steps 1–3 and 5 relocate code without changing logic,
so a green test run is sufficient evidence of correctness. Steps 0, 4 and 6 change
behaviour or API and need their own tests.

| # | Step | Effect | Risk |
| :-: | :--- | :--- | :---: |
| 0 | Reconcile the two `rotate` implementations | ✅ **Done**, as part of step 4. `RotateMode` + `Quarter` make the divergence unrepresentable | Low |
| 1 | Move the `RenderBackend` contract and its types from `fepdf-render` into `fepdf-content` | ✅ **Done.** Content-stream interpreter and backend contract live in `fepdf-content`; GPU is opt-in | Low |
| 2 | Extract the PDF-free half of `font/` into `fepdf-font` | ✅ **Done.** 3,710 lines are independently testable | Low |
| 3 | Move struct-tree handling out of `fepdf-gui` into `fepdf-doc` | ✅ **Done.** Extracted into `fepdf-doc`. The GUI calls `extract_struct_tree()`; the Rule A leak is closed | Medium |
| 4 | Introduce `Operation`; reduce the CLI subcommands and `WorkerRequest` to adapters over it | ✅ **Done.** Extracted into `fepdf-doc` with all 24 operations implemented and modularized. Rule D is structural | Medium |
| 5 | Move `writer` into `fepdf-model` (core) | ✅ **Done.** Restores the read/write round trip in `fepdf-model` (Rule C); `fepdf-sdk` re-exports for compatibility | Low |
| 6 | Introduce the `fepdf` facade | ✅ **Done.** `fepdf-sdk` renamed to `fepdf`, establishing the public facade crate and completing the target topology | Low |

Steps 0–6 are complete. The target crate topology (§2) is fully realised with
`fepdf` as the top-level public facade crate, and `fepdf-doc` and `fepdf-content`
owning document mutation and content interpretation respectively.

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
