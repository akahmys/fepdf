# ADR-0041: A CID font's character collection is declared, and the engine was guessing it from the font's name

- **Status**: Accepted
- **Date**: 2026-08-30
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0036](0036-a-base-encoding-is-not-a-cmap.md) closed with what was left: *"1,350 glyphs
in `fy05.pdf`, at codes `0x02`–`0x07` through a `/Differences` array, and 48 in
`intel_sdm.pdf`."* Asked which route each of those 1,350 reached, they were two
populations and not one:

| | glyphs | |
| ---: | ---: | :--- |
| reached the encoding and came back empty | 1,089 | codes `0x02`–`0x08`, glyph names `c033`–`c039` — [ADR-0042](0042-a-glyph-name-that-looks-like-a-character-code-is-not-one.md) |
| reached no route at all | 261 | one font, `DEBCFD+RyuminPr6N-Heavy` |

The second is this record. 261 glyphs is small, and **16 of them are the whole of page 1**:
the document's title page, 令和5年度決算検査報告 over 会計検査院, extracted as nothing.

The font is a `CIDFontType0` under an `Identity-H` `Type0`, and its `/CIDSystemInfo` says
what it is:

```
/Registry (Adobe) /Ordering (Japan1) /Supplement 6
```

The engine carries Adobe's `Adobe-Japan1-UCS2` table and consults it for a font it decides
is Japanese. It decided that by looking for `hira`, `koz`, `mincho`, `gothic`, `aj1` and
four more substrings **in `/BaseFont`**. Ryumin — リュウミン, Morisawa's mincho, and one of
the two most-set typefaces in Japanese publishing — contains none of them, so the table
never loaded and every code came back unnamed.

**Two defects, and the first hid the second.**

*The values were read as the wrong object type.* `/Registry` and `/Ordering` are **strings**
(9.7.3, Table 114) and `parse_csi_info` asked for names. A name is a different object
(7.3.5), so the accessor answered `None`. Measured over both corpora — 524 files — **116
of 116 Type0 fonts declare the pair and the engine read `None` for all 116.** Everything
that asks which collection a font belongs to was therefore deciding from the `/BaseFont`
name alone: `is_cjk`, whose first two branches read the registry and the ordering and had
never once fired; `resolve_gid`'s identity fallback; and the table loader above.

*The font that decodes was never asked.* Fixing the accessor changed nothing measurable,
because the collection was only ever read on the `Type0` — and a `Type0` is not what
decodes. The interpreter loads the descendant `CIDFont` on its own and decodes through
that resource (`fepdf-content/src/interpreter/font.rs`), and the `CIDFontType0` branch of
`parse_subtype_metrics_and_data` never looked at `/CIDSystemInfo` at all, although Table
115 puts one on every CIDFont. The deciding copy had nothing to decide from.

**And the guess was wrong in the other direction too.** Nineteen fonts of the external
corpus declare `Adobe-Korea1` or `Adobe-China1` and carry `Gothic` in their name —
`AdobeGothicStd-Bold` is a *Korean* typeface — so the **Japanese** table was applied to
them. Adobe-Japan1 puts `フ` at CID 16128; Adobe-Korea1 puts `췎` there. The engine was
offering the first as the document's text, with nothing to say it was a guess.

## Decision

**The collection is read from `/CIDSystemInfo`, on the CIDFont as well as the Type0.**
`parse_csi_info` accepts a string in either the literal or the hexadecimal form (7.3.4) —
`fy05.pdf` writes literals, `intel_sdm.pdf` writes hex — and still accepts a name, which
is what the code already did and which no measurement argues against.

**`Ordering (Japan1)` loads the table. Another named collection does not, and says so.**
A font declaring a collection this engine carries no table for records a `Violation` of
9.7.3 naming the collection, unless it supplies its own `/ToUnicode`, in which case 9.10.3
outranks the collection and nothing is lost. Reading such a font through Adobe-Japan1
would give a Japanese character for every CID and nothing to say it was a guess, which is
the same conclusion [ADR-0010](0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md)
reached about a synthesised `/ToUnicode`.

**`Ordering (Identity)` and a font that declares nothing keep the name heuristic.**
`Identity` is the file saying the codes are the font's own glyph order (9.7.4.2) — a
statement about indexing, not about characters — so there is nothing declared to obey.
That is the case the heuristic was written for and where it is the only thing to go on:
**75 fonts of the two corpora**, among them `DFHSMinchoPro6N-W3-Identity-H` in `fy05.pdf`
and `ANDGCF+YuGothic` in `intel_sdm.pdf`, both of which it gets right.

## Consequences

`fy05.pdf` loses **1,089 glyphs of 1,006,498**, from 1,350; the corpus loses **1,137 of
16,321,270**, from 1,398. Every glyph that reached *no route at all* is now named: the
`unmapped` column is 0 across the nine samples. Page 1 of `fy05.pdf` reads
令和5年度決算検査報告 / 会計検査院.

**One glyph of the external corpus goes the other way, and that is the trade being made.**
CID 16128 in `FNCTHN+AdobeGothicStd-Bold`, a font declaring `Adobe-Korea1`, used to come
out `フ`. It is now unnamed, with a 9.7.3 `Violation` naming the collection — and PDFKit
reads no text from that page at all. One wrong character was replaced by one recorded
absence. The external corpus goes from 62 lost of 127,424 to 63.

**Rendering is unchanged.** Making `cid_ordering` real for 41 fonts also makes
`resolve_gid`'s `is_identity` false for them, which was the risk in this change: that
fallback treats a CID as a glyph index when nothing else resolves, and it had been reached
for every CID font in existence because the ordering was always `None`. Twelve pages
across all nine samples, chosen for the fonts they exercise, render identically before and
after — to within the one pixel described below.

**The renderer is not deterministic, and this is where that was found.** Three of those
twelve pages differed by exactly one isolated pixel, which is not a shape a font change
makes — so the same page was rendered repeatedly with one binary. Every one of the three
flips between two images: `sample.pdf` page 1, `fy05.pdf` page 304, `fugaku.pdf` page 1,
each with its own pixel. It is not caused by this change; it is present at `27d19bd`.
Where it lives is settled, in [ADR-0043](0043-the-scene-repeats-and-the-rasteriser-does-not.md):
the engine encodes a byte-identical scene every time and vello's GPU pipeline turns that
one scene into more than one image.

**One thing this record said about that was wrong and is corrected here.** It read that
`scripts/visual_regression.py` "cannot be trusted to give the same answer twice" and that
its `constitution.pdf` baseline failure was 27 pixels of stale baseline plus one flaky
pixel. The flaky pixel is at a **channel delta of 1** on every page measured, and that
suite already tolerates a delta of 1 — so it was never flapping, and the failure was the
stale baseline and nothing else. The baseline is refreshed and the suite passes four of
four on three consecutive runs. The conclusion was drawn from "the suite fails and there
is a flaky pixel" without reading what the suite's own comparison does with that pixel.
