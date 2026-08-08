# GPU Rendering Constraints

> [!IMPORTANT]
> Prescriptive constraints for the rendering engine.
> Design specifications: @docs/specs/rendering.md

---

## 1. Coordinate Systems

- All internal logic (Interpreter, FontResource) MUST consistently use the **Positive Y = UP** coordinate system according to the PDF specification.
- Coordinate system inversion (Positive Y = DOWN) is permitted ONLY in the layer immediately before sending data to the rendering device (e.g., Vello). Flipping signs in intermediate pipeline layers is prohibited.

## 2. Font Resource Normalization

- All font-specific ambiguities MUST be resolved during the reconstruction phase. The resulting **Virtual OpenType (SFNT)** binary serves as the absolute single source of truth.
- Mandatory inheritance of metadata (WMode, Encoding, ToUnicode) from Type 0 parents to CIDFont descendants during ingestion.
- CIDFonts MUST be parsed using CID-specific metrics (`/W`), ensuring consistency between parent and descendant resources.
- The `EndPath` (`n`) operator is critical for graphics state isolation. Discarding `n` during sublimation leads to "Path Leakage."

## 3. CMap and Encoding Hygiene

- Each `FontResource` MUST have its own independent mapping table. "Rescue" logic is permitted only for clearly identified CJK fonts and MUST NOT cause side effects (cache pollution).
- CMap parsing MUST accurately handle both literal strings (UTF-16BE) and hex notations.

## 4. Context Propagation Guard

- Interpretation of document data MUST enforce type-level provision of a `Resolver` and `ResourceStack`.
- The `ResourceStack` MUST be initialized using late-bound resolution. Storing or passing `DictHandle` from previous passes is prohibited.
- Public high-level interpreters MUST NOT have default constructors that omit these dependencies.

## 5. Color Space Fidelity

- Maintain the original color model (DeviceGray, DeviceRGB, DeviceCMYK, Lab, ICCBased) throughout the sublimation and interpretation layers.
- Converting non-RGB colors to RGB during the IR phase is prohibited.
- Initial fill and stroke color states MUST be explicitly defined (defaulting to Gray 0.0).
- Lab → sRGB conversion MUST apply standard sRGB non-linear gamma companding equations.

## 6. CJK Rendering Integrity

- Multi-byte character decoding MUST accurately detect byte-length boundaries based on the specific CMap's range definitions.
- Missing character mappings MUST fallback to a diagnostic placeholder (e.g., `.notdef`) with logging, not silent guessing.
- Vertical writing (WMode=1) metrics MUST be applied strictly according to the CIDFont's W/W2 dictionaries.
- Whitespace CIDs (e.g., CID 1, 2, 3) in Japanese CID-keyed fonts MUST be correctly resolved to space glyphs.

## Reference

- @docs/specs/rendering.md — Text metrics, scaling formulas & CJK decoding specifications
