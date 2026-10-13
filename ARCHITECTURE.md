# fepdf — architecture

> **Not a rule.** This is the design as it stands. The rules that keep it that way are in
> [CODING.md](CODING.md) §2; how it came to be is in [docs/adr/](docs/adr/README.md).

Crate topology, the Sublimation Pipeline, and memory invariants. The design serves what
[README.md](README.md) says fepdf is: a translator from any PDF to ISO 32000-2, operations
on what it translated, and frontends for those operations.

> **Status.** Every crate in §3 exists, and Rule D holds — `status.sh` counts
> document-mutating methods on the facade and reads 0. This banner states only what a
> `status.sh` row derives
> ([ADR-0037](docs/adr/0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md)).

---

## 📐 1. Where the rules are

The five rules that decide where code goes — **A** storage abstractions stop at the
facade, **B** a crate defining a contract does not depend on its implementations, **C**
read and write live together, **D** frontends translate and never decide, **E** an
operation does not reach into an audit — are in [`CODING.md`](CODING.md) §2 with what
checks each. This document says what the design
*is*; a rule about structure is still a rule.

## 🗺️ 2. Topology

```
┌─ Frontends ─────────────────────────────────────────────────────────┐
│   fepdf-cli      fepdf-gui       fepdf-mcp       fepdf-wasm         │
│                      └──────┬───────┘                               │
│                      fepdf-script   library: ECMAScript → Operation │
└─────────────────────────┬───────────────────────────────────────────┘
                          │   ◄── Rule A boundary: no Arena / Handle above here
┌─────────────────────────▼───────────────────────────────────────────┐
│  fepdf            Public facade: Document, Page, SaveOptions        │
└─────────────────────────┬───────────────────────────────────────────┘
          ┌───────────────┼───────────────────┐
          ▼               │                   │
   ┌────────────┐         │                   │
   │ fepdf-audit│         │                   │
   │ Matterhorn │         │                   │
   │ judges only│         │                   │
   └─────┬──────┘         │                   │
         ▼                ▼                   ▼
   ┌────────────┐  ┌──────────────┐   ┌────────────────┐
   │ fepdf-doc  │  │ fepdf-content│   │  fepdf-render  │
   │ operations │  │ interpreter  │   │  Vello / wgpu  │
   │ reading    │  │ + Backend    │◄──┤  implements    │
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

   Beside the stack, for tests only: fepdf-fixtures — PDFs assembled by hand, a
   dev-dependency that depends on nothing (its `backend` feature reaches fepdf-content).
```

Dependencies flow strictly downward. `fepdf-render` is the one arrow that points *up*
into `fepdf-content`, because it implements a contract defined there — that is Rule B
working as intended, not a cycle. The facade reaches `fepdf-doc` directly as well, for the
operations; `fepdf-audit` stands between the two only in that it reads what `fepdf-doc`
reads, and nothing in `fepdf-doc` can name it (Rule E).

---

### 2.1 Directory layout

Where things live, and what may not move.

| Directory | Holds |
| :--- | :--- |
| `assets/` | Static, Read-only Resources (Fonts, Models) |
| `crates/` | Modular Rust Logic Layer |
| `docs/` | Technical Specs & Architectural History |
| `external/` | Submodules & Third-party Compliance Data |
| `crates/*/examples/` | Rust usage examples, each under the crate it exercises |
| `out/` | Ephemeral & Persistent Outputs (Ignored by Git) |
| `out/artifacts/`| Test results, renders, and temporary PDFs |
| `out/exports/` | Extracted document assets (Fonts, Images) |
| `samples/` | Test Input Corpus (PDFs) — **files this project chose**. Git-ignored |
| `fuzz/` | `cargo fuzz` targets, outside the workspace |
| `dist_release/`, `release_assets/` | Release packaging output. Git-ignored |
| `scripts/` | Automation & CI/CD Scripts |
| `target/` | Cargo's build directory, **and the external corpora**: 515 files this project did not choose, under `target/external/`, plus `encrypted/`, `malformed/`, `scans/`, `layers/` and `colour/`. Git-ignored, fetched by `scripts/test/fetch_external_corpus.sh` |

