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

**Where the time goes, re-measured 2026-09-10 — one machine, nothing else running, two
consecutive runs of each form, everything already built.** A run is **29.8 to 30.9
seconds** and reported **848 tests**. Derive both from one run:

```bash
time cargo test --workspace 2>&1 | grep -oE 'test result: ok\. [0-9]+' \
  | grep -oE '[0-9]+' | awk '{s+=$1} END{print s " tests"}'
```

**The count is 855 as of 2026-09-13**, re-derived with that command: the seven added are
`crates/fepdf/tests/text_string_encoding_test.rs`. Every other figure below belongs to the
2026-09-10 run and is left at what that run measured — the timings are a statement about
one machine, and re-deriving a number on a different one would not correct them.

**This paragraph used to say 1m 57s for 591 tests, and to recommend a shorter form on the
strength of it.** Both halves stopped being true:

| | measured 2026-08-30 | measured 2026-09-10 |
| :--- | ---: | ---: |
| `cargo test --workspace` | 1m 57s, 591 tests | **29.8–30.9s, 848 tests** |
| `cargo test --workspace --lib --bins --tests` | 1m 31s | 25.2–25.3s, 848 tests |
| the difference — the doc-test phase | **26s, a fifth of the run** | **4.7s, and 0 tests** |

257 more tests at **a quarter of what 591 tests cost**. The doc-test phase went from a
fifth of the run to 4.7 seconds of it, and it still counts nothing — see below for why
that is not a reason to reach for the short form.

**Two test binaries are 19 of the 30 seconds**, and both open documents:

| | 2026-09-10 | what it does |
| :--- | ---: | :--- |
| `tests/pattern_color_test.rs` | 9.9s | patterns and colour over the corpus. 5.8s of it is opening `samples/fy05.pdf` once, shared by the binary; the rest is extracting its 846 pages at 4.8ms each |
| `tests/parser_twin_test.rs` | 9.4s | opens each sample twice, refined and not, to hold the two content-stream readers to the same conclusions |

`fepdf-wasm`'s `reading_test.rs` was a third entry at 7.2s until 2026-09-10, when two of
its tests were found to be opening `fy05.pdf` — 5.8 seconds — for assertions that could
not fail. It runs in 0.07s now, and both assertions check what their names say. **A slow
test is worth reading before it is worth optimising**: the cost was the symptom and a
vacuous assertion was the defect.

`parser_twin_test` was 46 seconds until `intel_sdm.pdf` and `fy05.pdf` were left out of
it: measured, they were nine tenths of its cost and caught none of the three defects it
has found, and both are compared page for page against PDFKit by
`crosscheck_reading_order.sh` instead. **Not "the two largest"** — `samples/volvo_xc90.pdf`
is larger than either and stays, at 1.88s against `unicode_16.pdf`'s 3.43s for a file half
the size. The list follows measured cost and measured detection, not bytes; the test's own
comment carries the per-sample figures.

**The cost was the debug build, not the work**, and half of it is gone. The same nine
documents open twice in 5.4 seconds under `--release`, which is what asked the question.
Measured 2026-09-09 at 814 tests, `[profile.dev.package."*"] opt-level = 2` took `cargo
test --workspace` from **45.9s to 27.5s**. That setting is in `Cargo.toml` now, so the
29.8s above is the tuned figure and not something it could still buy. The one-off price is
**212 seconds** to rebuild the dependency graph, paid again whenever a dependency changes,
and recovered by the eighth run.

`package."*"` and not `[profile.dev]`: optimising this workspace's own crates would put
that cost on every edit-and-rebuild, which is the loop a contributor is actually in.

**What made the suite fast in the first place is unchanged**, and is most of why 848
tests cost a quarter of what 591 did:
[ADR-0074](docs/adr/0074-the-reader-copied-the-file-once-per-object.md) (the reader copied
the file once per object), [ADR-0075](docs/adr/0075-two-costs-a-caller-never-asked-for.md)
(the arena maintained a reverse index nothing queried) and
[ADR-0076](docs/adr/0076-what-the-arena-compresses-and-what-that-was-costing.md)
(compression level). The suite opens documents, so it inherited all three.

**Both forms report the same 848**, which is the doc-test phase saying in a second way
what the paragraph above says: it runs no examples, so it counts none.

