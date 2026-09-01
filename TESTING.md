# fepdf — testing

> **Phase: verification.** What must pass before a change lands. The automated audits are
> in [AUDITING.md](AUDITING.md).

This document details the testing methodology, test suites, visual regression framework, and quality assurance processes for fepdf.

---

## 1. Where a test goes

Three tiers: visual regression (Python driving the Vello renderer), crate integration
(`crates/*/tests/`), and unit tests inline in `src/`.

**A standalone test file in `src/` is forbidden** — Rule 14, checked by the audit.
Multi-file suites, scenarios and end-to-end tests belong in the crate's `tests/`
directory. Small private helper tests may sit in a `#[cfg(test)] mod tests` block at the
bottom of the file they test.

Binary crates are the exception the rule permits: an integration test cannot reach into a
binary, so `fepdf`'s unit tests are inline.

## 2. The suites

All crates in the workspace MUST maintain high test coverage for core data structures, object sublimation, and font handling.

### Run All Unit & Integration Tests
```bash
cargo test --workspace
```

**Where the time goes, measured 2026-08-30 — one machine, one load, both forms run back
to back.** A run with nothing to rebuild is **1m 57s** and reports 591 tests:

| | |
| ---: | :--- |
| 40.6s | `pattern_color_test` — `samples/fy05.pdf` is 846 pages and a debug build takes ~20s to open it. Its two tests share one open now; each opening its own cost 47.2s |
| 17.6s | `rasteriser_determinism_test` — a page rasterised twice on the host |
| 9.2s | `fepdf-syntax`'s unit tests |
| ~5s | everything else, across some fifty test binaries |
| **25s** | **the doc-test phase, which runs 0 tests** — the difference between the two forms below |

**The doc-test phase is a fifth of a warm run and currently checks nothing.** `rustdoc`
builds a harness for each of the eleven library crates and finds no examples: no doc
comment in the workspace carries a ```` ```rust ```` block, because the convention here is
```` ```text ````. Skipping it runs the same 591 tests in **1m 31s**:

```bash
cargo test --workspace --lib --bins --tests
```

That is a thing to know rather than a recommendation. **It stops guarding doc examples**,
and the moment somebody writes a real one the phase starts earning its 25 seconds while
the short form starts hiding a broken example. `cargo test --workspace` stays the gate.

**A run that has to *build* costs far more than either**: 8m 21s for a change that touched
`fepdf-render`, against under two minutes of tests. The compile is the cost, not the suite,
and none of the figures above move that.

### Key Subsystem Test Suites
- **`fepdf-model`**:
  - `tests/parser_tests.rs`: Lexer primitives, tokenizing, and object parser.
  - `tests/security_tests.rs`: R4 AES-128 and R5 AES-256 security handlers.
  - `tests/object_tests.rs`: Object reference resolution, name deduplication, dictionary traversal.
  - `tests/schema_tests.rs`: Font & ExtGState ISO schema expansion.
  - `tests/filter_tests.rs`: Clause 7.4 filters, against the worked example in 7.4.4.2
    and an `ASCII85Decode` table generated from an unrelated implementation. Three
    hand-written expectations in the first version were wrong while the decoder was
    right, which is why the vectors that can be generated now are.
  - `tests/mapping_tests.rs`: Unicode/CID mapping & encoding reconciliation, and the
    `/CIDSystemInfo` a CID font declares (9.7.3). The collection was read with a *name*
    accessor where Table 114 types both entries as strings, so 116 of 116 Type0 fonts in
    both corpora answered `None` and the engine decided from `/BaseFont` substrings
    instead ([ADR-0041](docs/adr/0041-a-character-collection-is-declared-not-guessed.md)).
    Each of the four cases was verified by putting one of the three defects back.
- **`fepdf-render`**:
  - `tests/path_tests.rs`: Bezier curves, path bounds, transformation matrices.
  - `tests/text_tests.rs`: Text positioning and text matrix initialization.
