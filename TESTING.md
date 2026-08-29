# 🧪 fepdf Testing & Validation Strategy

> **Phase: verification.** What must pass before a change lands. The automated audits are
> in [AUDITING.md](AUDITING.md).

This document details the testing methodology, test suites, visual regression framework, and quality assurance processes for fepdf.

---

## 🎯 1. Test Pyramid & Separation Policy

fepdf employs a 3-tier testing hierarchy with strict **Test Code Separation**:

```
                  ┌──────────────────────────────┐
                  │ 1. Visual Regression Tests   │ (Python + Vello Render)
                  ├──────────────────────────────┤
                  │ 2. Crate Integration Tests   │ (crates/*/tests/*.rs)
                  ├──────────────────────────────┤
                  │ 3. Workspace Unit Tests      │ (#[cfg(test)] in src/)
                  └──────────────────────────────┘
```

### 📁 Test Code Separation Guidelines
1. **Integration & Large Test Suites (`crates/*/tests/`)**:
   - Multi-file tests, scenario tests, schema expansions, and end-to-end integration tests MUST be located in the crate's root `tests/` directory (e.g., `crates/fepdf-model/tests/parser_tests.rs`).
   - Do NOT place standalone test files inside `src/` (e.g., `src/schema_tests.rs` is forbidden).
2. **Inline Unit Tests (`src/`)**:
   - Small, private helper unit tests may reside alongside production code inside `#[cfg(test)] mod tests { ... }` blocks at the bottom of `src/` files.
   - Test utilities or mock helpers must NOT pollute production module exports.

---

## 🧪 2. Workspace Unit & Integration Tests

All crates in the workspace MUST maintain high test coverage for core data structures, object sublimation, and font handling.

### Run All Unit & Integration Tests
```bash
cargo test --workspace
```

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
  - `tests/mapping_tests.rs`: Unicode/CID mapping & encoding reconciliation.
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

## 🖼️ 3. Visual Regression Testing

GPU compute rendering fidelity via **Vello** is verified against baseline images in
`samples/references/`, four pages chosen for what they exercise: Latin text, Japanese
text, print colour, and a page that is mostly vector art.

**It detects change, not correctness.** The baselines are this engine's own output,
frozen — so the suite answers "does this still render what it rendered yesterday" and
cannot answer "is that right". The check that asks a *second* renderer is
`crosscheck_image.sh`, and it compares images and layers rather than text. Text, layout
and colour are covered here and against nobody else, which is worth knowing before
trusting a pass.

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

## 🦀 4. Minimum Supported Rust Version (MSRV) Verification

The project requires **Rust 1.94+ (Edition 2024)**. MSRV compatibility is verified via:

```bash
cargo check --workspace
```

---

## 🛠️ 5. Pre-Merge Quality Checklist

Before submitting a Pull Request or completing a task:

- [ ] `./scripts/audit/verify_compliance.sh` completes with `=== AUDIT PASSED ===`.
- [ ] `cargo test --workspace` passes with 0 failures (269 tests across 35 suites, 2026-08-17).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean. `--all-targets` matters: without it tests, examples and benches are never linted.
- [ ] `cargo fmt --all --check` reports no diff. Enforced as Rule 19 by the audit, so `make audit` covers it.
- [ ] All integration tests are placed in `crates/*/tests/` following the Test Separation Policy. Binary crates (`fepdf`) are the exception: an integration test cannot reach into a binary, so their unit tests live in inline `#[cfg(test)] mod tests` blocks, which the rule permits.
- [ ] `cargo deny check licenses` returns `licenses ok`.
- [ ] `.git/hooks/pre-commit` passes secret scanning without leaks.
- [ ] `./scripts/test/cli_smoke.sh` — **a debug build**. Every other check in this list
      runs `--release`, and clap's duplicate-argument check is a `debug_assert`: a
      collision between two argument definitions once panicked seven subcommands while
      the whole verification suite stayed green.
- [ ] `./scripts/test/crosscheck_roundtrip.sh` — text preserved through a save, compared
      against a second implementation. Internal measurement cannot find a page the
      engine never built; this is what caught [ADR-0006] and [ADR-0010].
- [ ] `./scripts/test/crosscheck_signature.sh` — signs every sample and requires
      `openssl cms -verify` and `publish verify-signature` to agree about each one. Only
      run when signing is touched; it needs `openssl` on `PATH`. The engine's own tests
      can say a signature matches the digest the engine computed, which is a smaller
      claim than it sounds — the byte range and the digest are both its own work. The
      script also flips a byte to prove the check can fail.
- [ ] `./scripts/test/crosscheck_encryption.sh` — encrypts every sample and has PDFKit
      open it with the password, comparing per-page text against the plain save. Only
      run when encryption or string writing is touched. It earns its place: on its first
      run it found that the writer emitted an unescaped carriage return in literal
      strings while the lexer read one back unchanged — two mistakes that cancelled, so
      the engine's own round trip was clean and had been for as long as anyone looked.
- [ ] `./scripts/test/crosscheck_objstm.sh` — packs every sample and has PDFKit read it,
      with the size change beside it. Only run when the writer or the cross-reference
      code is touched. Packing moves almost every object inside a compressed container
      reached through a type 2 entry, so a reader that gets the indirection wrong finds
      nothing rather than something subtly wrong — which makes this the bluntest of the
      three cross-checks and the quickest to fail honestly. Set `PDFIUM` to a Python that
      has `pypdfium2` and a second, unrelated reader checks it too; the script's header
      says how. That second reader is what made packing the default.
