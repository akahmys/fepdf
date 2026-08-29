# 💻 fepdf Coding Standards & Hardening Protocol

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
| **Rule 20** | Recorded Interpretation | Where the engine accepts input the standard does not describe, it MUST record a `Decision` naming the clause and what was done. A silent acceptance is a defect even when the output is right. | Code review / ARCHITECTURE.md §5.3 |

**Rule 12 was resource limits, and it left this table while the practice stayed in the
code.** The table runs 1–11 and 13–20 with no note about the gap. Rule 12 capped how much
a filter would decode — `const MAX_DECODE_SIZE: usize = 256 * 1024 * 1024; // 256MB (RR-15
Rule 12)` — and the cap is still there, halved and uncited, as `MAX_DECOMPRESSED_SIZE` in
[`crates/fepdf-model/src/filters/flate.rs`](crates/fepdf-model/src/filters/flate.rs). The
rule went; the thing it required did not. Nothing in the workspace cites Rule 12 today, so
a reader meeting the 128 MB limit has no way back to why it is there — which is the
failure this protocol exists to prevent, arrived at by deletion instead of by omission.

**RR-15 names fifteen rules and this table has nineteen.** 1–11 and 13–20. The name was
accurate once and is now a label rather than a count; it is left as it is because renaming
it would break every `// RR-15 Limit:` marker in the tree, and knowing the number is
historical costs a sentence.

**Rule 17 is enforced by nothing, and this table said "Clippy / Compiler" for as long as
that was untrue.** No clippy lint in `[workspace.lints.clippy]` requires a float suffix:
the one that would, `default_numeric_fallback`, lives in `clippy::restriction`, and the
groups enabled are `pedantic`, `nursery` and `all`. The code answers the same way —
**1,118 unsuffixed float literals against 70 suffixed**, counted with comments and string
contents removed. The rule is kept because Edition 2024's inference fallback is real, and
its enforcement column now says what is true; making it hold means enabling the lint and
fixing 1,118 sites, which is a decision and not a tidy-up.

**`verify_compliance.sh`'s "[Rule 17]" is a different rule.** There it labels the clippy
audit — `cargo clippy --workspace --all-targets -- -D warnings` — which is this table's
enforcement column for Rules 4 and 5 and has nothing to do with float suffixes. Two rules
share a number across two files, which is how a reader concludes the audit checks this one.

**Rules 16 and 18 were added on 2026-08-22 and are not new.** `verify_compliance.sh` had
been enforcing both under those numbers since before this table existed, and this table —
which `README.md` calls "the RR-15 rules in full" — did not contain them. `AUDITING.md`
cited them as though it did. A rule that is checked and not stated is the mirror of the
failure the enforcement column exists to prevent, and is harder to notice, because
everything passes.

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

## 🏛️ 2. What code must satisfy elsewhere

This document says what code must satisfy. The design it satisfies lives in
`ARCHITECTURE.md`, and repeating it here is how the two came to disagree: this section
described a **Pass 1 (Arena Ingestion)** that [ADR-0003] removed when the reader stopped
converting another library's object model, and it had said so for as long as the reader
had been fepdf's own.

- **The Sublimation Pipeline** — `ARCHITECTURE.md` §5.4. A `Document` is the normalised
  state, not the file; the file is reached through the byte layer named in the same
  section.
- **`PdfArena` invariants** — `ARCHITECTURE.md` §5.6. Objects are reached through
  `Handle<Object>`, never a pointer or a raw index, and traversal is deterministic
  (Rule 10 above is the enforced half of this).
- **Rendering and the GUI** — `ARCHITECTURE.md` §5.7. Vello compute shaders on wgpu,
  `f64` preserved through path snapping and measurement, CJK font loading and
  English/Japanese localisation in `fepdf-gui`.

[ADR-0003]: docs/adr/0003-lopdf-was-not-providing-robustness.md
