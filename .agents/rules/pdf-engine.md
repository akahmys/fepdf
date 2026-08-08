# PDF Engine Constraints (Core Layer)

> [!IMPORTANT]
> Prescriptive constraints for the ingestion and normalization pipeline.
> Design specifications: @docs/specs/core-pipeline.md

---

## 1. Content Stream Sublimation

- Complex operators with implicit side effects (e.g., `TD`, `"`, `'`) MUST be decomposed into sequences of atomic IR commands (e.g., `SetTextLeading`, `MoveToNextLine`, `ShowText`).
- The `EndPath` (`n`) operator MUST be explicitly preserved. Discarding `n` triggers "path construction leakage" where clipping paths are erroneously inherited by subsequent painting operations.
- Explicit `SetWritingMode` commands MUST be injected into the IR stream during font selection (`Tf`) to flatten Writing Mode state.
- Original color space semantics (Gray, RGB, CMYK, Lab, ICCBased) MUST be maintained throughout the IR pipeline. Downgrading to RGB at the sublimation stage is prohibited.
- Non-standard "leaked" data in content streams (e.g., development debug logs) MUST be detected and sanitized.

## 2. Font SFNT Modernization

- Font format identification MUST rely strictly on binary signatures (Magic Bytes), not dictionary subtypes.
- Naked CFF or Type 1 outlines MUST be encapsulated into a minimal Virtual OpenType (SFNT) container.
- CIDFonts MUST utilize `/W` parsing. Standalone `CIDFontType0/2` resources MUST NOT use `/Widths` to prevent the 1000-unit default width regression.
- Custom or unsupported font type fallback MUST trigger an explicit `log::warn!` instead of a silent `log::debug!`.

## 3. Handle Stability

- Persistent structural components (Catalog, Page, StructTreeRoot) MUST utilize stable `Handle<Object>` references. Direct storage of volatile `DictHandle` is prohibited.
- `Handle<Object>` MUST be resolved to `DictHandle` at the point of access (late-bound dictionary resolution).

## 4. Logical Structure Traversal

- During DFS/visitor tree traversal, the engine MUST utilize infallible, dynamic name interning (`arena.name("K")`) rather than manual lookup unwraps (`get_name_by_str("K").unwrap()`).

## 5. Character Resolution

- **Non-CJK**: Prioritize linguistic metadata over structural PDF claims (Physical truth → Unicode Name → SFNT cmap → Identity fallback).
- **CJK**: Prioritize structural CID mapping (`Char Code → CMap → CID → GID`) to preserve document-specific glyph selection.

## Reference

- @docs/specs/core-pipeline.md — Five-phase pipeline design & sublimation memory model
