# ADR-0031: A script frontend cannot be a facade feature, and holds no `&mut Document`

- **Status**: Accepted. Amends [ADR-0025](0025-a-script-processor-is-a-frontend-not-a-subsystem.md), whose shape is otherwise unchanged.
- **Date**: 2026-08-23
- **Commit**: 911f0e3

## Context

ADR-0025 recorded the shape of a script processor before anything was built, which was
the right order — and building it found two sentences in it that cannot both be true, and
one that is false.

**Sentence one, false.** "That means holding `&mut Document` across boa calls, which is
the one genuine implementation risk in this design and is unverified."

Measured. boa's capture signature is
`Fn(&JsValue, &[JsValue], &T, &mut Context) -> JsResult<JsValue>` with `T: Trace +
'static`. The capture arrives by **shared** reference. A `&mut Document` cannot be held
there, and neither can anything borrowed. What the design actually needs — operations
applied *during* the run, so the Keystroke → Validate → Calculate → Format cascade can
read back what it set — works through an `Rc<RefCell<…>>`. A script that sets `a = 3`,
reads it back and sets `b` from it returns 30 with both writes applied in order.

The risk was real and the diagnosis was wrong: interior mutability is not a workaround
for the borrow checker here, it is the only shape the API admits.

**Sentences two and three, contradictory.** "The script processor is a fifth frontend"
and "the facade wires it behind `--feature script`, as ADR-0004 did for the GPU."

A frontend depends on the facade (Rule A). A facade feature depends on the crate it
enables. Both together is a cycle, and cargo says so:

```text
error: cyclic package dependency: package `fepdf` depends on itself
```

ADR-0004's shape does not transfer, because `fepdf-render` is **not** a frontend: it
depends on `fepdf-model` and `fepdf-content`, and on `fepdf` only in `[dev-dependencies]`.
It sits below the facade. That is what makes a facade feature possible for it.

## Decision

**`fepdf-script` is a frontend and is not wired into the facade.** A caller who wants
document scripting depends on `fepdf-script`, exactly as one who wants a command line
depends on `fepdf-cli`.

The alternative — putting it below the facade beside `fepdf-render` — was rejected
because it would give the script crate `fepdf-doc` and `fepdf-model` directly. That is
*more* privilege than a frontend has, and ADR-0025's central argument is that a script may
do what the API may do, "the same bound every frontend has". Buying a facade feature with
that argument would be paying for the packaging with the design.

**The subset property survives intact, and reads better.** Measured:

| | boa crates linked |
| :--- | ---: |
| `fepdf` | 0 |
| `fepdf-cli` | 0 |
| `fepdf-script` | all of them |

6.3.2.1's subset choice and cargo's dependency graph still agree on one line; the line is
a crate boundary rather than a feature flag.

**The document is reached through a `DocumentHandle`**, an `Rc<RefCell<PdfDocument>>`
behind a `#[derive(Trace, Finalize)]` wrapper.

## Consequences

**`boa_gc::Trace` is an unsafe trait, and RR-15 Rule 3 cannot see the impl.** The derive
emits an `unsafe impl`; a crate carrying `#![forbid(unsafe_code)]` compiles it anyway, and
Rule 3's check searches for an unsafe *block*. Fixing the check to match `unsafe impl`
would not help: the impl is macro-generated and appears in no source file. Verified —
`grep` over every workspace `src` finds no `unsafe impl` at all.

So this is unsafe code inside the audited tree that no available guard can catch. It is
recorded in `fepdf-script`'s crate documentation rather than left to be found, and
`#[unsafe_ignore_trace]` is at least visible where it is used.

**`this` is not a name that can be registered.** In a non-strict script `this` *is*
`globalThis`, so a global property called "this" is never consulted and every `this.x`
reads `undefined`. The script runs as the body of a function called on the document
object instead. It costs one thing: a top-level `var` or `function` becomes
function-scoped, so a document-level script cannot define a helper for a later one this
way. Nothing runs two scripts yet.

**The audit was passing without looking.** `verify_compliance.sh` named eleven crate
directories, `fepdf-script` was the twelfth, and the first full run reported `AUDIT
PASSED` having never opened it. `TARGET_DIRS` is derived from the workspace now — the
third instance this month of a check that names its places, after the `Decision` row and
two others in `status.sh`.
