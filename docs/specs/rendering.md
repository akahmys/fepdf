# Rendering Engine Design Specification

This document describes the technical specifications for text metrics, scaling, and CJK decoding in the fepdf rendering engine.

For prescriptive constraints, see [CODING.md](../../CODING.md) and [ARCHITECTURE.md](../../ARCHITECTURE.md).

---

## 1. Text Metrics and Scaling

- **Decoupling Principle**: Generation of the Glyph Path and calculation of Layout (Advance/Metrics) must clearly separate scales.
    - **Path Scale**: `size / units_per_em` (using Font-specific UnitsPerEm).
    - **Metrics Scale**: `size / 1000.0` (using PDF standard 1000-unit system).
- **Rounding**: Manage precision strictly to prevent the accumulation of floating-point errors in layout calculations.

---

## 2. High-Fidelity CJK Decoding

- **Boundary Precision**: Multi-byte character decoding (CMap) accurately detects byte-length boundaries (1-byte vs 2-byte) based on the specific CMap's range definitions.
- **Fail-Safe Mapping**: If a character mapping is missing, the engine falls back rather
  than silently guessing or shifting indices. (Checked 2026-08-22: there is no `.notdef`
  placeholder in the font or interpreter code.)

  **The rest of that check was wrong, and this document is where it was wrong.**
  It said the fallback "does not log — the engine holds exactly one `log::warn!` by
  design". Re-derived the same day: the engine holds **sixteen**, three of them deliberate
  reports about the *host*, and the other thirteen conclusions about the document that
  ARCHITECTURE §5.3 says should be `Decision`s. Seven sit in exactly the code this file
  describes: `fepdf-render` logs `[SKRIFA] Drawing failed for GID …` and `set_font: … NOT
  FOUND in cache`, `fepdf-font` logs a missing CFF table, and `fepdf-content` logs an
  unresolvable font and a failed Type 3 glyph.

  The check was not careless — it ran `status.sh`, and `status.sh` searched two crates,
  neither of them these. **A claim verified against a tool that cannot see the subject is
  indistinguishable from a claim nobody checked**, which is the more useful half of this
  correction. The row now derives the engine as every crate that is not a frontend
  (ROADMAP Phase Q), and reads 16.
- **WMode Fidelity**: Vertical writing (WMode=1) metrics are applied strictly according to the CIDFont's W/W2 dictionaries.
- **Glyph Replacement Integrity**: During Japanese CID-keyed font rendering, certain whitespace CIDs (e.g., CID 1, 2, 3) are correctly resolved to space glyphs to prevent "White Page" regressions in documents where Unicode mapping is missing.
