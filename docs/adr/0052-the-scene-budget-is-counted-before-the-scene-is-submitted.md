# ADR-0052: The scene budget is counted before the scene is submitted, because appending cannot be undone

- **Status**: Accepted
- **Date**: 2026-09-02
- **Commit**: (see the commit that adds this file)

## Context

The viewer panicked once on `samples/volvo_xc90.pdf`:

```
thread 'main' panicked at vello_encoding-0.9.0/src/config.rs:185:31:
attempt to subtract with overflow
```

**The subtraction has no floor.** `RenderConfig::new` computes
`binning_size: buffer_sizes.bin_data.len() - layout.bin_data_start`, where `bin_data` is a
constant `1 << 18` and `bin_data_start` is `encoding.draw_tags.iter().map(info_size).sum()`
(`resolve.rs:137`) — which grows with the scene and is bounded nowhere. vello says as much
about the constant: *"These should instead get derived from the scene layout using
reasonable heuristics."* In a release build the subtraction wraps instead of panicking, and
the wrapped value sizes GPU dispatch.

**No single page is anywhere near the line.** `examples/scene_budget` measures the same sum
the engine will make: the worst page of the nine samples is `volvo_xc90.pdf`'s page 409 at
**6,318 words of 262,144 — 2.4%**, and no page of any sample exceeds the budget.

**The viewer composes every visible page into one scene, and that is what crosses it.** The
shortest run of consecutive pages whose costs sum past the budget:

| file | pages | shortest run that crosses |
| :--- | ---: | ---: |
| `intel_sdm.pdf` | 5,057 | **39** |
| `unicode_16.pdf` | 1,140 | 126 |
| `volvo_xc90.pdf` | 415 | 158 |
| `fy05.pdf` | 846 | 204 |
| the other five | ≤195 | the whole document fits |

Against that, instrumentation in `render_viewport` measured 122 frames of `volvo_xc90.pdf`:
100% zoom put 1 page on screen and used 0.8% of the budget; 11% put 105 pages on screen at
57.7%; and **10%, the zoom clamp floor, put 138 pages on screen at 74.5%**. So the viewer
was measured 20 pages short of the line on that file — in one window size, at the floor —
and `intel_sdm.pdf` needs only 39 pages to cross it.

**What caused the original panic is still not established.** A first hypothesis — that it
came from many pages being visible — was reported as failing, because the frames captured
in the attempt to reproduce it were at the default zoom with one page visible. The zoom
history of the session that panicked was not captured, and the numbers above make repeated
zooming out the plausible path without demonstrating it. This record claims the mechanism
and the margin, not the incident.

## Decision

**A scene is counted before it is submitted, and a composition stops at the line rather
than past it.** `fepdf_render::budget` holds the budget, the cost of a scene computed the
way vello computes it, and the cost of one solid fill — measured rather than written down,
since a composer draws a page background before appending each page.

- `headless.rs` refuses the render with a message naming the cost, before
  `render_to_texture`.
- The viewer asks `pages_within_budget` how many of the visible pages fit **before
  appending any of them**, and draws that many.

**Counting afterwards is not an option**: `vello::Scene` has no way to remove what has gone
into it, so a composer that measures after appending has learned something it cannot act
on.

**The pages left out are reported in the status bar, not as a `Decision`.** Every severity
in that type describes the *document* — the sidebar renders a `Violation` as
`規格違反 [ISO 32000-2]` — and there is nothing wrong with a file that has more pages on
screen than one vello scene can hold. It is the engine's own limit and is reported as the
engine's own state.

## Consequences

A viewport that would have submitted an underflowing scene now draws as many pages as fit
and says how many it left out. A viewport that fits — every ordinary frame — is unchanged,
which is the first thing the tests check: a guard that stopped early would be worse than no
guard.

**This does not touch the other buffer.** `reject_if_nothing_was_rasterised` catches
`volvo_xc90.pdf` pages 10 and 389 coming back entirely transparent at 96 DPI and correct at
half that; those pages cost 6,318 words and 2.4% of this budget. Whatever runs out there
runs out during rasterisation, at one resolution and not another, and the budget check
cannot see it.

**Each part fails a test when removed**, verified by removing each in turn: never stopping
(3 tests), not counting the page background (1), stopping one page late (1), a cost function
that always answers zero (3), and a check that never refuses (1).

**The margin was thinner than the per-page figures suggested, and only the composed figure
showed it.** 2.4% per page reads like a wide margin; 39 pages on a document the viewer will
happily show 138 of does not. `examples/scene_budget` reports both.
