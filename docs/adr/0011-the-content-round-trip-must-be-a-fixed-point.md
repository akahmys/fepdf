# ADR-0011: The content round trip must be a fixed point

- **Status**: Accepted
- **Date**: 2026-08-16
- **Commit**: the signature investigation

## Context

Before deciding what fepdf should do about digital signatures, one question had to be
answered: opening a conforming PDF 2.0 file and saving it unchanged — is the result the
same document?

It was not, and it was not even stable. Running `samples/sample.pdf` through
`publish upgrade` and feeding each output back in:

| Pass | Size | Objects differing |
| ---: | ---: | :--- |
| 1 | 907,400 | — |
| 2 | 907,487 | 14 of 80, 13 in digits only |
| 3 | 907,539 | 14 of 80 |
| 5 | 907,643 | 14 of 80 |
| 7 | 907,747 | 14 of 80 |

Exactly 52 bytes per pass, indefinitely. There was no fixed point.

## The clipping defect

`W` and `W*` set a clipping path. They do not end it — the painting operator that
follows does, and `n` is the one that paints nothing.

The parser read a bare `W` as `Command::Clip` and left the following operator as its
own command. The serialiser wrote `Clip` back as **`W n`**. So the `n` was counted
twice:

| Source | Round trip | Effect |
| :--- | :--- | :--- |
| `re W n` | `re W n n` | grows by one operator per pass |
| `re W f` | `re W n f` | **the fill is lost** — `n` ends the path before `f` can fill it |
| `re W* S` | `re W* n S` | **the stroke is lost** |

The growth was the visible symptom: 26 clipping paths in `sample.pdf` at two bytes each
is the 52. The lost paint was the serious one, and nothing reported it.

`Clip` now serialises as `W` alone. Every case above is a fixed point, and the only
difference between two passes of the whole file is the XMP packet, whose
`xmpMM:InstanceID` is a fresh UUID per instance — the noise floor Phase A already
documented.

## The header-fill heuristic

The same parser replaced `f` with `n` — deleting a fill — whenever the preceding
rectangle had `y1 > 700`, height under 15 and width over 500:

```rust
// Heuristic: Suppress suspicious header bar fills that are likely PDF generator bugs.
// In Intel SDM, pages 2-4 have 're f' at the top where other pages have 're W n'.
```

The comment names the file it was written for. Measured across the corpus, it fired
**1,738 times on `intel_sdm.pdf` and 902 times on `volvo_xc90.pdf`** — a file it was
never aimed at, where the rectangles it deleted were one point high and 681 wide at
y-coordinates above 9,000. Those are rules, not header bars. The `y1 > 700` test
assumed a user space it had no way to check.

Removing it changed the extracted text of no corpus file. It helped none and deleted
marks on two.

It also reported through `log::info!` only, which `ARCHITECTURE.md` §5.3 calls a silent
acceptance and therefore a defect regardless of whether the output is right.

## Rectangles at full precision

`re` operands were written with `{}` where every other number in the serialiser uses
`{:.6}`, so they came out as `0.12000000000000455` and `12.600000000000001`.

The digits are not noise from nowhere. `re` gives x, y, width and height; the parser
adds to reach x1 and y1, and the serialiser subtracts to recover width and height.
Add-then-subtract on binary floating point does not return the input.

Six places is finer than any rendering distinguishes and makes the value parse back to
the same rectangle, so the round trip is a fixed point rather than merely a shorter
string. It did **not** explain `fy05.pdf`'s eight extra characters, which was the
reason for looking — that remains open.

## Decision

Serialise `Clip` as `W`/`W*` alone, write `re` operands at six places, and delete the
header-fill heuristic.

Round-tripping content is now a fixed point up to the XMP instance identifier, and
`serializer.rs` holds a test asserting exactly that for the clipping cases.

## Consequences

- **A processor that cannot rewrite a document unchanged cannot preserve anything that
  depends on the bytes.** That is why this was worth settling before signatures: a
  signature covers a byte range, and an engine whose output drifts on every pass has no
  hope of carrying one. The drift is fixed; the deeper obstacle — normalisation at load
  means there is still no faithful-copy path — is not, and is recorded in `ROADMAP.md`.
- **Deciding a producer's mark is a bug, from its geometry, is not something this
  engine can know.** The heuristic is the third instance of the same shape after
  ADR-0008 and ADR-0010: a well-meant correction, tuned to one file, that damaged
  others and said nothing. In all three the test was the same — *which files does it
  help?* — and in all three the answer was none.
- The clipping tests asserted whole output strings and failed when `re` operands
  changed format — a change unrelated to what they check. They compare operator
  sequences now. A test that breaks on an improvement it does not cover is a test
  written at the wrong altitude.
- Idempotence is now an assertable property rather than an assumption. It was neither
  asserted nor measured before, and the 52 bytes had presumably been accumulating for
  as long as the serialiser existed.
