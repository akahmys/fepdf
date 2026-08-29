# ADR-0039: The design document was narrating its own corrections

- **Status**: Accepted
- **Date**: 2026-08-29
- **Commit**: (see the commit that adds this file)

## Context

`ARCHITECTURE.md` answers "what is the design now". `AGENTS.md` gave it a second
question — "and why this shape?" — which is what `docs/adr/` answers, and the document
grew a section titled exactly that plus 523 words of past-tense self-correction inside
§5: *this row named five crates and one of them was wrong*, *this paragraph said "one"
for three phases*, *this listing was fiction until 2026-08-22*.

Measured on 2026-08-29: 6,801 words, of which §4 "Why This Shape" 657, §6 "Migration"
455 — a completed migration, in a document its own charter says holds no history — and
41% of the prose past tense. The same shape [ADR-0037](0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md)
found in `CODING.md`, arrived at independently.

## Decision

**`ARCHITECTURE.md` says what the design is. Why it came to be that way is here.**

* **§4 "Why This Shape" is deleted.** Its content is the question this log exists for,
  and most of it restates records that already exist — Rule C's split round trip is
  [ADR-0005](0005-layering-rules-are-enforced-by-cargo.md), the optional-content gap is
  [ADR-0021](0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md).
* **§6 "Migration" is deleted.** Every step was complete, and `ROADMAP.md` carries the
  sequencing.
* **§1's four layering rules move to `CODING.md`**, with §7's enforcement column beside
  them. A rule about structure is still a rule, and the document declaring itself free of
  coding rules had held four.
* **`AGENTS.md` stops giving `ARCHITECTURE.md` the "why" question.**

## Consequences

`ARCHITECTURE.md` goes from 6,801 words to about 4,600, and every section of it now
answers what is true now.

The self-corrections stripped from §5 are kept below rather than discarded: each is a
measurement of something this project believed and checked, which is what a record is for.

## What §5 was carrying

> Ambiguity that used to live in prose becomes a type. The rotate divergence in §4 was
> not fixable by convention; it was fixed by making the choice unrepresentable:

> Three of those nine now exist, and the plan turned out to have been right about them:
> `InsertFrom`, `Retag` and `Upgrade` were built as facade *methods* instead, which is
> precisely how Rule D came to be broken. Enforcing the rule was largely a matter of
> building what this listing had claimed for four phases. Re-derive it with:

> Structured, too. The audit alone used to show them, by stringifying each decision into
> a compliance issue at `IssueSeverity::Warning`: a JSON consumer was told "Warning"
> about something the engine had classified `Repaired`, and a `Violation` was
> indistinguishable from an `Ambiguity`. `DocumentSummary::decisions` now carries the log
> with its severities, and the audit no longer launders them.

> The nine newest are Phase P's: an operator this engine does not run (8.2), a pattern and a
> shading that would not build (8.7.3, 8.7.4.5.2), an operand count no colour model takes
> (8.6.8), a Type 3 glyph whose `/CharProcs` stream would not run (9.6.5), a `/Lab` colour
> converted through D65 sRGB (8.6.5.4), and the two the renderer reports (9.9, 9.6). Each
> was measured against the nine conforming samples before it was written, and each fires on
> none of them.

> The two that fired in the hundreds were reporting *ordinary* conditions. An SFNT container
> with no `CFF ` table is a TrueType font, and `inspect_cff` is called speculatively —
> every caller reaches it through `.unwrap_or(CffInfo::empty())`, so that `Err` is the
> expected answer. "Not SFNT" fired 423 times on `fugaku.pdf` alone, whose 72 fonts are all
> **Type 3**, which by 9.6.5 have no font program and so can never be SFNT; the rest were
> fonts with no `/FontFile`, where substituting is what 9.8 asks for. Converting either would
> have put 918 and 469 false departures on clean files and made `is_conforming` false for six
> of the nine — [ADR-0008](0008-an-indirect-length-is-not-an-ambiguity.md)'s mistake,
> made again and at scale.

> This paragraph said "one" for three phases. It was true of the two crates `status.sh`
> searched and of no larger set, and `fepdf-content`, `fepdf-font` and `fepdf-doc` were in
> **neither** the engine list nor the frontend list — invisible rather than miscounted,
> which is why doubling the figure changed no row. The lists are now complements derived
> from the workspace, so a new crate lands in one of them by construction.

> `FileStructure`, `CatalogReport`, `InteractiveReport` and `EncryptionReport` each take
> `&[u8]` and never see a refined arena. The two layers can disagree about the same file,
> and that is the design: they answer different questions. Before ADR-0013 named them,
> which one a command answered was an accident of how it had been written.

> This list named "Crypt Revision 6 (AES-256-GCM)" until it was checked against the
> standard: revision 6 is implemented, and the string `GCM` does not occur anywhere in ISO
> 32000-2. AES in this standard is CBC — "If using the AES algorithm, the Cipher Block
> Chaining (CBC) mode, which requires an initialization vector, is used." A namespace list
> is exactly where an invented detail survives longest, because nothing compiles against
> it.
