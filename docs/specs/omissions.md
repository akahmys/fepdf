# Implementation Limits and Simplifications Regarding ISO 32000-2 Compliance

This document records the items where intentional simplifications or limitations have been established relative to the full implementation of the ISO 32000-2:2020 (PDF 2.0) specification in the development of the fepdf PDF engine. These decisions are the result of prioritizing compliance with the Reliable Rust-15 (RR-15) coding rules and ensuring deterministic behavior in mission-critical environments.

## 1. Stream Filters

The specification (Clause 7.4) defines a wide variety of compression algorithms. fepdf currently limits support to the following filters:

- **FlateDecode / ASCIIHexDecode**: Implemented as general-purpose data compression and text-based encoding.
- **DCTDecode / JPXDecode**: Supports image decoding in JPEG and JPEG 2000 (via hayro-jpeg2000) formats.
- **LZWDecode / CCITTFaxDecode / JBIG2Decode**: Fully implemented for production use in Phase 12. Integrated `weezl` (LZW), `fax` (CCITT G4), and `justbig2` (JBIG2) within the constraints of Safe Rust, completing support for scanned and archived documents.
- **Unimplemented Items**: `RunLengthDecode`.
  - **Reason**: Extremely low utilization frequency; prioritized other higher-priority filter implementations.

## 2. Graphics and Color Spaces

Regarding graphics rendering (Clause 8), major rendering models are covered, with the following limitations:

- **Color Spaces**: In addition to the three basic types (`DeviceGray`, `DeviceRGB`, `DeviceCMYK`), `ICCBased` is fully supported (via lcms2-rs). High-precision color conversion based on color management profiles is achieved.
- **Patterns and Shading**: In addition to `Function-based Shading` (Types 1-3) and `Axial/Radial Shading`, full CPU tessellation support for `Mesh Gradients` (Types 4-7) is provided. Standard-compliant and high-definition rendering is possible through recursive subdivision of Coons Patches and Tensor-Product Patches.

## 3. Fonts and Text

Text processing (Clause 9) is one of the most complex areas of the specification.

- **Font Formats**: `Type 1`, `TrueType`, and `Type 0 (CIDFont)` are supported.
- **Progress (Phase 11)**: Implemented a pipeline to directly render extracted glyph outlines (BezPath) for all TrueType/CIDFonts. Fully compliant with WMode 1 (vertical writing) advance calculations and CID metrics (/W, /W2).
- **Simplification Content**: `Type 3` (User-defined fonts) is excluded due to the risk of recursive calls and stack overflows (violating RR-15 Rule 6), as they can encapsulate arbitrary drawing instructions.

## 4. Document Structure and Traversal

The logical and physical structures of a PDF document (Clauses 7/14) are defined as recursive tree structures.

- **Enforcement of Non-Recursive Traversal**: While `Page Tree` and `Resource Dictionary` are defined as recursive trees in the specification, all internal processing has been replaced with iterative processing using an explicit stack (`Vec`) to comply with RR-15 Rule 6 (No Recursion). This completely eliminates concerns about stack overflows for files with recursive depths as defined in the specification.

## 5. Interactive Features

Regarding forms, annotations, and security (Clauses 12/13), the following design principles are adopted:

- **Static and Dynamic Analysis**: `AcroForm` and `Digital Signatures` are fully integrated.
  - **AcroForm**: Provides APIs for bulk retrieval (export) and setting (import) of field values, enabling external integration in JSON format.
  - **Digital Signatures**: Beyond simple structural validation, cryptographic signature verification (SHA-256 + asymmetric key verification) using `x509-parser` and `rsa` / `ed25519-dalek` has been implemented.
- **Multimedia (Clause 13)**: For `RichMedia` and `3D` annotations, only dictionary structure analysis and validation via the Arlington model are supported.
  - **Limitations**: Actual rendering of U3D/PRC models and decoding/playback of external media (audio/video) have been intentionally deferred at this time from the perspective of ensuring runtime safety and dependency management.
- **Restrictions**: `XFA` (XML Forms Architecture) is not supported due to its complexity and numerous security attack vectors.

## 6. Automated Validation and Accessibility

- **Arlington Predicates**: In Phase 17, an AST parser and recursive evaluation engine using `nom` were fully implemented. This allows for dynamic evaluation of complex validation conditions (SinceVersion, Required, Dependent key check, etc.) included in the ISO 32000-2 specification.
- **Tagged PDF Repair**: `TaggedPdfValidator` verifies compliance with ISO 32000-2 Clause 14.8 but does not include functionality for "automatic inference and repair" of missing tag structures.

## 7. Policy: Liberal Read, Strict Write

The basic policy of the fepdf PDF Engine is to ensure maximum compatibility during reading (Liberal Read) while strictly adhering to the specification during writing (Strict Write). Many of the limitations described in this document are strategic decisions based on this policy, aimed at enhancing the quality and safety of generated PDFs by intentionally excluding old, complex, and error-prone specifications from the "writing" target.

## Overall Assessment

fepdf is an implementation that emphasizes "correct and safe processing of recognized definitions" rather than just "reading all definitions" of PDF 2.0. If these limitations are to be relaxed in the future, the RR-15 audit established in each milestone must be passed again.
