# ADR-0036: A base encoding is not a CMap, and a solidus is not a glyph name

- **Status**: Accepted
- **Date**: 2026-08-29
- **Commit**: 6b8688b

## Context

After `/ActualText` (ADR-0035), the corpus lost 38,264 glyphs of 16,321,270, and 36,914 of
them — 96% — were one file. Asked what they were, `intel_sdm.pdf`'s losses were not
exotic:

| code | character | lost |
|---|---|---|
| `0x97` | `—` | 8,563 |
| `0x95` | `•` | 6,558 |
| `0x93` `0x94` | `“` `”` | 12,295 |
| `0xAE` | `®` | 4,940 |
| `0x92` | `’` | 2,226 |
| `0x96` | `–` | 1,061 |

Every one is above `U+007E`, and the fonts are Verdana, Times New Roman, Arial and
NeoSansIntel — a technical manual losing its bullets and its dashes. 1,600 of the
document's font references declare `/Encoding /WinAnsiEncoding`.

**No file in the engine contained the string `WinAnsi`.** A named `/Encoding` went to
`cmap::CMap::load_named`, which searches Adobe's *CMap Resources*: CJK character
collections, which have never contained an Annex D table and are not the same kind of
object. The lookup returned `None`, the font was left with no encoding at all, and every
code the ASCII guess could not reach — it stops at `U+007E` — came back unnamed, with
nothing recorded to say an encoding had been asked for and not found.

## Decision

**`/WinAnsiEncoding` is carried as a table (D.2), consulted before the CMap loader.**

* **Code to text, not code to glyph name**, though Annex D is written in glyph names. The
  name is an intermediate: resolving one goes through an Adobe Glyph List that carries 61
  entries and answers the empty string for everything else. Composing the two at
  table-generation time gives the same answer without that step.
* **The two substitutions D.2 names are made**: `0xA0` is a space rather than a no-break
  space, `0xAD` a hyphen rather than a soft one. The table is CP1252 everywhere else, and
  the five codes CP1252 leaves undefined are absent here too.
* **`MacRomanEncoding` and `StandardEncoding` are deliberately not carried.** They are
  equally published and equally absent, and nothing in the corpus names one. A table
  written from memory against no document that exercises it is how a wrong entry gets in
  and stays.
* **A name that resolves to nothing records a violation**, saying whether it is an
  encoding this engine does not carry (D.2) or not an encoding at all (9.6.6.1). That is
  the difference between a gap and a silence, and it is what `MacRomanEncoding` gets
  instead of a guess.

**A mapping's value is a glyph name only when something follows its slash.**

The first run of the new table made things worse — 36,914 lost became 41,106 — and all of
the new loss was one code:

```
41058  encoding  code 0x002f
```

A `CMap`'s mappings carry two kinds of thing in one `String`: `/Differences` and a
`bfchar` with a name destination store `/glyphname`, everything else stores the characters
themselves, and the leading slash was the whole test. `U+002F` is a character. Every base
encoding names it, at `0x2F`, and its slash was read as an empty glyph name.

The collision is decidable, because a name token has something after its slash. That is
now the test, in both places that made it.

## Consequences

`intel_sdm.pdf` loses **48 glyphs of 12,150,074**, from 36,914. The corpus loses 1,398 of
16,321,270, from 38,264. Page 40 matches PDFKit, bullets and `®` and dashes included.

**The solidus was a latent defect with a wider reach than this change.** Any `/ToUnicode`
mapping a code to `U+002F` had been returning the empty string for as long as the sniffing
existed. The corpus does carry such mappings — two codes in `volvo_xc90.pdf` — but no
drawn glyph reached one: that file's loss tally is unchanged across this fix, and its
extracted solidi match PDFKit exactly, 878 against 878, as `intel_sdm.pdf`'s do at 41,116.
It took a table that names every ASCII character to make the convention produce a number,
and the number it produced was 41,058.

What remains is 1,350 glyphs in `fy05.pdf`, at codes `0x02`–`0x07` through a `/Differences`
array, and 48 in `intel_sdm.pdf`. Both are small enough to be looked at one at a time.

**The table rests on one document.** `intel_sdm.pdf` supplies 1,600 of the corpus's 1,666
`/WinAnsiEncoding` references; the other four files that name it contribute tens each, and
none of them exercises a code the first does not. A second document would not add much,
but it is worth saying that this is one document's worth of evidence rather than a
corpus's.
