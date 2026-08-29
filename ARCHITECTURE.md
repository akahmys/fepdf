# 🏛️ `fepdf` Architecture & System Design

> **Not a rule.** This is the design as it stands. The rules that keep it that way are in
> [CODING.md](CODING.md) §2; how it came to be is in [docs/adr/](docs/adr/README.md).

Crate topology, the Sublimation Pipeline, and memory invariants. The layering rules that
keep this shape are in [CODING.md](CODING.md) §2.

> **Status.** Realised: every crate in §3 exists, the migration that produced them is
> complete, and Rule D holds — `status.sh` counts document-mutating methods on the facade
> and reads 0. **This banner has twice asserted the opposite of what the file beneath it
> said**, which is why it now states only what a `status.sh` row derives
> ([ADR-0037](docs/adr/0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md)).

---

## 📐 1. Where the rules are

The four rules that decide where code goes — **A** storage abstractions stop at the
facade, **B** a crate defining a contract does not depend on its implementations, **C**
read and write live together, **D** frontends translate and never decide — are in
[`CODING.md`](CODING.md) §2 with what checks each. This document says what the design
*is*; a rule about structure is still a rule.

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
   │ resource resolution (§4.1)  │  │ (knows no PDF)   │
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

### 2.1 Directory layout

Where things live, and what may not move. This was `docs/conventions/DIRECTORY_LAYOUT.md`
until 2026-08-29; a document that states where code must go is part of the design, and a
`conventions/` directory outside the hierarchy was a second place to look for one rule.

| Directory | Holds | Owner |
| :--- | :--- | :--- |
| `assets/` | Static, Read-only Resources (Fonts, Models) | Project |
| `crates/` | Modular Rust Logic Layer | Engineering |
| `docs/` | Technical Specs & Architectural History | Architecture |
| `external/` | Submodules & Third-party Compliance Data | Engineering |
| `crates/*/examples/` | Rust Usage Examples & Demonstrations. Must live under the crate they exercise — a root `examples/` directory is never compiled, because the workspace root has no `[package]`. | Engineering |
| `out/` | Ephemeral & Persistent Outputs (Ignored by Git) | Pipeline |
| `out/artifacts/`| Test results, renders, and temporary PDFs | CI/CD |
| `out/exports/` | Extracted document assets (Fonts, Images) | Refinery |
| `samples/` | Test Input Corpus (PDFs) — **files this project chose**, 9 of them | QA |
| `scripts/` | Automation & CI/CD Scripts | DevOps |
| `target/` | Cargo's build directory, **and the external corpora**: 515 files this project did not choose, under `target/external/`, plus `encrypted/`, `malformed/`, `scans/`, `layers/` and `colour/`. Git-ignored, fetched by `scripts/test/fetch_external_corpus.sh` | QA |

**Organisation.**

1.  **Consolidation**: All static resources MUST reside within `assets/`. Prohibit root-level resource directories (e.g., `resources/`).
2.  **Output Isolation**: every generated file goes under `out/`, which is git-ignored
    and the only root directory that holds output.

    Held by four writers that did not, until 2026-08-29: `render_all_samples.rs`,
    `render_japanese_samples.rs`, `fepdf-mcp`'s render tool and `hiragana_render_test.sh`
    all wrote to a root-level `artifacts/`. It was git-ignored too, so nothing was ever
    going to notice — which is why this rule now names the directory rather than the
    principle. `fepdf debug extract-font` had the worse version of the same fault: it
    wrote to a root-level `exports/` that was **not** git-ignored and never created, so
    the write failed unless someone had made the directory by hand, and left untracked
    files in the repository root when they had.

3.  **Script Categorization**:
    *   `scripts/audit/`: Compliance, security, and static analysis.
    *   `scripts/dev/`: Developer productivity and UI utilities.
    *   `scripts/test/`: Integration and functional testing.
4.  **Documentation Locality**: All technical specifications and architectural history MUST reside within `docs/`. High-level vision documents (`README.md`, `ROADMAP.md`, `AGENTS.md`) are permitted at the root for maximum visibility.
5.  **Scratch & Utility Binaries**:
    *   Prototyping debug scripts in `src/bin/` are permitted for initial verification.
    *   Once stabilized, their logic MUST be integrated into standard product CLI subcommands (e.g., `fepdf debug <cmd>`) or standardized as formal regression tests.
    *   Redundant or obsolete prototyping files MUST be purged during milestone stabilization to prevent codebase rot.
    *   Infrastructure binaries (e.g., `verify_render.rs` for visual regressions, `bypass_decrypt.rs` for emergency recovery) are exempt but MUST be clean of hardcoded values and compile warning-free under RR-15.