What may go where, and what checks it, is [`CODING.md`](CODING.md) §5. Tool-owned dotfile
directories (`.git/`, `.github/`, `.cargo/`, `.claude/`, `.gemini/`) are not listed: this
table is what the project puts in the tree, not what its tools do.

## 🧩 3. Crate Responsibilities

What each crate holds is one command away, and is not written down here
([ADR-0080](docs/adr/0080-a-design-document-does-not-carry-a-number-that-moves.md)):

```bash
for c in crates/*/; do
  printf '%-16s %s\n' "$(basename "$c")" \
    "$(find "$c/src" -name '*.rs' | xargs wc -l | tail -1 | awk '{print $1}')"
done
```

The weights worth knowing do not move: `fepdf-model` is the largest crate and holds the
document graph, the reader and the writer; `fepdf-doc`, the operations, and `fepdf-gui`,
the largest frontend, come after it; `fepdf-wasm` and `fepdf-macros` are thin enough to read in a sitting.
The function evaluator (7.10), the colour-space resolver (8.6) and the mesh decoder
(8.7.4.5.5 to 8.7.4.5.8) are in `fepdf-model`'s `src/function/`, `src/color/space.rs`
and `src/graphics/mesh.rs`.

| Crate | Status | Responsibility |
| :--- | :---: | :--- |
| **`fepdf-syntax`** | ✅ | The byte layer: lexing and encryption/decryption. Depends on no model type, which is what lets the cryptography be reviewed on its own. Parsing and stream filters are *not* here — see `fepdf-model` below. |
| **`fepdf-font`** | ✅ | Font *programs*: CFF, TrueType, CMap, Adobe Glyph List, subset tags, reconstruction. Hardened against W/W2 out-of-bounds, CMap underflows (`e_val >= s_val`), and CID byte truncations. |
| **`fepdf-model`** | ✅ | The document graph: `PdfArena`, `Handle<T>`, `Object`, page tree, metadata — and, since Phase A, the reader (7.5) and `writer.rs`. Hardened with pool overflow guards, cyclic `resolve` limits (`64`), and safe `Null` reference fallbacks. |
| **`fepdf-content`** | ✅ | Content-stream interpreter, and the **`RenderBackend` contract** it drives (`TextGlyph`, `TextState`, `SMaskData`, path geometry). No GPU dependency. |
| **`fepdf-doc`** | ✅ | Owns the **`Operation` vocabulary** (§4.1) and is its only interpreter (`apply/`). Builds the documents the facade hands back new — merged, extracted, or copied for a save (`assembly`, beside the `cloning` it uses) — and forgets what a removed page leaves named (`page_removal`). Holds the structure tree and its parent tree, remediation, reading order, measurement, and the two GPU-free `RenderBackend`s. |
| **`fepdf-audit`** | ✅ | Judges a document against the Matterhorn Protocol (ISO 14289) and changes nothing: the conditions it decides (`audit_*`, `structure`), the ones it leaves to a person in the protocol's words (`matterhorn`), and the glyph and language readers the font audit uses. Stands above `fepdf-doc` and reads through it, so an operation cannot reach into an audit (Rule E). |
| **`fepdf-render`** | ✅ | A `RenderBackend` implementation on **Vello** + **wgpu**. Reached only through the facade's optional `render` feature. |
| **`fepdf`** | ✅ | The public facade: `PdfDocument`, `SaveOptions`, `Operation`. It is the Rule A boundary in fact — frontends depend on it and on nothing below. It holds documents and makes no arena: a new one is built in `fepdf-doc` (`layering.py` counts `arenas=`). |
| **`fepdf-cli`** | ✅ | Command-line binary (`fepdf`). |
| **`fepdf-gui`** | ✅ | Desktop application on **egui** + **eframe** + **wgpu**. |
| **`fepdf-mcp`** | ✅ | Model Context Protocol server for AI assistants. **The most complete frontend**: a tool is the serialised form of an operation (§4.1), so it is meant to name every `Operation` variant as a tool of its own. `status.sh` counts how many it names against the enum itself; one it does not name is still reachable through the generic `apply_operation` tool, without a schema saying it exists. An engine error reaches the client as the call's — a refusal, a missing name, a malformed document — or as the server's, for a fault of the engine ([ADR-0102](docs/adr/0102-an-error-says-whose-it-is.md)). |
| **`fepdf-wasm`** | ✅ | WebAssembly bindings for what needs no GPU: it opens a document, counts its pages, extracts a page's text, and hands over the decision log and the structure tree as JSON. `render_page` **returns an error** naming what it did not draw — it used to return `Ok(())` having drawn nothing, so a caller was told it succeeded and got a blank canvas. It constructs no `Operation` at all, which is why the §4.1 diagram no longer lists it as a frontend that does. It **compiles for `wasm32-unknown-unknown`**: `getrandom` arrives through the crypto stack and is declared with its `js` feature for that target. Verify with `cargo build -p fepdf-wasm --target wasm32-unknown-unknown`. |
| **`fepdf-script`** | ✅ | A library the frontends call ([ADR-0082](docs/adr/0082-the-script-crate-is-a-library-the-frontends-call.md)): ECMAScript (12.6.4.16) on **boa**, translating into `Operation` as the frontends translate argv, a button press and a tool call. Depends on the facade and nothing else, so it is **not** wired into `fepdf` behind a feature — that would be a cycle ([ADR-0031](docs/adr/0031-a-script-frontend-cannot-be-a-facade-feature.md)). A caller who does not depend on it links none of the crates boa brings. **ECMA-402 is refused rather than approximated**: a script naming a locale gets an error and a `Decision`. boa's `intl` feature was built and measured before this was settled — it has no `Intl.DateTimeFormat.prototype.format` and no currency style, which are the two things a form asks ECMA-402 for ([ADR-0034](docs/adr/0034-intl-is-declined-for-what-it-does-not-do.md)). |
| **`fepdf-macros`** | ✅ | Compile-time procedural macros. |
| **`fepdf-fixtures`** | ✅ | PDFs assembled by hand, for tests and examples across the workspace. A dev-dependency only, and it depends on nothing by default: a fixture the writer produced could not catch a writer defect. |

