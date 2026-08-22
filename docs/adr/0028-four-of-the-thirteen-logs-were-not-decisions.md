# ADR-0028: Four of the thirteen logs were deleted rather than recorded, because they fired on conforming files

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: (this change)

## Context

`ROADMAP.md` Phase P said the engine held sixteen `log::warn!`/`log::error!` sites, three
deliberate and **"the other thirteen are conclusions about the *document*"**, each of them
a `Decision` that had not been written. `ARCHITECTURE.md` §5.3 said the same. The obvious
reading is that the work is thirteen mechanical conversions.

§5.3 also carries a rule that cuts the other way:

> **A decision that fires on conforming input is worse than none**, because it makes the
> log a constant rather than a signal.

[ADR-0008](0008-an-indirect-length-is-not-an-ambiguity.md) is that rule being learnt:
recording an indirect `/Length` as an `Ambiguity` made `samples/sample.pdf` report 31
departures and `is_conforming` return `false` for a clean file. So each of the thirteen
was counted against the nine conforming samples *before* it was touched.

Three of the thirteen turned out not to be conclusions about anything.

| site | firings on the nine conforming samples |
| :--- | ---: |
| `interpreter/font.rs` "Font X is not SFNT, using fallback" | **469** |
| `reconstruction.rs` "CFF table not found in SFNT container" | **918** |
| `reconstruction.rs` "Unrecognized font format" | 0, but see below |
| the other ten | 0 |

**"CFF table not found" is the ordinary TrueType case.** An SFNT container holding `glyf`
outlines has no `CFF ` table, and every caller of `inspect_cff` reaches it through
`.unwrap_or(CffInfo::empty())` — it is asked speculatively and the `Err` is the expected
answer. 342 firings on `intel_sdm.pdf`, 342 on `unicode_16.pdf`.

**"Not SFNT" is worse.** 423 of its 469 firings are `fugaku.pdf`, whose 72 fonts are all
**Type 3** — and a Type 3 font (9.6.5) has no font program at all, so it can never be
SFNT. The rest are fonts with no `/FontFile`, where substituting a system font is what
9.8 asks for. The predicate tested something no conforming file can satisfy.

**"Unrecognized font format" fires nowhere new.** Measured across the whole external
corpus it appears on exactly the three `isartor-6-3-2-t01-fail-*` files, which are exactly
the files already carrying `fepdf-model`'s 9.9 `Violation` — "embeds a program in no
recognised format … skipped it and fell back to a system font" — gated, correctly, on the
font actually embedding something. It fired twice per document where the decision fires
once.

## Decision

**Nine sites become `Decision`s. Three are deleted. One becomes `log::debug`.**

Deleting is not losing the finding. For the two that fired in the hundreds there was no
finding: the condition is conforming, and converting them would have put 1,387 false
departures on clean files and made `is_conforming` false for six of the nine samples. For
the third the finding already exists upstream, recorded once instead of twice and with a
predicate that does not fire on Type 3 fonts.

**"SFNT assembly FAILED" becomes `log::debug` rather than a `Decision`**, because it is a
failure of *this engine's* assembler and not a conclusion about the file — the font data
arrived intact and we could not rebuild a container round it. Its document-level
consequence is already recorded: the raw outline returned in its place is a naked CFF that
`populate_u2g_from_data` then fails to read, raising the 9.9 `Violation` "the embedded
program for X did not parse". It fired on none of the 524 files in either corpus, and is
kept as a diagnostic because if it ever does fire it is a bug here.

**A backend records through the trait, not directly.** `RenderBackend::take_decisions` is
defaulted to empty — the text-extraction and collector backends reach no such conclusion —
and `render_page` drains it after the annotations, whose appearance streams draw glyphs
too. A backend sits below any `Document`: it is handed paths and glyphs, not a file, so it
accumulates and something above it records.

## Consequences

The count is three and all three are deliberate. `Decision` coverage is 84 sites, and
eight of the nine conforming samples still record nothing while `samples/fy05.pdf` records
its one real 14.3.3 ambiguity — the property `metadata.rs` already holds a test for, and
the property this work could most easily have destroyed.

**The `Decision` row could not see the two new renderer sites**, because it named five
crates and `fepdf-render` was not among them; it read 82 where the truth was 84. That is
the third time this figure has been wrong for that reason, and the row's own comment
predicted it — "a row that names the places it looks will keep missing new ones". It now
derives from the same workspace partition Phase Q gave the log row, so the two are
complements. The pattern is not fixed: other rows in `status.sh` still name their places,
and nothing checks which.

**An entry that says "convert thirteen things" is a claim about thirteen things.** Four of
them did not survive being measured, and the two largest were reporting that TrueType fonts
are TrueType and that Type 3 fonts have no font program. Nothing about reading the roadmap,
the function names or the messages themselves would have shown that — only counting the
firings did.
