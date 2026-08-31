# ADR-0046: Font construction is unified at load time

- **Status**: Accepted
- **Date**: 2026-08-31
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0045](0045-normalisation-at-load-does-not-reach-fonts.md) recorded that `normalize_resources`
cleared the font cache immediately after ingestion, forcing `Interpreter::get_font` to lazily
reconstruct fonts during rendering by merging Type0 descendants, propagating `/Encoding`, `/ToUnicode`,
and `/WMode`, and regenerating virtual SFNT tables on demand. As measured in ADR-0045, the font cache
was empty (0 fonts) on every sample document when `Document::open` returned, and mutated after open.

Furthermore, `normalize_resources` ended with `resolve_missing_font_data()`, an inert step that attempted
to mutate non-embedded fonts by stuffing system fonts into their raw font data, which corrupted standard
fallback glyph resolution when executed against a live cache.

## Decision

1. **Unified Font Ingestion**: Moved Type0 descendant font resolution directly into
   `FontResource::load`. When a Type0 composite font dictionary is encountered, its descendant CIDFont
   dictionary is loaded with the parent's `/Encoding`, `/ToUnicode`, and `/WMode` propagated before
   lifecycle initialization and reconstruction take place.
2. **Eliminated Font Cache Eviction**: Removed `self.font_cache.write().clear()` from
   `normalize_resources`. Fonts ingested during `Document::open` are retained in `font_cache`.
3. **Streamlined Interpreter Font Resolution**: Replaced the redundant on-demand reconstruction
   logic in `Interpreter::get_font` with a direct call to `self.doc.get_font(h)?`.
4. **Removed Corrupting Inert Fallback**: Removed `resolve_missing_font_data()` so that non-embedded
   fonts preserve their distinct non-embedded lifecycle and rely on the renderer's standard
   system fallback mechanisms.

## Consequences

- **100% Cache Invariance**: As verified by `load_state_probe` across all sample documents,
  `at open == after` across 100% of pages (e.g. `bokutokitan.pdf`: 9/9, `fugaku.pdf`: 36/36,
  `fy05.pdf`: 158/158, `intel_sdm.pdf`: 53/53, `unicode_16.pdf`: 40/40, `volvo_xc90.pdf`: 8/8).
  Zero post-open cache mutations occur.
- **Single Source of Truth**: Font decisions recorded during ingestion describe the exact
  `FontResource` instances used during rendering and content interpretation.
- **Visual Conformance Maintained**: Visual regression suite passes cleanly across all reference
  baselines (4/4 passed).
