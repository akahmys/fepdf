# Subsystem Notes

Background on individual subsystems. **Not authoritative.**

Most of this predates the current design and sits at the bottom of the hierarchy of
truth (`AGENTS.md`). Where it disagrees with `ARCHITECTURE.md`, `ARCHITECTURE.md` is
right; where it disagrees with a measurement of the code, the code is right.

Read it for context on *why* a subsystem was shaped a certain way, not for what the
code does today.

**Audited 2026-08-22 (Phase O-4).** Every claim in these files that a command could
check was checked, and three of the documents did not survive it:

| Was here | Where it went | Why |
| :--- | :--- | :--- |
| `sdk_design.md` | [`docs/history/archive/`](../history/archive/sdk_design.md) | Named four source files, a `serialize/` directory and five dependency versions that do not exist, and an Arlington predicate engine that has never existed |
| `app_design.md` | [`docs/history/archive/`](../history/archive/app_design.md) | Named five crates and four types that do not exist, and called the CLI binary the GUI |
| `charter_redesign_2026-04-13.md` | [`docs/history/`](../history/) | A dated deliberation record, which is a historical document and never updated — it was in the wrong directory, not wrong |

The three that remain were corrected in place rather than archived, because most of
each was true: `refinery_engine.md` claimed generation bits on `Handle`, a
`SafetyBitmask`, a text-encoding detector that was **removed** for corrupting a
conforming `/Title`, and Zstd compression of exactly the two stream kinds that are
excluded from it; `core-pipeline.md` claimed a non-recursive decryption walk that
recurses; `rendering.md` claimed a `.notdef` fallback that logs, in an engine that holds
one `log::warn!` by design. Each correction says what was checked and when, because a
line that is silently right today is indistinguishable from one that is silently stale.

To learn what the engine does now, read `ARCHITECTURE.md`; to learn how it got there,
read [`docs/adr/`](../adr/README.md).
