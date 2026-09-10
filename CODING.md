# 💻 fepdf Coding Standards & Hardening Protocol

> **Phase: implementation.** What code must satisfy. What to decide first is in
> [PLANNING.md](PLANNING.md); what to run afterwards is in [TESTING.md](TESTING.md).

This document defines the coding conventions, safety standards (**RR-15 Protocol**), and architectural patterns required across all crates in the fepdf workspace.

---

## 🛡️ 1. The RR-15 Hardening Rules

Derived from aerospace safety principles, the **RR-15 (Reliable Rust-15)** rules guarantee determinism, memory safety, and absolute runtime reliability.

### Rule Summary Matrix

| Rule | Area | Requirement | Enforcement |
| :--- | :--- | :--- | :--- |
| **Rule 1** | Function Length | Max 50 lines for standard functions.<br>Max 200 lines for `// RR-15 Limit: GUI`.<br>Max 500 lines for `// RR-15 Limit: Dispatcher`. | `./scripts/audit/verify_compliance.sh` |
| **Rule 2** | Panic Prevention | `unwrap()` and `expect()` are forbidden in production code. Use `?` or `unwrap_or()`. | Automated grep check |
| **Rule 3** | Unsafe Ban | `unsafe` blocks are forbidden. | **rustc** — `unsafe_code = "forbid"`, which cannot be overridden by an `#[allow]` |
| **Rule 4** | Control Flow | Avoid deep nesting (`if let` / `match`). Prefer early return with `?`. | Code review / Clippy |
| **Rule 5** | Match Exhaustiveness | Wildcard arms (`_ =>`) are forbidden when matching a **domain enum**. Named exceptions below. | `clippy::wildcard_enum_match_arm` via `verify_compliance.sh` |
| **Rule 6** | Stack Safety | Unbounded recursion is forbidden. Use heap-based loops with `Vec`. | `scripts/audit/unbounded_recursion.py` via `verify_compliance.sh`, for walks over a document's graph. Detail below |
| **Rule 7** | Global State | `static mut` and global mutable state are forbidden. | **rustc** — reading a `static mut` needs an `unsafe` block, which Rule 3 forbids |
| **Rule 8** | Invalid State | Use type-safe `enum` states instead of boolean flags or nested `Option`s. | Architecture review |
| **Rule 9** | Pure Rust | No dependency may compile C or C++ source, or bind a third-party native library. Platform API bindings the standard library already needs are not this. | `./scripts/audit/verify_compliance.sh` |
| **Rule 10** | Determinism | `HashMap` and `HashSet` are forbidden in core pipelines. Use `BTreeMap`, `BTreeSet`, or `PdfArena`. | Automated grep check |
| **Rule 11** | Error Transparency | Return typed `thiserror` enums. String-based errors (`Result<T, String>`) are forbidden in core APIs. | Automated grep check |
| **Rule 13** | Error Swallowing | `filter_map(Result::ok)`, and any `let _ =` over a call that can fail, are forbidden. | Grep, plus `scripts/audit/discarded_results.py` via `verify_compliance.sh`. Detail below |
| **Rule 14** | Test Code Separation | Standalone/Integration tests MUST be placed in `crates/*/tests/`. Do NOT pollute `src/` with dedicated test files. | Directory structure check |
| **Rule 15** | Clone Optimization | Avoid excessive `.clone()`. Use `Arc` or handle references where appropriate. | Code review / Density warning |
| **Rule 16** | Licences | Every dependency's licence must be on `deny.toml`'s allow-list. | `cargo deny check licenses` via `verify_compliance.sh` |
| **Rule 18** | Secrets and PII | No credential, key or personal datum may be committed. | `betterleaks` via `verify_compliance.sh` and a pre-commit hook |
| **Rule 19** | Formatting | The tree must satisfy `cargo fmt --all --check`. | `./scripts/audit/verify_compliance.sh` |
| **Rule 20** | Recorded Interpretation | Where the engine accepts input the standard does not describe, it MUST record a `Decision` naming the clause and what was done. A silent acceptance is a defect even when the output is right. | Code review / ARCHITECTURE.md §4.3. `scripts/audit/silent_branches.py` via `verify_compliance.sh` counts the arms and gates the one verdict it has — a stale exemption. Detail below |

