# Rendering Engine Design Specification

This document describes the technical specifications for text metrics, scaling, and CJK decoding in the fepdf rendering engine.

For prescriptive constraints, see `.agents/rules/gpu-rendering.md`.

---

## 1. Text Metrics and Scaling

- **Decoupling Principle**: Generation of the Glyph Path and calculation of Layout (Advance/Metrics) must clearly separate scales.
    - **Path Scale**: `size / units_per_em` (using Font-specific UnitsPerEm).
    - **Metrics Scale**: `size / 1000.0` (using PDF standard 1000-unit system).
- **Rounding**: Manage precision strictly to prevent the accumulation of floating-point errors in layout calculations.

---

## 2. High-Fidelity CJK Decoding

- **Boundary Precision**: Multi-byte character decoding (CMap) accurately detects byte-length boundaries (1-byte vs 2-byte) based on the specific CMap's range definitions.
- **Fail-Safe Mapping**: If a character mapping is missing, the engine falls back to a diagnostic placeholder (e.g., `.notdef`) and logs the incident, rather than silently guessing or shifting indices.
- **WMode Fidelity**: Vertical writing (WMode=1) metrics are applied strictly according to the CIDFont's W/W2 dictionaries.
- **Glyph Replacement Integrity**: During Japanese CID-keyed font rendering, certain whitespace CIDs (e.g., CID 1, 2, 3) are correctly resolved to space glyphs to prevent "White Page" regressions in documents where Unicode mapping is missing.