**Governance.**

1.  **No Redundancy**: Do not copy files from `external/` to `assets/`. Point the engine directly to the unified `external/` paths.
2.  **Script Placement**: Always place new automation in the appropriate `scripts/` subdirectory (`audit`, `dev`, or `test`).
3.  **Clean Root**: Keep the project root clean. Only core project metadata (`README`, `ROADMAP`, `VISION`, `LICENSE`) and workspace Cargo files should reside here.

**Maintenance.**

- Every new directory added to the root MUST be registered in this document. **Nothing
  checks this**, which is how `target/` — the largest and most load-bearing of them —
  went unregistered through five phases that measured against its contents.
- Root-level stray files are prohibited except for core configuration (`Cargo.toml`, `Makefile`, `LICENSE`).
- Tool-owned dotfile directories (`.git/`, `.github/`, `.cargo/`, `.claude/`, `.gemini/`)
  are not registered and do not need to be. This document governs what the project puts
  in the tree, not what its tools do.

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
| **`fepdf-syntax`** | ✅ | 3,377 | The byte layer: lexing and encryption/decryption. Depends on no model type, which is what lets the cryptography be reviewed on its own. Parsing and stream filters are *not* here — see `fepdf-model` below. |
| **`fepdf-font`** | ✅ (Audited ✅) | 3,740 | Font *programs*: CFF, TrueType, CMap, Adobe Glyph List, subsetting, reconstruction. Hardened against W/W2 out-of-bounds, CMap underflows (`e_val >= s_val`), and CID byte truncations. |
| **`fepdf-model`** | ✅ | 29,165 | The document graph: `PdfArena`, `Handle<T>`, `Object`, page tree, metadata — and, since Phase A, the reader (7.5) and `writer.rs`. Hardened with pool overflow guards, cyclic `resolve` limits (`64`), and safe `Null` reference fallbacks. |
| **`fepdf-content`** | ✅ | 3,915 | Content-stream interpreter, and the **`RenderBackend` contract** it drives (`TextGlyph`, `TextState`, `SMaskData`, path geometry). No GPU dependency. |
| **`fepdf-doc`** | ✅ | 3,744 | Owns the **`Operation` vocabulary** (§4.1) and is its only interpreter: **30** canonical mutation operations. Also structure-tree handling, conformance auditing, remediation. Grew by six when Rule D was enforced and the facade's mutating methods became operations. |
| **`fepdf-render`** | ✅ | 1,548 | A `RenderBackend` implementation on **Vello** + **wgpu**. Reached only through the facade's optional `render` feature. |
| **`fepdf`** | ✅ | 1,652 | The public facade: `PdfDocument`, `SaveOptions`, `Operation`. It is the Rule A boundary in fact — frontends depend on it and on nothing below. Lost 167 lines when ten document-mutating methods left for the vocabulary (§4.1); `duplicate_page` and `insert_pages_from` were not passthroughs but arena work, and belonged with the cloner in `fepdf-doc`. |
| **`fepdf-cli`** | ✅ | 3,027 | Command-line binary (`fepdf`). |
| **`fepdf-gui`** | ✅ | 8,507 | Desktop application on **egui** + **eframe** + **wgpu**. |
| **`fepdf-mcp`** | ✅ | 1,902 | Model Context Protocol server for AI assistants. **The most complete frontend by some distance**: all 30 `Operation` variants, where `fepdf-cli` constructs 8 and `fepdf-gui` 6. That is the shape §4.1 predicted — a tool is the serialised form of an operation — arriving on its own. It sat at 24 for a phase, missing exactly the six Rule D produced, because nothing counted; `status.sh` counts them now against the enum itself. |
| **`fepdf-wasm`** | ✅ | 63 | WebAssembly bindings, and thin: it opens a document and counts its pages. `render_page` **returns an error** naming what it did not draw — it used to return `Ok(())` having drawn nothing, so a caller was told it succeeded and got a blank canvas. It constructs no `Operation` at all, which is why the §4.1 diagram no longer lists it as a frontend that does. **It does not compile for `wasm32-unknown-unknown`**: `getrandom` arrives through the crypto stack and needs its `js` feature there. ROADMAP Phase Q carries that one. |
| **`fepdf-script`** | ✅ | 912 | The fifth frontend: ECMAScript (12.6.4.16) on **boa**, translating into `Operation` exactly as the other four translate argv, a button press and a tool call. Depends on the facade and nothing else, so it is **not** wired into `fepdf` behind a feature — that would be a cycle ([ADR-0031](docs/adr/0031-a-script-frontend-cannot-be-a-facade-feature.md)). A caller who does not depend on it links none of the 95 crates boa brings. **ECMA-402 is refused rather than approximated**: a script naming a locale gets an error and a `Decision`. boa's `intl` feature was built and measured before this was settled — it has no `Intl.DateTimeFormat.prototype.format` and no currency style, which are the two things a form asks ECMA-402 for ([ADR-0034](docs/adr/0034-intl-is-declined-for-what-it-does-not-do.md)). |
| **`fepdf-macros`** | ✅ | 183 | Compile-time procedural macros. |