**Two numbers are unused: 12 and 17.** Rule 12 was Invariant Enforcement and left the
table while the practice stayed in the code; Rule 17 required a float suffix and was
retired on 2026-08-29 because nothing it prevented could reach a build —
[ADR-0040](docs/adr/0040-a-rule-the-compiler-already-keeps-is-not-a-rule.md). The numbers
are left unused rather than reassigned, because reassigning one is what made Rules 9 and
14 mean two different things.

Two more things this table does not say for itself, recorded in
[ADR-0037](docs/adr/0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md)
rather than here: **Rule 12** was Invariant Enforcement and is gone from this table, while Rules 9 and 14
name different rules than RR-15's original fifteen did; **RR-15** is a name, not a count — the table
is 1–11 and 13–20.

### Rule 9 in detail: where the line is

"Pure Rust" cannot mean "no FFI", because `std` links libc and every program on every
platform reaches the operating system through a C ABI. A rule that forbade that would
forbid the language.

The line that *is* checkable, and the one that matters, is **whether a build compiles
foreign source**. A dependency that ships a C library and builds it takes a compiler, a
cross-compilation story, and a body of code that no Rust tool audits — `cargo clippy`,
the `unsafe` ban and RR-15 all stop at the language boundary. So:

| | Example | |
| :--- | :--- | :--- |
| **Forbidden** | `zstd-sys`, `libz-sys`, `openssl-sys`, `ring`, a QuickJS binding | Compiles vendored C; pulls `cc` as a build dependency |
| **Allowed** | `core-foundation-sys`, `windows-sys`, `libc` | Declares the platform's own API, which `std` already does |

Enforced as: **no crate named `cc` in any workspace member's dependency tree, on any
target this engine is built for**. `cc` is how a Rust build compiles C, and nothing
compiles C without it.

**Four targets, named.** The check reads `x86_64-unknown-linux-gnu`,
`x86_64-pc-windows-msvc`, `aarch64-apple-darwin` and `wasm32-unknown-unknown`; a
dependency tree is not one tree, and adding a target to that list is a claim that the
engine is built for it.

**One exemption**: `fepdf-gui` compiles C on Linux, through Wayland's build shim
(`rfd` → `ashpd`, and `eframe` → `winit` → `smithay-client-toolkit`). It is recorded in
`deny.toml`, scoped to that crate and that target
([ADR-0033](docs/adr/0033-the-linux-gui-keeps-wayland-so-rule-9-names-one-exemption.md)).
Any other `cc` in any tree fails the audit.

### Rule 6 in detail: what a checker can and cannot see

This column said **Code review** until 2026-09-06, and review missed five walks in one
week. A four-object file crashed `inspect catalog`
([ADR-0060](docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md));
`DeleteStructElem` and `SetFormFieldValue` aborted on a cyclic `/K` or `/Kids`
([ADR-0061](docs/adr/0061-four-walks-bounded-and-two-that-were-not-what-the-sweep-said.md));
the writer's page walk was unreachable only because the reader expanded a cyclic page tree
before it
([ADR-0062](docs/adr/0062-a-page-tree-that-is-not-a-tree-is-reported-not-expanded.md)).
The throwaway detector used for those sweeps produced two false positives of its own.

`scripts/audit/unbounded_recursion.py` finds cycles in one file's call graph and reports
those where no participant carries a depth, a visited set or a worklist. **Its first run
found a sixth walk none of the sweeps had**: a Type0 font whose `/DescendantFonts` names
itself aborted `inspect audit` on five objects.

### Rule 13's other half

`let _ = f();`, where `f` returns a `Result`, is the same swallow as
`filter_map(Result::ok)` written as a binding, and the grep cannot see it.
`scripts/audit/discarded_results.py` covers it.

`clippy::let_underscore_must_use` was tried first and is not used. It has the type
information the script lacks, and reports 99 sites — the great majority a `write!` into a
`String`, which cannot fail, or an `mpsc` `send` whose receiver has hung up, which means
the GUI is closing. It was 97 when the script was written; the shape of the ratio is what
matters, not the count, which is why the script names the two benign forms rather than a
number. A check that is 89% noise is a check nobody reads, so the script
names those two shapes as benign and requires a written reason for everything else, in
`ACCOUNTED_FOR`. An entry naming a line that no longer discards anything fails as stale,
so the list cannot become a permanent exemption.

**It reports one class and counts two others**, because they are different risks:

