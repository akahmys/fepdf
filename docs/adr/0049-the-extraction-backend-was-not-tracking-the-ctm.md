# ADR-0049: Sorting by `y` required a `y`, and the extraction backend was not tracking the CTM

- **Status**: Accepted
- **Date**: 2026-09-01
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0047](0047-text-extraction-sorts-runs-into-reading-order.md) gave text extraction a
reading-order sort. Its Context quotes the figure that sized the work — 7,093 of 7,727
pages differing from PDFKit by order alone — and its Consequences quote no figure at all.
[ROADMAP Phase T](../../ROADMAP.md) had set the completion condition as *"the order-only
column falls, **no file's identical column falls**, and a check fails when it does"*. The
first clause was met, the third was never built, and so the second went unmeasured.

Re-derived, it had not held:

| | before the sort | after it |
| :--- | ---: | ---: |
| `intel_sdm.pdf` | 15 | 1,909 |
| `constitution.pdf` | 1 | 12 |
| **`volvo_xc90.pdf`** | **61** | **0** |
| **`unicode_16.pdf`** | **28** | **7** |
| **`bokutokitan.pdf`** | **93** | **4** |
| **`fy05.pdf`** | **45** | **11** |
| corpus | 261 | 1,975 |

The total rose by 1,714 and four files fell. **A net figure hides a file**, which is why
the check this record adds compares per file and not per corpus.

`volvo_xc90.pdf` was the sharpest: 0 of 415 pages agreeing, and not as a matter of taste —
page 11 is a single-column page whose extracted text ran bottom to top, with a heading
emitted after its own paragraph.

**Where it came from.** Sorting the spans of that page by descending `y` — taken from
`extract_spans`, which uses `CollectorBackend` — reproduces PDFKit's order exactly. So the
geometry was right and the sort was reading a different number. The two backends sit beside
each other in one file:

```rust
// CollectorBackend                     // TextExtractionBackend
fn transform(&mut self, m: Affine) {    fn transform(&mut self, _affine: Affine) {}
    self.current_transform *= m;        fn set_transform(&mut self, _affine: Affine) {}
}                                       fn push_state(&mut self) {}
                                        fn pop_state(&mut self) {}
```

`show_text` is handed the **text** transform, not the whole CTM. `CollectorBackend`
composes the CTM it tracks; `TextExtractionBackend` tracked none, so a run's `y` was its
offset from whatever `cm` was last in force. `volvo_xc90.pdf` draws its note boxes inside
`q … cm … Q`, so each box's runs sorted against their own origin.

**This was harmless until ADR-0047.** Extraction emitted runs in arrival order and never
consulted `y`; the omission cost nothing and nothing could see it. Ordering by `y` gave the
number meaning, and an untracked CTM became a defect the same day.

## Decision

**`TextExtractionBackend` tracks the CTM**, with the same four methods and the same
`transform_stack` as the `CollectorBackend` beside it, and composes it where it reads a
position.

**`scripts/test/crosscheck_reading_order.sh` is the check Phase T asked for.** It compares
per page against PDFKit and classifies each as *identical*, *order-only* (the same
characters in a different sequence) or *content*, after stripping whitespace — the two
readers disagree about spacing by design (§9) and that is a different question. Each file
carries a **floor** it may not fall below and the **best** it has ever read; a file under
its best prints on every run without turning the suite red, because a check that is red is
a check nobody uses.

## Consequences

| | before the sort | after it | with the CTM |
| :--- | ---: | ---: | ---: |
| `volvo_xc90.pdf` | 61 | 0 | **182** |
| `unicode_16.pdf` | 28 | 7 | **707** |
| corpus | 261 | 1,975 | **2,857** |

Two of the four regressed files now read **past where they were before the sort existed**.
608 workspace tests pass, unchanged.

**Two files remain below their best** — `bokutokitan.pdf` at 4 against 93 and `fy05.pdf` at
11 against 45 — and the cause there is different: vertical Japanese puts ruby beside the
column it annotates, and the sort reads that offset as a separate line, so `まもり` leaves
the `守` it belongs to. That is Phase U's, and the floors hold the current readings so it
cannot quietly get worse first.

**Every part of the fix has something that catches its removal**, verified by removing each
in turn:

| removed | caught by |
| :--- | :--- |
| `transform` | both unit tests, and the crosscheck |
| `set_transform` | the crosscheck |
| `pop_state` | both unit tests |
| composing the CTM at the read | both unit tests, and the crosscheck |

**The first version of those unit tests passed against the defect.** They gave both runs
the same text `y` and differed only in the `cm`, so dropping the composition left the two
tied and arrival order settled it correctly by accident. They are built now so the text
transforms say one order and only the CTM says the other. This is the second time in two
sessions that a test written for a defect was green against it — the first was a sample
that happened not to flake ([ADR-0043](0043-the-scene-repeats-and-the-rasteriser-does-not.md))
— and both were found by the same habit of removing the fix and watching.
