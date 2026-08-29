# Technical Specification: The fepdf Refinery Engine

## 1. Overview
fepdf is a "Refinery" type PDF engine designed to extract pure PDF 2.0 representations from legacy PDFs and optimize them for modern computing environments.

## 2. Core Components

### 2.1 PdfArena (Typed Arena Storage)
All PDF objects are decoupled from their physical locations and stored in a type-safe arena structure.
- **Data-Oriented Design**: Dicts, Arrays, and Streams are managed in independent memory pools to maximize cache efficiency.
- **Handle System**: Objects are cross-referenced by lightweight `u32`-based handles.
  (Checked 2026-08-22: there are **no generation bits**. `Handle<T>` is a `u32` and a
  `PhantomData`; safety comes from the arena never freeing a slot, not from generations.
  An object's handle **is** its object number — see `ARCHITECTURE.md` §4.6.)

### 2.2 Refinery Pipeline (Refinement Process)
1. **Ingestion**: Deconstruct physical structures and transfer them to the arena.
   (Historic note: this was delegated to `lopdf`. See ROADMAP.md Phase A and
   [ADR-0003](../adr/0003-lopdf-was-not-providing-robustness.md).)
2. **Normalization & Sublimation**: 
   - **Content Sublimation (IR)**: Content streams are parsed into a high-level Intermediate Representation (`Command` IR). This performs early UTF-8 decoding and operator normalization.
   - **Font Reconstruction**: Embedded font binaries are surgically patched with widths derived from PDF `/Widths`, ensuring layout parity.
   - **Memory Optimization**: Streams over 4 KB are transparently compressed in memory
     with **`flate2`** — **except** images and fonts, which are the two kinds this line
     used to name as the examples. They are kept as `Raw` deliberately, because
     re-compressing an already-compressed codestream costs time and fidelity for nothing
     (`refine/mod.rs:367`, "High-Fidelity Preservation"). (Checked 2026-08-22: this said
     **Zstd** until Rule 9 removed it. The in-memory form never reaches a file, so nothing
     about the output changed; the `/ZstandardDecode` *filter*, which would have, is not in
     ISO 32000-2 and no file of the 530 carried it — see
     [ADR-0024](../adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md).)
   - **Text Recovery**: Text strings are decoded as 7.9.2.2 defines — PDFDocEncoding from
     Annex D, or a byte order mark. (Checked 2026-08-22: `chardetng` is not a dependency
     and `encoding_rs` is no longer in the tree at all — it came in through `reqwest`,
     which was itself used by no line of code and left with Rule 9. **Detection was
     removed**, because a Shift-JIS detector was found corrupting a conforming `/Title` —
     see `ROADMAP.md`, clause 14.3.)
   - **Color Harmonization**: Normalize device-dependent colors to OutputIntents (ICC) using `moxcms` (Pure Rust).
   - **Metadata Scrubbing**: Consolidate legacy Info into XMP streams using `xmp-writer`.
3. **Validation**: Departures from the standard are recorded as `Decision`s with the
   clause that governs them (`ARCHITECTURE.md` §4.3). (Checked 2026-08-22: there is no
   `SafetyBitmask` and no Arlington predicate engine — the same claim that got
   `omissions.md` archived and then deleted. Arlington is a shell wrapper around an
   external Python tool and nothing in the engine reads it.)

## 3. Flagship GUI (`fepdf-gui`)
- **Rendering**: GPU-accelerated rendering via Vello, using normalized data on the arena as the direct source.
- **Asynchronous Design**: Ingestion and refinement are executed on background threads,
  maintaining GUI responsiveness. (Checked 2026-08-22: this said "(Tokio/Rayon)" and only
  half was true. `rayon` is real but lives in `fepdf-model`, where the refinement is;
  `fepdf-gui` uses a plain `std::thread::spawn` and **declared `tokio` without using it
  anywhere**, which the unused-dependency audit removed — ROADMAP Phase Q.)

## 4. Security and Signatures
- **PAdES Compliance**: Digital signature application and verification using `cms` and `x509-parser`.
- **Strict 2.0 Conversion**: Always perform a "Full Rewrite" during saving to forge a high-purity PDF 2.0 binary free of impurities.
