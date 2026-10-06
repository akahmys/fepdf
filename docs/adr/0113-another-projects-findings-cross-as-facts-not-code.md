# ADR-0113: Another project's findings cross as facts, not as its code

- **Status**: Accepted
- **Date**: 2026-10-07
- **Commit**: (see the commit that adds this file)

## Context

fepdf is MIT-licensed. Phase Z took three things from outside the project: PrintCraft's
list of the inputs that broke it, pdf.js's test files, and `cargo-fuzz`.

- **PrintCraft** (`storytold/printcraft`) is MIT OR Apache-2.0. Its `vendor/README.md`
  says, for each patch, which input did what — `/Columns 4294967295` allocating two 4 GiB
  rows, a `/Kids` loop overflowing the stack — and names the test that holds it. Copying
  a test or a patch is permitted, but MIT then requires its copyright notice to travel
  with the copy. A file in this repository would carry two notices, and the next reader
  would have to work out which lines were whose.
- **pdf.js** (`mozilla/pdf.js`) is Apache-2.0 as a repository. The files in `test/pdfs`
  are not all its own: many are documents attached to bug reports, and 314 of the first
  1,000 entries are `.link` files naming a URL elsewhere. The repository's licence says
  nothing about who owns a document someone attached to an issue.
- **`cargo-fuzz`** and `libfuzzer-sys` are MIT OR Apache-2.0. libFuzzer itself is LLVM's,
  Apache-2.0 with the LLVM exception. They build the fuzzer; nothing they build ships.

Two precedents existed already. `fetch_external_corpus.sh` fetches the veraPDF corpus into
`target/` and never commits it, because that repository states no licence. And
`docs/reference/acrobat-features/` carries a PrintCraft file copied with its MIT notice
beside it. That is data, kept whole, in its own directory.

## Decision

**Material crosses from another project in one of three ways, chosen by what it is.**

1. **A finding is a fact, and is restated.** What input broke another implementation and
   how is not expression, and a test here is written from that description: its own
   fixture, its own code. The source is named in the test's doc comment or in ROADMAP. No
   copyright notice comes with it, because nothing was copied. PrintCraft's code — its
   patches, its tests, its fuzz targets — is not copied into this repository.
2. **A file whose ownership the licence does not settle is fetched, not committed.**
   pdf.js's committed test files go to `target/external/` as veraPDF's do. A `.link` URL
   is not followed: it points at a third party's document, with no licence and no
   guarantee it is still there.
3. **A tool stays outside what ships.** `fuzz/` is not a workspace member and no
   published crate depends on it, so `cargo-fuzz` and libFuzzer never reach a release
   artefact, and `deny.toml` has nothing new to judge.

Whole data copied under its own licence, as `docs/reference/acrobat-features/` is, stays
possible. It goes in its own directory with the notice beside it, and it is never mixed
into source.

## Consequences

- `LICENSE` stays a single MIT notice, and no source file in `crates/` carries another
  project's copyright.
- A test written from a description can differ from the original input that broke the
  other project. That is acceptable: the original broke *their* code. The test here is
  shown to fail against *this* code before the fix lands (AGENTS.md rule 5), as
  `type1_subroutine_fan_out_tests` did on 2026-10-07.
- The pdf.js files cannot be quoted file by file in a committed document as the samples
  are, since a reader may not have them. Figures over them are stated as over
  `target/external/pdfjs`, with the date of the fetch.
