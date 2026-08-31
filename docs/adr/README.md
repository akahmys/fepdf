# Architecture Decision Records

One file per decision that was **contested, reversed, or rests on a measurement**.
Not every choice: a decision whose alternative is obviously worse does not need a
record, and a log padded with those stops being read.

## When to write one

- A decision was made, then **measurement contradicted it**. Record both, so the
  reasoning that led there is visible and not repeated.
- Two defensible options exist and one was chosen. Record why, so the question is
  settled rather than relitigated.
- A constraint is being accepted deliberately — a dependency, a tolerance, a gap.

Ordinary implementation choices belong in code comments and commit messages.

## Format

```
# ADR-NNNN: <the decision, as a statement>

- **Status**: Accepted | Amended by ADR-NNNN | Superseded by ADR-NNNN
- **Date**: YYYY-MM-DD
- **Commit**: <sha of the change that implemented it>

## Context
What was true, and what question had to be answered.

## Decision
What was decided.

## Consequences
What follows, including what is now harder.
```

Keep each under a page. If it needs more, the design belongs in `ARCHITECTURE.md`
and the ADR should point at it.

## Relationship to other documents

`ARCHITECTURE.md` describes the architecture **as it is now**. These records describe
**how it came to be**, including paths not taken. When the two disagree,
`ARCHITECTURE.md` is authoritative for the present and the ADR is authoritative for
the history.

Note that `Decision` in `fepdf-model` is a different thing entirely: it records what
the *engine* decided about a non-conforming input file at run time
(`ARCHITECTURE.md` §4.3).

## A note on the first five

ADR-0001 through ADR-0005 were written **retroactively**, reconstructed from the
commits that implemented them. They were not written at the time the decisions were
taken — which is the reason this log exists, since in four of the five the original
reasoning had to be recovered from a diff rather than read.

ADR-0006 is the first written as the decision was taken, with the measurement that
forced it still to hand.

## The records

Generated from the files themselves; `./scripts/dev/status.sh` fails if this table and
`docs/adr/*.md` disagree, because an index maintained by hand is an index that goes
quietly wrong — which is the failure this log exists to make visible.

