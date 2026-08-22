# ADR-0025: A script processor is a frontend, not a subsystem

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: *(the commit that adds Phase Q)*

## Context

ADR-0022 declined ECMAScript actions and ADR-0024 settled which engine would be used if
that were reversed — boa, because RR-15 Rule 9 forbids compiling C and QuickJS cannot
satisfy it. Neither answered the question that actually decides the shape: **what a script
is allowed to do to the document.**

The first design proposed for that was wrong, and worth recording because the mistake is
this project's recurring one. It gave the script engine a result type of its own:

```rust
pub struct Outcome {
    pub changed: Vec<(String, FormValue)>,   // the fields the script set
    pub cancelled: bool,                     // event.rc = false
}
```

The reasoning was that letting a script touch the arena directly would put a mutable
borrow of the document across the boundary of a 124-crate dependency, so the run should
return a *value* the engine then applies. That much is right. What was wrong is that
`changed` is **a second vocabulary for document mutation**, invented next to one that
already exists and is poorer than it: 12.6.3's NOTE 2 says a field action "can … make any
other modification to the document", and this narrows that to setting field values.

The narrowing would have had to be declared as a deliberate limitation. It does not,
because the limitation was an artefact of the invented type.

**Four frontends already translate into `Operation` and hand it to `fepdf-doc::apply`.**
That is Rule D (ARCHITECTURE §5.1), and `fepdf-mcp` is the proof it works at full width:
it constructs all 24 variants, and it does so by deserialising **a JSON string**, which is
the form a JavaScript engine produces natively.

## Decision

**The script processor is a fifth frontend.** `fepdf-script` translates ECMAScript into
`Operation` values exactly as `fepdf-cli` translates argv and `fepdf-mcp` translates a
tool call. It holds no arena, defines no mutation type, and is not on the path of any
other caller.

```
    fepdf-cli      argv          ─┐
    fepdf-gui      button press  ─┤
    fepdf-mcp      tool call     ─┼─►  Operation  ─►  fepdf-doc::apply
    fepdf-script   this.getField(…).value = 3  ─┘
```

**Four things follow that were problems a moment earlier:**

- **The capability question is already answered.** What a script may do is what the API
  may do. That is not a narrowing invented for scripts; it is the same bound every
  frontend has, and it grows when the API grows.
- **The security question reduces to one already being asked.** "What can a malicious
  document do?" becomes "what can a CLI user do?" — no new privilege exists to reason
  about.
- **A gap appears in the right place.** `this.addAnnot()` maps to `AddAnnotation`, which
  exists. A script wanting something with no `Operation` is a **missing operation**,
  visible to the CLI, GUI and MCP users too, rather than a hole inside a script shim.
- **The bridge is nearly nothing.** `Operation` already derives `Deserialize` and
  `fepdf-mcp` already turns a JSON string into one.

**Reads go through the facade's existing queries; writes go through `Operation`. There is
no third path.** `Doc.getField("x").value` is a query, not a mutation, and the query API
is the one all four frontends use.

**Operations are applied during the run, not collected and applied after.** A script that
sets a value and reads it back must see the new one — the Keystroke → Validate →
Calculate → Format cascade depends on it. That means holding `&mut Document` across boa
calls, which is the **one genuine implementation risk** in this design and is unverified:
whether that borrow sits comfortably in a `boa_engine::Context` is the first thing the
measurement step below has to establish, and a negative answer reopens this decision.

**Rule B places the contract.** `fepdf-model` defines the trait and its event types;
`fepdf-script` implements it; the facade wires it behind `--feature script`, as ADR-0004
did for the GPU. A caller who does not choose it links none of the 124 crates — which is
6.3.2.1's subset choice and Cargo's feature graph agreeing on the same line.

**Scripts run only when a caller asks.** `Operation::RunDocumentScripts { trigger }` is
explicit, and no existing write path gains an implicit execution:
`crosscheck_selfread.sh` asserts that every combination reads back exactly as its input
does, and an automatic run inside `SetFormFieldValue` could break that. `SetFormFieldValue`
keeps recording a `Decision` when `/CO` is present, which is what tells a caller there is
something to run.

**Determinism is injected, because `app` is ours to write.** `new Date()`, `Math.random`
and `app.viewerVersion` are host properties, and RR-15's determinism rules apply to
anything that decides output. A `ScriptEnvironment { now, seed, viewer_version }` makes
the same input produce the same output.

**`/CO` supplies the calculation order** — the engine already reads it. The recursion
guard cannot be "do not calculate a field twice", because 12.6.3 permits A → B → A; it is
a bounded iteration count that records a `Decision` when it stops.

**Adobe's helper functions (`AFSimple_Calculate`, `AFNumber_Format`) may be written in
JavaScript**, which is how PDF.js does it and is easier to maintain than the Rust
equivalent. Two conditions, because `verify_compliance.sh` checks none of it — not the
function-length limit, not the error types, not determinism: each helper carries a test
that fails when the helper is broken, and `status.sh` gains a row counting the lines that
no audit covers, so that figure cannot grow quietly. `docs/specs/` held twelve false
claims for the want of exactly that.

## Consequences

- **Nothing is built yet.** This records the shape so the first step can be a measurement
  rather than a construction: run the corpus's six `/JavaScript` scripts against a minimal
  `app`/`this`, count how many complete, and find out whether boa's coverage and the
  `&mut Document` borrow are what this needs. Phase Q.
- **The motive is still thin, and the rule from Phase L still holds.** Of 524 files, two
  run code on open and `/AA /C` occurs zero times. A corpus is a reason to build and never
  a reason not to — but it is a reason to keep the first step small and cheap to discard.
- **This design assumes Rule D, which does not currently hold.** Eight frontend call sites
  mutate documents through facade methods instead of operations (ARCHITECTURE §5.1), and
  four of the mutations they perform have no `Operation` at all. A script frontend built
  on the vocabulary would be *more* conforming to Rule D than the GUI is. That is an
  argument for fixing Rule D first, not for giving the script engine its own path.
- **What ADR-0024 left open stays open.** Whether 124 crates and 528 `unsafe` occurrences
  are worth a capability the corpus has not asked for is not answered here. This says what
  the answer would cost to implement, which is less than it looked.
