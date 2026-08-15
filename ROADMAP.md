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
| **7.5** File structure | **Implemented, not yet wired.** Header scan, both cross-reference forms, `/Prev` chains, hybrid references, object streams, and recovery by scanning. `Document::open` still calls `lopdf`. |
| **7.6** Encryption | **Weakest area.** AES-256 R5/R6 is self-declared non-conformant; public-key (7.6.4) and unencrypted wrapper (7.6.7) are stubs. |
| **7.7** Document structure | 10 of ~30 catalogue entries typed. Untyped entries survive a round trip but cannot be reasoned about. |
| **PDF 2.0 additions** | `DSS`, `AF`, `DPartRoot` have spec types but no read or write path. |
| **8–14** Content, text, interactive, tagged | Interpreter, fonts and UA-2 auditing exist; interactive features (12) are largely unmodelled. |

Two measurements worth carrying forward: 19 of 24 `Operation` variants are stubs that
now report rather than claim success, and 14 places still detect non-conformance with
`log::warn!` instead of recording a `Decision`.

---

## Phase A — Own the reader *(in progress)*

Replacing `lopdf` is the gate on everything else: what the engine can read is
otherwise bounded by another project's coverage, and the robustness it was kept for
was measured absent ([ADR-0003](docs/adr/0003-lopdf-was-not-providing-robustness.md)).

- [x] Byte layer: header scanning, cross-reference tables, `startxref`, recovery scan
- [x] Cross-reference streams, `/Prev` chains, hybrid references
- [x] Indirect objects from offsets, with `/Length` repair recorded as a `Decision`
- [x] Object stream expansion
- [ ] **Assemble a document from those pieces** — object numbering into the arena,
      trailer resolution, decryption on the new path
- [ ] **Switch `Document::open` over**, comparing every sample before and after so the
      change is verifiable rather than asserted
- [ ] **Delete `lopdf`**, which removes ~90 references of conversion code
- [ ] Convert the 14 `log::warn!` sites to `Decision`s while their code paths are open

*Done when*: the six malformed files and all nine samples load through the new reader,
byte-identical output where the old path succeeded.

## Phase B — Read before write

Semantic completeness starts with being able to *see* a feature. `inspect` has five
commands against roughly fifteen clauses; nothing reports encryption, interactive
features, or file structure.

- [ ] `inspect structure` — file layout: sections, updates, object streams, and the
      decisions taken while reading
- [ ] `inspect encryption` — handler, revision, permissions, conformance
- [ ] `inspect interactive` — annotations, form fields, actions, outlines
- [ ] `inspect catalog` — every entry, typed or not, so gaps are visible
- [ ] Surface `DecisionLog` in every output format, not only `audit`

*Done when*: for any PDF 2.0 feature the engine claims to support, there is a command
that shows it. Reading a feature is the precondition for writing it correctly.

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

Each phase here therefore states what *done* means in terms that can be measured, and
the current state above is what the code does today rather than what it was intended
to do.

*Updated 2026-08-15, from measurements taken against the sample corpus and a set of
deliberately malformed files.*