- [ ] `./scripts/test/crosscheck_pubsec.sh` — builds certificate-encrypted documents
      with openssl and reads them back. Only run when clause 7.6 is touched. It runs
      backwards from the other cross-checks because there is no reader to compare
      against: pdf.js, PDFium and qpdf all decline the clause, so an independent
      *producer* stands in for an independent reader. It compares text rather than exit
      status: `inspect text` exits non-zero when a page will not extract, and
      `samples/fy05.pdf` has six that do not — on the plaintext file as much as the
      encrypted one.
- [ ] `./scripts/test/crosscheck_selfread.sh` — reads every file this engine writes back
      **with this engine**, across 21 combinations of packing, encryption and signing.
      The only cross-check needing no second implementation, and the only one that can
      answer "can it read what it just wrote" — the other four measure both sides with
      somebody else, so they are silent on it. It also asserts every page of every sample
      extracts, because a comparison is blind to a defect the reader makes on both sides.
      And it compares the *catalogue* in against the catalogue out, key by key, with the
      value's shape — `ROADMAP.md` opened with that claim for months and nothing checked
      it, long enough for the paragraph to still name two keys as untyped that had since
      been typed. Renumbering and the always-written `/Metadata` are normalised away;
      verified by renaming one key to a same-length name so the offsets still land, which
      the comparison reports as one differing entry out of seven. Since Phase K it also
      compares what the catalogue **says** — the entries are read now, so a save that
      preserved `/MarkInfo` as a dictionary while losing `/Marked` inside it is visible,
      where before there was `dictionary[3]` against `dictionary[3]`. That comparison
      found one difference on its first run and it was the expected one:
      `bokutokitan.pdf`'s page-tree root carries an inheritable `/MediaBox` the writer
      resolves onto each page (ADR-0013), normalised away like the others. A third
      comparison
      covers named destinations (12.3.2), which the catalogue check cannot: `/Dests`
      surviving as a key says nothing about whether the references into it still find
      their targets, and both of that clause's forms are indirect while saving renumbers
      every object. Verified the same way — renaming one declared key in `unicode_16.pdf`
      to a same-length hex string turns its four references into four dangling links.
      `status.sh --full` runs it.
- [ ] `./scripts/test/fetch_external_corpus.sh` then
      `./scripts/test/measure_external_corpus.sh` — **515** files this project did not
      choose (242 when this line was written: 37 from `pdf-association/pdf-differences`
      and 205 Isartor files). Run when
      the reader, the fonts or the filters are touched. The nine files in `samples/` were
      picked by this project and every "zero occurrences, so defer" judgement in
      `ROADMAP.md` is bounded by them; on its first run this found a panic in CFF INDEX
      reading that nine files had never produced, and that clause 7.4 has seven of its
      ten filters missing with no row in the roadmap for the clause. Run it against the
      **debug** binary too — arithmetic overflow panics there and wraps in release. It
      exits non-zero only on a panic, because most of this corpus is deliberately
      malformed and a reasoned refusal is a correct outcome.
- [ ] `cargo run --example make_scan_fixtures -p fepdf-model`,
      `make_layer_fixtures` and `make_colour_fixtures`, then
      `./scripts/test/crosscheck_image.sh` — the only check that looks at a *picture*.
      **It is currently red, and is meant to be**: `target/colour/` holds the two files
      ROADMAP.md's Phase P quotes, and they disagree with PDFKit until clause 7.10 gets a
      function evaluator. Everything else in it agrees within one part in 255, so a third
      red line is a regression and the two named ones are not.
      The other five compare text and structure, so the image codecs had nothing
      independent to answer to: neither corpus holds a scan. The fixtures are encoded by
      implementations that are not the decoders under test, and PDFKit renders the same
      files. Four numbers per file — the mean luminance of each quadrant — so that
      antialiasing does not fail it and an inversion, a flip or a wrong stride each read
      differently. Run it when a codec, `PixelFormat`, or anything in `draw_image`
      changes. Verified by removing the JBIG2 polarity flip: `DISAGREE by 255`.

      The layer fixtures are three of the thirteen optional-content constructions, and
      the other ten are deliberately not here: PDFKit paints them, so putting them in
      would make this red against a defect that is not this engine's. They are held by
      `crates/fepdf/tests/optional_content_test.rs` against clause 8.11 instead
      ([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md)).
      Run this when anything in `fepdf-content::canvas` or `optional_content` changes.
- [ ] `fepdf inspect coverage samples/*.pdf target/external/*/*.pdf` — the share of the
      constructs those files actually contain whose contents the engine reads, per axis
      (ADR-0019). Run it when a reader is added or a type changes: it is the only check
      here that answers "did that make the engine understand more", and it cannot be
      raised by adding a type for something no file carries. A minute over `samples/`
      alone, so `status.sh` runs it under `--full` and not in the default view.
- [ ] `./scripts/dev/status.sh` — the figures the documents quote, re-derived. A number
      that has gone stale shows up as a disagreement rather than reading as current.
      It exits non-zero when a row *stopped being about the code*: every counter here
      returns a number and 0 is legal for most of them, so an anchor that moves reports a
      false figure rather than no figure. That happened — the CLI was split into
      `args.rs`, `commands/` and `formatters/`, and `inspect subcommands` read 0 against
      a truth of 8 for as long as nobody checked by hand.

[ADR-0006]: docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md
[ADR-0010]: docs/adr/0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md
