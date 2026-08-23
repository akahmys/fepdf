# ADR-0032: Running a document's scripts is a frontend verb, not an `Operation`

- **Status**: Accepted. Amends [ADR-0025](0025-a-script-processor-is-a-frontend-not-a-subsystem.md) a third time, after [ADR-0031](0031-a-script-frontend-cannot-be-a-facade-feature.md).
- **Date**: 2026-08-23
- **Commit**: 6c04905

## Context

ADR-0025 said: "**`Operation::RunDocumentScripts { trigger }`, and nothing implicit.** No
existing write path gains an execution … The `/CO` `Decision` is what tells a caller there
is something to run."

The property that sentence protects is right and is met: nothing runs scripts unless a
caller asks. What it got wrong is where the asking lives.

`Operation` is defined in `fepdf-doc` and interpreted by `fepdf-doc::apply`. The script
engine is in `fepdf-script`, which sits **above** the facade. `apply` cannot call it. The
three ways out were measured rather than argued:

**Put the runner on the `Document`, as `set_system_fonts` does.** Two things stop it.
`Document` is `Send + Sync` today and a runner is not — the host holds
`Rc<RefCell<PdfDocument>>`, which is neither. And the runner holds the document while the
document holds the runner, which is a reference cycle by construction rather than by
oversight.

**Change `apply`'s signature to take a runner.** That reaches every caller in four
frontends and the Rule D check, to thread an argument that is `None` for twenty-nine of
the thirty variants.

**Let `apply` accept the variant and record that it cannot execute it.** An `Operation`
the engine cannot perform is a stub, and `fepdf-wasm::render_page` was removed for being
exactly that three commits ago.

## Decision

**There is no `Operation::RunDocumentScripts`.** `fepdf_script::run_calculations` is a
function on the frontend, and the vocabulary is untouched at thirty.

**The mutations still go through the vocabulary, which is what Rule D asks.** Running a
calculation order applies `SetFormFieldValue` per field, one operation each, and a script
writing `getField("x").value = 3` applies the same one. Nothing reaches the document by
another path.

**The precedent is already in the tree, twice.** `fepdf-cli`'s `edit` command composes six
different operations, and no one made "the edit command" an operation. `render_page` is a
headline capability of this engine and has **no** variant at all — because rendering is
something a caller does *with* a document, not something done *to* one. Running scripts is
the second kind in its effects and the first kind in its shape: a frontend verb that
composes operations.

**"Nothing implicit" survives unchanged.** No write path gained an execution:
`SetFormFieldValue` still records its 12.6.3 `Violation`, and a caller who wants the
calculation run calls for it. The property ADR-0025 was protecting never depended on the
variant existing.

## Consequences

The vocabulary stays at thirty and `fepdf-mcp` stays at thirty of thirty. A caller that
wants scripts depends on `fepdf-script`, exactly as one that wants rendering enables
`render`.

**What this costs is real and worth naming**: `fepdf-mcp` cannot offer "run this form's
calculations" as a tool without depending on `fepdf-script`, and neither can the GUI. That
is the same cost the `render` feature already carries, and it is paid in a dependency
rather than in a vocabulary the engine cannot interpret.

**Three of ADR-0025's sentences have now been corrected by building it**, and all three
were about *placement* rather than about the design: where the borrow lives, where the
feature flag lives, where the verb lives. The shape it recorded — a frontend translating
into `Operation`, reads through existing queries, no third path — has not needed changing.
That is roughly what writing a design down before building it is supposed to buy.
