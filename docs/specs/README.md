# Subsystem Notes

Background on individual subsystems. **Not authoritative.**

Most of this predates the current design and sits at the bottom of the hierarchy of
truth (`AGENTS.md`). Where it disagrees with `ARCHITECTURE.md`, `ARCHITECTURE.md` is
right; where it disagrees with a measurement of the code, the code is right.

Read it for context on *why* a subsystem was shaped a certain way, not for what the
code does today.

**Audited 2026-08-22 (Phase O-4).** Every claim in these files that a command could
check was checked, and three of the documents did not survive it:

| Was here | Why it went | |
| :--- | :--- | :--- |
| `sdk_design.md` | Named four source files, a `serialize/` directory and five dependency versions that do not exist, and an Arlington predicate engine that has never existed | |
| `app_design.md` | Named five crates and four types that do not exist, and called the CLI binary the GUI | |
| `charter_redesign_2026-04-13.md` | A dated deliberation record, which belonged with the history rather than the specifications | |

All three were archived under `docs/history/`, which was deleted with
`docs/retrospectives/` on 2026-08-29 ([ADR-0038](../adr/0038-one-hierarchy-of-truth-and-the-parallel-rulebook-is-deleted.md)):
both existed for a self-improvement loop that is no longer run, and git holds what
happened.

The three that remain were corrected in place rather than archived, because most of
each was true: `refinery_engine.md` claimed generation bits on `Handle`, a
`SafetyBitmask`, a text-encoding detector that was **removed** for corrupting a
conforming `/Title`, and Zstd compression of exactly the two stream kinds that are
excluded from it; `core-pipeline.md` claimed a non-recursive decryption walk that
recurses; `rendering.md` claimed a `.notdef` fallback that logs, in an engine that holds
one `log::warn!` by design. Each correction says what was checked and when, because a
line that is silently right today is indistinguishable from one that is silently stale.

**Audited again 2026-08-22 (Phase Q).** The first audit checked every claim a command
could check and corrected three files in place. Four months of drift was not the reason it
had to be repeated the same day — a wider re-derivation was, and it found that
**`rendering.md`'s correction was itself wrong**: it verified "the engine holds exactly one
`log::warn!`" against `status.sh`, and `status.sh` searched two crates, neither of them the
ones that file describes. The real figure is sixteen, and seven of those sit in the
rendering and font code.

A claim checked against a tool that cannot see its subject is indistinguishable from a
claim nobody checked. That is the one lesson worth carrying out of this directory.

| File | What the second pass found |
| :--- | :--- |
| `rendering.md` | "exactly one `log::warn!` by design" — measured against a row that could not see the crates in question. Sixteen sites, three deliberate |
| `core-pipeline.md` | A **"Structural Bar Suppression"** heuristic that deleted fills at `y > 700`. It is gone from the code, having fired 1,738 times on one file and 902 on another, deleting table rules. Also a reader "remapping table" that has never existed |
| `sdk-pipeline.md` | "Wildcards are prohibited in the primary dispatch loop" — the primary dispatch matches a `&str` and its `_` arm logs *"Unknown or unhandled operator"*. Also a "Zero-Fallback Policy" beside a working system-font fallback |
| `refinery_engine.md` | Zstd, removed by Rule 9; `encoding_rs`, no longer in the tree at all; and a GUI described as using Tokio, which it declared and never called |

Nothing here was archived this time. Each file now says at the point of each claim what
was checked and when, because **a line that is silently right today is indistinguishable
from one that is silently stale** — which is the same sentence the first audit ended with,
and it applied to that audit too.

To learn what the engine does now, read `ARCHITECTURE.md`; to learn how it got there,
read [`docs/adr/`](../adr/README.md).