| | Decision | Relation |
| ---: | :--- | :--- |
| 0001 | [Resource resolution stays in the model](0001-resource-resolution-stays-in-the-model.md) |  |
| 0002 | [The syntax layer is the lexer and the cryptography, nothing more](0002-the-syntax-layer-is-lexer-and-crypto-only.md) |  |
| 0003 | [lopdf is not what makes malformed files readable](0003-lopdf-was-not-providing-robustness.md) |  |
| 0004 | [Rule B makes the GPU dependency explicit, not absent](0004-rule-b-makes-the-gpu-dependency-optional.md) |  |
| 0005 | [The layering rules are enforced by Cargo, not by review](0005-layering-rules-are-enforced-by-cargo.md) |  |
| 0006 | [An object stream may not overwrite a newer revision of what it carries](0006-a-container-may-not-overwrite-a-newer-revision.md) |  |
| 0007 | [An option nothing reads is hidden, not removed](0007-an-option-that-is-not-read-is-hidden.md) |  |
| 0008 | [An indirect `/Length` is conforming, so reading one records nothing](0008-an-indirect-length-is-not-an-ambiguity.md) |  |
| 0009 | [`/P` is thirty-two bits, and reading it as a positive integer destroyed the content](0009-permissions-are-thirty-two-bits-not-a-positive-integer.md) |  |
| 0010 | [A `/ToUnicode` synthesised from glyph ids destroys text](0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md) |  |
| 0011 | [The content round trip must be a fixed point](0011-the-content-round-trip-must-be-a-fixed-point.md) |  |
| 0012 | [Saving produces a new document, not an edited one](0012-saving-produces-a-new-document.md) |  |
| 0013 | [A document is one normalised state, settled at load](0013-a-document-is-one-normalised-state.md) |  |
| 0014 | [The faithful-copy path is not built, and signing is limited to output this engine wrote](0014-the-faithful-copy-path-is-not-built.md) |  |
| 0015 | [This engine reads five encryption schemes and writes one](0015-this-engine-reads-five-encryption-schemes-and-writes-one.md) |  |
| 0016 | [Objects are packed into object streams by default](0016-objects-are-packed-by-default.md) |  |
| 0017 | [Declaring a catalogue key is not modelling it](0017-declaring-a-catalogue-key-is-not-modelling-it.md) |  |
| 0018 | [Interpreting a page can add to the decision log](0018-interpreting-a-page-can-add-to-the-decision-log.md) |  |
| 0019 | [Semantic understanding is measured against what a corpus presents](0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md) |  |
| 0020 | [A modelled entry reports how much of its own table it reads](0020-a-modelled-entry-reports-how-much-of-its-own-table-it-reads.md) |  |
| 0021 | [Optional content hides only what the document unambiguously turns off](0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md) |  |
| 0022 | ["What can this document do" is a settled question where "reads an action" is not](0022-what-a-document-does-is-a-settled-question-where-reads-an-action-is-not.md) | Amended by 0026 |
| 0023 | [A renderer that skips annotation appearances is not conforming](0023-a-renderer-that-skips-annotation-appearances-is-not-conforming.md) |  |
| 0024 | [Pure Rust is a rule, and therefore has a check](0024-pure-rust-is-a-rule-and-therefore-has-a-check.md) |  |
| 0025 | [A script processor is a frontend, not a subsystem](0025-a-script-processor-is-a-frontend-not-a-subsystem.md) | Amended by 0031 |
| 0026 | [The ECMAScript subset is taken, because work already undertaken depends on it](0026-the-engine-takes-the-ecmascript-subset-because-it-already-owes-it.md) |  |
| 0027 | [The shading function is sampled, and where PDFKit is wrong the check pins rather than yields](0027-a-function-evaluator-and-two-divergences-it-pinned.md) |  |
| 0028 | [Four of the thirteen logs were deleted rather than recorded, because they fired on conforming files](0028-four-of-the-thirteen-logs-were-not-decisions.md) |  |
| 0029 | [Halftones and transfer functions are declined on their own clauses, not on the corpus](0029-halftones-and-transfer-functions-are-declined-on-their-clauses.md) |  |
| 0030 | [Mesh shadings are flattened into triangles, and each one is grown by half a pixel](0030-a-mesh-is-flattened-and-its-triangles-are-grown.md) |  |
| 0031 | [A script frontend cannot be a facade feature, and holds no `&mut Document`](0031-a-script-frontend-cannot-be-a-facade-feature.md) | Amends 0025 |
| 0032 | [Running a document's scripts is a frontend verb, not an `Operation`](0032-running-scripts-is-a-frontend-verb-not-an-operation.md) | Amends 0025 |
| 0033 | [The Linux GUI keeps Wayland, so Rule 9 names one exemption](0033-the-linux-gui-keeps-wayland-so-rule-9-names-one-exemption.md) |  |
| 0034 | [The locale is recorded rather than ignored, and `intl` is declined for what it does not do](0034-intl-is-declined-for-what-it-does-not-do.md) |  |
| 0035 | [What a page shows and what it says are separate questions](0035-what-a-page-shows-and-what-it-says-are-separate-questions.md) |  |
| 0036 | [A base encoding is not a CMap, and a solidus is not a glyph name](0036-a-base-encoding-is-not-a-cmap.md) |  |
| 0037 | [A rules document holds rules, and the log holds how they were got wrong](0037-a-rules-document-holds-rules-and-its-log-holds-the-rest.md) |  |
| 0038 | [One hierarchy of truth, and the parallel rulebook that outlived it](0038-one-hierarchy-of-truth-and-the-parallel-rulebook-is-deleted.md) |  |
| 0039 | [The design document was narrating its own corrections](0039-the-design-document-was-narrating-its-own-corrections.md) |  |
| 0040 | [A rule the compiler already keeps does not need a grep, and Rule 17 did not need to exist](0040-a-rule-the-compiler-already-keeps-is-not-a-rule.md) |  |
| 0041 | [A CID font's character collection is declared, and the engine was guessing it from the font's name](0041-a-character-collection-is-declared-not-guessed.md) |  |
| 0042 | [A glyph name that looks like a character code is not one](0042-a-glyph-name-that-looks-like-a-character-code-is-not-one.md) |  |
| 0043 | [The scene repeats and the rasteriser does not](0043-the-scene-repeats-and-the-rasteriser-does-not.md) | Corrects 0041 |
| 0044 | [The other four character collections were already on disk](0044-the-other-four-collections-were-already-on-disk.md) | Completes 0041 |
| 0045 | [Normalisation-at-load does not reach fonts](0045-normalisation-at-load-does-not-reach-fonts.md) | Qualifies 0013 |
| 0046 | [Font construction is unified at load time](0046-unify-font-construction-paths-at-load.md) | Completes 0045 |
| 0047 | [Text extraction reconstructs logical reading order](0047-text-extraction-sorts-runs-into-reading-order.md) |  |
