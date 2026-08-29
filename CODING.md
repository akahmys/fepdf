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
| **Rule 3** | Unsafe Ban | `unsafe` blocks are forbidden (`workspace.lints.rust.unsafe_code = "forbid"`). | Rustc lint |
| **Rule 4** | Control Flow | Avoid deep nesting (`if let` / `match`). Prefer early return with `?`. | Code review / Clippy |
| **Rule 5** | Match Exhaustiveness | Wildcard arms (`_ =>`) are forbidden when matching a **domain enum**. Named exceptions below. | `clippy::wildcard_enum_match_arm` via `verify_compliance.sh` |
| **Rule 6** | Stack Safety | Unbounded recursion is forbidden. Use heap-based loops with `Vec`. | Code review |
| **Rule 7** | Global State | `static mut` and global mutable state are forbidden. | Automated grep check |
| **Rule 8** | Invalid State | Use type-safe `enum` states instead of boolean flags or nested `Option`s. | Architecture review |
| **Rule 9** | Pure Rust | No dependency may compile C or C++ source, or bind a third-party native library. Platform API bindings the standard library already needs are not this. | `./scripts/audit/verify_compliance.sh` |
| **Rule 10** | Determinism | `HashMap` and `HashSet` are forbidden in core pipelines. Use `BTreeMap`, `BTreeSet`, or `PdfArena`. | Automated grep check |
| **Rule 11** | Error Transparency | Return typed `thiserror` enums. String-based errors (`Result<T, String>`) are forbidden in core APIs. | Automated grep check |
| **Rule 13** | Error Swallowing | `filter_map(Result::ok)` and silent error swallowing are forbidden. | Automated grep check |
| **Rule 14** | Test Code Separation | Standalone/Integration tests MUST be placed in `crates/*/tests/`. Do NOT pollute `src/` with dedicated test files. | Directory structure check |
| **Rule 15** | Clone Optimization | Avoid excessive `.clone()`. Use `Arc` or handle references where appropriate. | Code review / Density warning |
| **Rule 16** | Licences | Every dependency's licence must be on `deny.toml`'s allow-list. | `cargo deny check licenses` via `verify_compliance.sh` |
| **Rule 17** | Type Explicitly | Explicitly specify floating-point types (`1.0_f32`, `2.5_f32`) to prevent Edition 2024 inference fallbacks. | **Nothing. See below.** |
| **Rule 18** | Secrets and PII | No credential, key or personal datum may be committed. | `betterleaks` via `verify_compliance.sh` and a pre-commit hook |
| **Rule 19** | Formatting | The tree must satisfy `cargo fmt --all --check`. | `./scripts/audit/verify_compliance.sh` |
| **Rule 20** | Recorded Interpretation | Where the engine accepts input the standard does not describe, it MUST record a `Decision` naming the clause and what was done. A silent acceptance is a defect even when the output is right. | Code review / ARCHITECTURE.md §4.3 |

Three things this table does not say for itself, recorded in
[ADR-0037](docs/adr/0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md)
rather than here: **Rule 12** was Invariant Enforcement and is gone from this table, while Rules 9 and 14
name different rules than the rulebook does; **Rule 17** is enforced by nothing, and `verify_compliance.sh`'s `[Rule
17]` is a different rule sharing the number; **RR-15** is a name, not a count — the table
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

**The targets are named, because a dependency tree is not one tree.** The check used to
run `cargo tree` with no `--target`, so it answered "does this compile C on the machine
running the audit" while this paragraph called it exact. It now reads
`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `aarch64-apple-darwin` and
`wasm32-unknown-unknown`; adding a target to that list is a claim that the engine is built
for it.

Widening it found a violation the same day. **`fepdf-gui` compiles C on Linux** — Wayland's
build shim, through `rfd` → `ashpd` and again through `eframe` → `winit` →
`smithay-client-toolkit`. It is not new, it is newly visible: a Linux GUI build has done
this for as long as the GUI has had a Linux target, and Rule 9 reported PASS throughout.
The Linux GUI **keeps Wayland** ([ADR-0033](docs/adr/0033-the-linux-gui-keeps-wayland-so-rule-9-names-one-exemption.md)):
an X11-only Linux GUI is a worse product than a rule kept clean, and Rule 9 exists to keep
unaudited C out of the *engine*, which compiles none on any target.

So `verify_compliance.sh` names one exemption — and it names **`wayland-backend`, the crate
that compiles the C, not `fepdf-gui`, the member that reaches it**. Exempting the member
would forgive whatever it acquires next; naming the cause forgives Wayland and nothing
else.

`--target all` finds one more, `chrono` → `iana-time-zone` → `iana-time-zone-haiku`, and
Haiku is deliberately not on the list.

The rule was stated after the fact, which is worth admitting: three dependencies had to go
before it could pass, and **two of them were never used by a line of code**
([ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)).

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
(see [§4.1](#41-the-operation-vocabulary)). A frontend's job is to turn argv, a button
press, an MCP call or a JS call into that value and hand it over. It never implements
the operation itself.

Where two frontends each implement "the same" operation, the two implementations
drift, silently, because nothing compares them. That has already happened here — see
[§4](#-4-why-this-shape).

---

### What checks them

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
  vocabulary, and frontends called them at eight sites (§4.1). The tell was that this row
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

## 🏛️ 3. What code must satisfy elsewhere

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
