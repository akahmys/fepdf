# ADR-0042: A glyph name that looks like a character code is not one, and the 1,089 glyphs it names are not characters

- **Status**: Accepted
- **Date**: 2026-08-30
- **Commit**: (see the commit that adds this file)

## Context

After [ADR-0041](0041-a-character-collection-is-declared-not-guessed.md), the whole of what
the corpus loses to extraction is 1,137 glyphs of 16,321,270, and 1,089 of them are one
document, one producer, and seven glyph names:

| name | glyphs |
| :--- | ---: |
| `c033` | 401 |
| `c037` | 160 |
| `c036` | 158 |
| `c034` `c035` `c038` | 122 each |
| `c039` | 4 |

They arrive at codes `0x02`–`0x08` through a `/Differences` array, in **127 subsetted
Type1 fonts** of `samples/fy05.pdf`, all named `EdiF-uSK…` and none carrying a
`/ToUnicode`. The Adobe Glyph List does not contain `c033`, and neither the AGL
specification nor ISO 32000-2 says what such a name means, so the engine answers the empty
string and records a 9.10.2 `Violation` with the count.

**PDFKit answers `!`.** Reading the same document, it takes the digits of a `cNNN` name as
a character code and emits `!` `"` `#` `$` `%` `&` `'` for `c033`–`c039`. The reading is
self-consistent, it is what an independent implementation does, and adopting it would have
closed the last open row in the text table.

It is also wrong, which took rendering the pages and looking at them to establish.

**Page 304.** Ten `c033` in `DEGPPN+EdiF-uSKTqEs5ei4V1d-01p`, and PDFKit puts ten `!`
into the text — `廃止するた!め!池`. Rendered, they are the **圏点**: the emphasis dots set
above ため in ため池, five occurrences, two dots each. The remaining twelve glyphs on that
page, `c034`–`c039` at two apiece, are the top, middle and bottom **pieces of stretched
parentheses** in two column headings — `(国庫補助対象事業費)` set over three lines, drawn
as three pieces a side.

**Page 30.** The same seven names, a different subset font, and PDFKit's own output gives
the anatomy away by printing the pieces in order:

```
〈13 件分〉
!                     ⎡ left bracket, top
% % % % %             ⎢ left bracket, five extensions
"                     ⎣ left bracket, bottom
506 億3269 万円
41 億2851 万円
#                     ⎤ right bracket, top
…
& & & & &             ⎥ right bracket, five extensions
1026 億6858 万円
$                     ⎦ right bracket, bottom
```

Two such brackets on the page, one over five lines and one over three, which is
`c033`×2, `c034`×2, `c035`×2, `c036`×2, `c037`×7, `c038`×7 — the counts measured, exactly.

**So the same name denotes different glyphs in different subset fonts of the same file.**
`c033` is an emphasis dot in `DEGPPN` and the top corner of a square bracket in `DEBKGJ`.
The digits are a subset-local index the producer assigned, not a character code, and
nothing outside the font knows what they mean.

## Decision

**`cNNN` is not mapped. The engine emits nothing for these glyphs and records that it
did.** The existing 9.10.2 `Violation` already names the count, the font's route and the
page, which is what a caller needs to know that something on the page was not said.

This is a decision *against* a change that would have moved the headline number, so the
reasoning is worth stating plainly: adopting PDFKit's reading would put **1,089 spurious
ASCII characters** into the extracted text of `fy05.pdf` — `廃止するた!め!池` where the
document says 廃止するため池, and `!%%%%%"` in the middle of a column of yen figures.
[ADR-0010](0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md) settled the
principle when a synthesised `/ToUnicode` was destroying text: **a reader can see that
nothing was extracted and cannot see that what was extracted is wrong.** An empty string
is a worse-looking answer and a better one.

**What a correct answer would look like, for whoever gets there.** An emphasis dot is
styling and has no text; a bracket drawn in pieces is one `(` and one `)` however many
segments it took. Both need to know what the glyph *is*, and the only thing in the file
that knows is the outline. Comparing a subset glyph's outline against a reference face is
a real technique and a large piece of work, and it is what this row is waiting for — not a
table of names.

## Consequences

The corpus loses **1,137 glyphs of 16,321,270** and this record accounts for 1,089 of
them. The remaining 48 are `intel_sdm.pdf`'s, and asking the same question of them settles a
row that was open: they are **32 glyphs of `ANEENA+Wingdings` and 16 of `ANCODA+Symbol`**,
discarded by the private-use filter (`is_withheld`), which is the case that filter exists
for — a legacy symbol font's codepoint means nothing outside the font. **No CJK font in
either corpus reaches that filter at all**, so the concern its doc comment records — that
a CID-keyed font's `0xF0000` is a CID value being thrown away without looking at the font
type — has zero occurrences across 524 files, and the supplementary range it also calls
wrong is never entered.

**The number will not go down, and that is now on the record rather than being rediscovered.**
The next reading of the text table will see 1,089 glyphs lost in one file with a
well-known second implementation extracting them, and the obvious move is to copy it. This
exists to say what happens if you do.

**It also says something about the second implementation.** `crosscheck_roundtrip.sh` and
its siblings exist because "the engine comparing its own output to itself cannot see a
symmetric defect", and that argument is sound. It does not make PDFKit right. Here the
character counts agree glyph for glyph — 22 on page 30, 22 on page 304 — and the
characters are junk. A crosscheck that only compared counts would have passed, and a
crosscheck that compared text would have reported this engine as the one that was wrong.
