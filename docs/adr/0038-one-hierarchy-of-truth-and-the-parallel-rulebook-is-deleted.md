# ADR-0038: One hierarchy of truth, and the parallel rulebook that outlived it

- **Status**: Accepted
- **Date**: 2026-08-29
- **Commit**: (see the commit that adds this file)

## Context

`.agents/rules/code-safety.md` is titled "Reliable Rust-15 (RR-15) Rulebook" and holds
exactly fifteen rules. It was written on 2026-04-11 as `.antigravity/rules/hardening.md`
and edited sixteen times through April and May — it was the rulebook.

On **2026-08-09 at 06:03** it was renamed into `.agents/rules/`. On **2026-08-09 at
06:12**, nine minutes later, a second commit created `CODING.md`, `AUDITING.md`,
`TESTING.md` and `PLANNING.md` at the repository root under the title "streamline
governance to single-agent model". Nothing deleted the first. It has been touched twice
since, for a project-wide rename and a one-line ownership note.

**The two rulebooks disagree, and not by drifting apart — by reusing numbers:**

| | `.agents/rules/code-safety.md` | `CODING.md` |
| ---: | :--- | :--- |
| 9 | Ownership-First Design | Pure Rust |
| 12 | Invariant Enforcement | absent |
| 14 | Locality of Declaration | Test Code Separation |
| 15 | Explicit Allocation | Clone Optimization |

Three more files in that directory instruct the opposite of a current rule:

* `desktop-ui.md`: "UI widgets must consume SDK-native handles or structures" — Rule A
  says arena types stop at the facade.
* `pdf-engine.md`: a font fallback "MUST trigger an explicit `log::warn!`" — Rule 20 says
  record a `Decision`, and [ADR-0028](0028-four-of-the-thirteen-logs-were-not-decisions.md)
  deleted the logs that did this.
* `iso-compliance.md`: mandates `scripts/audit/verify_secrets.sh`, which does not exist.

`code-safety.md` and `desktop-ui.md` also name `crates/fepdf-sdk`, gone since the topology
migration. Thirteen of the forty internal links under `.agents/` are broken, most of them
to `rules/hardening.md` — the name this file had before 2026-08-09. `.agents/session/`
holds a write-ahead log for Phase D. `.agents/skills/` is not loaded by the harness that
reads this repository.

**Both rulebooks say `CODING.md` wins**, and it did not help. On the day this was written
one agent searched for Rule 12, missed the file, and published an invented answer; a
second agent read the same directory and reported `desktop-ui.md` and `pdf-engine.md` as
live contradictions. A subordinate copy is still read, and being labelled subordinate does
not stop what is read from being applied.

## Decision

**One hierarchy, and everything outside it goes.**

1. **ISO 32000-2:2020** — the standard itself.
2. **Measurement** — what the code and the corpus are observed to do.
3. **`CODING.md` (RR-15) and `ARCHITECTURE.md` (layering)** — the rules.
4. **`AUDITING.md`, `TESTING.md`, `AGENTS.md`, `PLANNING.md`** — governance.
5. **`docs/specs/`** — background on a subsystem.

A claim lower in the list never overrides one higher. `docs/adr/` sits beside the list
rather than in it: it records how a level-3 or level-4 statement came to be, and never
states a rule of its own.

**`docs/history/` and `docs/retrospectives/` are deleted too**, with
`docs/conventions/reflections.md`. All three existed for an agent self-improvement loop —
retrospectives feeding protocol changes — that is no longer run: nothing under
`docs/retrospectives/` has been written since May, its phase numbering (7 to 23) predates
the current scheme, and `AGENTS.md` carried a rule whose only purpose was to stop anyone
editing them. What happened is in git, and why a decision was taken is in this log.

**`.agents/` is deleted in full** — rules, session, skills, workflows. It is a second
level 3 in a hierarchy that has one, and the parts that were not rules were a harness that
no longer exists.

## Consequences

3,687 words of parallel rulebook, 2,507 of dead session state, 2,712 of skills the harness
does not load, and 1,565 of workflows pointing at renamed files leave the tree. Nothing
that is enforced was in any of them: `verify_compliance.sh` reads `CODING.md`'s numbering
and always has.

**One thing was worth keeping and is kept here.** `.agents/session/lessons_learned.md`
recorded a constraint that no rule now states:

> **Handle stability.** `PdfArena` object indices are stable; the handles for
> *dictionaries* are not, because a dictionary can be re-allocated by a refinery pass.
> A cache keyed on a dictionary handle is keyed on something volatile. The only stable key
> for a resource is its top-level `Handle<Object>`.
>
> **Private use is signal in CJK.** A CID-keyed font without `/ToUnicode` encodes CID
> values in the `0xF0000` block. Suppressing private use as noise breaks those documents.

The first was RR-15's Rule 12, "Invariant Enforcement", and is now stated nowhere.

The second is **live and unguarded**. `is_withheld` in `crates/fepdf-model/src/font/mod.rs`
discards `0xF0000..=0x10FFFF` for every font, and takes only a character and a flag, so it
cannot know whether the font is `CIDFontType0` or `CIDFontType2`. Measured on
2026-08-29, no CJK document in the corpus loses a character this way — the 48 withheld
private-use glyphs are all in `intel_sdm.pdf` — so this is a latent risk with a warning
attached, recorded here rather than deleted with the file that carried it.

The other lesson in that file, "Pass 0 Normalization", is not carried here because the
code already carries it: `crates/fepdf-model/src/ingest/mod.rs` documents the pass at the
function that runs it, which is where a lesson stops needing a document.
