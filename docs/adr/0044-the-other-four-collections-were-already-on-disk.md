# ADR-0044: The other four character collections were already on disk, and the bar for reading them had been set past what the rules ask

- **Status**: Accepted
- **Date**: 2026-08-30
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0041](0041-a-character-collection-is-declared-not-guessed.md) stopped the engine
applying the *Japanese* CID-to-Unicode table to fonts that declare a Korean or Chinese
collection, and recorded a 9.7.3 `Violation` in its place. That left a row open, and the
reason given for leaving it open does not survive being read against the project's own
rules.

`scripts/dev/fetch_font_resources.sh` already fetches Adobe's `mapping-resources-pdf`,
which contains **five** CID-to-Unicode tables:

```
Adobe-CNS1-UCS2  Adobe-GB1-UCS2  Adobe-Japan1-UCS2  Adobe-KR-UCS2  Adobe-Korea1-UCS2
```

The loader asked for `Adobe-Japan1-UCS2` **by that literal name**, whatever the document
declared. So four tables sat unopened beside the one that was read, and a font declaring
`Adobe-Korea1` got the Japanese one until ADR-0041, and nothing at all after it.

**The reason recorded for not fixing this was that nothing could verify the result**:
across 524 files exactly one glyph is drawn through such a collection, and PDFKit reads no
text from that page. That is true, and it is the wrong test.

* **Principle 3** says a corpus can justify building something and *only a use case can
  justify not building it*. Zero occurrences measures the corpus.
* **[ADR-0036](0036-a-base-encoding-is-not-a-cmap.md)** declined `MacRomanEncoding`
  because "a table written **from memory** against no document is how a wrong entry gets
  in and stays". Adobe's published file for the collection the document names is not that.

The bar had been raised to "a second implementation must confirm it", which the rules ask
for when the engine is *guessing*. Reading the table the file asks for is the opposite of
a guess.

## Decision

**The resource name is built from the declaration**: `{Registry}-{Ordering}-UCS2`, with
the registry title-cased because it is part of a filename and one corpus file writes
`adobe`. A collection with no file on disk — `Adobe-Japan2`, say — still records the 9.7.3
`Violation` rather than borrowing a table from a collection that happens to be there.

The field that holds it is `collection_map` now, not `adj1_mapping`. A name that says
Adobe-Japan1 while holding Adobe-Korea1 is the kind of lie this log keeps being written
about.

## Consequences

**What verifies it is which table gets *selected*, not what the table says.** CID 16128
reads five different ways:

| collection | CID 16128 |
| :--- | :--- |
| Adobe-Japan1 | `フ` |
| Adobe-Korea1 | `췎` |
| Adobe-GB1 | `盦` |
| Adobe-CNS1 | `鬹` |
| Adobe-KR | `樸` |

A fixture per collection asserting on that one CID fails if the wrong table is loaded,
which is precisely the defect ADR-0041 found. A test that only asked whether *some*
character came back would have passed on the wrong table throughout.

**Adobe's two repositories agree, and Japan1 is the control that makes the number
readable.** `mapping-resources-pdf` gives CID→Unicode and the CMap Resources give
Unicode→CID; they are maintained separately and for different purposes, so a round trip
catches a table read wrongly, a byte order confused, an off-by-one:

| | CIDs | round-trip |
| :--- | ---: | ---: |
| **Adobe-Japan1 (control, in use since Phase P)** | 23,060 | **67.3%** |
| Adobe-Korea1 | 18,076 | 97.4% |
| Adobe-GB1 | 30,284 | 98.5% |
| Adobe-CNS1 | 19,179 | 98.1% |

The collection already trusted scores *worst*, because Japan1 carries far more variant
forms and its Unicode→CID direction names one preferred CID per character — 9,772 reverse
entries against 23,060 forward. Without the control the three figures would look like
evidence of quality; with it they say only "at least as consistent as the one you already
ship", which is what they are.

**The corpus moves by its one glyph.** `TWG test suite A007-pdfa2-fail-a.pdf` reads `췎`
where it read nothing, and the external corpus is back to 62 lost of 127,424 from 63. The
nine samples are unchanged at 1,137 of 16,321,270.

**What is still not verified is what no corpus here can verify**: that a real Korean or
Chinese document extracts correctly end to end. Neither corpus contains one — 18 files
declare such a collection and between them they draw a single glyph through it. That is a
gap in the corpus, and it is now written down as one rather than standing in for a reason
not to read a table the document asked for.