- **`fepdf`**:
  - `tests/sdk_tests.rs`: Facade API, color conversions, rotation modes, document lifecycle.
    Since Rule D removed the facade's mutating methods it exercises them as `Operation`s,
    which is what a caller now has. Two of its cases cover `DuplicatePages`: the ordering
    one was verified by putting the bug back, and the measured failure was worse than the
    one predicted when it was written — page 0 cloned three times rather than a
    mis-ordering, because after the first insertion the remaining indices name clones.
  - `tests/backend_operations_test.rs`: Document mutation operations execution.
  - `tests/encrypted_objstm_test.rs`: Encrypted object stream ingestion.
  - `tests/pattern_color_test.rs`: Pattern color extraction.
  - `tests/rasteriser_determinism_test.rs`: that `Rasteriser::Cpu` draws a page the same
    way twice, which `Rasteriser::Gpu` does not — RR-15 Rule 10 against the renderer, which
    nothing checked
    ([ADR-0043](docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md)). **Its
    sample is load-bearing**: written against `print_sample.pdf` it passed on the GPU six
    runs of six, because that page happens not to flake; on `sample.pdf` the GPU form fails
    3 in 6 and the CPU form passes 8 in 8. Needs the `render` feature, which
    `cargo test --workspace` supplies by unification, and about 18 seconds.
- **`fepdf-doc`**:
  - `tests/operation_json_tests.rs`: the `Operation` vocabulary as JSON, which is a public
    interface — `fepdf-mcp`'s `apply_operation` tool deserialises a caller's string into
    one. This crate had **no `tests/` directory at all** until Rule D moved six operations
    into it, so the vocabulary's serialised form had never been exercised. Its
    `variant_name` function matches every variant with no wildcard, so RR-15 Rule 5 turns
    a new operation into a compile error here until someone decides what its JSON is —
    verified by adding a variant and watching `E0004`.
- **`fepdf-mcp`**:
  - `tests/mcp_server_tests.rs`: Model Context Protocol server error display and schema validation.

### The malformed corpus

Six files, each damaging one part of ISO 32000-2 clause 7.5, are the reader's
acceptance test; `docs/adr/0003` and `ROADMAP.md` both quote results measured against
them. They are generated rather than committed, from `samples/sample.pdf`:

```bash
python3 scripts/test/make_malformed.py
cargo run -q -p fepdf-model --example read_probe -- target/malformed/*.pdf
```

Reading recovers 111 objects from five of them — the count of the undamaged file — and
77 from the truncated one, being all that survives. The truncated file has no
`/Type /Catalog` at all, so it reads but cannot be *opened* as a document; that is the
expected result, not a gap.

---

## 3. Visual regression

GPU compute rendering fidelity via **Vello** is verified against baseline images in
`samples/references/`, four pages chosen for what they exercise: Latin text, Japanese
text, print colour, and a page that is mostly vector art.

**It detects change, not correctness.** The baselines are this engine's own output,
frozen — so the suite answers "does this still render what it rendered yesterday" and
cannot answer "is that right". The check that asks a *second* renderer is
`crosscheck_image.sh`, and it compares images and layers rather than text. Text, layout
and colour are covered here and against nobody else, which is worth knowing before
trusting a pass.

**It compares to a channel delta of 1, and that tolerance is load-bearing.** The engine
encodes a byte-identical scene for a page every time; vello's GPU pipeline turns that one
scene into more than one image, and every such difference measured is one isolated pixel
at a delta of 1 ([ADR-0043](docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md)).
Anything a reader could see is far above it — the stale baseline this suite caught was 28
pixels at a delta of 222. Where a repeatable image is needed rather than a tolerated one,
`publish render --cpu` gives one:

```bash
cargo run -p fepdf-render --example render_determinism -- samples/sample.pdf 1 8
```

**A baseline is refreshed only with evidence that the new output is the better one.**
`constitution.pdf`'s was refreshed on 2026-08-30 because the engine had begun drawing the
page number at the foot of page 1 and the reference predated it — and PDFKit reads that
`1` in the page's text, which is the second opinion that makes the refresh a repair rather
than a way of making a failure go away.

**The baselines are not in the repository, and neither are the samples.** `.gitignore`
excludes `/samples/` outright, so `samples/references/` is local to whichever machine
generated it: a fresh clone reports `[FAIL] Reference baseline missing` for all four
cases, and "the suite passes" is a statement about one machine rather than about the
commit. That is a property of this check worth knowing before quoting a pass — it is the
one suite here whose subject cannot be reconstructed from what is tracked.

