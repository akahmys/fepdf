# SDK Engine Constraints

> [!IMPORTANT]
> Prescriptive constraints for the interpretation and serialization pipeline.
> Design specifications: @docs/specs/sdk-pipeline.md

---

## 1. Interpretation (Phase 3)

- The interpreter MUST utilize exhaustive pattern matching for the `Command` IR enum. The use of wildcards (`_`) in the primary dispatch loop is prohibited (RR-15 Rule 5).
- **Coordinate System Composition**: `BackendTransform = initial_transform * current_ctm`. The interpreter MUST maintain a pure, PDF-compliant Y-up `ctm` within each graphics state.
- Interpretation MUST enforce type-level provision of a `Resolver` and `ResourceStack`. Public high-level interpreters MUST NOT have default constructors that omit these dependencies.
- The `ResourceStack` MUST be initialized using late-bound resolution (e.g., `Page::resources_handle()`). Storing or passing `DictHandle` from previous passes for stack initialization is prohibited.

## 2. Rendering (Phase 4)

- Reconstructed SFNT buffers and exact GIDs MUST be passed to the hardware-accelerated backend.
- System font fallback is strictly prohibited for embedded resources; visual fidelity must be absolute.

## 3. Serialization (Phase 5)

- All IR commands (e.g., `SetFillColor`, `SetStrokeColor`) MUST be mapped back to their canonical PDF operators (`rg`, `RG`, `g`, `G`, `k`, `K`). Omissions lead to "Default-to-Black" regressions.
- Operators captured as `RawOperator` (e.g., `n`, `v`, `y`) MUST be emitted exactly as captured to preserve path logic and drawing order.
- The resulting PDF MUST pass iterative structural auditing for the target standard (e.g., PDF/UA-2).

## 4. Linearization (Fast Web View)

- **Section 2 (Primary)**: MUST contain the Catalog (ID 2), the Primary Hint Stream (ID 3), and all resources/ancestors required for Page 1.
- **Section 6 (Remaining Pages)**: Objects MUST be ordered by page number. Each page's section MUST start with its Page dictionary.
- Page dictionaries MUST NOT be stored in Object Streams.
- **Prohibited Objects in Object Streams**: Any object in Section 2, Page dictionaries for all pages, any object that has a stream.
- **Mandatory ID Mapping**: Object ID 1 = Linearization Dictionary, ID 2 = Document Catalog, ID 3 = Primary Hint Stream, ID 4 = First Page object.
- The main XRef Stream MUST include a Type 1 entry for itself. The `/Size` entry in both first-page and main trailers MUST be identical.
- The Page Offset Hint Table header MUST define all 13 items. Every page entry MUST include all fields defined by header bit widths.
- The first-page trailer MUST contain a `/Prev` entry pointing to the main cross-reference table. The main trailer MUST NOT contain a `/Prev` entry.

## Reference

- @docs/specs/sdk-pipeline.md — Interpretation, serialization & linearization design
