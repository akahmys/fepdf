# SDK Pipeline Design Specification

This document describes the design of Phase 3 (Interpretation) and Phase 5 (Serialization & Resurrection) of the fepdf processing pipeline.

For prescriptive constraints, see `.agents/rules/sdk-engine.md`.

---

## 1. Phase 3: Interpretation (Execution Layer)

*   **Responsibility**: Transform normalized objects into a stateless sequence of drawing commands.
*   **Actions**:
    *   **On-demand Sublimation**: Re-decompress and re-reconstruct data on-the-fly based on Phase 2 recipes.
    *   **Stateless Execution**: Execute the atomic IR commands produced in Phase 2. No heuristic guesswork or implicit state mutations are permitted during this phase.
    *   **Exhaustive Operator Dispatching (Rule 5 Hardening)**: The interpreter utilizes exhaustive pattern matching for the `Command` IR enum. The use of wildcards (`_`) in the primary dispatch loop is prohibited. Every variant—including XObjects, marked content, and Type 3 metrics—is explicitly routed to its corresponding handler to prevent "silent state loss" where operators are parsed but never executed.
*   **Coordinate System Decoupling**:
    *   **Baseline Transform (`initial_transform`)**: The immutable transform from PDF User Space (Points) to Device Space (Pixels). Handles MediaBox translation, Y-flipping, and DPI scaling.
    *   **Execution CTM**: Maintains a pure, PDF-compliant Y-up `ctm` within each graphics state.
    *   **Composition**: `BackendTransform = initial_transform * current_ctm`. This prevents double-inversion and isolates the interpreter from device-specific constraints.

---

## 2. Phase 4: Rendering (Output Layer)

*   **Responsibility**: High-fidelity visual rasterization.
*   **Actions**:
    *   **Pure SFNT Pipeline**: Pass reconstructed SFNT buffers and exact GIDs to the hardware-accelerated backend.
    *   **Zero-Fallback Policy**: System font fallback is strictly prohibited for embedded resources; visual fidelity must be absolute.

---

## 3. Phase 5: Serialization & Resurrection (Refinery Export)

*   **Responsibility**: Reify the normalized IR back into a physical PDF 2.0 file.
*   **Lossless Reversibility**:
    *   **State Preservation**: All IR commands (e.g., `SetFillColor`, `SetStrokeColor`) are mapped back to their canonical PDF operators (`rg`, `RG`, `g`, `G`, `k`, `K`). Omissions lead to "Default-to-Black" regressions.
    *   **Raw Operator Passthrough**: Operators captured as `RawOperator` (e.g., `n`, `v`, `y`) are emitted exactly as captured to preserve path logic and drawing order.
    *   **Compliance Verification**: The resulting PDF passes iterative structural auditing for the target standard (e.g., PDF/UA-2).

### 3.1. Linearization (Fast Web View) Hardening

*   **Strict Object Partitioning & Ordering**:
    *   **Section 2 (Primary)**: Contains the Catalog (ID 2), the Primary Hint Stream (ID 3), and all resources/ancestors required for Page 1.
    *   **Section 6 (Remaining Pages)**: Contains all other pages and their exclusive resources. Objects are ordered by page number.
    *   **Page Contiguity**: Each page's section in Section 6 starts with its Page dictionary. To ensure this, Page dictionaries are not stored in Object Streams.
*   **Object Stream Packing Constraints**:
    *   **Prohibited Objects**: Any object in Section 2, Page dictionaries for all pages, any object that has a stream (e.g., Font streams, ICC profiles).
    *   **Allowed Objects**: Non-stream shared resources in Section 8/9 (Others) are candidates for ObjStm packing.
*   **Mandatory ID Mapping**:
    *   **Object ID 1**: Linearization Dictionary.
    *   **Object ID 2**: Document Catalog.
    *   **Object ID 3**: Primary Hint Stream.
    *   **Object ID 4**: First Page object.
*   **XRef Stream Integrity (ISO 32000-2 Compatibility)**:
    *   **Self-Reference**: The main XRef Stream includes a Type 1 entry for itself in its own table, pointing to its physical starting offset.
    *   **Size Synchronization**: The `/Size` entry in both the first-page trailer and the main trailer are identical and reflect the total object count inclusive of the XRef Stream object.
*   **Hint Table Standardization**:
    *   **Header Completeness**: The Page Offset Hint Table header defines all 13 items.
    *   **Structural Alignment**: Every page entry includes all fields defined by the header bit widths (e.g., shared object references count), even if they are zero-bit wide or contain zero values, to prevent bit-offset desynchronization in strict parsers like Acrobat.
*   **Dual-Xref Linkage**:
    *   The first-page trailer (Section 3) contains a `/Prev` entry pointing to the main cross-reference table (Section 11).
    *   The main trailer (Section 11) does not contain a `/Prev` entry to avoid circular references.
