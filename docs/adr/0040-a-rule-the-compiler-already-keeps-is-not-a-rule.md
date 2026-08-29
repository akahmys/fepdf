# ADR-0040: A rule the compiler already keeps does not need a grep, and Rule 17 did not need to exist

- **Status**: Accepted
- **Date**: 2026-08-29
- **Commit**: (see the commit that adds this file)

## Context

`CODING.md`'s Rule 17 read:

> **Type Explicitly.** Explicitly specify floating-point types (`1.0_f32`, `2.5_f32`) to
> prevent Edition 2024 inference fallbacks. *Enforcement: Clippy / Compiler.*

Nothing enforced it. The lint that would, `clippy::default_numeric_fallback`, is in
`clippy::restriction`, and the groups enabled are `pedantic`, `nursery` and `all` —
recorded the same day in [ADR-0037](0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md),
which corrected the enforcement column to "Nothing".

Enabling it reports **857 sites**. Reading them is what settled the question:

```rust
if safety > 1_000_000 {                       // the comparand fixes the type
if cmap.wmode == 0 {                          // wmode is i32
let (mut depth, mut escaped) = (1, false);    // depth's later use fixes it
```

Every one has a type the context determines, and getting it wrong is a compile error —
Rust will not put an `f64` where an `f32` is wanted. The lint says a literal carries no
suffix, not that a type is wrong. And the rule's stated reason could not be substantiated:
no Edition 2024 change to numeric fallback was found.

**Rule 17 was the only rule of the nineteen whose violation could not change the
output.**

The same question, asked of the other eighteen, found two more — of a different kind.

| | Audit check | Measured 2026-08-29 |
| :--- | :--- | :--- |
| **Rule 3** — no `unsafe` | `grep -rn "unsafe {"` | Adding an `unsafe` block to `fepdf-model` **fails the build**: `error: usage of an unsafe block` |
| **Rule 7** — no `static mut` | `grep -rn "static mut"` | Adding `static mut _PROBE: u32` and reading it **fails the build** for the same reason: a `static mut` cannot be read without `unsafe` |

Both rules hold. Both greps were redundant, and weaker than what they duplicated: they
match `unsafe {` and miss `unsafe(`, and they run after a build that has already
succeeded.

## Decision

* **Rule 17 is retired.** A rule whose violation cannot reach a build is a style
  preference, and this protocol is nineteen rules that decide whether code ships.
* **Rules 3 and 7 keep their rules and lose their greps.** Their enforcement column names
  `rustc`, which is what actually holds them.
* **Numbers 12 and 17 are left unused.** Reassigning a retired number is what made Rules
  9 and 14 name two different things between the original rulebook and `CODING.md`
  ([ADR-0037](0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md)), and the
  table now says both are unused and why.
* **`verify_compliance.sh`'s `[Rule 17]` label is gone.** It was the clippy audit, a
  different rule sharing the number, and reads `[Clippy]` now.

## Consequences

Eighteen rules, thirteen mechanical checks. Nothing that was enforced stopped being
enforced: `unsafe_code = "forbid"` cannot be overridden by an `#[allow]`, so removing its
grep removes a second opinion, not the opinion.

**The check that was actually missing went in the same day.** Rule 20 — record a
`Decision` where the engine accepts input the standard does not describe — had "code
review" in its enforcement column and is the rule every semantic defect found on
2026-08-29 violated. It cannot be decided statically, but its blind spot can be counted:
`scripts/audit/silent_branches.py` finds `match` arms over a file-supplied *integer*,
where `clippy::wildcard_enum_match_arm` cannot reach, whose wildcard answers with a
default or `None` and no `Decision`. **Eleven**, reported by `status.sh`.

That script is deliberately named for what it counts rather than for the rule. Some of
the eleven are defensible — an unrecognised `/V` makes the document fail to open, which is
loud enough. The number exists so a new one is visible, not so that zero is the target.
