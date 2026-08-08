# Lessons Learned: Acrobat Compatibility & Pass 0 Normalization

## Context
During the processing of `UnicodeStandard-16.0.pdf`, we encountered **Adobe Acrobat Error 135** ("A problem occurred while reading this document"). While the file opened perfectly in macOS Preview and was logically valid, Acrobat rejected it.

## Key Discovery
1.  **`/Encrypt` Residuals**: Acrobat strictly enforces structural consistency. If a PDF is decrypted but the `/Encrypt` dictionary remains in the trailer, Acrobat attempts to use a security handler on plain data, leading to a crash.
2.  **Pass 0 Requirement**: Semantic ingestion (mapping objects to handles in the Arena) must happen on **clean, physical data**. Trying to decrypt objects "on-the-fly" during semantic mapping is fragile and can lead to partially encrypted structures (e.g., in Outlines/Bookmarks).

## Resolution: The "Pass 0 Normalization" Strategy
- **Mechanism**: Before anything else, perform an iterative (stack-based, Rule 6 compliant) walk of all raw objects.
- **Actions**:
    - Decrypt all strings and streams.
    - Remove the `/Encrypt` reference from the trailer.
    - Ensure all Metadata streams are UTF-8 compliant.
- **Result**: The internal `PdfArena` now only ever sees "Plaintext PDF 2.0" structures, regardless of the source file's physical state.

## Impact on Future Work
- **Rule 18**: Any new ingestion logic must support this normalization phase.
- **Acrobat as a Baseline**: Compliance tests MUST include a "Viewer Fidelity" check on Acrobat, as it is more pedantic than the specification itself.

---

# Lessons Learned: Handle Stability & PUA Invariance

## Context
During the optimization of font caching in `ferruginous-core`, we found that font resources were occasionally failing to resolve after a `ParallelRefinery` pass, despite no logical changes to the document structure.

## Key Discovery
1.  **RefCell Handle Churn**: While `PdfArena` object indices are stable, the `RefCell` handles for *Dictionaries* can change if the dictionary itself is re-allocated or refined.
2.  **Authoritative Keys**: Caching mechanisms that use dictionary pointers/handles as keys are volatile. The only stable key for a resource is its top-level `Handle<Object>` (the indirect reference).
3.  **PUA as Signal, not Noise**: In CJK (Japanese) CID-keyed fonts without `ToUnicode`, the `0xF0000` block is used to encode CID values. Traditional "PUA suppression" rules (designed to clean up garbage) will break CJK rendering if applied too aggressively.

## Resolution
- **Indexing Rule**: Always index resource caches by `Handle<Object>`. Never store transient `RefCell` handles in long-lived state.
- **CJK Bypass**: Modified `resolve_gid` to explicitly allow PUA character codes when the font type is `CIDFontType0` or `CIDFontType2`.

## Impact on Future Work
- **Rule 19**: Audit all resource lookup tables (Images, XObjects) to ensure they follow the "Invariance of Handles" rule.
- **Test Coverage**: Any character mapping change must be verified against at least one CJK document with vertical writing.

---

# Lessons Learned: Late-Binding & Structural Hardening

## Context
While implementing multi-pass structural remediation (UA-2 tagging), we discovered that high-level models like `Page` and `PdfCatalog` were occasionally holding "stale" dictionary handles. This occurred when one remediation pass updated a dictionary, causing its internal `DictHandle` (index) to change, which then invalidated the handles held by other modules.

## Key Discovery
1.  **Index Staleness**: Storing a `DictHandle` (an index into a `BTreeMap` in the arena) inside a persistent model like `Page` is a violation of structural stability. Any operation that modifies the arena's dictionary collection (refinement, compaction, or updates) can invalidate these indices.
2.  **Normalization vs. Stability**: "Normalization-at-load" ensures the data is clean, but it does NOT guarantee that the *memory address* or *handle index* of that data will remain static throughout the document lifecycle.

