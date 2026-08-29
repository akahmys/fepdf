# ADR-0037: A rules document holds rules, and the log holds how they were got wrong

- **Status**: Accepted
- **Date**: 2026-08-29
- **Commit**: d523a63

## Context

RR-15 is derived from NASA's *Power of Ten*, whose value is not the content of any one
rule but that there are ten of them, each a line long, and that a programmer can hold the
set in mind while writing the line of code the rule is about. A rule that has to be looked
up is applied afterwards, if at all.

Measured on 2026-08-29, twenty days after `CODING.md` was written:

| | 2026-08-09 | now | |
| :--- | ---: | ---: | ---: |
| `CODING.md` | 540 | 1,898 | 3.5× |
| `ARCHITECTURE.md` | 638 | 6,914 | 10.8× |
| `TESTING.md` | 300 | 2,248 | 7.5× |
| `AUDITING.md` | 375 | 867 | 2.3× |

**The rules did not grow; the commentary did.** `CODING.md`'s table is 633 words and the
prose around it 1,217 — the set of rules is outnumbered two to one by writing about the
set of rules. The rule count went from fifteen to nineteen in the same period.

Of that prose, 32% is past-tense narration of times a rule was wrong: Rules 16 and 18
missing from the table, Rule 12 deleted while its limit stayed in the code, Rule 17's
enforcement column naming a tool that checks something else. `ARCHITECTURE.md` is 41%,
`AUDITING.md` 44%, `TESTING.md` 84%.

**`AGENTS.md` already forbids this.** "Present tense is `ARCHITECTURE.md`; past tense is
`docs/adr/`." The rules documents were not following the project's own separation.

## Decision

**The rules documents keep rules and the definitions that make them checkable. Their
failure history moves here.**

* **A rule's statement, its enforcement, and where its line falls** stay in `CODING.md`.
  "Rule 9 in detail: where the line is" and "Rule 5 in detail: what no wildcards can and
  cannot mean" are not commentary — a rule whose boundary is undefined is not checkable,
  and both sections define one.
* **"This said X while X was untrue"** moves to the log. It is a decision record by
  construction: a claim, a measurement that contradicted it, and what was done.
* **What stays behind is a pointer**, not a summary. A summary of a reversal drifts from
  the reversal.

### The three the rules document was carrying

**Rule 12 was resource limits, and it left the table while the practice stayed in the
code.** It capped how much a filter would decode — `const MAX_DECODE_SIZE: usize = 256 *
1024 * 1024; // 256MB (RR-15 Rule 12)`. The cap is still there, halved to 128 MB and
citing nothing, as `MAX_DECOMPRESSED_SIZE` in `crates/fepdf-model/src/filters/flate.rs`.
The rule went; the thing it required did not, so a reader meeting the limit has no way
back to why it is there — the failure this protocol exists to prevent, reached by deletion
rather than by omission.

**Rule 17 is enforced by nothing, and the table said "Clippy / Compiler".** No lint in
`[workspace.lints.clippy]` requires a float suffix: `default_numeric_fallback` lives in
`clippy::restriction` and the enabled groups are `pedantic`, `nursery` and `all`. The code
agrees — 1,118 unsuffixed float literals against 70 suffixed, comments and string contents
removed. The rule is kept, because Edition 2024's inference fallback is real; making it
hold means enabling the lint and fixing 1,118 sites, which is a decision and not a
tidy-up. Separately, `verify_compliance.sh`'s `[Rule 17]` labels the clippy audit, which
is a different rule sharing a number across two files.

**Rules 16 and 18 were added to the table on 2026-08-22 and were not new.**
`verify_compliance.sh` had enforced both under those numbers since before the table
existed, and the table — which `README.md` calls "the RR-15 rules in full" — did not
contain them. A rule that is checked and not stated is the mirror of a rule that is stated
and not checked, and harder to notice, because everything passes.

**RR-15 names fifteen rules and the table has nineteen**: 1–11 and 13–20. The name is a
label rather than a count. Renaming it would break every `// RR-15 Limit:` marker in the
tree.

## Consequences

`CODING.md` goes from 1,898 words to about 1,470, and its table stops being outnumbered by
prose about its table. Nothing is lost: every paragraph moved is above, and the rules
document links here.

**The reason to care is that agents write most of this repository, and length harms them
in a way it does not obviously harm a person.** Three things happened on the day this was
written, all in documents whose length was the mechanism:

* The clause 8 status row said two colour defects were open, months after they were
  closed. That row was 6,578 characters in a single table cell. It was grepped, edited and
  quoted from during the session that found the contradiction — and the contradiction was
  found by reading `color.rs`, not by reading the sentence asserting it.
* A sentence was duplicated inside that same cell, by an agent appending to a row that
  already contained it.
* `ARCHITECTURE.md`'s status banner denied that Rule D was realised, in lines 5 to 13 of
  the file — the most-read position there is — and was found by comparing `status.sh`
  output against it rather than by reading it.

And the mechanism that compounds: **the 77-word paragraph about Rules 16 and 18 was
written on 2026-08-22, and on 2026-08-29 an agent appended 225 more words in exactly that
shape without deciding to.** A document teaches its register to whatever writes in it
next. That is how commentary grows two-to-one against the rules it surrounds, and it is
why the separation has to be a rule rather than a preference.
