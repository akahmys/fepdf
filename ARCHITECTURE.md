# 🏛️ `fepdf` Architecture & System Design

The authoritative architectural blueprint for **fepdf**: crate topology, layering
rules, the Sublimation Pipeline, and memory invariants.

> **Status.** The topology below is **realised**: every crate in §3 exists and every step
> in [§6](#-6-migration) is done. This banner said the opposite — that several crates did
> not exist yet and their code lived in `fepdf-sdk` — for as long as it took steps 0 to 6
> to complete and nobody to re-read the top of the file. `fepdf-sdk` has not existed since
> step 6 renamed it.
>
> What is **not** realised is §5.1's Rule D, and that is a defect rather than a stage:
> the vocabulary exists and eight frontend call sites go round it. Read the rules as
> load-bearing and the enforcement column as the thing to check.

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
implementor. `RenderBackend` belongs beside the interpreter that drives it; the GPU
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

`~Lines` is what the crate holds today. **Re-measured 2026-08-22** with

```bash
find crates/<name>/src -name '*.rs' | xargs cat | wc -l
```

so any figure here can be checked in one command — which is the only reason the drift was
visible. The 2026-08-18 figures had gone stale everywhere but `fepdf-syntax`, and one was
stale by 5.8×: `fepdf-mcp` read 330 while it held 1,902, having become the only frontend
that constructed all 24 operations of the day. A size that is quoted and not re-derived says what the
crate *was* for.

Every crate below the facade is now larger than the row that described it, so nothing here
should be read as a budget. Re-measured 2026-08-22, with the command that re-derives them —
a figure in this document that carries no way to check it is how the last set came to be
stale by 5.8×:

```bash
for c in crates/*/; do
  printf '%-16s %s\n' "$(basename "$c")" \
    "$(find "$c/src" -name '*.rs' | xargs wc -l | tail -1 | awk '{print $1}')"
done
```

`fepdf-model` and `fepdf-content` grew in Phase P: the function evaluator (7.10), the
colour-space resolver (8.6) and the mesh decoder (8.7.4.5.5 to 8.7.4.5.8) are the first's
`src/function/`, `src/color/space.rs` and `src/graphics/mesh.rs`, and the interpreter
changes that reach them are most of the second.

| Crate | Status | ~Lines | Responsibility |
| :--- | :---: | ---: | :--- |
| **`fepdf-syntax`** | ✅ | 3,377 | The byte layer: lexing and encryption/decryption. Depends on no model type, which is what lets the cryptography be reviewed on its own. Parsing and stream filters are *not* here — see §4. |
| **`fepdf-font`** | ✅ (Audited ✅) | 3,740 | Font *programs*: CFF, TrueType, CMap, Adobe Glyph List, subsetting, reconstruction. Hardened against W/W2 out-of-bounds, CMap underflows (`e_val >= s_val`), and CID byte truncations. |
| **`fepdf-model`** | ✅ | 29,165 | The document graph: `PdfArena`, `Handle<T>`, `Object`, page tree, metadata — and, since Phase A, the reader (7.5) and `writer.rs`. Hardened with pool overflow guards, cyclic `resolve` limits (`64`), and safe `Null` reference fallbacks. |
| **`fepdf-content`** | ✅ | 3,915 | Content-stream interpreter, and the **`RenderBackend` contract** it drives (`TextGlyph`, `TextState`, `SMaskData`, path geometry). No GPU dependency. |
| **`fepdf-doc`** | ✅ | 3,744 | Owns the **`Operation` vocabulary** (§5.1) and is its only interpreter: **30** canonical mutation operations. Also structure-tree handling, conformance auditing, remediation. Grew by six when Rule D was enforced and the facade's mutating methods became operations. |
| **`fepdf-render`** | ✅ | 1,548 | A `RenderBackend` implementation on **Vello** + **wgpu**. Reached only through the facade's optional `render` feature. |
| **`fepdf`** | ✅ | 1,652 | The public facade: `PdfDocument`, `SaveOptions`, `Operation`. It is the Rule A boundary in fact — frontends depend on it and on nothing below. Lost 167 lines when ten document-mutating methods left for the vocabulary (§5.1); `duplicate_page` and `insert_pages_from` were not passthroughs but arena work, and belonged with the cloner in `fepdf-doc`. |
| **`fepdf-cli`** | ✅ | 3,027 | Command-line binary (`fepdf`). |
| **`fepdf-gui`** | ✅ | 8,507 | Desktop application on **egui** + **eframe** + **wgpu**. |
| **`fepdf-mcp`** | ✅ | 1,902 | Model Context Protocol server for AI assistants. **The most complete frontend by some distance**: all 30 `Operation` variants, where `fepdf-cli` constructs 8 and `fepdf-gui` 6. That is the shape §5.1 predicted — a tool is the serialised form of an operation — arriving on its own. It sat at 24 for a phase, missing exactly the six Rule D produced, because nothing counted; `status.sh` counts them now against the enum itself. |
| **`fepdf-wasm`** | ✅ | 40 | WebAssembly bindings. Currently a stub, and worse than unimplemented: `render_page` **returns `Ok(())` having drawn nothing**, so a caller is told it succeeded and gets a blank canvas. It also constructs no `Operation` at all, which is why the §5.1 diagram no longer lists it as a frontend that does. |
| **`fepdf-macros`** | ✅ | 183 | Compile-time procedural macros. |

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
write. `writer.rs` now lives in `fepdf-model` — 2,548 lines when that sentence was
written, 3,020 today — and the compatibility re-export went with `fepdf-sdk` when step 6
renamed it.

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

**Which subsets this processor claims** is not here. 6.3.2.1 makes that the thing
conformance is measured against, and the declaration lives beside the refusals it
formalises, in [`ROADMAP.md`](ROADMAP.md#the-subsets-this-processor-has-chosen) — reading
broadly, rendering, writing 2.0 and nothing else. This document says what the design *is*;
that one says what it has undertaken to be.

## 🛡️ 5. Cross-Cutting Concerns

### 5.1 The operation vocabulary

Every document mutation is a value of one type, defined in `fepdf-doc` and re-exported
through the facade. Frontends construct it; only `fepdf-doc` interprets it.

```
   fepdf-cli    argv          ─┐      8 of 30 variants
   fepdf-gui    button press  ─┤      6 of 30
   fepdf-mcp    tool call     ─┼─►  Operation  ─►  fepdf-doc::apply
   fepdf-wasm   —             ─┘     (a value)      (the only implementation)
                                     30 variants      and the only way in
```

Ambiguity that used to live in prose becomes a type. The rotate divergence in §4 was
not fixable by convention; it was fixed by making the choice unrepresentable:

```rust
pub enum Operation {
    // (unlabelled in the source: the core page and structure group)
    Rotate { pages: PageSelection, mode: RotateMode },
    Reorder { from: usize, to: usize },
    RemovePages(PageSelection),
    ReorderBatch { sources: Vec<usize>, target: usize },
    DuplicatePages(PageSelection),
    InsertFrom { source: Vec<u8>, at: usize },
    AddLtvInfo { certificates: Vec<Vec<u8>> },
    Retag,
    Upgrade { standard: PdfStandard },
    UpdateStructElem(StructElemUpdate),
    DeleteStructElem { handle_index: u32 },

    // --- Metadata & Structure ---
    CreatePortfolio(PortfolioCollection),
    UpdateOutlines(OutlineTree),
    UpdateLayers(OptionalContentProperties),
    AttachAssociatedFile(AssociatedFile),
    SetOutputIntent(OutputIntent),
    SetPronunciationLexicon { lexicon_xml_bytes: Vec<u8> },

    // --- Decorations & Annotations ---
    AddPageDecoration { pages: PageSelection, text: String,
                        position: DecorationPosition, layer: Option<String> },
    ApplyBatesNumbering { pages: PageSelection, prefix: String,
                          start_number: u64, digits: usize, position: DecorationPosition },
    AddAnnotation(AnnotationSpec),
    SetMeasurementScale(MeasurementScale),

    // --- Interactive Forms ---
    SetFormFieldValue(FormFieldSpec),

    // --- Navigation, Structure & Actions ---
    SetPageLabels(Vec<PageLabelSpec>),
    UpdateArticleThreads(Vec<ArticleThread>),
    AddUserProperties { target_handle: u32, properties: Vec<UserProperty> },
    ExecuteAction(PdfAction),

    // --- Advanced Graphics & GIS ---
    SetGeospatialAnchor(GeoSpatialAnchor),
    AddMeshShading(MeshShadingSpec),

    // --- Font & Cryptography ---
    SetUnencryptedWrapper(UnencryptedWrapperSpec),
    AddPublicKeyRecipient(PublicKeyRecipientSpec),
}

pub enum RotateMode {
    /// Set `/Rotate` to this angle. Rejected unless a multiple of 90.
    Absolute(Quarter),
    /// Add to the current angle, normalised into [0, 360).
    Relative(Quarter),
}
```

**This listing was fiction until 2026-08-22.** It named 21 variants of which **nine did
not exist** — `InsertFrom`, `Retag`, `Redact`, `Upgrade`, `CreateLayer`,
`SetLayerVisibility`, `AddHyperlink`, `AddStamp`, `AddPageDecorations` — and omitted
**twelve that did**, including everything Phases 5 to 7 added. It was the plan, written
before the code and never re-read against it, in the section that defines the rule the
rest of the architecture leans on.

Three of those nine now exist, and the plan turned out to have been right about them:
`InsertFrom`, `Retag` and `Upgrade` were built as facade *methods* instead, which is
precisely how Rule D came to be broken. Enforcing the rule was largely a matter of
building what this listing had claimed for four phases. Re-derive it with:

```bash
sed -n '/^pub enum Operation {/,/^}/p' crates/fepdf-doc/src/operation.rs | grep -oE '^    [A-Z][A-Za-z]*'
```

Four of the nine turn out to be the more interesting half — see Rule D below.

A caller must now say which it means, and `Quarter` makes 45° unconstructible.

**Three consequences fall out rather than being designed in:**

- **Undo/redo.** Operations are values, so they can be recorded, inverted and
  replayed. The GUI gets history without a parallel mechanism.
- **MCP tool surface.** A tool becomes the serialised form of an `Operation`. New
  operations reach AI assistants without new bridging code.
- **Testability.** An operation sequence can be applied and asserted without starting
  a GUI or spawning a process.

**Rule D did not hold, and what was checking said it did.** The rule says every mutation
is an `Operation`; §7 called that "enforced by construction". Nothing enforced it, because
the facade exposed each mutation *twice* — as a variant and as a plain `&mut self` method —
and a frontend that called the method had re-implemented nothing but had still left the
vocabulary. Eight frontend call sites did exactly that:

| Where | Method | Was there an `Operation`? |
| :--- | :--- | :--- |
| `fepdf-gui/src/worker.rs` ×2 | `remove_page` | **yes** — `RemovePages`, unused by the GUI |
| `fepdf-gui/src/worker.rs` ×2 | `insert_pages_from` | no |
| `fepdf-gui/src/worker.rs`, `thumbnail_sidebar.rs` | `duplicate_page` | no |
| `fepdf-cli/src/commands/publish.rs` | `upgrade_to_standard` | no |
| `fepdf-cli/src/commands/edit.rs` | `retag_document` | no |

The first row is the rotate divergence of §4 in its early form: two ways to remove a page,
one of them the vocabulary and one of them not, with nothing comparing them.

**It holds now, and by construction rather than by assertion.** Ten mutating methods left
the facade — the eight above plus `swap_pages` and `add_ltv_info`, which had **no caller
anywhere** — and six became operations. `apply` is the only way in, so a frontend has
nothing to reach for. That is the same move `RotateMode` and `Quarter` made for the
divergence that created the rule: make the alternative unrepresentable rather than
discouraged.

What the enforcement taught, in order of how much it cost to learn:

- **A check that greps call sites cannot do this job.** The first version of the
  `status.sh` row searched the four frontends for each facade mutator's name. It missed
  `reorder_pages_batch`, whose signature spans two lines, and it counted
  `app.duplicate_page` — the GUI's *own* method, which merely shares a name. The row now
  reads `crates/fepdf/src/lib.rs` alone and counts `&mut self` methods that are not `apply`
  and not the four that configure saving. One file, no receivers to disambiguate. Verified
  by adding a method back with a multi-line signature, which the old version could not see
  and the new one reads as 1.
- **Two of the ten were not passthroughs.** `duplicate_page` and `insert_pages_from` held
  arena work and the object cloner — document logic in the crate that exists to expose it.
  Their new home in `fepdf-doc` is where `ObjectCloner` already lived.
- **The frontend was doing the engine's arithmetic.** The GUI removed pages by sorting
  indices descending and looping, so that removing one did not move the next. `RemovePages`
  takes the set and owns the order. `DuplicatePages` had to solve the same problem, and
  getting it wrong is not a mis-ordering: selecting three pages and inserting ascending
  clones page 0 three times, because after the first insertion the remaining indices name
  clones. A test asserts the widths and was verified by putting the bug in.
- **`fepdf-mcp` now constructs 24 of 30** rather than all of them, and should gain the six.

**What this is not.** The GUI keeps its worker thread:**What this is not.** The GUI keeps its worker thread: `WorkerRequest` remains, but as
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
vocabulary of which 19 of 24 *were* stubs (all 24 are implemented now, and `status.sh`
holds the row that would say otherwise), and ingestion options nothing reads
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

**Current coverage is 85 sites**, up from one, re-derived 2026-08-23: `reader.rs` 19,
`refine/color.rs` 12, `font/mod.rs` 8, `interpreter/ops/xobject.rs` 7, `decrypt.rs` 6,
`document.rs` 4, `interpreter/ops/color.rs` 4, `fepdf/lib.rs` 3, `optional_content.rs` 3,
`object/sublimation/parser.rs` 3, `metadata.rs` 2, `ingest/mod.rs` 2,
`apply/appearance.rs` 2, `interpreter/ops/marked.rs` 2, `interpreter/mod.rs` 2,
`fepdf-render/text.rs` 1, `fepdf-render/lib.rs` 2, `refine/mod.rs` 1,
`apply/annotations.rs` 1, `interpreter/ops/text.rs` 1.

**The row that counts them named five crates, and `fepdf-render` was not one.** So when
the renderer learnt to report a glyph whose outline would not build and a font that never
reached its cache, `status.sh` read 82 where the truth was 84 — the miss the row's own
comment had predicted, and the third time this figure has been wrong for the same reason.
It derives from the workspace now, reusing the partition the log row above was given in
Phase Q, so the two rows are complements and a new crate lands in both by construction.

The nine newest are Phase P's: an operator this engine does not run (8.2), a pattern and a
shading that would not build (8.7.3, 8.7.4.5.2), an operand count no colour model takes
(8.6.8), a Type 3 glyph whose `/CharProcs` stream would not run (9.6.5), a `/Lab` colour
converted through D65 sRGB (8.6.5.4), and the two the renderer reports (9.9, 9.6). Each
was measured against the nine conforming samples before it was written, and each fires on
none of them.

**A site is not a firing, and which sites can fire depends on the command.** Over the
251 files both corpora then held — 524 now, and this paragraph has not been re-derived
against the larger set — `inspect structure` reports 11 decisions in total, from five
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

**Three `log::warn!`/`log::error!` sites remain in the engine, and all three are
deliberate.** They report properties of the *host*: which fonts this machine has
(`fepdf-model/src/font/mod.rs`), the GPU failing to initialise so the CPU renderer takes
over, and a system fallback font that would not load from its path.

**The thirteen that were conclusions about the document did not all become `Decision`s,
and the reason is the rule below.** Each was measured against the nine conforming samples
before it was touched, and three of the thirteen turned out not to be conclusions at all:

| site | fired on conforming input | became |
| :--- | ---: | :--- |
| `interpreter/font.rs` "not SFNT, using fallback" | **469** | deleted |
| `reconstruction.rs` "CFF table not found in SFNT container" | **918** | deleted |
| `reconstruction.rs` "Unrecognized font format" | 0 | deleted |
| `reconstruction.rs` "SFNT assembly FAILED" | 0 | `log::debug` |
| the other nine | 0 | `Decision` |

The two that fired in the hundreds were reporting *ordinary* conditions. An SFNT container
with no `CFF ` table is a TrueType font, and `inspect_cff` is called speculatively —
every caller reaches it through `.unwrap_or(CffInfo::empty())`, so that `Err` is the
expected answer. "Not SFNT" fired 423 times on `fugaku.pdf` alone, whose 72 fonts are all
**Type 3**, which by 9.6.5 have no font program and so can never be SFNT; the rest were
fonts with no `/FontFile`, where substituting is what 9.8 asks for. Converting either would
have put 918 and 469 false departures on clean files and made `is_conforming` false for six
of the nine — [ADR-0008](docs/adr/0008-an-indirect-length-is-not-an-ambiguity.md)'s mistake,
made again and at scale.

"Unrecognized font format" was a third copy of a test that already exists: measured across
the whole external corpus it fired on exactly the three `isartor-6-3-2-t01-fail-*` files,
which are exactly the files already carrying the 9.9 `Violation` "embeds a program in no
recognised format" — twice per document where the decision fires once.

**A backend cannot record for itself**, because it sits below any `Document`: it is handed
paths and glyphs, not a file. `RenderBackend::take_decisions` is defaulted to empty and
`render_page` drains it after the annotations, so the two `fepdf-render` sites — a glyph
whose outline the font program will not yield, and a font the interpreter selected that
never reached the cache — are recorded against the document that caused them. The drain was
verified by injecting a decision into the backend and watching it arrive.

This paragraph said "one" for three phases. It was true of the two crates `status.sh`
searched and of no larger set, and `fepdf-content`, `fepdf-font` and `fepdf-doc` were in
**neither** the engine list nor the frontend list — invisible rather than miscounted,
which is why doubling the figure changed no row. The lists are now complements derived
from the workspace, so a new crate lands in one of them by construction.

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

`fepdf-content` walks the content stream and issues calls against `RenderBackend`.
`fepdf-render` answers them with **Vello** compute shaders on **wgpu**. Path snapping
keeps double-precision `kurbo` geometry until rasterisation; `skrifa` and `read-fonts`
handle glyph mapping, Japanese fallback fonts, and Type 3 precipitation.

Because the contract is separate, the same interpreter drives text extraction and
geometry collection without a GPU present.

Rendering a page is **two** walks, because 6.3.2.2 asks for two: the content streams, and
then the appearance stream of every annotation that has one and is not hidden by its flags
(12.5.5). Each appearance is a form XObject with its own coordinates and resources, so it
gets its own interpreter with the placement 12.5.5's algorithm computes
([ADR-0023](docs/adr/0023-a-renderer-that-skips-annotation-appearances-is-not-conforming.md)).
A content stream that will not decode no longer cancels the second walk.

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
| 4 | Introduce `Operation`; reduce the CLI subcommands and `WorkerRequest` to adapters over it | ✅ **Done**, and for the first time in the sense it was written — the vocabulary was introduced *beside* the facade's mutating methods for four phases, and both were called. Removing the methods finished it: 30 operations, and `apply` is the only way in | Medium |
| 5 | Move `writer` into `fepdf-model` (core) | ✅ **Done.** Restores the read/write round trip in `fepdf-model` (Rule C) | Low |
| 6 | Introduce the `fepdf` facade | ✅ **Done.** `fepdf-sdk` renamed to `fepdf`, establishing the public facade crate and completing the target topology | Low |

Steps 0–6 are complete. The target crate topology (§2) is fully realised with
`fepdf` as the top-level public facade crate, and `fepdf-doc` and `fepdf-content`
owning document mutation and content interpretation respectively.

**Step 4 is complete in the sense it was written and not in the sense it was meant.** It
reads "reduce the CLI subcommands and `WorkerRequest` to adapters over it", and they were
not reduced — the vocabulary was introduced *beside* the methods rather than in place of
them, so both survive and both are called (§5.1). A migration step that adds the new thing
and leaves the old one is the shape that produces two implementations of "the same"
operation, which is what Rule D exists to prevent.

**Deliberately not planned.** Splitting `fepdf-doc` into separate operation and
verification crates: auditing and remediation act on the same document surface, so
module boundaries suffice until that changes. Treating `fepdf-wasm` as a peer
frontend: at 40 lines with an unimplemented renderer, whether to build it is a product
decision, not an architectural one.

---

## 🔍 7. Enforcement

Architecture rules that are not checked become comments. These are:

- **Rules A–C**: enforced by Cargo, and the claim used to be narrower than the topology.
  It said "no frontend declares `fepdf-model`" — true, and it left room for what was
  actually there: **`fepdf-gui` declared `fepdf-render`**, reaching the GPU crate directly
  rather than through the facade's `render` feature, which is the opt-in
  [ADR-0004](docs/adr/0004-rule-b-makes-the-gpu-dependency-optional.md) exists to provide.
  It needed two names, `VelloBackend` and `FallbackFontType`, and the facade re-exports
  both, so the fix was one line of `Cargo.toml` and one `use`.

  **All four frontends now declare `fepdf` and nothing else**, which is what §2's diagram
  has always drawn. `status.sh` counts internal dependencies that are not the facade and
  expects 0, alongside the older row that greps for arena types — the first is structural
  and the second catches a type that arrives some other way. Verified by putting the
  `fepdf-render` line back and watching the row read 1.

  `fepdf-render` declares `fepdf` as a **dev-dependency**, for its own tests. That is not
  the cycle it looks like: it does not exist in the build graph of anything that links
  `fepdf-render`.
- **Rule D**: enforced by construction, and for four phases that was an assertion rather
  than a fact — the facade exposed ten document-mutating `&mut self` methods beside the
  vocabulary, and frontends called them at eight sites (§5.1). The tell was that this row
  named no tool while every other one did. The methods are gone; `apply` is the only way
  in; and the claim is now backed by a `status.sh` row that counts `&mut self` methods on
  the facade and expects 0. **The rule and its check disagreed for longer than the rule
  had been true**, which is the argument for never writing "by construction" without
  naming what would notice.
- **RR-15 protocol**: [`CODING.md`](CODING.md), checked by
  [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh).
- **Lints**: `cargo clippy --workspace --all-targets -- -D warnings`. `--all-targets`
  is required — without it tests, examples and benches go unlinted.
- **Licences**: `cargo deny check licenses` ([`deny.toml`](deny.toml)).
- **Secrets and PII**: `betterleaks` pre-commit hook ([`.betterleaks.toml`](.betterleaks.toml)).

Governance sits in [`AGENTS.md`](AGENTS.md), [`CODING.md`](CODING.md),
[`AUDITING.md`](AUDITING.md), and [`TESTING.md`](TESTING.md).