## Resolution: The "Late-Binding" Architecture
- **Stable Modeling**: All core models (`Page`, `PdfCatalog`, `StructElement`) were refactored to hold only a `Handle<Object>` (the stable indirect reference).
- **Just-in-Time Resolution**: Implemented a mandatory "Late-binding" pattern. Models now resolve their `Handle<Object>` to a concrete `DictHandle` only at the exact moment of access using `doc.resolve_to_dict(handle)`.
- **Synergy with Normalization**: This architecture preserves the performance benefits of "normalization-at-load" (flattened structures, baked inheritance) while gaining the robustness of stable object references.

## Impact on Future Work
- **SDK-Wide Standard**: Late-binding is now the mandated pattern for all SDK-level structural manipulation.
- **Refinery Safety**: The `ParallelRefinery` can now safely re-allocate and optimize dictionaries without risking dangling handles in the SDK layers.

---

# Lessons Learned: Path Integrity & Serialization Fidelity

## Context
During the stabilization of the Intel SDM document, we encountered persistent "black mask" artifacts and a "Default-to-Black" regression in regenerated PDFs.

## Key Discovery
1.  **Path Construction Leakage**: Discarding the `n` (EndPath) operator during sublimation is hazardous. In PDF, `n` resets the current path without painting. If missing, subsequent painting operators (e.g., `f`) will include the previous "construction-only" segments (like clipping rectangles), resulting in unintended solid fills.
2.  **Serialization Gap**: The "Desublimation" (serialization) phase MUST handle 100% of the IR command variants. Omissions in `SetFillColor` or `SetStrokeColor` mappings lead to the loss of all color state, causing documents to default to black during physical reconstruction.

## Resolution
- **Operator Completeness**: Added explicit support for the `n` operator in the `Sublimator` and ensured it resets the `Interpreter` path state.
- **Fidelity Synchronization**: Implemented missing color command mappings in `serializer.rs`. Every IR command now has a verified bidirectional mapping between PDF operators and the internal model.

## Impact on Future Work
- **Rule 20**: The `Sublimator` (Parser) and `Desublimator` (Serializer) MUST be updated in sync. Any new IR command added for normalization MUST have a corresponding serialization mapping.
- **Acrobat as Ground Truth**: Visual validation against Acrobat is the definitive proof of serialization fidelity, as it exposes structural and state-management errors that simpler renderers might ignore.

---

# Lessons Learned: High-Fidelity Color Spaces & Compliant Key Derivations

## Context
During the final verification of CIELAB colors and PDF 2.0 security parameters, we addressed two core gaps: (1) color matching rendering regressions where Lab colors rendered excessively dark due to linear space mapping, and (2) audit-level complaints regarding V5 key derivation simplicity.

## Key Discovery
1.  **Gamma Companding Cruciality**: Simply mapping Lab to XYZ and then linearly mapping to RGB results in dark, high-contrast colors. Standard sRGB requires applying the non-linear gamma companding curve ($C_{\text{srgb}} = 1.055 \cdot C^{1/2.4} - 0.055$ for $C > 0.0031308$) to transition from linear space to perceptual space.
2.  **SHA-256 Iteration Specifications**: ISO 32000-2 Section 7.6.4.3.3 requires that R5 key derivation uses exactly 50 rounds of nested SHA-256 hashes incorporating the validation/key salts. Raw one-shot SHA-256 is cryptographically weaker and will fail pedantic compliance check engines.
3.  **Workspace Lints Inheritance**: When setting workspace-wide lints in Cargo.toml, individual member crates must explicitly inherit them via `lints.workspace = true` to apply custom exclusions (like `unnecessary_wraps = "allow"`) cleanly and prevent build command failures under `-D warnings`.

## Resolution
- **Precision Color Engine**: Upgraded `Color::Lab` to perform standard D65 illuminant XYZ transformations, BT.709-6 matrix mappings, and sRGB gamma companding.
- **Standard V5 Key Deriver**: Refactored `SecurityHandler::new_v5` to fully implement the 50-round nested SHA-256 key derivation with deterministic salt inputs.

## Impact on Future Work
- **Strict Specifications Over Approximations**: When implementing color spaces or cryptographic protocols, always implement the standard mathematical spec rather than linear approximations to maintain absolute SSoT fidelity.
- **Unified Workspace Lints**: Maintain the `Cargo.toml` workspace-level lints structure to keep clean builds across all core, render, and SDK crates without cluttering individual crate files.
