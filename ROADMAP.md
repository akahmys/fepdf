# fepdf Roadmap

**Goal**: an engine that understands ISO 32000-2 (PDF 2.0) semantically — not merely
one that round-trips it. PDF 1.7 and earlier are **read-only** targets; output is
always 2.0.

That distinction sets the work. Round-trip fidelity already holds: the arena preserves
objects it has no typed view of, verified key-for-key on a catalogue carrying
`PageLabels`, `ViewerPreferences`, `Threads` and `AcroForm`, none of which are typed.
Understanding them is what remains.

---

## Where the engine actually stands

| ISO 32000-2 | State |
| :--- | :--- |
| **7.3** Objects | Complete. Every type in the clause. |
| **7.5** File structure | **Complete and in use.** Header scan, both cross-reference forms, `/Prev` chains, hybrid references, object streams, incremental updates, and recovery by scanning. `Document::open` reads the file itself; `lopdf` is gone. |
| **7.6** Encryption | **Weakest area.** AES-256 R5/R6 is self-declared non-conformant; public-key (7.6.4) and unencrypted wrapper (7.6.7) are stubs. |
| **7.7** Document structure | 10 of ~30 catalogue entries typed. Untyped entries survive a round trip but cannot be reasoned about. |
| **PDF 2.0 additions** | `DSS`, `AF`, `DPartRoot` have spec types but no read or write path. |
| **8–14** Content, text, interactive, tagged | Interpreter, fonts and UA-2 auditing exist; interactive features (12) are largely unmodelled. |

One measurement worth carrying forward: 19 of 24 `Operation` variants are stubs that
now report rather than claim success. In the engine (`fepdf-model`, `fepdf-syntax`)
the `log::warn!` count is down from 14 to one, and that one is deliberate: it reports
which fonts *this machine* has, not anything the document says. Frontends still log
freely, which is their job.

`./scripts/dev/status.sh` re-derives these figures, so a number that has gone stale
shows up as a disagreement rather than reading as current.

---

## Phase A — Own the reader *(complete)*

Replacing `lopdf` was the gate on everything else: what the engine could read was
otherwise bounded by another project's coverage, and the robustness it was kept for
was measured absent ([ADR-0003](docs/adr/0003-lopdf-was-not-providing-robustness.md)).

- [x] Byte layer: header scanning, cross-reference tables, `startxref`, recovery scan
- [x] Cross-reference streams, `/Prev` chains, hybrid references
- [x] Indirect objects from offsets, with `/Length` repair recorded as a `Decision`
- [x] Object stream expansion
- [x] Document assembly — an object's handle **is** its object number, so the
      remapping table is gone; decryption runs on the arena (`decrypt.rs`)
- [x] `Document::open` switched, with every sample compared before and after
- [x] `lopdf` deleted: 95 references, the dependency, and the credits entries
- [x] `log::warn!` sites converted to `Decision`s

### What the switch actually changed

Round-tripping all nine samples through `publish upgrade` on both paths, compared by
`examples/compare_documents.rs` — which walks the catalogue, numbers objects by the
order they are reached, and sorts dictionary keys, so neither renumbering nor key
order can masquerade as a difference:

| Sample | Reachable objects | Differing |
| :--- | ---: | :--- |
| `bokutokitan`, `constitution`, `fugaku`, `sample`, `print_sample`, `volvo_xc90` | 80–26,847 | 1 each |
| `intel_sdm` | 332,814 | 1 |
| `fy05` | 4,586 | 2 |
| `unicode_16` | 8,280 | 179 |

Every "1" is the XMP packet, whose `xmpMM:InstanceID` is a fresh UUID per instance.
**Byte-identical was not an achievable criterion**: the old path was not byte-stable
against itself either, differing in exactly those 31 bytes between two runs of the
same binary. The remaining 178 differences in `unicode_16` and one in `fy05` are real
numbers: `lopdf` parsed them as `f32`, so `302.498454` came back as `302.498444`.

On the six deliberately malformed files, `publish upgrade` now succeeds on five where
it previously succeeded on one. The sixth is truncated before its trailer and has no
`/Type /Catalog` anywhere; it now fails with a message that says so rather than
`Object Handle<Object>(0) is not a dictionary`.

One defect was found by cross-checking against an independent reader rather than by
any of the above — see
[ADR-0006](docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md).

## Phase B — Read before write

Semantic completeness starts with being able to *see* a feature. `inspect` began with
four commands — `info`, `audit`, `text`, `tree` — against roughly fifteen clauses, and
nothing reported encryption, interactive features, or file structure.

- [x] `inspect structure` — file layout: sections, updates, object streams, and the
      decisions taken while reading. Text, JSON and Markdown; reads the bytes rather
      than a normalised `Document`, so it reports the file as written
- [ ] `inspect encryption` — handler, revision, permissions, conformance
- [ ] `inspect interactive` — annotations, form fields, actions, outlines
- [ ] `inspect catalog` — every entry, typed or not, so gaps are visible
- [ ] Surface `DecisionLog` in every output format, not only `audit`

