# ADR-0010: A `/ToUnicode` synthesised from glyph ids destroys text

- **Status**: Accepted
- **Date**: 2026-08-16
- **Commit**: the Phase C round-trip audit

## Context

Cross-checking the corpus with PDFKit after every round trip left one file
disagreeing. `samples/fy05.pdf` — 846 pages, unencrypted, nothing to do with clause
7.6 — came back 93 characters short, and stayed on the open list for two commits as
"unexplained".

Ninety-three turned out to be the count of *pages* that differed, not characters. Five
of them lost their text entirely:

| Page | Source | Output |
| ---: | :--- | :--- |
| 1 | `令和5年度決算検査報告 / 会 計 検 査 院` | *(nothing)* |
| 3 | `目 次` | *(nothing)* |
| 22 | `第1章 検 査 の 概 要` | *(nothing)* |

The engine's own extractor reads nothing from those pages either, in the source or in
the output — a separate limitation. The loss here is that **PDFKit reads them from the
source and not from what the engine writes**, which makes it the engine's doing.

The page's only font is `RyuminPr6N-Heavy-Identity-H`, a `Type0` whose descendant is an
embedded `CIDFontType0` subset. The source carries no `/ToUnicode` for it. The output
does — the refinement pass adds one:

```rust
// HARDENING: Only inject a generated ToUnicode map if it's missing.
// This prevents clobbering authoritative subset mappings in documents like unicode_16.pdf.
```

The guard is sound as far as it goes: it never overwrites a map the file supplies. The
defect is in what it inserts. `generate_standard_tounicode` builds the CMap from
`unicode_to_gid`, which is a **glyph id** map. Under `Identity-H` the codes in the
content stream are CIDs. Glyph ids equal CIDs only for a `CIDFontType2` written with
`CIDToGIDMap /Identity`; a `CIDFontType0` maps CIDs through its CFF charset, and
`CIDToGIDMap` does not apply to it at all.

So the file gained a `/ToUnicode` keyed on the wrong numbers, and a reader trusts
`/ToUnicode` over the registry ordering it would otherwise resolve through. A wrong map
is worse than no map.

Measured across the corpus, output read by PDFKit:

| File | Source | With the map | Without |
| :--- | ---: | ---: | ---: |
| `fy05.pdf` | 251,922 | **251,829** | **251,930** |
| the other eight | — | identical | identical |

The synthesis helped no file and harmed one.

## Decision

Stop synthesising. `normalize_type0_font` no longer inserts anything.

`generate_standard_tounicode` stays, with no caller and a note saying why: the narrow
case where a glyph-keyed map is correct is real, and reinstating it needs a file that
proves it plus a check that the descendant is a `CIDFontType2` with an identity map.
`generate_tounicode_from_utf8`, which had no caller before this change either and keyed
its map on UTF-8 byte sequences, is deleted.

## Consequences

- Thirteen of fourteen files now round-trip with their text preserved exactly. `fy05`
  comes back eight characters *longer* than its source, spread as ±1 over 78 of 846
  pages — a word-spacing difference, not lost content, and still unexplained.
- **A tolerance labelled "hardening" was doing harm.** The comment names the risk it
  guards against and not the one it creates, which is the shape to watch for: the guard
  was tested against clobbering, never against what it wrote when it did fire.
- The engine's text extractor reads nothing from those five pages, before or after.
  That is a separate gap and is not addressed here; the file is now no worse for having
  passed through.
- Nothing internal could have found this. The engine extracts no text from the affected
  pages either way, so its own before-and-after comparison is identical. It took a
  reader that *can* read them. This is the third time (ADR-0006, ADR-0009) and the
  pattern is now explicit: **compare against another implementation, on output as well
  as input.** `scripts/test/crosscheck_roundtrip.sh` does it, and `status.sh` runs it.
- That check had to be written twice. The first version compared document character
  totals against a one-percent threshold, and re-injecting this very defect did not
  trip it: emptying five pages of an 846-page file moves the total by 0.02%. It counts
  **pages that had text and now have none** instead, which is the failure named rather
  than a proxy for it.