Two `RenderBackend` implementations besides the GPU one — `TextExtractionBackend` and
`CollectorBackend` — sit alongside the operations, in `fepdf-doc`. Neither pulls in a GPU,
which is exactly what Rule B makes possible.

---

## 🛡️ 4. Cross-Cutting Concerns

### 4.1 The operation vocabulary

Every document mutation is a value of one type, defined in `fepdf-doc` and re-exported
through the facade. Frontends construct it; only `fepdf-doc` interprets it.

```
   fepdf-cli    argv          ─┐
   fepdf-gui    button press  ─┤
   fepdf-mcp    tool call     ─┼─►  Operation  ─►  fepdf-doc::apply
   fepdf-wasm   —             ─┘     (a value)      (the only implementation)
                                                      and the only way in
```

The variants are listed by the enum itself, not here
([ADR-0080](docs/adr/0080-a-design-document-does-not-carry-a-number-that-moves.md)): a
list written beside the code once named nine variants that did not exist, and a later one
showed thirty of sixty-three while saying it was derived. How many there are, and how many
each frontend constructs, are `./scripts/dev/status.sh`'s `operations …` rows.

```bash
sed -n '/^pub enum Operation {/,/^}/p' crates/fepdf-doc/src/operation.rs | grep -oE '^    [A-Z][A-Za-z]*'
```

Where two readings of an argument lead to different documents, the type makes the caller
choose: `RotateMode` is `Absolute` or `Relative`, and `Quarter` makes 45° unconstructible.

**What follows from it, rather than being designed in:**

