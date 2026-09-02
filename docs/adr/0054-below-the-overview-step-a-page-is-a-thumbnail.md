# ADR-0054: Below the overview step a page is a thumbnail, which is what makes the budget stop mattering

- **Status**: Accepted
- **Date**: 2026-09-02
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0052](0052-the-scene-budget-is-counted-before-the-scene-is-submitted.md) stopped the
viewer submitting a scene that underflows vello's `binning_size`, by counting what fits and
drawing that. It did not remove the reason the count is ever reached.

**The reason is that a zoomed-out viewport composes vector scenes.** Every visible page's
full encoding goes into one `Scene`, scaled down, so the cost of a page on screen is the
cost of everything on that page — whether it is drawn at 800 pixels or at 60. Thirty-nine
consecutive pages of `intel_sdm.pdf` cross the fixed bin-data buffer that way, and the zoom
floor put 138 pages of `volvo_xc90.pdf` on screen in one measured window.

**Most of the machinery for the alternative was already written and unreachable.**
`render_thumbnail`, `thumbnail_textures` and `thumb_renderer` were complete and marked
`#[allow(dead_code)]`; `clear_thumbnails_pending` and `invalidated_thumbnails` were set by
every page mutation and consumed each frame. Content staleness was therefore already
handled — invalidation removes the entry, so the next request rebuilds it.

**What was not handled is that the cache only grew.** `thumbnail_textures` had no bound. At
200×280 RGBA a thumbnail is 224KB, so `intel_sdm.pdf`'s 5,057 pages is **1.1GB of texture**
for a reader who scrolls to the end.

## Decision

**At or below `PDFView::OVERVIEW_STEP` (33%) each page is drawn from its own small texture
instead of being composed into a viewport scene.** A thumbnail costs one draw whatever the
page holds, so the budget stops being reachable; ADR-0052's count remains as the backstop
for the zooms that still compose.

**33% is where the pixels run out anyway.** An A4 page is 196 wide at that zoom and 60 at
the floor, so a single thumbnail 200 wide serves every zoom that uses thumbnails without
being upscaled. Above 33% a downscaled full render is the better image and is what is used.

**The cache is bounded to what is on screen plus 64**, evicted least-recently-used, and a
frame creates at most 8 new thumbnails. Pages already held do not count against that quota,
or a screen of 138 pages with 130 of them ready would fill in eight at a time having done
no work. Pages still waiting keep the placeholder card that already existed.

**The two sources of pixels are a type, not a flag.** `PagePixels` is either one viewport
texture or a map of per-page thumbnails; passing both and choosing inside would allow a
state that means nothing, and `show_virtual` already takes fifteen arguments.

## Consequences

The composition that reaches vello's ceiling is not performed at the zooms that reach it.
Scrolling in overview costs a texture blit per page rather than a re-encode.

**Eviction is by recency, not by page number.** The obvious cheap version — drop the
highest indices — throws away the page the reader is looking at.

**Each part fails a test when removed**, verified by removing each in turn: no quota (1),
held pages consuming the quota (1), no eviction (2), eviction by page number (1), and
evicting everything past the limit (2). The decisions are pure functions
(`pages_to_create`, `stale_thumbnails`) because everything around them needs a GPU.

**Measured at the zoom floor on `intel_sdm.pdf`, the quota never binds.** In a 1466x876
viewport at 10% the flow puts 23 pages in a row and 11 rows on screen — **253 visible**,
which is exactly what the arithmetic gives. `ensure_thumbnails` costs **0.05ms** on a frame
that creates nothing, 0.50ms for one, 0.79ms for two and 1.68ms for three; and **no frame
ever created more than three**, because the worker supplies scenes at that rate. Raising
the quota from 8 to 64 changed nothing: still a peak of three. So 8 is safe — eight would
be around 3ms of a 16.7ms frame — but it is not what limits the fill. **Scene production
is**, which is where a later improvement would go: a thumbnail does not need the full
vector scene it is currently built from.

**Eviction was never reached either.** The cache peaked at 253, the visible count, against
a limit of 253 + 64; thumbnails are only ever created for visible pages, so the headroom
matters only while scrolling, which this measurement did not exercise.

**The limit being relative to `visible` is the weak part of it.** At 253 pages the cache is
52MB, but nothing bounds `visible` itself. One frame during a zoom transition reported 1,960
visible; that figure could not be reproduced and is not explained — at the floor the
arithmetic gives 253 — but a limit of `visible + 64` would have authorised 450MB had it
been real. An absolute cap alongside the relative one would cost nothing.
