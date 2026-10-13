# fepdf — testing

> **Phase: verification.** What must pass before a change lands. The automated audits are
> in [AUDITING.md](AUDITING.md).

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

### Run all unit and integration tests
```bash
cargo test --workspace -- --include-ignored
```

**A test that reads a file the repository does not hold is `#[ignore = "needs …"]`.**
`samples/` and `docs/specs/` are not committed — the samples are other people's documents
and the repository is public — so on a clone, and in CI, those tests can only fail. CI runs
`cargo test --workspace`, which skips them and says how many it skipped; here,
`--include-ignored` runs them as well, and that is the gate. Ignoring a test for any other
reason switches it off, and the audit's Rule 14 step fails on it. Measured 2026-10-10 with
both directories moved away: 94 tests failed, and each is now one of these.

**What a run costs is derived, not quoted.** One command gives both the time and the count:

```bash
time cargo test --workspace 2>&1 | grep -oE 'test result: ok\. [0-9]+' \
  | grep -oE '[0-9]+' | awk '{s+=$1} END{print s " tests"}'
```

On 2026-09-10, warm, a run took about 30 seconds, and two binaries that open documents —
`suite/pattern_color_test.rs` (patterns over the corpus; most of it opening
`samples/fy05.pdf` once) and `suite/parser_twin_test.rs` (each sample opened twice, to
hold the two content-stream readers to the same conclusions) — were 19 of them.
`parser_twin_test` leaves out `intel_sdm.pdf` and `fy05.pdf` because measured they were
nine tenths of its cost and caught none of its defects; `crosscheck_reading_order.sh`
compares both against PDFKit instead. The list follows measured cost and detection, not
bytes, and the test's own comment carries the per-sample figures.

**A slow test is worth reading before it is worth optimising.** Two tests once spent 5.8
seconds opening `fy05.pdf` for assertions that could not fail: the cost was the symptom
and a vacuous assertion was the defect.

**The dependencies build at `opt-level = 2` in the dev profile**
(`[profile.dev.package."*"]`), which took a warm run from 45.9s to 27.5s on 2026-09-09 at
a one-off rebuild of about 212 seconds. It is `package."*"` and not `[profile.dev]`:
optimising the workspace's own crates would put that cost on every edit-and-rebuild.
**A run that has to *build* costs far more than the run** — 8m 21s for a change to
`fepdf-render`, measured 2026-08-30.

**The doc-test phase checks nothing**, so `--lib --bins --tests` saves only its few
seconds and is not worth reaching for: no doc comment in the workspace carries a
```` ```rust ```` block — the convention here is ```` ```text ````.