Two `RenderBackend` implementations besides the GPU one — `TextExtractionBackend` and
`CollectorBackend` — sit alongside the operations, in `fepdf-doc`. Neither pulls in a GPU,
which is exactly what Rule B makes possible.

---

## 🛡️ 4. Cross-Cutting Concerns

### 4.1 The operation vocabulary

Every document mutation is a value of one type, defined in `fepdf-doc` and re-exported
through the facade. Frontends construct it; only `fepdf-doc` interprets it.

```
   fepdf-cli    argv          ─┐      8 of 30 variants
   fepdf-gui    button press  ─┤      6 of 30
   fepdf-mcp    tool call     ─┼─►  Operation  ─►  fepdf-doc::apply
   fepdf-wasm   —             ─┘     (a value)      (the only implementation)
                                     30 variants      and the only way in
```

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
is an `Operation`, which this document once called "enforced by construction". Nothing enforced it, because
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

The first row is the rotate divergence Rule D was written for, in its early form: two ways to remove a page,
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

### 4.2 Document sources

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

### 4.3 Interpretation policy

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

**A decision that fires on conforming input is worse than none**, because it makes the
log a constant rather than a signal. Reading an indirect `/Length` — which 7.3.8.2
permits — was recorded as an `Ambiguity`, so `samples/sample.pdf` reported 31
departures and `is_conforming` returned `false` for a clean file
([ADR-0008](docs/adr/0008-an-indirect-length-is-not-an-ambiguity.md)).

The rule has caught a second one since. Settling `/Info` against the metadata stream
(§4.4) began by recording the move of the entries 14.3.3 deprecates — which every one
of the nine samples carries, so every one of them grew a `Repaired` line. Carrying a
deprecated entry is not non-conformance and moving it loses nothing, so it is not a
decision; the disagreement that *does* lose something is, and that fires on one file.
Eight samples record nothing and `samples/fy05.pdf` records its one real ambiguity.
`metadata.rs` holds a test asserting exactly that, because the property is easy to
break from a distance.

When adding a decision point, check it against a conforming file as well as a broken
one. `./scripts/dev/status.sh` re-derives the site count above, so a figure that has
gone stale shows up as a disagreement rather than reading as current.

### 4.4 The Sublimation Pipeline: normalisation-at-load

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
  is recorded as a `Decision` (§4.3). When the cross-reference is unusable the file is
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

**What the model cannot hold is lost here, with no later stage to recover it.** That is
the price of normalising at load, and it is not hypothetical: the text decoder corrupted
a conforming `/Title` at this point, and the only reason output was ever right was that
the save path happened to overwrite the value from XMP. Changes to reading carry more
weight than their size suggests.

### 4.5 Unified Extension Architecture (Anti-Ad-Hoc Policy)

To prevent drift, ad-hoc struct additions and uncoordinated writer logic, a new backend
capability belongs in one of four domain namespaces owned by `fepdf-model` (and `fepdf-doc`):

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

#### 4.5.1 Multi-Format Provider Architecture

When introducing support for external document formats (Word `.docx`, Excel `.xlsx`,
SVG, HTML), each format follows Rule C by keeping its ingestion and emission in one
provider crate (`fepdf-import-docx`). Providers translate into the `Operation`
vocabulary or intermediate layout structures without exposing format-specific
dependencies to `fepdf-model`. See [§4.2](#42-document-sources) for what such a provider
actually owes.

### 4.6 Safety invariants

- **Handles, not pointers.** Objects are reached only through `Handle<Object>`,
  eliminating use-after-free and dangling references by construction.
- **Deterministic traversal.** `PdfArena` uses `BTreeMap` and indexed handle arrays
  throughout, so iteration order — and therefore produced bytes — is reproducible.
  RR-15 Rule 10 forbids `HashMap`/`HashSet` in the crates that decide output.
- **Zero unsafe.** `unsafe_code = "forbid"` across the workspace.

### 4.7 Rendering

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