`scripts/test/verify_visuals.sh` used to sit beside this, running
`cargo test --package fepdf-render --test visual_regression`. There has never been such a
test target, so it exited 101 every time it was run and nothing referenced it. Deleted
rather than repaired: it duplicated a suite that works.

### Run Visual Regression Tests
```bash
python3 scripts/visual_regression.py
```

### Update Baseline Reference Images
```bash
python3 scripts/visual_regression.py --update
```

---

## 4. Before a change lands

**Always:**

| | |
| :--- | :--- |
| `./scripts/audit/verify_compliance.sh` | Must end `=== AUDIT PASSED ===`. **Read the last line, not the first**, and run it with no other edits in flight — a run racing an edit reads a tree that no longer exists. |
| `cargo test --workspace` | 0 failures. It does not imply the audit: two of the audit's findings on 2026-08-29 were invisible to `cargo test -D warnings`. |
| `./scripts/test/cli_smoke.sh` | **A debug build.** Every other check here builds `--release`, where `debug_assert!` is compiled out, so a debug-only panic ships. |

**When the area is touched**, each against a second implementation:

| | Covers | Why it earns its place |
| :--- | :--- | :--- |
| `crosscheck_roundtrip.sh` | Text survives a save, against PDFKit | The engine comparing its own output to itself cannot see a symmetric defect |
| `crosscheck_signature.sh` | `openssl cms -verify` and `publish verify-signature` agree. Needs `openssl` | The engine's own test says a signature matches the digest the engine computed — the byte range and the digest are both its own work |
| `crosscheck_encryption.sh` | PDFKit opens what this engine encrypted, per-page text | Found the writer emitting an unescaped `\r` in literal strings and the lexer reading one back unchanged: two mistakes that cancelled |
| `crosscheck_objstm.sh` | PDFKit reads packed object streams | |
| `crosscheck_pubsec.sh` | Certificate-encrypted documents | |
| `crosscheck_reading_order.sh` | Whether the two readers put the same characters in the same *order*, per page, per file | A net figure hides a file. ADR-0047's sort took the corpus from 261 agreeing pages to 1,975 while `volvo_xc90.pdf` went from 61 to **0** and `bokutokitan.pdf` from 93 to 4, and nothing said so. Each file carries a floor it may not fall below and the best it has ever read; a file under its best is printed on every run without turning the suite red |
| `crosscheck_selfread.sh` | This engine reads back what it wrote, 21 combinations of packing, encryption and signing | The only one needing no second implementation, and so the only one that can answer "can it read what it just wrote". Also compares the catalogue key by key and the named destinations, which a byte comparison cannot |

**Corpus and measurement:**

| | |
| :--- | :--- |
| `./scripts/test/fetch_external_corpus.sh` | 515 files this project did not choose. Zero occurrences measures the corpus, not the world |
| `fepdf inspect coverage samples/*.pdf target/external/*/*.pdf` | The share of what the files contain whose contents the engine reads |
| `./scripts/dev/status.sh` | Re-derives every figure the documents quote, so a stale one reads as a disagreement. It exits non-zero when a row stops being *about the code*: `inspect subcommands` once read 0 against a truth of 8 after the CLI was split, because 0 is a legal answer |
| `cargo run --release -p fepdf --example glyph_loss -- samples/*.pdf` | What extraction loses, and what it was out of. `status.sh --full` runs it. `--codes` adds the font, the character code, the glyph name the encoding gave it, the route that failed and a page to look at, which is the difference between a count and a direction. **The denominator was not derivable before this existed**: the 9.10.2 violation is recorded only on pages that lost something, so summing the messages counts the glyphs on lossy pages and not the ones on the rest |

**Each check must be shown to fail.** `crosscheck_signature.sh` flips a byte;
`crosscheck_selfread.sh` renames one catalogue key to a same-length name. A check nobody
has broken is a check nobody has tested — several here passed against the defect they
were written for.

[ADR-0006]: docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md
[ADR-0010]: docs/adr/0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md
