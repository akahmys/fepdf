# ADR-0043: The scene repeats and the rasteriser does not, so a caller who needs the same image twice asks for the CPU

- **Status**: Accepted
- **Date**: 2026-08-30
- **Commit**: (see the commit that adds this file)

## Context

`samples/sample.pdf` page 1, rendered eight times by one binary on one machine, comes out
as **more than one PNG** — four and four across eight separate `publish render`
invocations, and three distinct images when the same scene is rasterised eight times
inside one process. `fy05.pdf` page 304 and `fugaku.pdf` page 1 do the same, each with its
own pixel. RR-15 **Rule 10 makes determinism a rule**, and nothing was looking for this.

A count of variants does not say which layer stopped repeating itself, and the two answers
want opposite work: a scene built differently is a defect in this workspace, and a scene
rasterised differently is not. `examples/render_determinism` separates them — it
fingerprints the vello `Encoding` (`path_data` and `draw_data` carry every coordinate and
every colour in encoding order, so two glyphs swapping places move the hash), then hands
*one* scene to the rasteriser repeatedly:

```
samples/sample.pdf page 1, 8 runs

  scene built 8 times                    -> 1 distinct
  one scene, Gpu rasteriser, 8 times     -> 3 distinct
  one scene, Cpu rasteriser, 8 times     -> 1 distinct
```

**The engine is deterministic.** One scene from eight builds, on every page tried. The
variation is entirely in vello's GPU compute pipeline, which is below anything this
workspace owns — and vello's own CPU shaders, running the same pipeline stages on the
host, give one image from eight.

**The difference is one isolated pixel at a channel delta of 1**, measured on four pages of
four files. GPU output and CPU output differ by no more than 75 pixels of 892,785 (0.01%)
at a worst channel delta of 6, so the two rasterisers agree about the picture.

## Decision

**`Rasteriser::{Gpu, Cpu}` is a parameter, and `Gpu` stays the default.** `publish render
--cpu`, `PdfDocument::render_page_to_file_with` and `headless::render_to_bytes_with` are
the seams. A caller who wants a picture gets the GPU, which is what the GUI draws with and
what a user sees; a caller who needs the *same* picture twice — a check, a hash, a
signature over a rendering — asks for `Cpu` and says so.

**The GPU path is not made deterministic, because it cannot be from here.** Which stage of
a compute pipeline reorders a float reduction is vello's question, and a workaround at this
level would be guessing at somebody else's shader.

**`scripts/visual_regression.py` is left on the GPU.** It already tolerates a channel delta
of 1, which is exactly the size of the defect, so it does not flap — and moving it to the
CPU would stop it exercising the pipeline users actually get, in exchange for nothing. What
changed there is that the tolerance now says *why* it exists and what it costs, with the
measurement beside it. A tolerance nobody can justify is indistinguishable from one that is
hiding something.

## Consequences

The suite passes **4 of 4 on three consecutive runs**, which it had not done at `27d19bd` —
for an unrelated reason, recorded below.

**A claim in [ADR-0041](0041-a-character-collection-is-declared-not-guessed.md) was wrong
and is corrected there.** It read that the visual suite "cannot be trusted to give the same
answer twice" and that its `constitution.pdf` failure was a stale baseline *plus* the flaky
pixel, so that refreshing the baseline would leave it passing about half the time. The
flaky pixel is delta 1 and the suite tolerates delta 1: it was never flapping. The failure
was the stale baseline alone — 27 pixels at a delta of 222, the page number `1` at the foot
of page 1, which the engine draws, the frozen reference lacks, and PDFKit reads in the
page's text. The baseline is refreshed and the suite catches its replacement being put back — locally,
because `.gitignore` excludes `/samples/` and the baselines are therefore not tracked, so
this repair travels with the machine and not with the commit.

The reasoning that produced the wrong claim is worth naming, because this repository keeps
meeting it: **the suite was observed to fail and a flaky pixel was observed to exist, and
the two were joined without reading what the suite's own comparison does with that pixel.**
`TESTING.md` already says to read the last line of the audit rather than the first; this is
the same mistake one level down — reading a check's *result* instead of its *rule*.
