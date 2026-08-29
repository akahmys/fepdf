# fepdf — governance

Where each thing is written down, and what to do when two of them disagree.

## Hierarchy of truth

```
1. ISO 32000-2:2020, the standard itself
   └── 2. Measurement of the code as it is
        └── 3. This file — the principles below
             └── 4. The rules of each phase, in the order work happens:
                    PLANNING.md → CODING.md → TESTING.md → AUDITING.md
                 └── 5. docs/specs/ — background on a subsystem
```

**Measurement outranks documentation.** A claim about this codebase is established by
running something, not by reading a document or a function name. Records in `docs/adr/`
exist because a document asserted what the code did not do — [ADR-0017](docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md),
[ADR-0037](docs/adr/0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md) and
[ADR-0039](docs/adr/0039-the-design-document-was-narrating-its-own-corrections.md) among
them. A document that disagrees with a verified measurement is corrected, not argued from.

## Which document answers what

Each answers one question. Writing something in the wrong one is how two documents come
to disagree.

| Document | Answers | Does **not** contain |
| :--- | :--- | :--- |
| **[AGENTS.md](AGENTS.md)** | Where is everything written, and what outranks what? | The rules themselves |
| **[PLANNING.md](PLANNING.md)** | What is decided before code is written? | Any specific plan |
| **[CODING.md](CODING.md)** | What must code satisfy? RR-15, and the layering rules | Why the design is this shape |
| **[TESTING.md](TESTING.md)** | What must pass before a change lands? | Test results |
| **[AUDITING.md](AUDITING.md)** | What is checked mechanically, and by what? | The rules being checked |
| **[ARCHITECTURE.md](ARCHITECTURE.md)** | What is the design **now**? | Why it came to be, history, rules |
| **[ROADMAP.md](ROADMAP.md)** | What is measured, built, and next? | Rules |
| **[README.md](README.md)** | What is this, and how is it built? | Design or rules |
| **[docs/adr/](docs/adr/README.md)** | How did a rule or a design come to be? | The present design |

Only the four phase documents state rules. When another appears to, the rule belongs in a
phase document and an ADR records why.

## Writing rules

1. **One fact, one home.** If it belongs in two places, one links instead of restating.
2. **Present tense here and in `ARCHITECTURE.md`; past tense in `docs/adr/`.** A reversal
   is recorded, not deleted, so the reasoning is not repeated.
3. **A quoted figure carries its date**, or is re-derived before quoting. `status.sh`
   re-derives the ones these documents lean on, so a stale figure reads as a
   disagreement rather than as current.
4. **A rule that is not checked is a comment.** Every entry in `CODING.md` names what
   enforces it, and says "nothing" where nothing does.
5. **Prove a check fires by breaking the thing it checks.** Tests here have passed
   against the defect they were written for.

## Principles

1. **Safety over speed.** Memory safety, determinism and ISO conformance come before
   optimisation.
2. **What the engine finds, it records; what you want to know, you measure.** A finding
   about a *document* is a `Decision` naming its clause (`ARCHITECTURE.md` §4.3) — not a
   log line, because a warning on stderr cannot tell a caller *this loaded* from *this was
   conforming*. The engine keeps three `log::warn!`/`log::error!` sites, counted by
   `./scripts/dev/status.sh`.
3. **A corpus can justify building something. Only a use case can justify not building
   it.** Zero occurrences measures the corpus, not the world.

Each phase document carries the commands for its phase.
