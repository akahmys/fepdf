# The fuzzer

ROADMAP Z-1. Two targets, each taking any bytes the way the engine meets them:

| Target | What it does with the bytes |
| :--- | :--- |
| `open_draw_save` | Opens them as a document, draws the first page, extracts its text and saves it |
| `type1_program` | Reconstructs them as an embedded font program, a Type 1 one converted to CFF |

A refusal is an answer. A panic, a hang past the per-input timeout, or an allocation past
the memory limit is a finding, and becomes a test the way Z-2's did
(`crates/fepdf/tests/suite/hostile_input_test.rs`).

**Not in the gate, and not in the workspace** ([ADR-0113](../docs/adr/0113-another-projects-findings-cross-as-facts-not-code.md)):
`cargo-fuzz` builds on nightly, and the gate's toolchain is pinned to 1.98.1.

## Running

Once: `cargo install cargo-fuzz`. Then, from the repository root:

```bash
cargo +nightly fuzz run --fuzz-dir fuzz -a -O open_draw_save target/fuzz/corpus/open_draw_save -- -max_total_time=900 -timeout=60 -rss_limit_mb=2048 -max_len=65536 -artifact_prefix=target/fuzz/artifacts/open_draw_save/
```

`-a` keeps debug assertions on, so an arithmetic overflow is a finding rather than a
wrapped value. **The timeout is 60 s because the build is instrumented**: under the address
sanitizer and debug assertions an input runs some thirty times slower than in a release
build, so 60 s here is about two seconds there. 10 s flagged as hangs pages that a release
build draws in a third of a second.

The page is drawn into a backend that counts calls and keeps none. The fixtures'
`Recorder` keeps every call, and a page nesting Type 3 glyphs to the limit makes about six
million of them (2026-10-08): a run with it ran out of memory measuring the recorder.

The corpus and what a run finds live under `target/fuzz/`, beside the other generated
corpora, and are not committed.

## Seeds

- `open_draw_save`: every PDF of 64 KiB or less under `target/external/`
  (`scripts/test/fetch_external_corpus.sh`) and `target/malformed/`
  (`scripts/test/make_malformed.py`).
- `type1_program`: the `/FontFile` programs of the external files that embed Type 1,
  taken out with `qpdf --show-object=N --filtered-stream-data`.