| | |
| :--- | :--- |
| **Reported** — a walk over a document's own graph | An arena, a `Handle`, a `Dict`. A file can make these into a loop, and nothing but a guard stops it |
| **Counted** — a walk over an owned Rust structure | A `Vec<Node>` the program built cannot be cyclic; Rust will not allow it without an `Rc`. The depth is however deep the thing was built |
| **Exempt** — named, with the guard given | Eight, all one guard: these follow *direct* nesting and not `Object::Reference`, and `Parser` refuses past 512 levels |

**Where it does not reach**, and so what the exemption list is for: recursion through a
trait object or a closure, and mutual recursion split across two files. It also had to be
taught that `a.cmp(b)` inside `fn cmp` is not recursion and that `Vec::new()` inside
`fn new` is not either — accepting any capitalised path put eighty cycles in its first run
and made it useless, which is the same failure the throwaway detector had.

It exits non-zero on an unguarded, unexempted cycle, and on an exemption naming a function
that is no longer recursive.

### Rule 5 in detail: what "no wildcards" can and cannot mean

The point of Rule 5 is that **adding a variant must break the build at every place
that decides on it**, rather than silently falling into a catch-all. That property is
only achievable — and only worth anything — for enums we own and expect to grow.

A blanket ban is not implementable. Matching on `&str`, `u8` or `usize` *requires* a
wildcard, because the domain is open. There are 193 syntactic `_ =>` arms in this
workspace (2026-08-16), and how many are of that kind cannot be established by reading
the text — which is the argument, not a gap in it. Telling the two apart needs to know
what the scrutinee's type is. So enforcement uses `clippy::wildcard_enum_match_arm`,
which has that information and fires only on enums.

**Where it does not reach.** A PDF's domain values arrive from a file as integers —
`/ShadingType`, `/V`, `/LC`, `/LJ` — and a `match` on an integer needs a
wildcard, so the lint cannot see them. Measured 2026-08-29: 30 such matches, of which 11
answer an unrecognised value with a default or `None` and no `Decision` — `/LC 7` becomes
a butt cap, `/LJ 7` a mitre join. Rule 20 is what covers this ground.

`scripts/audit/silent_branches.py` is what looks at it, and **the count it prints is not
gated, on purpose**: an unknown `/V` makes the document fail to open, which is loud enough
without a `Decision`, so a new arm is a question rather than a defect. What the audit does
gate is the tool's own verdict — an exemption naming a site that no longer matches a
silent arm, which reads as a check still being made and is not one. Until 2026-09-08 only
`status.sh` ran the tool, and nothing read its exit code, so that verdict was reachable
and unread.

**Those three were recorded first**, on 2026-08-30: `LineCap::from_i64`, `LineJoin::from_i64`
and `TextRenderingMode::from_i64` return `Option` instead of a default, so each of their
call sites — the interpreter, the content-stream pre-parse, and since 2026-09-06 the
`/LC` and `/LJ` an `/ExtGState` carries — has to say what it substituted, and each records
a `Violation` naming the table. `scripts/audit/silent_branches.py` took the count from 11
to 8 that day and **to 0 on 2026-08-31**, when the remaining eight were audited and each
registered against the caller that records for it. It now reads 0 with 11 exemptions, and
exits non-zero if an exemption names a site that has gone. No file in either corpus
presents such a value — 0 of 524 — so this reports rather than counts, and a fixture is
what proves it fires.

**Forbidden** — wildcard arms over domain enums such as `ColorSpaceKind`,
`SublimatedData`, `Color`, `PixelFormat`, and any enum added from here on. These gain
variants as features land, and a catch-all turns "unsupported colour space" into
"silently renders black".

**Exempt** — the following are named in `verify_compliance.sh`:

| Type | Why |
| :--- | :--- |
| `Object`, `Token`, `Command`, `IrObject`, `RefinedObject` | Mirror the ISO 32000-2 object and operator taxonomy, whose variant set is fixed by the standard. They are matched at dozens of sites that care about one or two variants; spelling out all 11 `Object` variants at each would add ~220 lines and push functions past the Rule 1 limit for no safety gain. |
| `syn::Data`, `syn::Fields` | Owned by an external crate and `#[non_exhaustive]`. Exhaustive matching is impossible. |

`Self` is exempt only in the three files whose `match self` is over an exempt type,
listed explicitly in the script. A `match self` on a new domain enum anywhere else
still fails the audit.

Adding a type to the exemption list is a deliberate act: it belongs in this table with
its reason, not as an inline `#[allow]`.

---

## 🧱 2. The layering rules

