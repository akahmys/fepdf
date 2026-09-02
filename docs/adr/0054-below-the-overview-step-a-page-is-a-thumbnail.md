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

**The quota and the headroom are guesses.** 8 per frame and 64 beyond the screen were not
derived from measurement; they are the numbers that keep a frame's new work small and a
scroll-back cheap. What would settle them is a frame-time measurement at the zoom floor on
`intel_sdm.pdf`, which has not been made.