**So the short form is no longer worth knowing about.** It was documented because it saved
a fifth of the run; it now saves 4.7 seconds of 30, and it still stops guarding doc
examples. `cargo test --workspace` is the gate and there is no reason to reach past it for
that.

What has not changed is that the doc-test phase checks nothing: `rustdoc` builds a harness
for each library crate and finds no examples, because no doc comment in the workspace
carries a ```` ```rust ```` block — the convention here is ```` ```text ````. Verify with:

```bash
grep -rn '```rust' crates/*/src --include='*.rs' | wc -l   # 0
```

**A measurement quoted and not re-derived is the thing this file exists to warn about.**
This one stood for eight days and was wrong by 4x; `ARCHITECTURE.md`'s crate-size table
was stale by up to 1,956 lines on the same day. Both are re-derived by a command written
beside the number.

**A run that has to *build* costs far more than either**: 8m 21s for a change that touched
`fepdf-render`, measured 2026-08-30 and not re-derived since. The compile is the cost, not
the suite — and the gap widened when the suite fell to half a minute, so a change that
compiles is now roughly fifteen times a warm run rather than four.

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
  - `tests/text_string_encoding_test.rs`: that the entries ISO 32000-2 types as *text
    strings* (7.9.2.2) survive being written and read back, and that the byte strings
    beside them — a `/EmbeddedFiles` name-tree key, the collection `/D` that must equal
    one, a filespec's `/F` — still match each other byte for byte. Every value is outside
    PDFDocEncoding, because an ASCII one round-trips through the defect untouched. Each of
    the eleven sites was verified by putting `Object::String(Bytes::from(…))` back, and
    each of the three byte-string cases by making it `Object::Text` instead.
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

**Six and not seven: the cyclic-reference case is not here.** The walks bounded on
2026-09-05 ([ADR-0060](docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md),
[ADR-0061](docs/adr/0061-four-walks-bounded-and-two-that-were-not-what-the-sweep-said.md))
were found with a file whose catalogue names an object holding a reference to itself, and
the obvious home for it was this directory. It is not one: a self-reference is conforming
*syntax* — 7.3.10 lets any object be an indirect reference — and what is wrong with it is
a cycle in the object graph, not a damaged clause 7.5 structure. Adding it would make this
corpus two things, and the sentence above it would stop being true of every file in it.
The fixtures live in the tests that need them instead, built inline, where each says which
walk it is about.

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
| `crosscheck_pubsec.sh` | Certificate-encrypted documents | Reported IT OPENED WITHOUT A CERTIFICATE about an engine that had refused, for one run in 2026-09-09 — see the note below on `grep -q` |
| `crosscheck_reading_order.sh` | Whether the two readers put the same characters in the same *order*, per page, per file | A net figure hides a file. ADR-0047's sort took the corpus from 261 agreeing pages to 1,975 while `volvo_xc90.pdf` went from 61 to **0** and `bokutokitan.pdf` from 93 to 4, and nothing said so. Each file carries a floor it may not fall below and the best it has ever read; a file under its best is printed on every run without turning the suite red |
| `crosscheck_selfread.sh` | This engine reads back what it wrote, 21 combinations of packing, encryption and signing | The only one needing no second implementation, and so the only one that can answer "can it read what it just wrote". Also compares the catalogue key by key and the named destinations, which a byte comparison cannot |

**A check that cries wolf is worse than one that is missing**, and these scripts had nine
ways to do it. `printf '%s' "$out" | grep -q PATTERN` under `set -o pipefail` reports 141:
`grep -q` exits at the first match, the writer dies of SIGPIPE, and the pipeline's status
is the writer's. It fires only when the output is long enough that the writer has not
finished, so it arrives with a document that got wordier rather than with the change that
introduced it. On 2026-09-09 `crosscheck_pubsec.sh` printed **IT OPENED WITHOUT A
CERTIFICATE** and **A STRANGER'S CERTIFICATE OPENED THE DOCUMENT** about an engine whose
refusal was correct, and whose refusal message had grown to 538 lines of decisions.

Use a herestring — `grep -q PATTERN <<<"$out"` — and there is no pipe to break. All nine
sites do, across `cli_smoke.sh`, `measure_external_corpus.sh`, `crosscheck_pubsec.sh`,
`crosscheck_selfread.sh` and `crosscheck_signature.sh`.

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
