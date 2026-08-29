# fepdf — planning

> **Phase: planning.** What to settle before code is written. The rules for writing it are
> in [CODING.md](CODING.md).

## When to write a plan down

Before a significant structural change. Where it goes depends on what it is: a decision
that is contested, reversed, or rests on a measurement belongs in
[`docs/adr/`](docs/adr/README.md); sequencing belongs in [ROADMAP.md](ROADMAP.md). There
is no standing plan file.

A plan says:

1. **The goal** — scope, and what outcome would count as reaching it.
2. **What needs a decision** — breaking changes, architectural choices, trade-offs where
   two readings lead to different work.
3. **What is still open** — requirements not yet settled.
4. **The changes**, grouped by crate, marked `[NEW]`, `[MODIFY]` or `[DELETE]`.
5. **How it will be verified** — `cargo test --workspace`,
   `./scripts/audit/verify_compliance.sh`, and the checks in [TESTING.md](TESTING.md) the
   release suite cannot make: `cli_smoke.sh` in a debug build, `crosscheck_roundtrip.sh`
   against a second implementation. **Name the check that would fail if the change were
   wrong.** A verification plan that cannot fail is a list of commands.

## Finding out how something works

Never guess at implementation, schema or location.

1. **Run something.** A function's name, a doc comment and a governance document are all
   claims about the code, and decisions have been reversed here after being taken from
   each. What the engine finds in a *document* it records as a `Decision`
   (`ARCHITECTURE.md` §4.3), not a log line — "read the warnings" is not available.
2. **Say where you looked, and check the edges of it.** A search feels exhaustive from
   inside its own boundary. `git log -S` and a grep over `crates/`, `scripts/` and the
   root's `*.md` missed a rulebook two directories away and produced a confident wrong
   answer ([ADR-0037](docs/adr/0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md)).
   Confidence tracks effort spent, not ground covered.
3. **Establish that a search finds nothing by making it find something.** An absent call
   site, an unfired gate and a broken pattern look identical.
4. **Read whole definitions.** Structs, enums and traits entire, not truncated.
5. **Check the manifests.** `Cargo.toml`, workspace dependencies, and what `mod.rs` and
   `lib.rs` actually export.
