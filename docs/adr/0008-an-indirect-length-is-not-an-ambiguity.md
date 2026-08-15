# ADR-0008: An indirect `/Length` is conforming, so reading one records nothing

- **Status**: Accepted
- **Date**: 2026-08-15
- **Commit**: the Phase B structure survey

## Context

Phase B begins with `inspect structure`, whose last column is "the decisions taken
while reading". Surveying the corpus before designing the command — which is the habit
Phase A established — produced a figure that did not make sense:

| File | Decisions recorded |
| :--- | ---: |
| `samples/sample.pdf` | 31 |
| `samples/constitution.pdf` | 31 |
| every other sample | 0 |

All 31 were the same: `[Ambiguity] 7.3.8.2 /Length absent or an indirect reference ->
delimited the data by scanning to endstream`.

`sample.pdf` has exactly 31 streams, and every one of them writes `/Length` as an
indirect reference — `5 0 R`, `15 0 R`, and so on. That is not a defect. ISO 32000-2
7.3.8.2 permits the form explicitly, and producers use it because the length is not
known when the dictionary is written.

`examples/length_crosscheck.rs` resolved each reference against the file's own integer
objects and compared it with the extent the reader scanned:

| File | Agree | Disagree |
| :--- | ---: | ---: |
| `sample.pdf` | 31 | 0 |
| `constitution.pdf` | 31 | 0 |

So the file was conforming, the reader's extent was correct, and only the record was
wrong. Two things followed from it:

- `DecisionLog::is_conforming` returned `false` for a conforming file, contradicting
  its own documented meaning.
- Phase B's last item is to surface the log in every output format. Doing that would
  have reported 31 departures from the standard on a clean document.

The cause was a signature. `resolve_stream_extent` took `declared: Option<usize>`, so
"`/Length` is an indirect reference" and "`/Length` is missing" arrived as the same
`None` and shared an arm. They are opposites: one is permitted, the other omits a
required key.

## Decision

Distinguish the three states in the type, and record only what departs:

```rust
enum DeclaredLength { Direct(usize), Indirect, Absent }
```

- `Indirect` with an `endstream` found: **nothing recorded.** Conforming.
- `Absent` with an `endstream` found: `Repaired`. A required key is missing.
- Everything else keeps the severity it had.

## Consequences

- All nine samples now read with zero decisions, and the five readable malformed files
  each report what is wrong with them. The log became a signal rather than a constant.
- Two silent tolerances surfaced once the noise was gone, and were fixed alongside:
  a header at a non-zero offset (7.5.2) and a missing trailer dictionary (7.5.5) were
  both accepted without a word. `target/malformed/no_trailer.pdf` produced a document
  with no `/Root` and reported a clean read.
- **A test asserted the defect.** `an_indirect_length_is_resolved_by_scanning` required
  an `Ambiguity` to be recorded, and passed throughout. A test written from the code's
  behaviour rather than from the standard will hold a misreading in place, which is the
  more general lesson here.
- One case remains silent and is not fixed: `target/malformed/bad_length.pdf` points an
  indirect `/Length` at the wrong object. The reader scans, gets the right answer, and
  never compares — so a file whose `/Length` lies reads as clean. Catching it means
  retaining the reference through the parse and verifying it after assembly, which is a
  structural change to `parse_indirect_at`'s signature and belongs in its own commit.
  `examples/length_crosscheck.rs` sees it from outside in the meantime, as `30 agree,
  0 disagree, 1 unresolved` against the undamaged file's `31 agree, 0 unresolved` — the
  damaged reference names an object that is not an integer at all.
