> **Archived 2026-08-22, and not maintained.** A record of an earlier design, in the
> sense `PLANNING.md` uses of everything under `docs/history/archive/`, kept for the same
> reason `omissions.md` beside it is: deleting the evidence of a documentation failure
> removes the only proof that it happened.
>
> **Checked against the code on 2026-08-22 (Phase O-4), and most of it is contradicted.**
> It sat in `docs/specs/` under a README saying the directory is not authoritative, which
> is true and was not enough — a document naming a dependency version reads as a fact
> whatever the directory says.
>
> | Claim | What is true |
> | :--- | :--- |
> | Links `RR-15` and `HDD` to `.antigravity/rules/*.md` | No such directory. The rules are `CODING.md` and `.agents/rules/`. |
> | `trailer.rs`, `resolver.rs`, `graphics.rs`, `editor.rs` | None of the four exists. |
> | `serialize/object.rs`, `serialize/increment.rs` | No `serialize/` directory. The writer is one file, `writer.rs`. |
> | "Non-Destructive Editing (Incremental Update): Append mode" | Saving produces a **new document** ([ADR-0012](../../adr/0012-saving-produces-a-new-document.md)), and the faithful-copy path is deliberately not built ([ADR-0014](../../adr/0014-the-faithful-copy-path-is-not-built.md)). |
> | Parser: `nom` 7.1 | `nom` is used, in `object/sublimation/resurrection.rs` — for content streams, not for the file. |
> | Compression: `miniz_oxide` 0.7 | Not a direct dependency; `flate2` decodes `/FlateDecode`. |
> | Image Filters: `jpeg-decoder` 0.3.2 | Not in `Cargo.lock` at all. JPEG is `zune-jpeg` 0.5. |
> | `hayro-jpeg2000` 0.3.4 | 0.4. |
> | Cryptography: `md-5` 0.10 | The crate is `md5`, at 0.8. |
> | `SublimatedData::Commands(Vec<Command>)`, three variants | Four variants, and `Commands { items }` is a struct variant. `Image` is missing from the list. |
> | "Dynamic Verification ... via the Arlington Model predicate evaluation engine" | There is none. The same claim got `omissions.md` archived. |
> | `MAX_WS_ITER`, `MAX_STR_LEN` | Neither constant exists. The parser's limit is `MAX_RECURSION_DEPTH`, 512. |
> | Graphics State "based on ISO 32000-2 Table 51" | `GraphicsState` exists and the listed fields are close, but `font_size` and the text fields live on a nested `text_state`. |
>
> What the design is **now** is in [`ARCHITECTURE.md`](../../../ARCHITECTURE.md); how it
> came to be is in [`docs/adr/`](../../adr/README.md).

# Technical Specification: fepdf SDK

> [!IMPORTANT]
> ISO 32000-2:2020 Strict Compliance. A pure Rust PDF parsing and editing engine. The RR-15 and HDD protocols serve as the "Development Charter."

## 1. Development Protocol

This project strictly adheres to **[RR-15](../../.antigravity/rules/hardening.md)** (Safety) and **[HDD](../../.antigravity/rules/pipeline.md)** (Process) as its core development charter.

## 2. 4-Layer Architecture

### A. Object Layer (Physical & Logical)

- **`lexer.rs`/`loader.rs`**: Tokenization and stream reading.
- **`xref.rs`/`trailer.rs`**: Handling of ISO 32000-2 recommended XRef streams and hybrid references.
- **`resolver.rs`**: Indirect reference resolution. The entry point for Arlington validation.
- **`security.rs`**: AES-256 Revision 6 (PDF 2.0) decryption and encryption.

### B. Logical Layer (Structure)

The SDK implements higher-level logical structures (Pages, Outlines, Resources) while maintaining strict ownership and safety boundaries as defined by RR-15.

### C. Engine Layer (The Interpreter)

- **`content.rs`**: Content stream parsing and instruction loop.
- **`graphics.rs`**: Graphics State Stack based on ISO 32000-2 Table 51.
- **`text.rs`/`font.rs`/`cmap.rs`**: UTF-8 glyph placement and CMap resolution.
  - **Precision Rendering**: Standardization of FontMatrix (0.001) based on ISO 32000-2 Table 112 and strict Width Synchronization (`W` array support).

### D. Export Layer (The Writer)

- **`editor.rs`**: Non-destructive editing capabilities.
- **`writer.rs`**: Physical binary serializer.

## 3. Technology Stack