- **Undo/redo.** Operations are values, so they are recorded and replayed:
  `fepdf-gui` keeps the bytes it opened alongside the operations applied to them, and
  takes one back by opening those bytes again and replaying the rest. The GUI gets
  history without a parallel mechanism.

  **They are not inverted.** Nothing in the engine inverts an operation, and for two of
  them there is no inverse to write: `Retag` rebuilds the structure tree from
  heuristics, and `ApplyBatesNumbering` draws into content streams. Two verbs of the
  three are enough, at a cost of one open per undo — between 28ms and 1.7s across the
  samples, which is why the window says that a rebuild is running.

  ```bash
  grep -rn "fn invert\|fn inverse\|fn undo" crates/fepdf-doc/src crates/fepdf-model/src \
      crates/fepdf/src --include='*.rs' | wc -l    # 0
  ```
- **The only way in, and all of it or none.** The facade seals a document's arena once
  it is opened, and `apply` alone unseals it, for the one operation: a write while
  sealed panics in a debug build, so a path round `apply` fails the tests that take it
  ([ADR-0109](docs/adr/0109-a-document-changes-only-inside-apply.md)). While unsealed,
  the arena records what each write replaces, and an operation that fails is put back
  whole — the arena, the page list, the decisions it recorded
  ([ADR-0110](docs/adr/0110-an-operation-that-fails-changes-nothing.md)). There is no
  exception: redaction is `Operation::Redact`. A copy made to be saved is another arena
  and is never sealed.
- **MCP tool surface.** A tool becomes the serialised form of an `Operation`. New
  operations reach AI assistants without new bridging code.
- **Testability.** An operation sequence can be applied and asserted without starting
  a GUI or spawning a process.

**Rule D holds because the facade offers nothing else.** It has no mutating method but
`apply` and the save settings, so a frontend has nothing to reach for — the alternative is
unrepresentable rather than discouraged — and what checks it is in `CODING.md` §2. Logic
that once sat in facade methods, such as page duplication and insertion, lives in
`fepdf-doc` beside `ObjectCloner`, and operations that take a page set own the order of
their own edits: inserting clones ascending would otherwise duplicate page 0 three times,
because after the first insertion the remaining indices name clones.

**What this is not.** The GUI keeps its worker thread: `WorkerRequest` remains, but as
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
vocabulary that was once mostly stubs, and ingestion options nothing reads
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

Those choices are therefore **recorded, not logged**. The record is how a translation
accounts for itself, and it serves the translation rather than standing beside it. `Document::decisions` carries a
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

**How many sites there are is `./scripts/dev/status.sh`'s `Decision sites in the engine`
row**, and this section does not state it
([ADR-0080](docs/adr/0080-a-design-document-does-not-carry-a-number-that-moves.md)).
Derive it with:

```bash
grep -rn "Decision::violation(\|Decision::ambiguity(\|Decision::repaired(" \
    crates/*/src --include='*.rs' | wc -l
```

The row takes its crates from the workspace, as the log row above does, so the two are
complements and a new crate is counted by both.

**A site is not a firing, and which sites can fire depends on the command.** Over the
525 files of both corpora, `inspect structure` reports 16 decisions in total (measured
2026-10-06), from five clauses and all of them the reader's: 7.5.4, 7.5.7 and 7.5.8 four
each, and 7.3.8.2 and 7.5.2 two each. The font, decryption and colour sites contribute
nothing to that figure, because inspecting the structure does not run those paths — not
because they are dead. `isartor-6-3-2-t01-fail-b.pdf` makes the difference visible: `inspect
structure` reports no decision on it, and `inspect text` on the same file reports a
`Violation` of 9.9, a font program in no recognised format, skipped for a system font.
So `is_conforming` answers "no departure **in what has been examined**", and the log is
only as complete as the work the caller asked for. That is now the documented meaning
rather than an accident of which command was run: the log is behind a lock, a page being
interpreted can add to it, and `inspect text` prints what reading decided before the
text and what interpreting decided after it (ADR-0018).

**Two `log::warn!`/`log::error!` sites remain in the engine, and both are deliberate.**
They report properties of the *host*: which fonts this machine has
(`fepdf-model/src/font/gid_resolution.rs`), and the GPU failing to initialise so the CPU
renderer takes over. `status.sh` counts them.