Where code goes. These were `ARCHITECTURE.md` §1 and §7 until 2026-08-29, on the reading
that a rule about structure is architecture; they are here because they are rules, and
`ARCHITECTURE.md` says what the design *is*. Its own charter had said it holds no coding
rules while it held these four.

Four rules decide where code goes. They are what keeps the topology from eroding;
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
(see [`ARCHITECTURE.md` §4.1](ARCHITECTURE.md#41-the-operation-vocabulary)). A frontend's job is to turn argv, a button
press, an MCP call or a JS call into that value and hand it over. It never implements
the operation itself.

Where two frontends each implement "the same" operation, the two implementations
drift, silently, because nothing compares them. That has already happened here
([ADR-0005](docs/adr/0005-layering-rules-are-enforced-by-cargo.md)).

---

### What checks them

| | Checked by |
| :--- | :--- |
| **Rules A–C** | Cargo, through [`scripts/audit/layering.py`](scripts/audit/layering.py) in the audit. A frontend declares `fepdf` and a library that stands above it ([ADR-0082](docs/adr/0082-the-script-crate-is-a-library-the-frontends-call.md)); no arena type appears above the facade at all. Both expect 0, and the audit fails on either. `status.sh` reports what that script returns. **It reported them and nothing gated on either until 2026-09-07**, when a frontend gained a dependency the row counted and the audit passed regardless. |
| **Rule D** | [`scripts/audit/layering.py`](scripts/audit/layering.py) in the audit, with Rules A–C. It counts `&mut self` methods on the facade that are neither `apply` nor a save setting, expects 0, and fails on any. **Measured here and gated nowhere until 2026-09-07**, which is the shape that let Rule A read 1 with the audit passing ([ADR-0082](docs/adr/0082-the-script-crate-is-a-library-the-frontends-call.md)). |
| **RR-15** | [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh) |
| **Lints** | `cargo clippy --workspace --all-targets -- -D warnings`. `--all-targets` is required, or tests, examples and benches go unlinted. |
| **Licences** | `cargo deny check licenses` ([`deny.toml`](deny.toml)) |
| **Secrets and PII** | `betterleaks`, pre-commit and in the audit ([`.betterleaks.toml`](.betterleaks.toml)) |

Rule D read "enforced by construction" for four phases while ten facade methods went
round it, and the tell was that its row named no tool where every other row did. **Do not
write "by construction" without naming what would notice.**

`fepdf-render` declares `fepdf` as a dev-dependency for its own tests. That is not the
cycle it looks like: it is absent from the build graph of anything that links
`fepdf-render`.

## 🖼️ 3. The interface rules

> **Phase: implementation, and the window is code too.** Why these exist and why they are
> not a document of their own is in
> [ADR-0084](docs/adr/0084-the-gui-gets-rules-not-a-rulebook.md), which records the two
> GUI documents this repository has written and deleted.

### The principles they come from

A rule answers *is this allowed*. A principle answers *both are allowed, so which*. Each
of these rejects something a reasonable person would otherwise do, which is the only test
that keeps one from being decoration.

| | | It rejects |
| :--- | :--- | :--- |
| **P1** | Make it reversible before you make it reachable | "a confirmation dialog makes it safe" — a confirmation slows the action down; it does not undo it |
| **P2** | Show it before you let it be edited | "the engine has an `apply` for it, so put it in the GUI" |
| **P3** | An entry point nobody can find is not an entry point | a menu-less design, and a naive reading of minimalism |
| **P4** | Run the product's own checks over the product's own window | "the interface is outside the product" |

They apply in that order: **reversible → visible → findable**, which is also the order to
build them in. Reversing it makes each worse — a discoverable delete that cannot be undone
is worse than a hidden one, and an entry point onto something invisible is a button that
appears to do nothing.

**Where the interface and the look of it disagree, the interface wins.** Discoverability
is not traded for simplicity. The design document ADR-0084 records made that trade, and
what it bought is a window whose editing operations are mostly unreachable.

Nothing checks the four. They sit here rather than in `AGENTS.md` because the rules below
are derived from them and a derivation is worth reading in one place.

### Rule Summary Matrix

`UI-` rather than a number continued from RR-15: those are aerospace-derived safety rules
and these are not, and reassigning a number is what made Rules 9 and 14 mean two things.

| Rule | Area | Requirement | Enforcement |
| :--- | :--- | :--- | :--- |
| **UI-1** | Icon vocabulary | Every codepoint drawn as an icon resolves to a glyph that draws, in the font intended for it, and is declared in `app/icons.rs` | `scripts/audit/icon_glyphs.py` via `verify_compliance.sh` |
| **UI-2** | Accessible name | A widget whose only content is a glyph carries a name by some other means | **nothing** |
| **UI-3** | Notice typing | A success and a failure do not share a type | **rustc** — `Notice::done` is the only way to say a thing worked |
| **UI-4** | Reachability | No feature lacks a visible entry point. A shortcut and the command palette are shortcuts, not entry points | **nothing** |
| **UI-5** | Localisation | No user-facing string literal in the source; all through the locale keys | **nothing** — `locale.rs` holds the two key sets equal, which is a different claim |
| **UI-6** | Reversibility | An operation that changes the document can be undone | **nothing** |
| **UI-7** | Progress | Work over ~100ms says that it is happening | **nothing** |
| **UI-8** | Contrast | Body text 4.5:1; a non-text boundary that carries meaning 3:1 (WCAG 1.4.11) | **nothing** — computable from `theme::colors`, unimplemented |
| **UI-9** | Colour source | A colour is written in `app/theme.rs` or it is not written | `scripts/audit/palette.py` via `verify_compliance.sh` |
| **UI-10** | One accent | Rust marks what the reader is touching, and marks nothing else | **nothing** |
| **UI-11** | Dimensional tokens | Spacing, type size and corner radius come from the declared scales | **nothing** |
| **UI-12** | One home per action | An action belongs to one surface; the others are shortcuts to it | **nothing** |
| **UI-13** | Layout grid | Chrome stands on the 4pt grid; the page keeps the 72pt one | a build-time `assert!` over the tokens; the rest **nothing** |

**Ten of thirteen say "nothing", and that is the honest state rather than an omission.**
Rule 4 above requires the word to be written where nothing checks — and UI-6 and UI-7,
which are the two the measurements rank highest, are among the ten.

### The three vocabularies

Every value the window is built from lives in `crates/fepdf-gui/src/app/theme.rs`, in one
of three groups, and a widget may not invent a fourth.

| | | |
| :--- | :--- | :--- |
| **Colour** | 紙 `paper` — surfaces | four grades of one white ground |
| | 鋼 `steel` — lines and letters | `TEXT` `MUTED` `EDGE` `RULE`, where `EDGE` is any boundary that carries meaning and `RULE` is decoration |
| | 錆 `rust` — the one accent | what the reader is touching; nothing else (UI-10) |
| | `note` — what to do about it | `PASS` `INFO` `WARN` `FAIL`, each at least 26° of hue from the accent |
| **Dimension** | space | `ITEM` `GROUP` `SECTION` `PANE` — the step says how big the break is |
| | text | `SMALL` `BODY` `HEAD` `TITLE` |
| | radius | `FLAT` for a plane, `CONTROL` for anything clickable |
| **Layout** | chrome | the 4pt grid, and one click-target size |
| | canvas | the 72pt grid, which `app/layout.rs` already held |

**An overlay drawn on the page may not rely on its hue.** The document's colours are the
document's — a drawing printed in orange is as likely as one in black — so an overlay is
told apart by shape, by the word written on it, or by sitting outside the sheet, and it
carries a paper-coloured halo so it reads on any ground (`theme::canvas`).

---

## 🏛️ 4. What code must satisfy elsewhere

This document says what code must satisfy. The design it satisfies lives in
`ARCHITECTURE.md`, and repeating it here is how the two came to disagree: this section
described a **Pass 1 (Arena Ingestion)** that [ADR-0003] removed when the reader stopped
converting another library's object model, and it had said so for as long as the reader
had been fepdf's own.

- **The Sublimation Pipeline** — `ARCHITECTURE.md` §4.4. A `Document` is the normalised
  state, not the file; the file is reached through the byte layer named in the same
  section.
- **`PdfArena` invariants** — `ARCHITECTURE.md` §4.6. Objects are reached through
  `Handle<Object>`, never a pointer or a raw index, and traversal is deterministic
  (Rule 10 above is the enforced half of this).
- **Rendering and the GUI** — `ARCHITECTURE.md` §4.7. Vello compute shaders on wgpu,
  `f64` preserved through path snapping and measurement, CJK font loading and
  English/Japanese localisation in `fepdf-gui`.

[ADR-0003]: docs/adr/0003-lopdf-was-not-providing-robustness.md