The following libraries are selected to guarantee deterministic and safe operation of the SDK.

- **Language & Foundation**: Rust 1.94 / Edition 2024 (SSoT: Cargo.toml)
- **Parser**: `nom` 7.1 (Parser combinators)
  - Fast and safe binary/text analysis.
- **Compression**: `miniz_oxide` 0.7 (Pure-Rust Flate/Zlib)
  - Used for `FlateDecode` decompression.
- **Cryptography**: `aes` 0.8 / `cbc` 0.1 / `md-5` 0.10 / `sha2` 0.10
  - Cryptographic logic for standard PDF security handlers.
- **Geometry**: `kurbo` 0.13 (2D geometry)
  - Coordinate transformations and Bézier curve operations for drawing commands.
- **Image Filters**: `jpeg-decoder` 0.3.2 / `hayro-jpeg2000` 0.3.4
  - Decompression for `DCTDecode` (JPEG) and `JPXDecode` (JPEG 2000).

## 4. Logical Structure Interpretation

### GraphicsState (PDF 2.0 Compliant)

```rust
pub struct GraphicsState {
    pub ctm: Matrix,
    pub stroke_color: Color,
    pub fill_color: Color,
    pub font_size: f64,
    pub char_spacing: f64,
    pub word_spacing: f64,
    pub horizontal_scaling: f64,
    pub text_rise: f64,
    pub wmode: u8, // Writing Mode (0: Horizontal, 1: Vertical)
    // ...
}
```

### Command IR (Sublimated Representation)

```rust
pub enum Command {
    PushState, PopState,
    Transform(Affine),
    MoveTo(Point), LineTo(Point), CurveTo(Point, Point, Point),
    Fill(WindingRule), Stroke(StrokeStyle),
    BeginText, EndText,
    ShowText(String), // UTF-8 Normalized
    SetFont { font: String, size: f64 },
    SetWritingMode(u8),
    DrawXObject(String),
}
```

### SublimatedData (Memory Optimization)

```rust
pub enum SublimatedData {
    Raw(Bytes),
    Compressed { original_len: usize, data: Vec<u8> }, // Zstd
    Commands(Vec<Command>), // Middle Representation
}
```

## 5. Physical Serializer [M28-S]

### Design Principles

- **ISO 32000-2 Compliance**: Prioritize XRef Stream (PDF 2.0 recommended) output.
- **Non-Destructive Editing (Incremental Update)**: Append mode: "Modified objects + New XRef + New Trailer" appended to existing binaries.
- **RR-15 Compliance (Determinism)**: Ensure hash consistency during binary output and eliminate undefined object references statically and dynamically.

### Key Components

- **`writer.rs`**: Abstraction for binary writing to `std::io::Write`.
- **`serialize/object.rs`**: Recursive output logic for basic types (Dictionary, Array, String, Name).
- **`serialize/increment.rs`**: Appending to existing PDF files and generating Trailers with `/Prev` links.

## 6. Guardrails

- **Static Analysis**: Enforcement of RR-15 clauses via `verify_compliance.sh`.
- **Dynamic Verification**: Self-audit of output PDF specification compliance (including dynamic conditions) via the Arlington Model predicate evaluation engine.
- **Error Handling**: Prevent data corruption caused by partial writes during output failures.

## 7. Robustness & Security Policy

The fepdf PDF Engine adheres to the following core principles to balance high tolerance for "imperfect PDFs" in the real world with reliable "standard-compliant PDF" output.

> [!TIP]
> **Liberal Read, Strict Write**
>
> - **Liberal Read**: Minor specification violations (e.g., extra whitespace before keywords, non-standard line endings, junk at file beginning) are tolerated gracefully as long as the structure remains recoverable and no security risks are introduced. The goal is to open any file that major PDF viewers can handle.
> - **Strict Write**: Every byte sequence generated or edited by the SDK must be 100% compliant with ISO 32000-2:2020. Outputting incomplete or non-standard structures is prohibited.

### Security Guardrails

The following limits are enforced to prevent flexible reading from becoming a vulnerability vector:

- **Resource Limits**: Constants such as `MAX_WS_ITER` (whitespace scanning limit), `MAX_STR_LEN` (string length limit), and memory caps after decompression are established to prevent DOS attacks.
- **Hierarchy Depth Limits**: Recursive object traversal (e.g., PDF Pages Tree) uses iterative processing with explicit depth limits to prevent stack overflows.
- **Deterministic Behavior**: Always generate the same analysis result for the same input, eliminating environment-dependent behavior.
