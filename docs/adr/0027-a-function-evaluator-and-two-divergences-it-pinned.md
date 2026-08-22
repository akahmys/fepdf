# ADR-0027: The shading function is sampled, and where PDFKit is wrong the check pins rather than yields

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: 2834347

## Context

`ROADMAP.md` Phase P opened with "there is no PDF function evaluator (7.10), and two
visible defects come from it", measured against PDFKit at `DISAGREE by 229` on a
`/Separation` fill and `by 101` on a stitching gradient. Building the evaluator — types
0, 2, 3 and 4 — settled the arithmetic. It did not settle three questions, and each of
the three is a choice with a defensible alternative.

**The first was not 7.10 at all.** `/Spot cs` names a *resource*, and `handle_cs` matched
the operand against device-space names only, so the separation resolved to
`ColorSpaceKind::Unknown` before any function could have been reached, and `scn` then
guessed the colour model from the operand count — one number, read as grey. An evaluator
alone would have fixed nothing. The entry named one defect and there were two, stacked.

**A shading wants a function of a continuous parameter; the renderer wants stops.** Vello
interpolates linearly between colour stops, so the function has to be reduced to a finite
list somewhere.

**Two files then disagreed with PDFKit for reasons that were not this engine's.**
`target/colour/separation.pdf` went from `254 254 254 254` to `0 254 254 254` against
PDFKit's `25 255 255 255` — the tint transform now runs and the quadrant is black instead
of white, but `/DeviceCMYK` → RGB here is `(1 − c)(1 − k)` and PDFKit puts a CMYK profile
through it. And `UnknownFilter-ICC.pdf` moved from agreeing to disagreeing *because it
started working*: its ICC profile stream carries `/Filter /XXXDecode`, 8.6.5.5 and Table
65 make `/Alternate` default to the space `/N` determines, `/N` is 4, and `1 0 0 0 scn`
in `/DeviceCMYK` is cyan. Sampled with a colour histogram rather than inferred from the
means: this engine paints `0 255 255` and `255 0 255`, PDFKit paints `0 0 0` for both and
logs a CoreGraphics error.

## Decision

**Sample the shading function at 33 points rather than solve it.** The renderer
interpolates linearly between stops, so a piecewise-linear function is reproduced
*exactly* when its breakpoints land on the grid; 33 points puts a stop on every 1/32,
which covers the halves, quarters and eighths `/Bounds` are written at. The alternative —
walking a stitching function's `/Bounds` and emitting a stop per breakpoint — is exact for
type 3 and no help for type 4, which is the type that actually needs the resolution. The
constant is named and the docstring says it is a sampling, so the approximation is
visible where it is made rather than discovered later.

**Evaluation returns `Option`, and a type 4 program that will not run fails.** An unknown
operator is not skipped: skipping one leaves the stack the wrong depth, the remaining
operators consume the wrong operands, and the result is a plausible colour computed from
nonsense. The caller falls back and records a `Decision` (RR-15 Rule 20) instead.

**Where PDFKit is the one that is wrong, `crosscheck_image.sh` pins rather than yields.**
Both files carry an `expected_divergence` entry holding the four numbers *this* engine
produces and the reason. The check still renders both sides and still prints both; it
fails if this engine moves off the pinned four, and it also fails if a pinned file starts
agreeing, so the list cannot rot. Both failure modes were verified by forcing them.

**`/Indexed` stays on the old operand-count path.** Its operand is an index into a
palette, not a colour, and turning it into one needs the lookup table the image path owns.
Routing it through the new resolver would change what it paints on files this change did
not measure.

## Consequences

The gradient agrees with PDFKit (`by 101` → `worst 1`). The separation's defect is fixed
and its residue is now isolated to one named cause, which `ROADMAP.md` Phase P carries as
its own entry: `/DeviceCMYK` → RGB is not colour managed, and `color/mod.rs` had claimed
for as long as it existed that `moxcms` made it so. That header is corrected in place.

A bare red check was tolerable while it stood for a defect being worked on. Two of them,
standing for a defect *not* being worked on and a defect that is someone else's, would
have been red forever and would have stopped being read — which is the failure mode the
pin exists to prevent, and the reason it fails on a stale entry as loudly as on a moved
one.

Line cap, line join, flatness and rendering intent: `i` and `ri` consumed no operands at
all before this, because they fell into a catch-all. `/Perceptual ri` ahead of a `scn`
made the colour operator count one operand too many and take its fallback arm — which is
why `UnknownFilter-ICC.pdf` was painting black and *agreeing with PDFKit for the wrong
reason*. Two defects cancelling is indistinguishable from correctness in a four-number
comparator, and only fixing one of them made either visible.