*Done when*: for any PDF 2.0 feature the engine claims to support, there is a command
that shows it. Reading a feature is the precondition for writing it correctly.

### What surveying the corpus first turned up

`examples/structure_survey.rs` was written before the command, because a column whose
value is the same for every file is a column not worth printing. It found the opposite
problem — a column that was wrong.

The reader recorded an `Ambiguity` for every indirect `/Length`, a form the standard
permits, so `sample.pdf` reported 31 departures and `DecisionLog::is_conforming` was
`false` for a conforming file. Fixing it exposed two further tolerances the noise had
hidden: a header at a non-zero offset and a missing trailer dictionary were both
accepted in silence ([ADR-0008](docs/adr/0008-an-indirect-length-is-not-an-ambiguity.md)).

| Corpus | Decisions recorded, before → after |
| :--- | :--- |
| nine samples | 31, 31, 0×7 → **0 each** |
| five readable malformed files | 31, 31, 31, 22, 0 → **1–3 each, naming the damage** |

One gap is left deliberately: an indirect `/Length` pointing at the *wrong* object is
still read silently, because the reader never resolves the reference to compare. The
correct extent is found by scanning either way, so nothing is misread — but the file's
non-conformance goes unreported. `examples/length_crosscheck.rs` detects it from
outside until the reader can.

## Phase C — Clause 7.6

Independent of A and B, and the area where a partial implementation is most harmful.

- [ ] AES-256 R5/R6 to Algorithms 2.A, 3.A, 8 and 9 — the current key derivation is
      documented in-source as not conforming
- [ ] Owner-password validation and permission enforcement
- [ ] Public-key encryption (7.6.4)
- [ ] Unencrypted wrapper documents (7.6.7)
- [ ] A corpus of encrypted files as regression tests

*Done when*: an AES-256 document written by Acrobat round-trips, and one written by
fepdf opens in Acrobat.

## Phase D — The catalogue and PDF 2.0 features

Only now do the 19 stub operations become worth implementing, because reading exists
to verify them against.

- [ ] Type the remaining catalogue entries, `DSS`/`AF`/`DPartRoot` first — they are
      the 2.0 additions
- [ ] Implement operations in order of how much of the standard they unlock:
      catalogue edits (`UpdateOutlines`, `SetOutputIntent`, `UpdateLayers`,
      `SetPageLabels`) before page elements (`AddAnnotation`, `SetFormFieldValue`)
      before content synthesis (`ApplyBatesNumbering`, `AddPageDecoration`)
- [ ] Un-hide each CLI subcommand as its operation lands
- [ ] Decide the fate of the operations no frontend reaches; an unreachable operation
      is a maintenance cost without a user

*Done when*: `Operation` has no stubs, and `fepdf edit --help` lists only working
commands because they all work.

## Phase E — Structure, once the contents exist

Deferred deliberately. Splitting `fepdf-doc` out today would produce a crate that owns
the operation vocabulary while 79% of it is hollow — the shape of the mistake in
[ADR-0001](docs/adr/0001-resource-resolution-stays-in-the-model.md).

- [ ] `fepdf-content`: move the interpreter beside the contract it already drives.
      Independent of the stub problem, so it can happen at any point
- [ ] `fepdf-doc`: after Phase D
- [ ] `fepdf` as its own crate — currently a rename, since Rule A is already enforced
      by Cargo ([ADR-0005](docs/adr/0005-layering-rules-are-enforced-by-cargo.md))

## Not planned

- **A DOCX converter.** The `DocumentSource` boundary exists so one has a place to go
  (`ARCHITECTURE.md` §5.2), but writing it means a layout engine — style resolution,
  line breaking, pagination — which shares almost nothing with reading PDF.
- **`fepdf-wasm` as a peer frontend.** Forty lines with an unimplemented renderer.
  Whether to build it is a product decision, not an architectural one.
- **Writing PDF 1.7.** Output is 2.0; earlier versions are read-only targets.

---

## How this roadmap differs from its predecessor

The previous version marked Phases 1–27 complete against a goal of "the world's most
robust and ISO-compliant PDF 2.0 toolkit". Several of those completions did not
survive measurement: `open_repair` returned without repairing, `ColorPolicy` was never
read, and five `fepdf edit` subcommands reported success while writing nothing.

`ColorPolicy` is still not read, and a second ingestion option turned out to share the
condition; both flags are now hidden rather than advertised
([ADR-0007](docs/adr/0007-an-option-that-is-not-read-is-hidden.md)). Naming a defect is
not fixing it — `./scripts/dev/status.sh` now counts them, so the gap is measured
rather than remembered.

Each phase here therefore states what *done* means in terms that can be measured, and
the current state above is what the code does today rather than what it was intended
to do.

*Updated 2026-08-15, from measurements taken against the sample corpus and a set of
deliberately malformed files.*