```bash
grep -rn '```rust' crates/*/src --include='*.rs' | wc -l   # 0
```

### Key subsystem suites
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
  - `tests/text_tests.rs`: Text positioning and text matrix initialization.
- **`fepdf`**:
  - `tests/suite/sdk_tests.rs`: Facade API, color conversions, rotation modes, document lifecycle.
    Since Rule D removed the facade's mutating methods it exercises them as `Operation`s,
    which is what a caller now has. Two of its cases cover `DuplicatePages`: the ordering
    one was verified by putting the bug back, and the measured failure was worse than the
    one predicted when it was written — page 0 cloned three times rather than a
    mis-ordering, because after the first insertion the remaining indices name clones.
  - `tests/suite/backend_operations_test.rs`: Document mutation operations execution.
  - `tests/suite/encrypted_objstm_test.rs`: Encrypted object stream ingestion.
  - `tests/suite/pattern_color_test.rs`: Pattern color extraction.
  - `tests/suite/text_string_encoding_test.rs`: that the entries ISO 32000-2 types as *text
    strings* (7.9.2.2) survive being written and read back, and that the byte strings
    beside them — a `/EmbeddedFiles` name-tree key, the collection `/D` that must equal
    one, a filespec's `/F` — still match each other byte for byte. Every value is outside
    PDFDocEncoding, because an ASCII one round-trips through the defect untouched. Each of
    the eleven sites was verified by putting `Object::String(Bytes::from(…))` back, and
    each of the three byte-string cases by making it `Object::Text` instead.
  - `tests/suite/rasteriser_determinism_test.rs`: that `Rasteriser::Cpu` draws a page the same
    way twice, which `Rasteriser::Gpu` does not — RR-15 Rule 10 against the renderer, which
    nothing checked
    ([ADR-0043](docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md)). **Its
    sample is load-bearing**: written against `print_sample.pdf` it passed on the GPU six
    runs of six, because that page happens not to flake; on `constitution.pdf` the GPU form fails
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
them. They are generated rather than committed, from `samples/constitution.pdf`:

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
cargo run -p fepdf-render --example render_determinism -- samples/constitution.pdf 1 8
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

### Run visual regression
```bash
python3 scripts/test/visual_regression.py
```

### Update the baselines
```bash
python3 scripts/test/visual_regression.py --update
```

---

## 4. Before a change lands

**Always:**

| | |
| :--- | :--- |
| `./scripts/audit/verify_compliance.sh` | Must end `=== AUDIT PASSED ===`. **Read the last line, not the first**, and run it with no other edits in flight — a run racing an edit reads a tree that no longer exists. |
| `cargo test --workspace -- --include-ignored` | 0 failures. Without `--include-ignored` the tests that need `samples/` do not run (§2). It does not imply the audit: two of the audit's findings on 2026-08-29 were invisible to `cargo test -D warnings`. |
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
| `./scripts/test/fetch_external_corpus.sh` | 515 files this project did not choose. Zero occurrences measures the corpus, not the world. It also fetches pdf.js's 989 test files into `target/pdfjs/`, apart, so figures quoted over the 515 stay true of them |
| `CORPUS=target/pdfjs ./scripts/test/measure_external_corpus.sh` | The same measurement over pdf.js's files. Each step may take `LIMIT` seconds, 60 by default, and one that takes longer fails the run as a panic does |
| `fepdf inspect coverage samples/*.pdf target/external/*/*.pdf` | The share of what the files contain whose contents the engine reads |
| `./scripts/dev/status.sh` | Re-derives every figure the documents quote, so a stale one reads as a disagreement. It exits non-zero when a row stops being *about the code*: `inspect subcommands` once read 0 against a truth of 8 after the CLI was split, because 0 is a legal answer |
| `cargo run --release -p fepdf --example glyph_loss -- samples/*.pdf` | What extraction loses, and what it was out of. `status.sh --full` runs it. `--codes` adds the font, the character code, the glyph name the encoding gave it, the route that failed and a page to look at, which is the difference between a count and a direction. **The denominator was not derivable before this existed**: the 9.10.2 violation is recorded only on pages that lost something, so summing the messages counts the glyphs on lossy pages and not the ones on the rest |

**Each check must be shown to fail.** `crosscheck_signature.sh` flips a byte;
`crosscheck_selfread.sh` renames one catalogue key to a same-length name. A check nobody
has broken is a check nobody has tested — several here passed against the defect they
were written for.

For a test, `scripts/test/mutate_once.py FILE OLD NEW -- <cargo test arguments>` breaks
one place in the code it guards, runs it, and puts the file back byte for byte: `FIRES`
is the answer a test owes, and names each failed test with its panic; `SURVIVES` says it
does not guard what it is about, and `ERROR` that cargo failed with no test failing.

**A linearised save is held against qpdf** when a change touches the linearised writer:
`scripts/test/check_linearization.sh` linearises every sample and runs
`qpdf --check-linearization` on each, and says how many read with errors. qpdf is a
developer's tool here, not a dependency, and the script stops rather than passes where it
is not installed. The engine reads a linearised file back without its hint tables, so
nothing else checks them. A warning this engine keeps on purpose is listed, with its reason, in
`scripts/test/linearization_known.tsv` (ADR-0108), and is reported rather than counted.

**A save is held against the Arlington PDF Model.** `suite/arlington_test.rs` saves each
sample, reads the save back, and checks every dictionary it reaches against the model in
`external/arlington` for each key's type, the keys a dictionary requires, those 2.0
deprecates, and 7.9.4's dates. What a save departs from is listed in
`suite/arlington_known.tsv`, each line with the ROADMAP item that takes it; the test fails
on a departure not listed, and on a listed one a save no longer makes, so a fix removes its
line. A condition the model writes as `fn:` is not evaluated.

**The facade's tests are one binary, `suite`**, and each file in `crates/fepdf/tests/suite/`
is a module of it, listed in `main.rs`. A file is run alone with
`cargo test -p fepdf --test suite <file>::`, and a new file is not compiled until
`main.rs` names it. One binary took the edit-and-gate loop from 2,122 to 924 seconds
(ROADMAP Y-9); the price is that its tests share a process, so a test may not change
process-wide state — an environment variable, the working directory — that another reads.

---

## 5. Releasing

| | Rule | Enforcement |
| :--- | :--- | :--- |
| **V1** | The version is `0.MINOR.PATCH`, written once, in the workspace `Cargo.toml` | Cargo: every crate takes `version.workspace = true` |
| **V2** | A release is cut when at least one phase of [ROADMAP.md](ROADMAP.md) has closed since the last, and raises MINOR. One that only fixes the last release raises PATCH | **nothing** |
| **V3** | `ROADMAP.md`'s release table carries each release, its date and the phases it closed, and its last row is the version `Cargo.toml` declares | `status.sh`'s `release table behind Cargo.toml (expect 0)`, gated by the audit |
| **V4** | A release's tag is `v` and that version | the release workflow fails a tag that differs |
| **V5** | 1.0 is the owner's call, recorded in an ADR | **nothing** |

Phase letters stay the names of the work and are never renumbered: code, commits and
records name them. A version names a release, and the table is where the two meet
([ADR-0131](docs/adr/0131-a-version-is-a-release-and-a-release-closes-a-phase.md)).

Before tagging, the gate in §4 passes on the commit the tag names.
