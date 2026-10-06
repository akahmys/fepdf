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

Three of these six are checked by `scripts/audit/documents.py`, which
`verify_compliance.sh` runs: the tense rule 2 states, the links rule 1 implies, and the
integrity of the ADR index. Rules 3, 5 and 6 are held by review, and rule 4 says what that
costs ([ADR-0081](docs/adr/0081-the-writing-rules-had-nothing-behind-them.md)).

1. **One fact, one home.** If it belongs in two places, one links instead of restating.
   *Checked in part: every relative link in the documents and in source doc comments must
   resolve — seven in source did not — and every intra-doc link, by `cargo doc` with
   warnings denied (52 failing on 2026-10-04, before the fix).*
2. **Present tense here and in `ARCHITECTURE.md`; past tense in `docs/adr/`.** A reversal
   is recorded, not deleted, so the reasoning is not repeated. *Checked: a line carrying
   both a date and a past-tense verb in either document fails the audit.*
3. **A quoted figure carries its date**, or is re-derived before quoting. `status.sh`
   re-derives the ones these documents lean on, so a stale figure reads as a
   disagreement rather than as current. **A count stated twice is the case to watch**: the
   subset table's unmet rows and the sentence above them each carried the number, and the
   sentence went on saying "three" after Phase P had met one of them. `status.sh` derives
   the first and checks the second against it.
4. **A rule that is not checked is a comment.** Every entry in `CODING.md` names what
   enforces it, and says "nothing" where nothing does. These rules say the same, above.
5. **Prove a check fires by breaking the thing it checks.** Tests here have passed
   against the defect they were written for.
6. **One ADR, one decision.** A record that reaches a second subject is split there.
   [ADR-0071](docs/adr/0071-three-declarations-that-read-nothing-and-one-that-wrote-nothing.md) carries
   eight and is the shape this rule exists to prevent.

## Before you start

`./scripts/dev/status.sh` re-derives the figures these documents lean on, so a stale one
reads as a disagreement rather than as current. The gate is two commands, and each phase
document holds its own: `cargo test --workspace` ([TESTING.md](TESTING.md)) and
`./scripts/audit/verify_compliance.sh`, which must end `=== AUDIT PASSED ===`
([AUDITING.md](AUDITING.md)).

## Principles

1. **Safety over speed.** Memory safety, determinism and ISO conformance come before
   optimisation.
2. **What the translator chooses, it records; what you want to know, you measure.** The
   record serves the translation ([README.md](README.md) says what fepdf is). A finding
   about a *document* is a `Decision` naming its clause (`ARCHITECTURE.md` §4.3) — not a
   log line, because a warning on stderr cannot tell a caller *this loaded* from *this was
   conforming*. The engine keeps two `log::warn!`/`log::error!` sites — which fonts this
   machine has, and the GPU failing to initialise — counted by `./scripts/dev/status.sh`.
   It read three until 2026-09-05, when unifying the three fallback-font assemblies into
   one took the third with the duplicated code.
3. **A corpus can justify building something. Only a use case can justify not building
   it.** Zero occurrences measures the corpus, not the world.

Each phase document carries the commands for its phase.
