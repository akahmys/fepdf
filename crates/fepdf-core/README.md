# fepdf-core

The technical foundation of the fepdf PDF engine.

## Core Components

### 1. `PdfArena` (Generational Object Storage)
`PdfArena` is a high-performance, generational arena designed for storing PDF objects. 
- **Generational Handles**: Objects are accessed via `Handle<Object>`, which contains an index and a generation ID. This ensures that even if an object is deleted and its slot is reused, old handles will not point to new data.
- **Deduplication**: Resources (like `ExtGState` or `ColorSpace`) are automatically deduplicated to minimize memory footprint.

### 2. `Ingestor` (The Physical-to-Semantic Gateway)
The `Ingestor` manages the lifecycle of document ingestion. It follows a multi-pass approach to ensure data integrity.

#### **Pass 0: Physical Normalization**
- **Method**: `perform_pass_0_normalization`
- **Technique**: Stack-based iterative traversal (Rule 6).
- **Functionality**:
    - **Global Decryption**: Decrypts all strings and streams using the active `SecurityHandler`.
    - **Acrobat Compatibility**: Strips the `/Encrypt` dictionary from the trailer after successful decryption.
    - **Header/Trailer Repair**: Normalizes object IDs and generation numbers to prevent structural drift.

### 3. `Refinery` (The Semantic Normalizer)
Once data is in the Arena, the `Refinery` performs context-aware transformations.
- **Unicode Re-encoding**: Intelligent conversion of `PdfString` to UTF-8, handling CMap differences and CID-to-GID mapping.
- **Object Cloning**: Provides the logic for deep-copying objects between documents while maintaining reference integrity.

## Usage Example

```rust
use fepdf_core::ingest::Ingestor;

let mut ingestor = Ingestor::new(arena);
// Pass 0 normalization is called internally during ingest() or manually
ingestor.ingest(raw_doc)?;
```

## Compliance & Safety
- **RR-15 Hardened**: Zero `unsafe` blocks, zero wildcards in `match`, and no recursion.
- **ISO 32000-2**: Optimized for PDF 2.0 object streams and cross-reference streams.
