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
| 0048 | [Choice fields (`/FT /Ch`) are read and updated with appearance regeneration](0048-reading-and-setting-choice-fields.md) |  |
| 0049 | [Sorting by `y` required a `y`, and the extraction backend was not tracking the CTM](0049-the-extraction-backend-was-not-tracking-the-ctm.md) | Completes 0047 |
| 0050 | [Ruby is bound to the base it reads](0050-ruby-is-bound-to-the-base-it-reads.md) | Completes 0047 |
| 0051 | [Almost nothing declares a binding direction, and a vertical book that is silent opens the wrong way round](0051-a-binding-direction-nothing-declares.md) | Applies 0041 |
| 0052 | [The scene budget is counted before the scene is submitted, because appending cannot be undone](0052-the-scene-budget-is-counted-before-the-scene-is-submitted.md) | Bounds 0043 |
| 0053 | [The viewer has two modes and a zoom ladder, because a multiplier could not reach 100%](0053-the-viewer-has-two-modes-and-they-are-now-named.md) | Bounds 0052 |
| 0054 | [Below the overview step a page is a thumbnail, which is what makes the budget stop mattering](0054-below-the-overview-step-a-page-is-a-thumbnail.md) | Resolves 0052 |
| 0055 | [The tile arrangement does not depend on the zoom, so zooming is a change of distance](0055-the-tile-arrangement-does-not-depend-on-the-zoom.md) | Refines 0053 |
| 0056 | [A clone that was never finished, because the step that completes it was a separate method](0056-a-clone-that-was-never-finished.md) | — |
| 0057 | [The released binaries could not read Japanese, because the resources they look for were never built](0057-the-released-binaries-could-not-read-japanese.md) | Ships 0041, 0044 |
| 0058 | [A user space unit need not be a seventy-second of an inch](0058-a-user-space-unit-need-not-be-a-seventy-second-of-an-inch.md) | — |
| 0059 | [What holds fy05.pdf below its old reading is a running head in the margin](0059-what-holds-fy05-below-its-old-reading.md) | Measures 0050 |
| 0060 | [A reference chain is bounded by what it has already seen, not by a number](0060-a-reference-chain-is-bounded-by-what-it-has-seen.md) | — |
| 0061 | [Four more walks are bounded, and two of the six were not what the sweep said](0061-four-walks-bounded-and-two-that-were-not-what-the-sweep-said.md) | Closes 0060 |
| 0062 | [A page tree that is not a tree is reported, not expanded](0062-a-page-tree-that-is-not-a-tree-is-reported-not-expanded.md) | Found by 0061 |
| 0063 | [One set of accessors, because two of them disagreed about one dictionary](0063-one-set-of-accessors-because-two-disagreed-about-one-dictionary.md) | — |
| 0064 | [Redaction removed the second run of a page and no other](0064-redaction-removed-the-second-run-of-a-page-and-no-other.md) | — |
| 0065 | [`gs` reaches Table 57's line parameters, and PDFKit says which output was right](0065-gs-reaches-table-57s-line-parameters.md) | — |
| 0066 | [Rule 6 gets a check, and the check finds a sixth walk on its first run](0066-rule-6-gets-a-check-and-the-check-finds-a-sixth-walk.md) | Closes 0061 |
| 0067 | [A substitute face is declared, not guessed](0067-a-substitute-face-is-declared-not-guessed.md) | Extends 0041 |
| 0068 | [A suite that skipped itself in silence and asserted nothing when it ran](0068-a-suite-that-skipped-itself-and-asserted-nothing.md) | Covers 0064 |
| 0069 | [Two things the file said that nothing read](0069-two-things-the-file-said-that-nothing-read.md) | With 0065 |
| 0070 | [A command that located the structure tree instead of printing it](0070-a-command-that-located-the-tree-instead-of-printing-it.md) | — |
| 0071 | [What the unreferenced items turned out to be](0071-three-declarations-that-read-nothing-and-one-that-wrote-nothing.md) | Closes 0014, applies 0017 |
| 0072 | [A page selection nobody could parse meant every page](0072-a-page-selection-nobody-could-parse-meant-every-page.md) | With 0071 |
| 0073 | [Rule 13's other half, and a type that was a check all along](0073-rule-13s-other-half-and-three-declarations-that-were-checks.md) | Closes 0071's open items |
| 0074 | [The reader copied the rest of the file once per object](0074-the-reader-copied-the-file-once-per-object.md) | Closes 0071's last item |
| 0075 | [Two costs a caller never asked for](0075-two-costs-a-caller-never-asked-for.md) | Answers 0074, closes 0061's `Drop` |
| 0076 | [What the arena compresses, and what that was costing](0076-what-the-arena-compresses-and-what-that-was-costing.md) | Continues 0075 |
| 0077 | [A command was seventy-two bytes because of one rare variant](0077-a-command-was-seventy-two-bytes-because-of-one-rare-variant.md) | Continues 0076 |
| 0078 | [What sweeping 752 tests found, and what the sweeps got wrong](0078-what-a-test-suite-sweep-found-and-what-it-did-not.md) | Extends 0068 |
| 0079 | [The pre-parsed command form stays eager](0079-the-pre-parsed-command-form-stays-eager.md) | Closes 0077's open question |
| 0080 | [A design document does not carry a number that moves](0080-a-design-document-does-not-carry-a-number-that-moves.md) | — |
| 0081 | [The writing rules had nothing behind them](0081-the-writing-rules-had-nothing-behind-them.md) | Enforces 0039 |
| 0082 | [The script crate is a library the frontends call](0082-the-script-crate-is-a-library-the-frontends-call.md) | Amends 0025, after 0032 |
| 0083 | [A fixture crate that depends on nothing](0083-a-fixture-crate-that-depends-on-nothing.md) | Thirty-two hand-written assemblers |
| 0084 | [The GUI gets rules, not a rulebook](0084-the-gui-gets-rules-not-a-rulebook.md) | Third attempt after 0038 |
| 0085 | [Editing what a page draws is in scope](0085-editing-what-a-page-draws-is-in-scope.md) | Rests on 0079 |
| 0086 | [The engine does not read a scan; it binds what does](0086-the-engine-does-not-read-a-scan-it-binds-what-does.md) | After Phase M |
| 0087 | [A form field is created here, not only filled](0087-a-form-field-is-created-here-not-only-filled.md) | Extends 0048 |
| 0088 | [What a crop puts outside the sheet is removed](0088-what-a-crop-puts-outside-the-sheet-is-removed.md) | Continues 0064 |
| 0089 | [A face is embedded only where it permits it, and nothing is substituted](0089-a-face-is-embedded-only-where-it-permits-it.md) | Settles 0085's open question |
| 0090 | [The face a document embeds is not a licence to set new text in it](0090-the-face-a-document-embeds-is-not-a-licence-to-set-new-text.md) | Amends 0089, from 9.9.1 |
| 0091 | [Paragraphs are not inferred; the reader marks the range, and overflow shows](0091-paragraphs-are-not-inferred-and-overflow-is-shown.md) | Extends 0085 |
| 0092 | [The Matterhorn Protocol measures PDF/UA-1, and this engine declares PDF/UA-2](0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md) | |
| 0093 | [The protocol's tables enumerate 137 failure conditions, and its prose says 136](0093-the-protocols-tables-enumerate-137-failure-conditions.md) | Corrects 0092 |
| 0094 | [The auditor reads the ingested document, so ingestion answers checkpoint 06](0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md) | Bounds 0092, rests on 0013 |