**A log site is a `Decision` when it is a conclusion about the document, and only then.**
Thirteen candidates were measured against the nine conforming samples before any was
converted; three turned out to be conclusions about the *engine* instead, and one was a
third copy of a test that already existed. What each became, and the counts that decided
it, are in [ADR-0028](docs/adr/0028-four-of-the-thirteen-logs-were-not-decisions.md).

**A backend cannot record for itself**, because it sits below any `Document`: it is handed
paths and glyphs, not a file. `RenderBackend::take_decisions` is defaulted to empty and
`render_page` drains it after the annotations, so the two `fepdf-render` sites — a glyph
whose outline the font program will not yield, and a font the interpreter selected that
never reached the cache — are recorded against the document that caused them. The drain was
verified by injecting a decision into the backend and watching it arrive.

**A decision that fires on conforming input is worse than none**, because it makes the
log a constant rather than a signal. Reading an indirect `/Length` — which 7.3.8.2
permits — was recorded as an `Ambiguity`, so `samples/constitution.pdf` reported 31
departures and `is_conforming` returned `false` for a clean file
([ADR-0008](docs/adr/0008-an-indirect-length-is-not-an-ambiguity.md)).

The same rule governs settling `/Info` against the metadata stream (§4.4). Moving an entry
14.3.3 deprecates is not non-conformance and loses nothing, so it records nothing; the
*disagreement* between the two places does lose something, so it records an `Ambiguity`.
Eight of the ten samples therefore record nothing here, and `samples/fy05.pdf` and
`samples/02_低段汚水ポンプ電動機.pdf` record one each (measured 2026-10-06). `metadata.rs`
holds a test of both cases.

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

What each feature 2.0 deprecates is translated to — at load, in the interpreter, on save,
or not at all — is one table, [pdf-2-0-deprecations.md](docs/specs/pdf-2-0-deprecations.md),
with what the engine does now beside each rule.

Pass 1 no longer exists. It converted another library's object model into ours; the
reader now produces the arena directly
([ADR-0003](docs/adr/0003-lopdf-was-not-providing-robustness.md)).

**A `Document` is therefore one normalised state, not the file.** Everything above
happens before application code sees anything, so by the time a `Document` exists the
revision chain has been merged, the ciphertext is gone, and the metadata has one
answer. Nothing later can put those back
([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)).

**Fonts are the exception, and it is a live one.** Those three are the whole of what that
sentence covers. The font cache is **empty on every sample when `open` returns** —
`normalize_resources` clears what the ingest pass built — and every font is rebuilt during
rendering by `Interpreter::get_font`, which merges a Type0's descendant into it and runs
reconstruction again before writing the result back. A document's font state therefore
settles at first draw, not at load, and the resource that decodes text is not the one whose
decisions the reading log carries. Fixing the load-time copy alone moves nothing, which
cost [ADR-0041](docs/adr/0041-a-character-collection-is-declared-not-guessed.md) an
attempt. Measured by `cargo run -p fepdf --example load_state_probe -- samples/*.pdf`,
recorded in [ADR-0045](docs/adr/0045-normalisation-at-load-does-not-reach-fonts.md), owed
by [ROADMAP Phase T](ROADMAP.md).

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

The catalogue's XMP packet is one place where the model holds only part of what the file
says. It is rebuilt from the fields `MetadataInfo` models. Everything else in it, such as
a standard's identification, a PDF Declaration or an extension schema, is carried into the
rebuilt packet as written
([ADR-0099](docs/adr/0099-the-xmp-packet-carries-what-the-engine-does-not-write.md)).

### 4.5 Safety invariants

- **Handles, not pointers.** Objects are reached only through `Handle<Object>`,
  eliminating use-after-free and dangling references by construction.
- **Deterministic traversal.** `PdfArena` uses `BTreeMap` and indexed handle arrays
  throughout, so iteration order — and therefore produced bytes — is reproducible.

What keeps these true — no `unsafe`, no `HashMap` in the crates that decide output — is
RR-15 Rules 3 and 10 in [`CODING.md`](CODING.md).

### 4.6 Rendering

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
