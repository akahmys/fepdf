# fepdf Roadmap

> **Not a rule.** This is the work: what was measured, what was built, what is next.
> The rules are in [CODING.md](CODING.md); a claim here that contradicts the code is a
> defect in this file.

**Goal**: an engine that understands ISO 32000-2 (PDF 2.0) semantically — not merely
one that round-trips it. PDF 1.7 and earlier are **read-only** targets; output is
always 2.0.

That goal is not a predicate. Its nearest measurable form is **the share of the
constructs a corpus presents whose contents this engine reads**: 96% over the nine
samples and 88% over all 524 files, with the catalogue axis at 21 of 22
(`fepdf inspect coverage`,
[ADR-0019](docs/adr/0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md)).
It counts only what can be enumerated from a file. Colour spaces, shadings and functions
cannot be, and they are where Phase P found the engine drawing the wrong picture. Nor
does it say whether what was read was read correctly.

Round-trip fidelity holds: no catalogue key is lost through a save, and
`crosscheck_selfread.sh` makes that comparison rather than this sentence asserting it.
The differences are by design:
- `/Metadata` is added where a source had none.
- Objects are renumbered, since saving produces a new document
  ([ADR-0012](docs/adr/0012-saving-produces-a-new-document.md)).
- Inherited attributes are resolved onto the pages
  ([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)).

Phase Y is open (2026-09-28): the code the last cleanup never saw.
`./scripts/dev/status.sh` re-derives the figures this file leans on, so a stale one reads
as a disagreement rather than as current.

---

## The subsets this processor has chosen

**PDF 2.0 has no "conforming reader".** 6.3.2.1 replaces it with a rule: each PDF
processor chooses which subsets of PDF functionality to support, and complies with the
provisions of the ones it chose. "Full support" would mean choosing every subset. The
standard carries 5,891 `shall`s, and clause 2 makes 81 other documents requirements of
it, among them PRC, U3D, ECMAScript for PDF, XFA 3.3 and the CAdES/PAdES parts. So every
implementation chooses. This table is where this one chooses, in the standard's
vocabulary, so that the choice can be checked.

| Subset (6.3.1) | Chosen | What it commits this project to |
| :--- | :--- | :--- |
| **PDF reader** — interpret a document to display, print or extract data | **yes**, broadly | Reading 1.x as well as 2.0, including files that are wrong in recoverable ways (ADR-0003). This is the subset the coverage index measures. |
| **PDF processor providing rendering** (6.3.2.2) | **yes** | Two `shall`s: render the page contents, and the appearance stream of every annotation that has one unless its flags say otherwise; and respect optional content. Both are met, the second since [ADR-0023](docs/adr/0023-a-renderer-that-skips-annotation-appearances-is-not-conforming.md) (Phase P). |
| **PDF writer** (6.3.2.1) | **yes**, for 2.0 only | Output conforms, and nothing 2.0 deprecates is written: encryption is AES-256 R6 ([ADR-0015](docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)), and a field value builds its appearance rather than setting `/NeedAppearances`. It makes unencrypted wrapper documents (7.6.7), chosen 2026-09-30 ([ADR-0104](docs/adr/0104-the-writer-makes-unencrypted-wrapper-documents.md)). Writing 1.7, and amending a file this engine did not produce, are **not** chosen ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)). |
| **Interactive PDF processor** (6.3.2.3) | **yes** | `fepdf-gui` is one. The layer panel follows `/OCProperties`, and the reading decisions (`doc.decisions()`) reach the window. |
| **ECMAScript actions** (12.6.4.17) | **yes**, for document and field scripting | Taken 2026-08-22 ([ADR-0026](docs/adr/0026-the-engine-takes-the-ecmascript-subset-because-it-already-owes-it.md)). Scope: 12.6.4.17's execution, and the objects form scripts reach for (`app`, `this`, `Field`, `event`, `util`, `color`). The engine is boa ([ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)), run by a frontend over `Operation` ([ADR-0025](docs/adr/0025-a-script-processor-is-a-frontend-not-a-subsystem.md), [ADR-0032](docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md)). Phase R. |
| **Multimedia** (13.2–13.7) | **no** | 13.4 is deprecated in 2.0; PRC and U3D are two more standards. |
| **XFA** (Adobe XFA 3.3) | **no** | Deprecated in 2.0, and a second form model beside the one that works. |

**All chosen subsets are met.** `status.sh` counts the rows marked as chosen and not
met, and it reads 0.

**One constraint cuts across all of them**: nothing in this workspace compiles C (RR-15
Rule 9, [ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)). The
standard says nothing about it, but it decides *how* a subset can be taken. It is why
ECMAScript is boa rather than QuickJS.

**A declaration is one side of an exchange.** `/Requirements` (12.11) is how a document
says which subsets it needs, and `inspect actions` reports those this processor does not
satisfy.

**How a row gets decided.** A subset is required when the engine has already undertaken
work whose correctness depends on it. Demand is not the test, because a capability that
does not exist has no users. Multimedia and XFA stay refusals: nothing here depends on
them, and both are deprecated in 2.0.

## Where the engine actually stands

| ISO 32000-2 | State |
| :--- | :--- |
| **7.3** Objects | Complete. Every type in the clause. |
| **7.4** Filters | Nine of the ten. The tenth is `/Crypt`, which the security layer handles. [↓](#74-filters) |
| **7.5** File structure | Complete and in use, recovery by scanning included. [↓](#75-file-structure) |
| **7.6** Encryption | Every password handler the standard defines, and the public-key ones. [↓](#76-encryption) |
| **7.7** Document structure | 23 of Table 29's 32 entries modelled; 10 occur in no file of 524. [↓](#77-document-structure) |
| **PDF 2.0 additions** | Re-derived against all 524 files, 0 unreadable. [↓](#pdf-20-additions) |
| **8** Graphics | Optional content honoured, tint transforms and shading functions evaluated. All five shading types read, Type 1 as a sampled grid. [↓](#8-graphics) |
| **9** Text | Extraction loses **1,137 glyphs of 16,321,270**, and all of them are named. [↓](#9-text) |
| **10** Rendering | Both of 6.3.2.2's `shall`s met. `[/ICCBased …]` colours go through their profile (10.3); `/DeviceCMYK` without one follows 10.4.2.1. [↓](#10-rendering) |
| **11** Transparency | Blend modes, constant alpha and soft masks all reach the backend. [↓](#11-transparency) |
| **12** Interactive features | Read and written. 15 entries have no reader: eleven are clause 13, which is declined, and four are `/Redact` keys written on a `/Stamp`, which no table defines. [↓](#12-interactive-features) |
| **13** Multimedia | Declined: 13.4 is deprecated in 2.0. The corpus does carry it — `/3D` ten times, `/Movie` five, `/RichMedia` three — which changes the premise and not the refusal. |
| **14** Document interchange | Marked content and logical structure are read, audited and edited. [↓](#14-document-interchange) |
| **14.3** Metadata | Settled at load into one state; what the engine does not model is carried. [↓](#143-metadata) |

### 7.4 Filters

`FlateDecode`, `LZWDecode`, `ASCIIHexDecode`, `ASCII85Decode` and `RunLengthDecode`
decode, with Table 8's predictors on LZW as on Flate, and Table 6's abbreviations are
matched. `DCTDecode` reads JPEG. `CCITTFaxDecode`, `JBIG2Decode` and `JPXDecode` decode
through the pure-Rust `hayro` crates (Phase M). `Crypt` is the security layer's (7.6).
An image that will not decode is skipped rather than aborting the page, and the skip
names the share of the page it covers.

`inspect structure` takes a census of the filters a file's streams name. Across all 524
files: `/FlateDecode` 485, `/DCTDecode` 21, `/XXXDecode` 8, `/JPXDecode` 3,
`/LZWDecode` 3, `/CCITTFaxDecode` 2, `/ASCIIHexDecode` 1, and `/JBIG2Decode` none.
`ZstandardDecode` is not in ISO 32000-2 and was removed
([ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)).

### 7.5 File structure

Header scan, both cross-reference forms, `/Prev` chains, hybrid references, object
streams, incremental updates, and recovery by scanning. `Document::open` reads the file
itself; `lopdf` is gone ([ADR-0003](docs/adr/0003-lopdf-was-not-providing-robustness.md)).

Recovery has two halves:
- A scan finds objects written `N G obj`.
- Every object stream in the file is expanded too, because an object stored in one is
  not written that way. The expansion fills holes and never overrides a section that
  read ([ADR-0006](docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md)).

A section that fails to read is a `Decision` naming its offset and filter.

### 7.6 Encryption

**Reading:**
- RC4 (V1/V2), AES-128 (V4/R4) and AES-256 (V5/R5, V5/R6) decrypt, to Algorithms 1, 2,
  2.A, 2.B and 4–6.
- `/Perms` is checked, both password roles authenticate, and passwords are SASLprepped.
  All of it was checked against PDFKit on fourteen files
  ([ADR-0009](docs/adr/0009-permissions-are-thirty-two-bits-not-a-positive-integer.md)).
- `/P` is reported and never enforced.

**Writing** is AES-256 at revision 6 and nothing else, because 2.0 deprecates the rest
([ADR-0015](docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)).

**Also covered:**
- Public-key handlers (7.6.5) are read and written: `--encrypt-to` produces one.
- Unencrypted wrappers (7.6.7) are recognised and reported.
- Signatures (12.8) are written as `ETSI.CAdES.detached` over the engine's own output,
  and verified with their coverage reported apart from the verdict.

### 7.7 Document structure

Every one of Table 29's 32 entries is a field of `PdfCatalog`, and **23 are modelled**:
the field's type says what the entry holds. Measured against all 524 files:
- 10 of the 32 keys occur in no file at all, and are declined a reader
  (`catalog::ABSENT_FROM_BOTH_CORPORA`, held by a test from the other side).
- Of the 22 keys those files do carry, **21 are modelled**. The one that is not is
  `/Type`, which is a check and not a type.

`inspect catalog` also reports how much of each entry's *own* table its reader covers
([ADR-0020](docs/adr/0020-a-modelled-entry-reports-how-much-of-its-own-table-it-reads.md)),
because declaring a key is not modelling it
([ADR-0017](docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md)).

### PDF 2.0 additions

`CatalogReport::survey` over every file of both corpora, 0 unreadable:
- `/OutputIntents` 188, `/AF` 17, `/OCProperties` 5, `/PageLabels` 5, `/Threads` 1. Every
  one that a file carries reports `Modelled`.
- `/Collection`, `/DSS` and `/DPartRoot` occur in 0 of 524, and wait for a file.

### 8 Graphics

**Optional content (8.11)** is honoured while drawing:
- The default configuration's `/BaseState`, `/ON`, `/OFF`, `/Intent` and `/AS` are read.
- So are membership dictionaries, all four `/P` policies, and `/VE` expressions.
- It hides only what the document unambiguously turns off
  ([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md)).

**Colour and shading:**
- Patterns paint.
- `/Separation` and `/DeviceN` evaluate their tint transforms through the function
  evaluator (7.10), all four `/FunctionType`s
  ([ADR-0027](docs/adr/0027-a-function-evaluator-and-two-divergences-it-pinned.md)).
- Shading types 1 to 7 are read, the mesh types into triangles
  ([ADR-0030](docs/adr/0030-a-mesh-is-flattened-and-its-triangles-are-grown.md)), and
  Type 1 as a sampled grid.
- `/Default*` colour spaces and `/CalRGB` are read.

**The corpus is thin here.** `/FunctionType 0` occurs seven times in all 524 files,
`/Separation` twice, and `/FunctionType 2` and `3` not at all. The evidence is Phase P's
fixtures with PDFKit beside them.

### 9 Text

**Extraction loses 1,137 glyphs of 16,321,270**, and every one is accounted for:
- The loss was 85,982 of 12,305,585 before the causes below were found. `volvo_xc90.pdf`
  loses 0 of 718,262, and `intel_sdm.pdf` 48.
- The other **1,089 stay lost on purpose.** They are `/Differences` names `c033`–`c039`
  in `fy05.pdf`, which draw 圏点 and bracket pieces rather than ASCII
  ([ADR-0042](docs/adr/0042-a-glyph-name-that-looks-like-a-character-code-is-not-one.md)).

What it reads, and from where:
- **CID tables:** Adobe's `mapping-resources-pdf`, from one resource root
  (`fepdf_font::resources`).
- **Character collections:** each font's `/CIDSystemInfo`, where the file declares one.
  A name heuristic is used only where it declares `Identity` or nothing
  ([ADR-0041](docs/adr/0041-a-character-collection-is-declared-not-guessed.md)). All five
  Adobe collections are read
  ([ADR-0044](docs/adr/0044-the-other-four-collections-were-already-on-disk.md)).
- **Base encodings:** Annex D's, read as tables and not as CMaps
  ([ADR-0036](docs/adr/0036-a-base-encoding-is-not-a-cmap.md)).
- **`/ActualText`** (14.9.4).

How the text is assembled:
- Runs separated by more than a quarter em along a line are words.
- Runs are sorted into reading order
  ([ADR-0047](docs/adr/0047-text-extraction-sorts-runs-into-reading-order.md)), and ruby
  is bound to its base ([ADR-0050](docs/adr/0050-ruby-is-bound-to-the-base-it-reads.md)).

### 10 Rendering

6.3.2.2's two `shall`s are met: optional content is honoured, and annotation appearance
streams are drawn
([ADR-0023](docs/adr/0023-a-renderer-that-skips-annotation-appearances-is-not-conforming.md)).

**Colour, and what is declined:**
- `[/ICCBased …]` colours go through their profile.
- `/DeviceCMYK` without one follows 10.4.2.1.
- Halftones (10.6) and `/TR`/`/TR2` are declined on their own clauses
  ([ADR-0029](docs/adr/0029-halftones-and-transfer-functions-are-declined-on-their-clauses.md)).
- Text rendering modes are drawn as Table 106 says, so mode 3 paints nothing.

**The rasteriser:**
- Vello scan-converts, and a clip is pushed as a clip layer, not a blend.
- The scene is byte-identical on every run. The GPU rasteriser is not; the CPU one
  (`publish render --cpu`) is
  ([ADR-0043](docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md)).

### 11 Transparency

Blend modes (11.3.5), constant alpha, and luminosity soft masks reach the backend, and
a mark in a mode other than `Normal` is composited in a layer of that mode. All 160 of
`volvo_xc90.pdf`'s masks are the plain case. `/S /Alpha`, `/BC` and `/TR` are recorded
where they are asked for rather than approximated.

Transparency groups' isolation and knockout (11.6.6) are not read. They are recorded when
`/I` or `/K` asks for something.

### 12 Interactive features

**Reading:**
- `inspect interactive` reads Table 166 entire, and the markup entries of Table 172. It
  reads subtype-specific entries for every subtype a corpus writes more than once, and
  the form walk with 12.7.4.2's inheritance.
- Named destinations resolve through both of 12.3.2.3's forms.
- `inspect actions` reads what a document does and whether the reader has to touch
  anything first
  ([ADR-0022](docs/adr/0022-what-a-document-does-is-a-settled-question-where-reads-an-action-is-not.md)).

**Writing:**
- Twelve annotation kinds, each with the appearance it is drawn by.
- The nine field types (12.7.4); setting a value builds its appearance (12.7.4.3).
- Tab order (`/Tabs`) and calculation order (`/CO`).
- A form's calculation scripts are run by the script frontend (Phase R).

29 entries across annotation subtypes that occur once or twice have no reader.

### 14 Document interchange

**Reading:**
- Marked content (14.6) and logical structure (14.7) are read; 142 files of 524 carry
  both `/MarkInfo` and `/StructTreeRoot`.
- Associated files (14.13) occur in 17 files and page-piece dictionaries (14.5) in 1,
  both `Modelled`. `/SpiderInfo` (14.10) and `/Legal` (14.11) occur in 0 of 524.

**Auditing:** the Matterhorn Protocol auditor decides 116 of its 137 failure conditions
(2026-10-04), and leaves 13 to a person (W-21, Y-F16).

**Editing:**
- The structure tree is edited through the vocabulary: tags, text strings, attributes,
  `/Ref`, namespaces, associated files.
- Marking content as an artifact, and wrapping kids in a new element (W-22).

### 14.3 Metadata

**At load:**
- `/Info` and the metadata stream are settled into one state; disagreements are
  recorded, and the entries 14.3.3 deprecates move out of `/Info`
  ([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)).
- Text strings decode to 7.9.2.2.

**The XMP packet** is rebuilt from the fields the engine models, and everything else in
it is carried as written: a standard's identification, PDF Declarations, extension
schemas ([ADR-0099](docs/adr/0099-the-xmp-packet-carries-what-the-engine-does-not-write.md)).

**At save**, `--strip` removes every metadata stream.

---

## Read broadly, write 2.0

**The faithful-copy path and general PDF 1.7 output are out of scope**
([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)), and that settles
three capabilities at once:
- **PDF/A-3 output.** A-3 is PDF 1.7. An e-invoice is served where the recipient takes
  PDF/A-4f, and not where it requires A-3.
- **Signing a document this engine did not write.** The same question as the faithful
  copy.
- **Encryption other than AES-256 R6.** The same rule as not writing what 2.0 deprecates
  ([ADR-0015](docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)).

Reading is where this engine is meant to be broad. Four reading gaps were declined on
corpus counts alone, which is not a reason, so they became questions:

| | What | State |
| :--- | :--- | :--- |
| **P1** | `/Ch` choice fields | **Built** ([ADR-0048](docs/adr/0048-reading-and-setting-choice-fields.md)) |
| **P2** | `/AF` associated files (14.13) | **Built**, `/AFRelationship` included; 17 files carry them |
| **P3** | `/DSS` and `/Perms` — long-term validation data, and DocMDP | **Open.** Neither occurs in any of 524 files, so a use case would have to justify it |
| **P4** | What a document does when opened | **Built**, `inspect actions`; two files of 524 run code with no interaction ([ADR-0022](docs/adr/0022-what-a-document-does-is-a-settled-question-where-reads-an-action-is-not.md)) |

## Not planned

These are refusals that rest on the nature of the thing, not on how often a corpus
happened to contain it.

> **A corpus can justify building something. Only a use case can justify not building it.**

- **A DOCX converter.** The `DocumentSource` boundary exists so that one has a place to
  go (`ARCHITECTURE.md` §4.2). Writing it means a layout engine, which shares almost
  nothing with reading PDF. Editing a word in place does not reach one
  ([ADR-0085](docs/adr/0085-editing-what-a-page-draws-is-in-scope.md)).
- **`fepdf-wasm` as a peer frontend.** A product decision, not an architectural one.
- **Writing PDF 1.7, a faithful-copy path, and signing documents this engine did not
  produce** ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)). They
  cost PDF/A-3 output, as above. A tool that never rewrites the file is the right
  place for them.
- **Reading an entry no corpus carries and no use case names.** Ten keys of Table 29
  are declined in the code.
- **Multimedia** (`/Movie`, `/Sound`, `/Screen`, `/3D`) and **XFA**. Both are
  deprecated in 2.0, which is a reason a corpus cannot overturn.
- **An OCR engine.** A scan is read by an external engine, and this engine binds what
  comes back
  ([ADR-0086](docs/adr/0086-the-engine-does-not-read-a-scan-it-binds-what-does.md),
  W-O1).

---

## What was built

Each phase in a few lines, with the records behind it. The narrative each entry carried
— how a defect was found, the before-and-after tables, the mutations that proved a test
— is in git, and a decision it made is in `docs/adr/`.

### Phase A — Own the reader

`lopdf` replaced by a reader of this project's own:
- Header scan, both cross-reference forms, `/Prev` chains, object streams, recovery.
- A `/Length` repair is recorded as a `Decision`.
- An object's handle **is** its object number.

Every sample was compared before and after, and only the XMP instance ID and lopdf's
`f32` rounding differed. Five of the six malformed files open where one did
([ADR-0003](docs/adr/0003-lopdf-was-not-providing-robustness.md),
[ADR-0006](docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md)).

### Phase B — Read before write

`inspect` went from four commands to eight: `structure`, `catalog`, `interactive` and
`encryption` joined `info`, `audit`, `text` and `tree`. Each carries the decision log,
structured rather than stringified. An indirect `/Length` stopped counting as an
ambiguity, so a conforming file reads as one
([ADR-0008](docs/adr/0008-an-indirect-length-is-not-an-ambiguity.md)).

### Phase C — Clause 7.6

**Encryption, both ways:**
- Every password handler decrypts. `/P` is read as 32 bits
  ([ADR-0009](docs/adr/0009-permissions-are-thirty-two-bits-not-a-positive-integer.md)).
- Algorithms 2.A, 2.B, 6 and 7 are implemented, and passwords are SASLprepped.
- Public-key handlers are read and written, and unencrypted wrappers reported.
- Writing is AES-256 R6 only
  ([ADR-0015](docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)).

**Signatures:** signing is a two-pass write, `ETSI.CAdES.detached`, over the engine's
own output. Verification reports coverage.

**Saving:**
- Object streams are written, and packed by default
  ([ADR-0016](docs/adr/0016-objects-are-packed-by-default.md)).
- The content round trip is a fixed point
  ([ADR-0011](docs/adr/0011-the-content-round-trip-must-be-a-fixed-point.md)).
- A save produces a new document
  ([ADR-0012](docs/adr/0012-saving-produces-a-new-document.md)).
- Every `SaveArgs` option is implemented or deleted
  ([ADR-0007](docs/adr/0007-an-option-that-is-not-read-is-hidden.md)).

**Fixes:** `fy05.pdf`'s lost pages came from a synthesised `/ToUnicode`
([ADR-0010](docs/adr/0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md)).

**Cross-checks:** encrypted fixtures are built independently of the engine, and
`crosscheck_roundtrip.sh` compares a file's text in PDFKit before and after a save.

### Phase C′ — The hole in the cross-checks

`crosscheck_selfread.sh` reads every produced file back with this engine, across 21
combinations of packing, encryption and signing. It also asserts that every page of every
sample extracts, because a comparison cannot see a defect that is the same on both sides.

### Phase D — The catalogue and PDF 2.0 features

**The catalogue:**
- All 32 catalogue entries are typed.
- `ViewerPreferences` is read with every field optional.
- `Dests` is read in both of its forms, through the name tree (7.9.6). That found a
  link in `intel_sdm.pdf` that goes nowhere.

**Operations:**
- Implemented in order of how much of the standard each unlocks.
- CLI subcommands are unhidden as their operations land.
- `color_policy` is read.

### Phase E — Structure, once the contents exist

- `fepdf-content` holds the interpreter beside `RenderBackend`.
- `fepdf-doc` owns the `Operation` vocabulary.
- `fepdf` is the facade crate
  ([ADR-0005](docs/adr/0005-layering-rules-are-enforced-by-cargo.md)).

### Phase F — Robustness

- Structure-tree `/Pg` pruning on page deletion.
- Depth limits of 64 on `q`/`Q` and on marked content.
- Operation preconditions validated before anything changes.
- Fallback advance widths where `/Widths` is missing.

### Phase G — Measured against files this project did not choose

`fetch_external_corpus.sh` brought 242 files: pdf-differences, and Isartor. Against them:
- A CFF INDEX panic was fixed.
- The four plain byte filters were built.
- A lost cross-reference section now leaves a `Decision`, and a scan fills its holes.
- An `ExtGState` `/Font` is read.
- A failing image no longer takes the page's text with it.

`measure_external_corpus.sh` exits non-zero only on a panic.

### Phase H — A decision the interpreter takes is still a decision

`DecisionLog` is reachable from `&Document`, so interpreting a page can record
([ADR-0018](docs/adr/0018-interpreting-a-page-can-add-to-the-decision-log.md)).
`is_conforming` answers for what has been examined. `inspect structure` takes a census of
stream filters.

### Phase I — Give the goal a completion condition

`fepdf inspect coverage` measures three axes against what a corpus presents: catalogue
entries, annotation entries per subtype, and stream filters
([ADR-0019](docs/adr/0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md)).
A construct no file carries counts in neither direction.

### Phase J — Read the interactive features the corpus presents

`PdfAnnotation` reads Table 166 entire, `/AP` included. It reads subtype entries for
every subtype written more than once, and Table 172 for all markup subtypes. The form walk
reads `/V`, `/DA`, `/Ff`, `/T` and `/Kids`, with 12.7.4.2's inheritance.

### Phase K — The catalogue's contents, in the order the corpus asks

- Five entries were wired and nine gained readers, taking the carried keys to 19 of 20
  modelled.
- The keys no file carries are declined in the code.
- The figure is qualified by each entry's own table
  ([ADR-0017](docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md),
  [ADR-0020](docs/adr/0020-a-modelled-entry-reports-how-much-of-its-own-table-it-reads.md)).

### Phase L — The image codecs, declined in writing

A skipped image names the share of the page it covers, which is the determinant of the
CTM. On that measurement the codecs were declined. Phase M reopened them on a use case,
which is the rule under "Not planned". The same measurement found a page tree lost to a
swallowed error, which is now a 7.7.3.2 `Violation`.

### Phase M — Scanned documents

**Codecs:** `CCITTFaxDecode`, `JBIG2Decode` and `JPXDecode` decode through the `hayro`
crates, behind one `FilterContext` contract:
- JBIG2's polarity is inverted to PDF's.
- JPX takes its colour space from the codestream only where the dictionary is silent.

**Fixes:** `DCTDecode` no longer converts colour, the component count comes from
`/N`, and a 1-bit gray image no longer crashes the GPU upload.

**Evidence:** fixtures are encoded by other implementations, and
`crosscheck_image.sh` compares against PDFKit. Nine files agree within one part in 255,
five of them not written by this project.

### Phase N — What the engine gets wrong

- Optional content is honoured while drawing
  ([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md)).
- A page tree inside an object stream is recovered.
- Headless rendering of a small page works.
- The layers the engine writes mark their content.
- `/SMaskInData` is read.

### Phase O — The holes in the checking

- Phase O-1 doubled the corpus to 524 files.
- `/JBIG2Decode` still occurs in none. O-2 waits for a JBIG2 image this project did not
  make.
- A visual script that ran a non-existent target was deleted, and the rest of
  `docs/specs/` was audited.

### Phase P — What the rendering subset owes

**Rendering:**
- The function evaluator (7.10)
  ([ADR-0027](docs/adr/0027-a-function-evaluator-and-two-divergences-it-pinned.md)).
- 10.4.2.1's CMYK conversion, `/Default*` and `/CalRGB`.
- Standard-14 fallback fonts, and a 9.6 `Violation` for a run that paints no glyph.
- Mesh shadings.
- Annotation appearances
  ([ADR-0023](docs/adr/0023-a-renderer-that-skips-annotation-appearances-is-not-conforming.md)).
- Halftones declined on their clause
  ([ADR-0029](docs/adr/0029-halftones-and-transfer-functions-are-declined-on-their-clauses.md)).

**The window:** a layer panel.

**Logging:** thirteen stderr logs became `Decision`s
([ADR-0028](docs/adr/0028-four-of-the-thirteen-logs-were-not-decisions.md)).

### Phase Q — The rules the architecture asserts, and what checks them

- Rule D is enforced: the facade mutates only through `apply`.
- The engine/frontend log split is derived from the workspace.
- 45 unused dependency declarations were removed, and `status.sh` counts them.
- Frontends declare only `fepdf`.
- `debug extract-font` writes under `out/`.
- `fepdf-mcp` named all thirty operations of the time.
- `fepdf-wasm` builds for WebAssembly, and reports what it cannot draw.

### Phase R — Running the document's code

**The ECMAScript subset is met.** boa runs document and field scripts through a
frontend, and Rule 9's check reads four targets. It is built around three things:
- A script cannot reach an operation of its own
  ([ADR-0032](docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md)).
- Determinism is injected through `ScriptEnvironment`.
- `/CO` supplies the calculation order, and Adobe's `AF*` helpers are in `aform.js`.

**What is declined, and why:**
- Wayland stays, with Rule 9 naming one exemption
  ([ADR-0033](docs/adr/0033-the-linux-gui-keeps-wayland-so-rule-9-names-one-exemption.md)).
- `Intl` is absent
  ([ADR-0034](docs/adr/0034-intl-is-declined-for-what-it-does-not-do.md)).

### Phase S — What the text pass turned up in the renderer

- The scene repeats and the GPU rasteriser does not, so `--cpu` exists
  ([ADR-0043](docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md)).
- A stale visual baseline was refreshed.
- The four non-Japanese character collections are read
  ([ADR-0044](docs/adr/0044-the-other-four-collections-were-already-on-disk.md)).

### Phase T — What is left, sized

- Reading order
  ([ADR-0047](docs/adr/0047-text-extraction-sorts-runs-into-reading-order.md),
  [ADR-0049](docs/adr/0049-the-extraction-backend-was-not-tracking-the-ctm.md),
  [ADR-0050](docs/adr/0050-ruby-is-bound-to-the-base-it-reads.md)).
- One font construction path
  ([ADR-0045](docs/adr/0045-normalisation-at-load-does-not-reach-fonts.md),
  [ADR-0046](docs/adr/0046-unify-font-construction-paths-at-load.md)).
- The silent wildcard arms, judged one by one.
- The window shows the engine's decisions.
- ECMAScript wired to `fepdf-mcp`'s field writes.
- Korean and Chinese extraction tested end to end.

### Phase U — The work that stops at no crate boundary

**The gate:**
- Rule 1 sees `pub(crate) fn`.
- `silent_branches.py`'s count is a question, not a gate.
- `TESTING.md` stopped quoting a stale test count.
- `[profile.dev.package]` takes the suite from 45.9 s to 27.5 s.

**Shared code:**
- One fixture crate
  ([ADR-0083](docs/adr/0083-a-fixture-crate-that-depends-on-nothing.md)).
- One content-stream reader, where the interpreter's raw-byte path had been an
  incomplete second one.
- `fepdf-mcp` runs a form's calculation order on a field write, and not on every
  operation.
- `color::ColorSpace` was deleted.

### Phase V — The window, looked at

Screenshots of the running window found what no test could, and each was fixed:
- Icons that drew nothing.
- Selected widgets drawing transparent text.
- A save that emptied the window.
- A palette that governed half the colours.
- Sheets with no edge.
- A reading-order overlay that drew nothing.
- Two constants shown as readings.
- A grid drawn under the texture.

`--capture` plans made the window drivable. The GUI rules are rows of `CODING.md` §3
([ADR-0084](docs/adr/0084-the-gui-gets-rules-not-a-rulebook.md)).

### Phase W — What a shipping editor does that this one does not

`fepdf-gui` was compared with JUST PDF [編集Pro] on 2026-09-19. Four decisions came out
of it:
- **D-1**, editing what a page draws is in scope
  ([ADR-0085](docs/adr/0085-editing-what-a-page-draws-is-in-scope.md)).
- **D-2**, no OCR engine; bind one
  ([ADR-0086](docs/adr/0086-the-engine-does-not-read-a-scan-it-binds-what-does.md)).
- **D-3**, forms through to creating fields
  ([ADR-0087](docs/adr/0087-a-form-field-is-created-here-not-only-filled.md)).
- **D-4**, what a crop puts outside is removed
  ([ADR-0088](docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)).

The critical path was that this engine had never embedded a font. As of 2026-09-28 the
window builds 30 of the vocabulary's 54 operations, `fepdf-mcp` 52 and `fepdf-cli` 9.

**Wiring**
- [x] Seven operations the window could not ask for — headers and footers, permissions,
      certificates, stripping, page images, signature verification, page replacement —
      reach it, and the window's history records acts.
- [x] A page selection naming a page that is not there is refused by number
      (`pages_named`).

**Fonts (W-E1, W-E2)**
- [x] **W-E1a** — `OS/2.fsType` is read (`fepdf-font::embedding`).
- [x] **W-E1b** — TrueType subsetting with glyph ids kept.
- [x] **W-E1b1** — a collection is read from its first font.
- [x] **W-E1b2** — CFF subsetting, the Japanese path: 19,409,608 bytes to 105,011 for
      eleven glyphs.
- [x] **W-E1b3** — the document's own program is reachable
      ([ADR-0039](docs/adr/0039-the-design-document-was-narrating-its-own-corrections.md)).
- [x] **W-E1b4** — a document's fonts are counted once.
- [x] **W-E1c-i** — descriptor metrics, `hmtx` tail included.
- [x] **W-E1c-ii** — a Type 0 / `CIDFontType2` embedding with `/W` and `/ToUnicode`.
- [x] **W-E1c-iii** — `/W` agrees with the program both ways.
- [x] **W-E1c-iv** — a CFF face is embedded as a `CIDFontType0`.
- [x] **W-E1d** — the ladder: a permitting installed face, or refuse
      ([ADR-0089](docs/adr/0089-a-face-is-embedded-only-where-it-permits-it.md),
      [ADR-0090](docs/adr/0090-the-face-a-document-embeds-is-not-a-licence-to-set-new-text.md)).
- [x] **W-E2a** — a decoration's font is written indirect.
- [x] **W-E2b** — Bates, watermarks and headers embed their glyphs.
- [x] **W-E2c** — a direct font dictionary is lifted at load.
- [x] **W-E2d** — one face per operation, not per page.
- [x] **W-E2f** — 図面-0001 embeds, draws and extracts.
- [x] **W-E2g** — the regular face is taken from a collection.

**Annotations**
- [x] **W-8** — `AddAnnotation` writes appearances, `/QuadPoints` and a stamp's picture.
- [x] **W-14** — drawing them was already done.
- [x] **W-13** — twelve kinds and fourteen pens; blend modes composited.

**Editing what a page draws (W-E3, W-E4, W-E5, W-E6)**
- [x] **W-E3a** — `TextSpan.op_index` is `None` where it has no referent.
- [x] **W-E3b** — the sample corpus holds no duplicate (`sample_corpus_test.rs`).
- [x] **W-E3** — `EditRun`, one run's text
      ([ADR-0091](docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
- [x] **W-E3c** — the run tools are served by `fepdf-mcp`.
- [x] **W-E3d** — `find_on_page` finds a string across runs.
- [x] **W-E3e** — 9.4.4's spacing is one function, `advance_of`.
- [x] **W-E4a** — a run split by kerning is one run.
- [x] **W-E4b** — inserting and deleting inside a run is the four verbs together.
- [x] **W-E4c** — `SplitRun`.
- [x] **W-E4d** — `DeleteRun`.
- [x] **W-E4e** — `MergeRuns`.
- [x] **W-E4f-a** — a run's origin.
- [x] **W-E4f-b** — `'`, `"` and `TD` branches that nothing reached are gone.
- [x] **W-E4f-c** — a cut or join would have lost glyphs.
- [x] **W-E4f-d** — runs are cut in codes.
- [x] **W-E4f-e** — a literal string's `0x0D` is escaped.
- [x] **W-E4f** — `MoveRun`.
- [x] **W-E5** — `EditXObject`: move, scale, rotate, replace.
- [x] **W-E6-a** — a run's box.
- [x] **W-E6** — the run drawer.
- [x] **W-E6-b** — a run is moved by dragging.

**Forms**
- [x] **W-F1** — filling a form from the window.
- [x] **W-F2-a** — `AddFormField`, nine kinds.
- [x] Filling finds a field by qualified name and writes a text string.
- [x] **W-F2-b** — `SetTabOrder` and `SetCalculationOrder`.

**Page geometry (D-4, W-G1)**
- [x] **W-10** — `CropPages`.
- [x] **W-G1-a** — the text a crop puts outside is removed.
- [x] **W-G1-b** — images are cut to what remains.
- [x] **W-G1-c** — paths are cut to what remains.
- [x] **W-11** — `SplitPage`.
- [x] **W-12** — `CombinePages`, and a page's scene is clipped to its sheet.

**Beside the editor**
- [x] **W-O1** — `page_for_ocr` and `AddTextLayer` bind an external OCR engine; mode 3
      text paints nothing.
- [x] **W-15** — finding text over runs, by extraction's route
      ([ADR-0096](docs/adr/0096-a-run-reads-its-codes-by-the-route-extraction-reads-them.md)).
- [x] **W-16** — perimeter, area, and a viewport's measure (12.9).
- [x] **W-17** — printing, to the spooler.
- [x] **W-18** — comparing two documents, text and pixels.
- [x] **W-19a** — `reading`: structure order, language and lexicon.
- [x] **W-19b** — the platform's synthesiser, as a process.
- [x] **W-20** — the snapshot: **W-20a** a region rasterised, **W-20b** the gesture and
      clipboard, **W-20c** the notice.

**The gate and the toolchain**
- [x] **W-T1** — the gate's cost measured; nothing taken.
- [x] **W-T2** — the audit names the check that failed.
- [x] **W-T3** — `rust-toolchain.toml` is a pin, and `msrv_build.sh` builds the floor.
- [x] **W-T4** — Rust 1.98.1. Clippy 1.98's `chunks_exact_to_as_chunks` fired on five
      calls, and none of the survey's twelve compatibility notes did.

**Accessibility: the Matterhorn Protocol (W-21)**
- [x] **W-21e** — three condition numbers corrected against the protocol
      ([ADR-0092](docs/adr/0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md)).
- [x] **W-21a** — the report says how much is checked.
- [x] **W-21b** — eight conditions with no new machinery
      ([ADR-0094](docs/adr/0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md)).
- [x] **W-21c** — checkpoint 01 from the content stream
      ([ADR-0095](docs/adr/0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md)).
- [x] **W-21i** — role maps, tables, and conditions outside the tree: 32 of 137.
- [x] **W-21h** — the conditions that cite ISO 32000-1, once the document was here.
- [x] **W-21j** — annotations' structure elements, through the parent tree.
- [x] **W-21k** — fonts as the file wrote them.
- [x] **W-21l** — Adobe's Glyph List, whole.
- [x] **W-21m** — embedded files, and fonts drawn with.
- [x] **W-21n** — 7.18.1's exempt annotations.
- [x] **W-21o** — 9.6.6.4's TrueType lookup.
- [x] **W-21p** — `MacRomanEncoding`.
- [x] **W-21q** — `/CharSet` and `/CIDSet` against the program.
- [x] **W-21r** — which glyph a code selects.
- [x] **W-21s** — widths, dictionary against program.
- [x] **W-21t** — every code shown maps to Unicode.
- [x] **W-21u** — the language of the text, and the last `M`.
- [x] **W-21v** — seventeen `H` conditions a document answers itself.
- [x] **W-21w** — ten more.
- [x] **W-21x** — the last eight, and the thirteen left to a person.
- [x] **W-21g** — a condition checked and not broken is `Sound`.
- [x] **W-21f** — the report a reader reads.
- [x] **W-21d** — the 48 `H` conditions, in the protocol's words.
- [x] **W-21** — closed: 112 decided, 13 for a person, 10 answered by ingestion, 2 with
      no test
      ([ADR-0093](docs/adr/0093-the-protocols-tables-enumerate-137-failure-conditions.md),
      [ADR-0098](docs/adr/0098-an-h-condition-is-decided-only-where-the-document-answers-it.md)).

**Accessibility: Well-Tagged PDF (W-22)**
- [x] **W-22a** — an element's `/Lang`, `/ActualText` and `/E`.
- [x] **W-22b** — `SetStructAttribute`, any owner's attribute.
- [x] **W-22c** — `SetStructRefs`.
- [x] **W-22d** — `SetStructNamespace` and `MapStructType`.
- [x] **W-22e** — `AttachStructAssociatedFile`.
- [x] **W-22f** — `MarkArtifact`.
- [x] **W-22g** — `WrapStructElem`.
- [x] **W-22h** — XMP claims survive a save
      ([ADR-0099](docs/adr/0099-the-xmp-packet-carries-what-the-engine-does-not-write.md)).
- [x] **W-22i** — `Upgrade` identifies UA-2 and A-4 in XMP, and refuses X-6
      ([ADR-0100](docs/adr/0100-upgrade-identifies-a-standard-where-the-standard-says.md)).
- [x] **W-22j** — `DeclareConformance`.
- [x] **W-22** — closed; no WTPDF checker is claimed
      ([ADR-0101](docs/adr/0101-a-declaration-is-the-callers-statement-and-w-22-closes.md)).

### Phase X — The arena, looked at

`PdfArena` was read end to end, and its structure is sound. What was wrong was the
mechanisms meant to close its gaps, which were declared and not built.
- [x] **W-A1** — two silent wrong answers no longer reach the font census.
- [x] **W-A2** — `ObjectEntry.generation`, which described nothing, is gone.
- [x] **W-A3** — a handle names the arena it indexes, at no memory cost.
- [x] **W-A4** — clone-per-read is four call sites, not 268.
- [x] **W-A5** — the arena only grows, measured.

### Phase Y — The code the last cleanup never saw

The crate-by-crate cleanup ended at `c4331cf` (2026-09-10). Phases V, W and X added
54,355 lines after it, across 59 new source files and 81 changed ones, and none of it has
been read the way that cleanup read the rest. That cleanup found its defects by reading a
claim beside its implementation, and not by counting `pub`. A duplicate-block scan over
`src/` on 2026-09-28 found almost nothing, so this phase is structural and not
deduplication. The duplication that exists is semantic: a dictionary lookup is written
eight times, and the copies do not agree on whether to resolve.

**The order of what is left**, set by the owner 2026-10-03 against what fepdf is — a
translator from any PDF to ISO 32000-2, operations on what it translated, and frontends
for those operations, with reporting second (Y-F26). The entries below keep their IDs and
their places; this is the order they are taken in.

1. **The output conforms, and that is measured.** Y-F24 first: a save read back by a test
   that fails on a departure, since output conforming to ISO 32000-2 is what the engine
   is for and nothing measures it. Under it, Y-F23, Y-F15 and what is open of Y-F21; the
   save's metadata together, Y-F2, Y-F4, Y-F5 and Y-F6, and Y-F29 with them; Y-F25
   decided; and Y-F28, what the test found the saves keep from their sources.
2. **One way to save.** Y-F1, Y-F30, Y-F31, Y-F22 and what is open of Y-0b are one defect: a
   linearised save is a second writer path, which ignores the options and packs no object
   streams. Linearising becomes a stage of the save the options already drive.
3. **An operation is the only way a document changes, and it is whole.** Y-11 before
   Y-10, with the two redaction routes let through by name until Y-10 replaces them; Y-F12
   designed with Y-11, since the span the arena is unsealed is the span `apply` must
   undo on failure. Then Y-10, its four questions put to the owner when it starts. Then
   Y-F11 and Y-F19.
4. **The frontends.** Y-F3 and Y-F27.
5. **The reporting.** Y-F16, Y-F17 and Y-F8.
6. **The documents.** Y-F26 last, with `ARCHITECTURE.md` §3's "an order of magnitude",
   which `fepdf-model` at 32,989 lines against `fepdf-gui`'s 22,211 is not; and whether
   `fepdf-doc` splits, below.

Y-F9 and Y-F10 are held: each is about a platform this machine is not, and is taken when
it can be run there.

**The net**
- [x] **Y-0** — `scripts/test/golden_outputs.sh` compares, between `HEAD` and the working
      tree, over `samples/` and `target/external/`: the bytes of a save (plain,
      linearised, encrypted), `inspect fonts`, `text` and `coverage`, and
      `doc.decisions()`. *Done when* changing one value in the linearisation hint table
      fails it. It did: one page's object count in the hint table's Item 1
      moved fourteen `linearized.pdf` files and nothing else, and not the one-page
      sample, which has no page for it to move. A run is 367 s over 16 inputs.
- [x] **Y-0a** — its first run did not finish: linearising `samples/intel_sdm.pdf`
      (5,057 pages) followed each article bead's `/T`, `/N` and `/V` from every page, so
      each page reached every bead, 24.7 million objects in all. A page now stops at its
      own bead (`7af21a0`).
- [ ] **Y-0b** — the same file linearises to 129 MB against a 25.6 MB plain save: 71 MB
      of it is the hint stream's reserve, zero-filled. `calculate_worst_case_hint_size`
      sizes the page table as though every page named every shared object
      (`num_pages × total_shared`). *Done when* the reserve is sized from the counts the
      writer already has, and the linearised file is within a few percent of the plain
      one.
      **The reserve is sized from the counts, 2026-10-03:** each page's own shared
      references and the widths the table is written with. `intel_sdm.pdf` linearises to
      58 MB from 129 MB, its hint stream 146 KB of which 16 KB is padding;
      `unicode_16.pdf` to 11.7 MB from 17.0 MB. `linearized_hint_test.rs` holds it, and
      putting the old sizing back failed it. **The second half is not met, and the reason
      is not the reserve**: the rest is Y-F22.
      **After Y-F22, 2026-10-03: 31.8 MB against 25.6 MB.** What remains is parts 7 and 8
      written directly — 22,619 link annotations, 7,326 streams and the page objects
      beside them. F.3.1 lets all but the page objects be packed, with the page and
      shared-object hint tables naming the object stream instead of the object; the
      writer's tables name objects.

- [x] **Y-0c** — saves were not reproducible: the XMP `InstanceID` was salted with the
      clock's seconds. `SaveOptions::stamped_at` decides it now, and `fepdf-cli` reads
      `SOURCE_DATE_EPOCH` (`d2cef3e`).

**Found while building the net**, each to be taken on its own
- [x] **Y-F1** — `save_linearized` keeps its own copy of the metadata handling and
      ignores `password`, `strip`, `lang`, `copyright` and `obj_stm`, beside a comment
      saying it is consistent with `save_with_options`.
      Fixed 2026-10-03: `save_linearized` is `write_out` with linearising on, so the
      metadata, the 2.0 translation and every option are the save's. A password, which
      the linearised layout cannot carry, is refused instead of dropped; `obj_stm` is read
      and not yet acted on by the linearised writer, which is Y-F22.
      `linearize_options_test.rs` holds it, and passing the old subset of the options
      fails it.
- [x] **Y-F2** — `SaveOptions::creation_date` is read by nothing.
      Fixed 2026-10-03: a creation date the caller gives is written, in the packet and in
      `/Info`, and one that is no date is refused as `PdfError::Refused`.
- [x] **Y-F3** — `fepdf-cli` panics when a certificate named by `--encrypt-to` cannot be
      read (`args.rs`, `From<SaveArgs>`), where Rule 2 asks for an error.
      Fixed 2026-10-04: the conversion is `TryFrom`, and an unreadable certificate is an
      error naming the file. `args.rs`'s test holds it, and dropping the name failed it.
- [x] **Y-F4** — the XMP `DocumentID` is `md5` of the title alone, so every untitled
      document shares one, and two documents of one title collide.
      Fixed 2026-10-03: the save's document is a new one (ADR-0012), so its ID is drawn
      from the document it derives from, every metadata field, and the stamp. Two
      untitled documents with different `/ID`s get two IDs; one stamped alike gets one.
- [x] **Y-F5** — a document with no date is given `2026-05-26T06:00:00Z` as its
      `CreateDate`, `ModifyDate` and `MetadataDate`, with no `Decision`.
      Fixed 2026-10-03: a date nobody stated is not written; a creation date no longer
      stands in for a missing modification, nor the other way round.
- [x] **Y-F6** — `ModifyDate` and `MetadataDate` are copied from the source. Saving
      produces a new document (ADR-0012) and rewrites the packet, so both are the save's
      moment, which `stamped_at` now supplies.
      Fixed 2026-10-03: both are the stamp, in UTC so one stamp writes one date on any
      machine, and `/Info /ModDate` is the same moment in 7.9.4's form.
- [x] **Y-F8** — `cargo doc --workspace --no-deps` prints 44 warnings, most of them
      intra-doc links that resolve to nothing, and nothing gates it. `documents.py`
      checks relative Markdown links, which is the half AGENTS.md rule 1 names.
      Fixed 2026-10-04, at 52 rustdoc warnings — one Y-F16's own — and one of cargo's.
      Twenty-seven came from a module carrying `///` lines at its `mod` as well as its
      own `//!`, which rustdoc joins and resolves from the parent; in `fepdf-model` those lines had slid onto the
      wrong modules, "Annotations (12.5)" over `access`. They go where they broke a link.
      The rest are links renamed (`ContentFit` is `ContentScale`), private, or not links,
      and the CLI's binary no longer documents over the facade's `fepdf`.
      `verify_compliance.sh` step 17 runs `cargo doc` with warnings denied and fails on
      any warning, shown to fire by an unresolved link put back.
- [ ] **Y-F9** — reading aloud on Windows hands PowerShell the words on standard input
      and reads them with `[Console]::In`, whose encoding is the console's code page, not
      UTF-8; nothing sets it, so Japanese may arrive garbled (`speech.rs`). Unverified: no
      Windows here. Held until it can be run on Windows.
- [ ] **Y-F10** — stopping speech on Linux kills `spd-say`, and the speaking is done by
      the speech-dispatcher server; whether the passage under way stops is unverified,
      and `spd-say --cancel` is the call that says so (`speech.rs`). Held until it can be
      run on Linux.
- [x] **Y-F11** — writing the bookmark panel's draft replaces the whole tree with what
      `OutlineNode` carries: a title, a page, children. Every item's `/C`, `/F`, `/SE`
      and open state goes, which its module says; so does a non-`GoTo` `/A`, which it
      does not say — a bookmark to a web address is read as page 0 and written back
      pointing at page 1. One retitled bookmark rewrites the meaning of another.
      **Fixed 2026-10-04**: an `OutlineNode` read from a file names the item it was read
      from (`source`, an object number, as the structure operations name elements), and
      writing a tree back copies from that item what the node does not model — `/C`,
      `/F`, `/SE`, a closed item's negative `/Count`, and an action other than a go-to,
      which then stands instead of a destination. A node made new names none.
      `outline_tree_test.rs` holds it; dropping the source, `/SE`, the action or the
      closed count each failed it.
- [x] **Y-F12** — `PdfDocument::apply` states no atomicity, and the window's journal
      relies on it: a failed act is rebuilt from the history only when its second or
      later operation failed, so a first operation that changed the document and then
      failed would leave it differing from what undo replays. No such path was found in
      the form writer; the contract is what is missing.
      **Stated and held, 2026-10-03**: an operation that fails is put back whole, from a
      journal the arena keeps while `apply` has it unsealed
      ([ADR-0110](docs/adr/0110-an-operation-that-fails-changes-nothing.md)).
      `sealed_document_test.rs` holds it, and taking out the rollback failed it.
- [x] **Y-F13** — the window calls redaction 黒塗り (blacking out) and draws nothing
      black: `apply_physical_redaction_to_page` replaces every string of a show-text
      operator touching a rectangle with `[REDACTED]`, drawn in the page's own font,
      and leaves the page's images, vector graphics, annotations, form fields and form
      XObjects' text where they are. Either the drawing covers what it names, or the
      name says what it does. It also changes the open document outside `apply`, in
      the window's export and in `fepdf-mcp`, so no history holds it (Rule D).
      Taken as Y-10, and closed with it 2026-10-04.
- [x] **Y-F18** — **a page taken out of a document is still written into it.** Measured
      2026-09-30 on a two-page fixture whose first page links to the second and whose
      one bookmark names it: `RemovePages` of the second, then a save, leaves its
      content stream in the file — its text extracts from no page and decodes from the
      stream. `fepdf edit split --pages 1` does the same. The link's and the bookmark's
      `/Dest` still reference the page object, and the writer writes what is reachable.
      `prune_struct_tree_pages` clears an element's `/Pg` and nothing else: an `/MCR`'s
      `/Pg`, a destination, an annotation's `/P`, and the annotations and elements that
      belonged only to that page all keep it. A reader who removes a page to send the
      rest has sent it.
      Fixed by `page_removal.rs`, which both paths end in: what named the page goes
      with it — the user's choice over leaving the references empty — and what still
      points at it is made `null`.
- [x] **Y-F21** — **a page was two objects in a merged or extracted document, and a
      direct dictionary is read as an object it is not.** Measured 2026-10-02 on
      `extract_pages_test.rs`'s fixtures:
      - `extract_pages(vec![0, 1])` on a two-page document deleted the link from page 0
        to page 1. A page was cloned as a dictionary into a new object, and the link's
        destination through the cloner's map into another one no tree held, so
        `forget_absent_pages` (Y-F18) took it for a link to a page left out. Fixed: the
        pages are cloned by handle.
      - `merge` produced no outline at all, and with that fixed, the second source's
        bookmark went to page 0. `/Outlines` was written direct, which Table 29 forbids,
        and the destination named the second copy of the page. Fixed: the root is an
        object of its own, and the page is the one copy.
      - Open: `struct_tree::resolve_to_node_handle` answers a direct dictionary with
        `Handle::new(dh.index())` — a number from the `dicts` pool used as one from the
        `objects` pool, the shape `font_census_test.rs` records for `object_id`. A file
        whose `/Outlines` is written direct reads as having no bookmarks, and which
        object it reads instead is whatever shares the index.
      - Fixed 2026-10-03: loading gives an outline dictionary or a structure element
        written in place an object of its own (`ingest::indirect`), the outline's with a
        `Decision`, since Tables 29 and 151 say an indirect reference and Table 355 does
        not; `resolve_to_node_handle` answers a reference or nothing. A direct
        `/Outlines`, direct items, and a direct element in `/K` read; taking the lifting
        out failed each.
- [x] **Y-F32** — **a save broke every inline image.** The parser kept an inline
      image's width, height, a placeholder format and the encoded bytes after `ID`, and the
      serializer wrote them out as `/CS /RGB /BPC 8` with no filter: measured 2026-10-04,
      an 8 by 2 one-bit gray image saved as RGB with two bytes of its 48. Found reading
      for Y-10's inline images. Fixed: the command keeps the operator as the stream wrote
      it, `BI` through `EI`, and that is what is written. `inline_image_test.rs` holds a
      one-bit gray image and a filtered one, and writing the old way failed both.
- [x] **Y-F34** — **a save took the font out of a form that reads the page's.** A
      form with no resources of its own reads them from what draws it (7.8.3); the
      content parser read such a form alone, found its `/F1` defined nowhere, and kept
      `/Fallback-Sans` in its place, which every save wrote out. Found 2026-10-04
      redacting such a form. Fixed: the name the stream wrote is kept, and the
      interpreter draws in a fallback face only where no resource in reach defines it.
      `form_font_name_test.rs` holds it, and writing the stand-in again failed it.
- [ ] **Y-F33** — **an inline image is drawn as RGB whatever it is.** The interpreter
      hands the backend the bytes after `ID`, still encoded, as eight-bit RGB — the same
      placeholder format Y-F32 found — so a one-bit, gray, CMYK or filtered inline image
      draws as noise. Found with Y-F32; not yet measured on the corpus.
- [x] **Y-F22** — **a linearised file writes no object streams.** `intel_sdm.pdf`'s
      plain save packs 325,000 of its 332,818 objects into 3,277 compressed object
      streams; the linearised one writes every object directly, 58 MB against 25.6 MB
      (measured 2026-10-03). Annex F allows compressed objects in a linearised file with
      a cross-reference stream, and the hint tables would have to say where each is. The
      same writer path is the one Y-F1 found ignoring `obj_stm`.
      **Part 9 is packed, 2026-10-03**, when `obj_stm` is on: no hint table names its
      objects, the outline excepted, which is written directly. What the streams hold
      is numbered last, as F.3.1 requires and qpdf checks, and the first-page
      cross-reference reserves the first page's entries rather than every number after
      them. `intel_sdm.pdf` linearises to 31.8 MB, 284,052 objects in 2,841 streams;
      `scripts/test/check_linearization.sh` reads all ten samples clean but for
      ADR-0108's. `linearized_hint_test.rs` holds the numbering and the reserve, and
      undoing either failed it. Pages after the first are not packed: that is Y-0b.
- [x] **Y-F20** — **what a crop cut off an image was still in the file.** The page drew
      the cut part under a name of its own, and the whole image stayed in its resources
      under its old one, so the writer wrote every pixel the crop took away — measured
      2026-09-30 on `crop_image_test.rs`'s four-by-two image: cropped to its left half,
      the file held it at four by two and at two by two. Fixed by
      `image_crop::drop_undrawn_images`, which takes an image out of resources no page
      using them draws any more.
- [x] **Y-F19** — a page decoration and a Bates number are placed on the `/MediaBox`,
      with neither the `/CropBox` nor `/Rotate` taken into account
      (`annotations::calculate_decoration_coords`). On a page whose crop box is inside
      its media box the text can fall outside what shows; on a page turned by `/Rotate
      90`, which scanned landscape pages commonly are, "top left" lands on a side edge and
      the words run up the sheet. Placing it on what shows means drawing it turned as well.
      **Fixed 2026-10-04**: the position is worked out on the visible box — `/CropBox`
      within `/MediaBox` — turned as `/Rotate` turns it, taken back into the page's
      space, and the text drawn turned back so it reads level on the page as shown.
      `decoration_placement_test.rs` holds a plain page, a cropped one and pages turned a
      quarter and three quarters; ignoring the crop, the turn or the 270 case each
      failed it.
- [x] **Y-F14** — a composite font's codes are taken as two bytes each, whatever its CMap's
      codespace says (9.7.6.2). `FontResource::get_min_len` asks every Type0 font for at
      least two, and `apply/text.rs` cuts its strings into pairs for reading, encoding,
      widths and places. A CMap with one-byte ranges beside two-byte ones, such as
      `90ms-RKSJ-H`, has its one-byte codes read with the byte after them. The CMap
      already carries its ranges (`CMap::decode_next`); nothing between it and these
      callers splits a string by them.
      Writing is wrong the other way round: `FontResource::unified_map` gives a
      character's CID, which is a code only under an `Identity` CMap. Measured 2026-10-01
      on `sample_02c.pdf`'s `KozMinPr6N` under `UniJIS-UTF16-H`: 東 maps to 3174, where
      the code is `6771`. `text.rs`'s `encode` writes those CIDs as codes; a field's
      appearance no longer does (`appearance::shown_in`).
      Fixed: reading splits a composite font's codes by its CMap's codespace and reads a
      code through the CID the CMap gives it (`b4c4eb3`), and the run tools cut, measure
      and write by the same codes, a character written as a code its CMap reaches
      (`FontResource::codes`, `code_for`).
- [x] **Y-F15** — ingestion fills a missing `/CIDToGIDMap` with `/Identity`
      (`refine/font.rs`) and records no `Decision`, so the audit cannot see what
      ISO 14289-1 7.21.3.2 fails: 31-004's arm for an absent entry passes, and never runs.
      It also gives a `CIDFontType0` the entry, which Table 115 defines for `CIDFontType2`
      alone: measured 2026-09-30, none of the 4 in `sample_02c.pdf` or the 23 in
      `fy05.pdf` carries one, and every one does once loaded. Either ingestion records
      what it repaired, or it stops repairing it.
      Y-F24's test finds a Type 3 font in `fugaku.pdf` carrying one too, which no table of
      the model has (2026-10-03).
      Fixed 2026-10-03, by reading Table 115 in ISO 32000-2: the entry is required of a
      `CIDFontType2` whose descriptor carries `/FontFile2`, with no default, and of no
      other font. `refine::font` is gone; `ingest::discovery::require_cid_to_gid_maps`
      gives that font `/Identity` and records a repair, and leaves every other font as
      written, which the loader already read as `Identity`. 31-004 calls an absent map
      broken, as the condition says; `audit_scope_test`'s sound document had relied on the
      silent fill and states the map now. The saves of `bokutokitan.pdf`, `fy05.pdf` and
      `sample_02c.pdf` lose the 2, 13 and 3 entries loading had added, and nothing else of
      the golden outputs moves. The Type 3 font's is the source's, and stays in Y-F28.
- [x] **Y-F16** — 31-005 to 31-008, about a Type 0 font's CMap, are not asked. They were
      left out because ingestion was said to rewrite every CMap to `Identity-H`, which it
      never did to a font that loads (ADR-0105). They need ISO 32000-1 Table 118 read out
      of `docs/specs/PDF32000_2008.pdf`.
      Fixed 2026-10-04. 31-005 is not about the CMap: it is a `CIDFontType2` with no
      `/CIDToGIDMap`, which 31-004 had been reporting; an absent map is 31-005's now, and
      one loading filled is found by the record of the repair, whose words have one home
      (`missing_cid_to_gid_map`). `audit_cmaps.rs` asks 31-006 to 31-008 of the font's
      `/Encoding` against `unicode_map.rs`'s Table 118, already read out: a name it lists
      or a stream; the stream's `/WMode` against its program's; and what it uses, by
      `/UseCMap` or `usecmap`. `audit_scope_test.rs` and `cid_to_gid_map_test.rs` hold
      them, and breaking each of six parts failed one.
- [x] **Y-F17** — 28-005 and 11-005 are left to a reader because "the enclosing
      structure element is reached through an `/OBJR`, which `struct_tree.rs` resolves to
      nothing" (`structure.rs`, `audit_form`). Since W-21j the audit reaches an
      annotation's element through `/StructParent` and the parent tree
      (`audit_objects::Belonging`), and a field's widget is an annotation, so both halves
      of each condition are reachable. What is missing is the way from a `FormField` to
      its widgets: it carries no handle.
      Fixed 2026-10-04: `form_widgets` gives each terminal field with its widgets, and both
      conditions are decided through the element each widget belongs to — a field is
      described where every widget's element states an `/Alt`, and a `/TU` is in the
      language of that element or its ancestors before the catalogue's. Neither is left
      to a reader now. `audit_scope_test.rs` holds both, and breaking the `/Alt` read, the
      element's language or the widget walk failed it.
- [x] **Y-F7** — `compare.rs`'s `to_f64` is used only under the `render` feature and is
      not gated with it, so `cargo build -p fepdf` warns. The workspace build unifies
      features and never sees it. Gated with `render` in Y-4 (`5467c37`).
- [x] **Y-F23** — **a save writes `/Info`'s dates in the metadata stream's form.**
      `publish upgrade --no-obj-stm` of `fy05.pdf` writes `/CreationDate
      (2024-11-08T09:05:36+09:00)`, where 7.9.4 asks for `D:20241108090536+09'00'`
      (measured 2026-10-03). `metadata.rs` inserts the settled value as it is, under a
      comment saying it formats it as `D:`. *Done when* the written dates are 7.9.4's form
      and a test fails with the conversion taken out.
      Fixed 2026-10-03: `metadata::pdf_date` writes 7.9.4's form, and a date that does not
      parse is left out of `/Info` with a `Decision`. The ten samples' twenty dates left
      `arlington_known.tsv`, and writing the settled value as it was failed the test.
- [x] **Y-F24** — **nothing checks that what a save writes conforms.** Re-reading the
      Y-F23 output reports `none — nothing in reading this document departed from the
      standard`: the reader does not check a date's syntax, and no test reads a save back.
      `Strictness::Strict` is named in `interpretation.rs` and by no test. Output
      conforming to ISO 32000-2 is what the engine is for, so it is a measured claim or
      none. *Done when* a corpus test reads every sample's save back and fails on a
      departure, and putting Y-F23 back fails it.
      Done 2026-10-03, against the Arlington PDF Model, a submodule now:
      `suite/arlington_test.rs` reads each sample's save back and holds every dictionary
      it reaches against the model, one test a sample. `arlington_known.tsv` lists what a
      save departs from, each with its item; a departure not listed fails, and so does a
      listed one no longer made. Held against the sources as well, the only departure a
      save introduced was Y-F23's dates; the rest the sources carried and the save kept,
      which is Y-F28. Not checked: a condition the model writes as `fn:`.
- [x] **Y-F25** — **a save keeps `/ProcSet`, which 14.2 deprecates**: 1,086 arrays in
      `fy05.pdf`'s output (measured 2026-10-03), while settling removes the `/Info`
      entries 14.3.3 deprecates. Not a violation, since a processor shall ignore it; the
      asymmetry is what is undecided. Either it goes, or an ADR says why it stays.
      Decided and done 2026-10-03, by the owner: it goes. `ingest::discovery::drop_procsets`
      takes every `/ProcSet` out at load and records one repair a document, with the count;
      nothing in the engine reads one. The nine samples' lines left `arlington_known.tsv`,
      and taking the drop out fails `procset_test.rs`.
- [x] **Y-F27** — **two operations have no MCP tool of their own**: `RemoveOutside` and
      `ResizePages`, of 52 (`status.sh`, 2026-10-03). Each is reachable through the
      generic `apply_operation` tool, without a schema saying it exists, so a caller has
      to know it to ask for it. `ARCHITECTURE.md` said every variant had one, which was
      corrected the same day.
      Fixed 2026-10-04, with `ApplyRedactAnnotations`, which Y-10 added: `resize_pages`,
      `remove_outside` and `apply_redact_annotations` are tools of their own, 54 of 54.
      `mcp_server_tests.rs` holds each reaching the file.
- [x] **Y-F28** — **a save keeps what its source departed by.** Held against the
      Arlington model (Y-F24, 2026-10-03), the samples' saves keep keys 2.0 deprecates —
      `/CIDSet` in five font descriptors, `/CharSet` in two, a font's `/Name` in five, an
      AcroForm resource's `/Encoding`, `intel_sdm.pdf`'s `/Info /Title` — and keys no table
      of the model names: `/Type` in `/MarkInfo` and `/ViewerPreferences`, an image's
      `/ColorTransform`, a CIDFont descriptor's `/Subtype`, a Type 0 font's `/Name`. A
      widget in `sample_02c.pdf` seemed to lack `/DA` and `/FT`, which it inherits from its
      field: the checker read required keys without `/Parent`, and reads them with it now. Every one
      is in the source as read; none is the save's own. A translator to ISO 32000-2 drops
      or repairs them, each with a `Decision`. `arlington_known.tsv` lists them.
      Done 2026-10-03, each read against ISO 32000-2 first. `ingest::conform` makes them
      2.0's at load, one `Decision` a kind with the count: a font's `/Name` (deprecated,
      Table 109, 9.6.2.1) goes, and a descriptor's `/CIDSet` and `/CharSet` (deprecated)
      are left out of what a save writes — not at load, since the audit asks 31-012 to
      31-015 of exactly those claims, which taking them at load had made unanswerable; a
      key a dictionary of its kind does not have goes — `/Type` in `/MarkInfo` (Table
      353) and `/ViewerPreferences` (Table 147), a Type 0 font's `/Name`, a descriptor's
      `/Subtype`, a Type 3 font's `/CIDToGIDMap`; a DCT image's `/ColorTransform` moves to
      the filter's decode parameters, where Table 13 puts it, keeping its value; a
      widget's `/DR` joins the form's (Table 224) and the form's PDF 1.0 `/Encoding` goes.
      A key no table names anywhere is left, as 7.3.7 allows. Two lines were the checker's:
      the widget's `/DA` and `/FT` are inherited, and a merged field's `/AA` holds a
      field's `/C`. One line stays: `intel_sdm.pdf`'s `/Title` is a thread's `/I`, which
      Table 162 does not deprecate and the model reads with the document's table.
      `conform_test.rs` holds each translation, and taking any out fails it.
- [x] **Y-F29** — **opening a file writes a packet of its own over the file's.**
      `metadata::settle` runs at load with the document's provenance empty and the stamp
      `seconds_now()`, so the packet the engine holds names a `DocumentID` drawn from the
      clock and no `DerivedFrom`, and a second opening of one file reads another ID
      (measured 2026-10-03, reading a save back while building Y-F4's test, which reads
      without refinement because of it). A save is unaffected: it reads the source's
      identity before settling and writes the packet again. What `inspect` and the audits
      read of `xmpMM:` is the engine's, not the file's.
      Fixed 2026-10-03: a packet rewritten outside a save — on opening, and when a
      declaration is stated — keeps the file's `DocumentID`, `InstanceID`, `DerivedFrom`
      and `OriginalDocumentID` (`xmp_carry::carry_identity`), and a file with no packet
      is given none; only a save, which makes a new document, draws a new one. One file
      opened twice holds one ID, and `save_metadata_test.rs` reads a save back as any
      caller does.
- [x] **Y-F30** — **a linearised file puts before its first page what Annex F puts after
      it.** F.3.5 lets part 4 hold the catalogue and the values of its `/ViewerPreferences`,
      `/PageMode`, `/Threads` (the thread dictionaries alone), `/OpenAction` and `/AcroForm`
      (the top-level dictionary alone), and says every other object *shall not* be there:
      named destinations, the structure tree, the field hierarchy, the information
      dictionary belong in part 9 (F.3.10). `trace_doc_reachable_selective` takes all a
      catalogue reaches but `/Pages`, so `intel_sdm.pdf`'s 279,508 named destinations sit
      between 9.9% and 54.8% of the file and its first page at 53.7% (measured
      2026-10-03): a reader has read half the file before it can show page one. Part 9 is
      what Y-F22 packs, so this goes first.
      Partly done 2026-10-03: part 4 is the catalogue and what F.3.5 names, the page tree
      is part 9's, and the outline is part 6's only under `/PageMode /UseOutlines` and
      otherwise one run at the head of part 9, which the outline hint table points at.
      `intel_sdm.pdf`'s first page moves from 53.7% of the file to 0.2%, and
      `linearized_hint_test.rs` holds a page ahead of a destination. The information
      dictionary, which F.3.5 also puts in part 9, is placed and numbered there with the
      rest; `fy05.pdf`'s moves to 99.8% of the file, qpdf reads all ten clean, and the
      test holds it after the page.
- [x] **Y-F31** — **qpdf finds the hint tables wrong in nine samples of ten.**
      `scripts/test/check_linearization.sh` holds every sample's linearised save against
      `qpdf --check-linearization` (qpdf 12.4.2, 2026-10-03): only `unicode_16.pdf` reads
      clean. Two kinds of fault. Page 0's length and `/E` overstate the first-page
      section by 152 to 15,343 bytes in seven samples. A page's object count is off in
      five — 4,962 of `intel_sdm.pdf`'s pages, mostly by one. The engine reads a
      linearised file back without its hint tables, and the golden comparison says only
      that bytes moved, so nothing here saw it. *Done when* the script reports no
      linearization errors for every sample.
      Eight of ten clean 2026-10-03: the first page's shared objects were taken by an id
      range worked out again from counts, one past where `assign_lin_ids` began them, so
      one of the other shared objects went into the first-page section; and a page's
      thumbnail was counted with the page, where F.3.10 puts it in part 9. **Open**:
      `intel_sdm.pdf` and `sample_02c.pdf`, whose pages hold objects a catalogue entry
      reaches as well — article beads through `/Threads`, widgets through `/AcroForm` —
      which the writer counts as the page's own and qpdf does not.
      Closed the same day by the owner's choice: the writer follows Annex F's text, which
      puts beads with their pages, and the two warnings are listed in
      `scripts/test/linearization_known.tsv`, which the script reports and does not count
      ([ADR-0108](docs/adr/0108-a-page-keeps-what-annex-f-gives-it-where-qpdf-counts-otherwise.md)).
      The script reads all ten clean, and fails with the list removed.
- [ ] **Y-F26** — **the documents name the wrong thing as what fepdf is.** The owner's
      definition, 2026-10-03: a translator from any PDF to ISO 32000-2, operations on
      what it translated, and frontends for those operations; reporting what was done
      is secondary to that. `README.md` opens on the reporting, and `AGENTS.md`
      principle 2 and `ARCHITECTURE.md` §4.3 carry the same weight. Held until the work
      under way lands.

**Reading what was added**, in the order the last cleanup found defects
- [x] **Y-1a** — `fepdf-font`: `cff`, `subset`, `program_glyphs`, `metrics`, `embedding`,
      and the changes to `agl`, `annex_d`, `reconstruction` and `lib`. Three defects,
      each with a test that fails with the fix taken out: `fsType` read without its `OS/2`
      version (`ff025d1`); a CFF subset moving offsets by operator rather than by where
      they point (`0a1c6f3`); a TrueType subset for a CIDFont keeping the `cmap` 9.9 says
      shall not be there (`abdea2d`). Three claims corrected: that no sample CFF is
      CID-keyed (fourteen are), a doc line on the wrong module, and two comments
      describing a face choice `regular_face` now makes.
- [x] **Y-1b** — `fepdf-gui`: `view`, `view/draw`, `worker`, `capture`, `annotate`,
      `speech`, `measuring`, `finding`, `printing`, the sidebars. Eleven defects, each
      with a test that fails with the fix taken out, and the gesture ones seen in the
      window through a capture plan:
      - `/Rotate 90` and `270` drawn mirrored, in the window and by `page_to_pixels`, and
        every box taken to start at `(0, 0)` (`a6af4dc`); every tool mapping the page as
        upright and unoffset (`20e9865`). No page of the 18,282 in the corpus is turned;
        the window's own rotate and crop make them.
      - A stripped export erasing the open document's title and author (`409e68d`).
      - A radio group's widgets read as fields (`6db2ac5`), and radio groups and push
        buttons drawn as check boxes (`f9314de`).
      - The caliper snapping to invented points (`1f7b1fd`) and ignoring `/UserUnit`
        (`1a96559`); the snapshot ignoring it too (`6206792`).
      - Reading aloud from past the last passage reading the whole document
        (`e4b17cc`); a one-code run offered a cut (`9355f5b`).
      Stale claims corrected beside them; Y-F9 to Y-F12 are what could not be settled
      here.
- [x] **Y-1c** — `fepdf-mcp` and `fepdf-cli`: each tool description against what the tool
      does. What it found reached past the descriptions:
      - The window's export, with its redactions burned as it opens, wrote the file with
        none of them (`955391a`).
      - `apply_redaction` claimed to sanitise what it only replaces the text of, and the
        export box claimed atomic sanitisation (`a1f9fea`); Y-F13 holds the rest.
      - A save wrote any version it was handed into the header, and the window offered
        1.7 over 2.0 content (`df46fe3`); the summary called every file 2.0 (`0380475`).
      - `ExecuteAction` ran nothing and is `SetOpenAction` (`727a610`);
        `audit_document` said the cross-reference resolved of files it repaired
        (`b686b81`).
      - Two operations wrote what no reader reaches and are removed (ADR-0103); the
        wrapper document is built to 7.6.7 and chosen for the writer (ADR-0104).
- [x] **Y-1d** — `fepdf-doc`: `apply/*`, `audit_*`, `measure`, `reading`, `glyph_map`,
      `unicode_map`, `struct_tree`, `outline_tree`, `tagging`. Each fix with a test that
      fails with it taken out:
      - What an operation removed was still written: a removed or extracted page
        (Y-F18, `92828c6`), what a crop cut off an image (Y-F20, `15c67c9`), a replaced
        image (`9835cad`), a deleted structure element (`ef2f0d3`).
      - The audit asked tags as written, not as role-mapped (`b855d5a`); missed shadings
        (`e4faea6`) and appearance states written in place (`e7c9a73`); and stepped over
        an empty `/Lang` (`ec71b41`).
      - Text: a mark read on its element's first page (`300d734`, `a2f618d`); a run
        after `Q` read in the state inside it, and a move losing its `TJ` spacing
        (`c74df96`); a text layer set across a turned page (`3e13113`); a field's value
        drawn as its UTF-8 bytes (`74933d2`).
      - Forms: radio buttons that were no group (`5a9c658`), field words not text
        strings (`7fc79f9`), `/DA` not inherited.
      - What 12.3–12.4 cannot express refused or kept: links, bookmarks and beads to
        pages not there (`4dd4371`, `58b6493`, `7e0969b`), page labels (`970dca0`), the
        name trees an attachment replaced (`e254498`), the layers content is in
        (`fda395a`), an output intent's `/N` (`69f763a`).
      - Pages whose box does not start at the origin (`5aa04e3`) or is turned
        (`f0e6a00`), and a mark of one number on two pages (`75ddcff`).
      - A Type 0 CMap rewrite that never ran is removed (ADR-0105). Y-F14 to Y-F17 and
        Y-F19 are what was found and not settled.
- [x] **Y-2** — the tests that were added: `fepdf/tests` (+12,112 lines) and
      `mcp_server_tests` (+540). Each assertion that cannot fail is replaced. *Done when*
      each file's central assertion has been shown to fail with the behaviour it
      guards broken, starting with `audit_scope_test.rs` (2,630 lines). Each of the 59
      files had its central behaviour broken once in the code it guards, with
      `scripts/test/mutate_once.py` — one edit, the file restored byte for byte, no
      `git checkout` — or during Y-1d. What that found:
      - Nineteen tests passed with `samples/` absent, which every clone is (`7356517`).
      - Fourteen in `backend_operations_test.rs` asserted that a struct held what had
        just been put in it, and are removed (`9898445`).
      - The font census's dedup and the summary's reading of the header could be
        removed failing nothing: both compared two answers that the breakage changed
        alike (`900480e`, `72247d8`).
      - `vacuum_test` survives breaking the writer's trace alone because the output
        copy drops the same object first; its positive control is what shows it can
        fail, and the redundancy is two mechanisms, not a test that cannot.
      - `audit_scope_test` holds both directions: five conditions across five modules
        stopped reporting broken, and two made always broken, each failed it.

**Structure**
- [x] **Y-3** — finish the move into `fepdf-model/src/access.rs`, which already exists
      for this and calls itself "the destination, not the finished move". It replaces `audit_objects::{entry, name_of,
      items}`, `audit_fonts`' and `outline_tree`'s methods, `decrypt::entry`,
      `function::entry`, `mesh::entry`, `interactive::name_of` and
      `catalog::resolve_dict`. Where a site's resolving changes, whether the old
      difference was meant is decided first. *Done when* the golden comparison agrees
      and `unbounded_recursion.py` passes.
      `access` is public, and `entry`, `name_in` and `items` take the object a dictionary
      is written as. Two resolvings stay different on purpose: `decrypt::as_written`
      needs the reference `/Encrypt` is written as, to skip that object, and
      `outline_tree`'s `entry` needs the reference that names the next node, which a
      resolved value has lost; `catalog::resolve_dict` still refuses a stream. The
      golden comparison differs only in `sample_02c`'s text, the Y-F14 correction, and
      `unbounded_recursion.py` passes.
- [x] **Y-4** — `fepdf-doc`'s operations stop importing helpers from its audit modules
      (`glyph_map`, `glyph_widths`, `unicode_map` and `formula_marks` import from
      `audit_objects`). Those four are audits themselves — only the audits reach them —
      and Y-3 already took their `audit_objects` imports. The one operation that
      imported from an audit was `apply/artifacts.rs`, through `audit_fonts::names_in`,
      which is `fepdf_model::access::names_in` now. Rule E checks that it stays so
      ([ADR-0107](docs/adr/0107-an-operation-does-not-reach-into-an-audit.md)).
- [x] **Y-5** — `merge` and `extract_pages` leave the facade for `fepdf-doc`, beside the
      cloner. They build a `PdfArena` in `fepdf/src/lib.rs`. `layering.py` fails on a
      `PdfArena::new` in the facade. *Done when* adding one back fails the audit.
      `fepdf_doc::assembly` holds both and the copy a save writes, the third arena the
      facade made; merging and extracting share one page tree. `layering.py` counts
      `arenas=`, and a `PdfArena::new` appended to `fepdf/src/lib.rs` read `arenas=1`
      and failed it. `merge` had no test, and the ones written for the move found Y-F21.
- [x] **Y-6** — `PdfError::Other` is removed, and its 199 sites take the variant for
      their kind
      ([ADR-0102](docs/adr/0102-an-error-says-whose-it-is.md)).
      **The engine's half is done:** the variant is gone, so a new `PdfError::Other(`
      does not compile, and its 216 sites (counted 2026-10-03; 199 on 2026-09-28) are
      `NotFound`, `Refused`, `ClauseViolation`, `Internal`, `DepthLimitExceeded` or
      `Io`. `Missing` has seven kinds; a page past the end is `Missing::Page { index,
      count }` from `Document::page_handle`, which nine `get_page_handle(..).ok_or_else`
      sites became. `fepdf-font`'s errors are `ClauseViolation` 9.9: every message it
      carries is about a font program's bytes. One reading differs from the ADR's: a
      content-stream operator popping an empty stack is the document's fault — it wrote
      too few operands — so it is `ClauseViolation` 7.8.2, not `Internal`.
      **The server's half:** `McpError::Pdf { during, error }` carries the `PdfError`, and
      the tools answer `Result<String, McpError>`. `McpError`'s `IntoCallToolResult`
      returns a refusal, a missing name, a malformed document or an unreadable path as
      the call's result marked as an error, and `Internal`, `Arena` and the two
      linearisation faults as the server's — decided by a match naming every variant.
      `lib.rs`'s test holds both directions, and making one engine fault the call's
      failed it.
- [x] **Y-7** — no `impl` block in production code runs past 800 lines. On 2026-09-28
      eight did, the largest `FontResource` (2,457), `PdfWriter` (2,423),
      `PdfDocument` (1,566), `FontReconstructor` (1,371) and `FepdfApp` in
      `view_panel.rs` (1,220). `fepdf-gui`'s `view.rs` and `worker.rs` go first, because
      they grew most in V-X. Each is a move and not a rewrite, and the golden comparison
      is what shows it.
      Measured 2026-10-03, the eight were `FontResource` (2,570), `PdfWriter` (2,440),
      `FontReconstructor` (1,372), `PdfDocument` (1,311), `FepdfApp` in `view_panel.rs`
      (1,205), `Document` (1,177), `PDFView` (1,135) and `Sublimator` (815). Each is
      in files of one subject now, the largest block 701 lines (`PDFView`'s drawing,
      which was already its own file). Each split is a move: the methods that were
      private are `pub(super)`, and the code lines before and after differ only where
      rustfmt wrapped a signature that grew by that. The golden comparison agreed after
      each. `layering.py`'s Rule D read `lib.rs` alone and reads every file of the
      facade now, since `PdfDocument`'s methods are in four. Rule 1 gates the 800
      ([ADR-0106](docs/adr/0106-an-impl-block-is-held-to-800-lines.md)).
- [x] **Y-8** — `ARCHITECTURE.md` §3 carries no count that moves (ADR-0080). It quotes 30
      operations, 8 built by `fepdf-cli` and 12 by `fepdf-gui`, where `status.sh` reads
      54, 9 and 30. It also names `fepdf-script` and `fepdf-fixtures` in the diagram, and
      says what `fepdf-doc` has come to hold.
      §3 states no operation count and no crate count, and no line-count history; the
      diagram draws `fepdf-script` as the library the frontends call (ADR-0082 — the
      row still called it "the fifth frontend") and `fepdf-fixtures` beside the stack as
      the dev-dependency it is; and `fepdf-doc`'s row names what it holds by module.
- [x] **Y-9** — whether `fepdf/tests`' 82 files, each linking the GPU stack as its own
      binary, become one binary is decided by an A/B of the gate's own command, cold and
      warm. The threshold is written down before the measurement.
      **Written 2026-10-03, before measuring.** Warm only, by the owner's choice: a cold
      half deletes `target/debug` twice for about three hours. Each half is built once
      untimed with `cargo test --workspace --no-run`, then `crates/fepdf-model/src/lib.rs`
      is touched and `cargo test --workspace` is timed by wall clock — the edit-and-gate
      loop, with every test binary above `fepdf-model` relinked. **One binary is taken
      if its time is at least 15% below the 82 binaries'**; below that, each file keeping
      a process of its own is worth more than the time.
      **Measured, the same day: 2,122 s for the 82 binaries, 924 s for one — 56% below,
      and one is taken.** Compiling went from 3 min 59 s to 2 min 40 s; the rest is the
      run, since one binary runs every test in one pool where 82 ran one after another.
      Both halves passed the same 1,459 tests. The files are `crates/fepdf/tests/suite/`,
      each a module `main.rs` names, and a test there checks that every file is named and
      that none sets an environment variable or the working directory; adding an unnamed
      file failed it. `TESTING.md` says how one file is run alone.

**Taken from the findings**
- [x] **Y-10** — **redaction removes what it covers, and draws over it** (Y-F13, taken by
      the owner 2026-10-03). Measured the same day: no `Operation` redacts —
      `annotation.rs` names an `Operation::RedactDocument` that does not exist — and
      `PdfDocument` and `fepdf-mcp` call `apply_physical_redaction_to_page` directly.
      That replaces the whole string of each show-text operator touching a rectangle,
      covered or not, with `[REDACTED]`, and nothing else on the page. `RedactEntries`
      reads a `/Redact` annotation's `/QuadPoints`, `/IC` and `/RO`, and nothing acts on
      them.
      *Goal*: after a redaction and a save, nothing inside a region can be recovered
      from the file, and the region shows a fill.
      - An `Operation` redacts, so the window and `fepdf-mcp` go through `apply` and the
        history holds it (Rule D); applying a document's own `/Redact` annotations is
        the same operation fed from 12.5.6.23.
      - Text: the glyphs inside the region go, and the ones outside keep their places
        (a `TJ` adjustment where a glyph was), so no length stands in for the words.
        `/ActualText` and `/Alt` on what was removed go with it.
      - Images: the pixels inside, in image XObjects and inline images, are replaced
        and re-encoded; the old stream is reached by nothing, so the writer drops it.
      - Form XObjects are entered; one another page also draws is copied first.
      - Annotations and widgets whose `/Rect` meets the region are removed, and with each
        its `/Popup` and the replies naming it by `/IRT`, which carry `/Contents` of
        their own.
      - A form field keeps its value in the field tree, not in the widget, so removing
        the widget leaves `/V` written. A field whose widgets are all inside the region
        goes from the tree — its parent's `/Kids`, `/Fields` and `/CO`. One with a widget
        outside keeps its value, which is shown there. An `/XFA` entry, which holds the
        values again as XML and which a save writes untouched, is removed with a
        `Decision`.
      - A structure element left with no content is pruned, with its `/Alt`,
        `/ActualText`, `/T` and `/E`, as page removal does through `struct_tree_pruning`.
      - A redacted page's `/Thumb`, an image of the page, is removed. An image's `/SMask`
        is replaced over the same region, and its `/Alternates` removed.
      - Content in a hidden optional content group is removed like any other: 12.5.6.23
        asks for all traces, not the visible ones.
      - The fill follows 12.5.6.23, Table 195, for a region a `/Redact` annotation
        names: `/RO` when present, drawn with its origin at the lower left of `/Rect`,
        and then `/IC`, `/OverlayText`, `/Repeat`, `/DA` and `/Q` are ignored; else
        `/OverlayText` set by `/DA` and `/Q`, repeated when `/Repeat` is true, over
        `/IC`; else `/IC` alone; else nothing — **the region is left transparent**.
        Black is this engine's choice only for a region a caller names with no
        annotation, and is recorded as one.
      - A `/Redact` annotation applied is removed from the document with what it
        named, as 12.5.6.23 requires.
      *Decided by the owner, 2026-10-03*, after MuPDF's and iText pdfSweep's documented
      behaviour was read:
      - A vector path under a region is **cut to it**: what lies inside goes, what lies
        outside stays. MuPDF removes a path whole, which here would take a full-page
        background or a table's rules with it.
      - A glyph whose box **overlaps** a region at all is inside, as MuPDF documents.
      - An element whose content goes in part has its `/ActualText` and `/Alt`
        **replaced with a marker**, and so do its ancestors, as pdfSweep 5.0.8 does. An
        element whose content goes whole is pruned.
      - Type 3 glyph procedures and tiling pattern cells are **not entered**: a Type 3
        glyph goes whole as a glyph, and a path a pattern fills is cut like any path.
      - **What a redaction will remove can be shown before it is applied**: the same
        computation answers a query that writes nothing, and the window highlights
        what it returns. The overlap rule takes more than the region at its edges, and
        this shows it before it is done.
      **Text, 2026-10-03**: `Operation::Redact` removes every glyph whose box meets a
      region — the box reaching 0.3 em under the baseline, deeper than a text face's
      descender — keeps the rest where they were, and fills the regions black with a
      12.5.6.23 `Decision`. A page with a string in a font its resources do not name is
      refused, since its glyphs cannot be placed. `what_redaction_removes` answers what
      will go and writes nothing; `fepdf-mcp` reports from it, and the window applies the
      redaction as a recorded act before it saves. `redaction_test.rs` holds it, and
      taking out the descent, the refusal or the fill failed it.
      **The fill is the caller's to choose, 2026-10-03** (the owner's request):
      `Redaction::fill` takes what a `/Redact` annotation's `/IC` takes — none for no
      fill, gray, RGB or CMYK — and only when it is absent is black the engine's choice
      and recorded. `fepdf-mcp` takes it as `fill`, and the window's export asks for a
      colour or none.
      **Image XObjects, 2026-10-04**: each one the page's content draws and a region
      meets is replaced, for that drawing, by a copy with every pixel the region touches
      blanked — zero, or for a stencil what paints nothing — and its `/SMask` and `/Mask`
      blanked with it and its `/Alternates` dropped; the original, drawn nowhere else
      from those resources, is not written. An image that cannot be decoded is refused.
      `what_redaction_removes` names the areas. `redaction_image_test.rs` holds it, and
      taking out the blanking, the masks, the stencil's value or dropping the original
      failed it.
      **Inline images, 2026-10-04**: a redacted page's inline images are lifted into
      image XObjects first — keys, filters and device spaces spelled out (Tables 91 and
      92), a space named from the resources taken from them — and drawn with `Do`, so
      they are blanked as image objects are. An unfiltered one's samples are counted
      rather than read to the first ` EI`, which bytes among them can spell; the content
      parser cut such an image short too, and counts the same way now.
      **Paths, 2026-10-04**: a filled path, or one that clips, is clipped to the four
      strips round each region, which keeps the fill rule's reading; a stroke loses every
      stretch within half a pen of a region, and a mitred corner whose point could reach
      one is broken there. Curves are split where they cross, so what is left is the same
      curve. A dashed stroke's pieces start their dash at the phase the line had reached.
      `redaction_path_test.rs` holds a background, a circle, a stroke, a dash, a mitred
      corner and a clip; taking out a strip, the phase, the corner break, the clip or the
      cutting failed it.
      **Form XObjects, 2026-10-04**: each form the content draws and a region meets is
      copied with resources of its own, the drawing pointed at the copy, and the copy
      redacted as the page is — inline images, text, images, paths and its own forms —
      with the regions taken into its space through the box round them there, which for a
      form drawn turned is a little more than the region. The original goes from those
      resources once nothing there draws it; another drawing keeps it. Forms nested past
      16 are refused, since one may draw itself. `redaction_form_test.rs` holds it, and
      taking out the entering, the dropping of the original or the placing of the
      preview's boxes failed it.
      **Annotations and fields, 2026-10-04**: an annotation whose `/Rect` meets a region
      goes from the page with its `/Popup` and every reply naming it by `/IRT`, on any
      page; a widget leaves the field tree, and a field left with no widget goes from its
      parent's `/Kids` or `/Fields` and from `/CO`; one with a widget elsewhere keeps its
      value. The form's `/XFA` goes with a `Decision`, and the structure tree's
      references to what went are pruned, since either would keep it in the file.
      `redaction_annot_test.rs` holds it; taking out the popups and replies, the
      structure pruning, the field tree, the climb to an emptied field or the `/XFA`
      failed it.
      **Marked content and structure, 2026-10-04**: each marked-content sequence is
      classed before anything is removed — gone where everything in it lies inside a
      region, touched where a region meets any of it. A touched or gone sequence's
      `/ActualText`, `/Alt` and `/E`, written in place or in a named property list, become
      `[REDACTED]`; so do those of every element holding one of its marks, and of its
      ancestors; a gone mark leaves the tree, and an element left holding nothing is
      pruned, from the parent tree and the `/IDTree` too. `redaction_structure_test.rs`
      holds it; taking out the content's marker, the named list's, the elements', the
      pruning or the gone classing failed it.
      **Marks in forms, 2026-10-04**: a mark inside a form is held by the form's stream
      (`/Stm`), not the page, for marking and pruning both; and a marked content reference
      on the page naming a form that was replaced by a redacted copy is pointed at the
      copy — named, the original stayed in the file with what was removed from it.
      `redaction_form_test.rs` holds it; taking out the pointing, or reading the holder
      as the page in the marking or the pruning, failed it.
      **Thumbnails and `/Redact` annotations, 2026-10-04**: a redacted page's `/Thumb`
      goes. `Operation::ApplyRedactAnnotations` applies a document's own redaction
      annotations: what `/QuadPoints`, else `/Rect`, marks is removed as `Redact`
      removes it, the annotation goes with it, and its place is drawn as Table 195
      says — `/RO` at `/Rect`'s lower-left corner, else `/IC` and then `/OverlayText` in
      the font `/DA` names from `/DR`, placed by `/Q` or repeated, else nothing.
      `redaction_apply_test.rs` and `redaction_test.rs` hold it; taking out the
      thumbnail, the quadrilaterals, `/RO`, `/IC` or the overlay text failed them.
      **The *done when*, held, 2026-10-04**: `redaction_complete_test.rs` plants a marker
      in the page's text, a form's text, an image's pixels, an annotation with its popup
      and reply, marked content's `/ActualText` and a named property list's, two
      elements' `/Alt`, a text field merged with its widget and one held by a parent,
      a choice field's options, `/XFA`, the thumbnail and an optional content group that
      is off; after one redaction and a save, no string and no decoded stream in the file
      holds one, as text, hexadecimal or UTF-16, and the text outside is kept. Taking out
      the text, the images, the forms, the marked content, the structure, the
      thumbnail, the annotations or the fields each failed it, naming the markers that
      part keeps out.
      **The window's preview, 2026-10-04**: drawing a zone asks the worker what the
      page's zones will remove, and every glyph, image area, path area and annotation
      it names is tinted over the zone, so a glyph the zone only touches shows; a page
      the redaction would refuse says so then, not at the save. The worker's answer and
      the brush's state are held in `worker.rs` and `redaction.rs`; the gesture that asks
      is not, and wants trying in the window.
      **The glyphs that remain keep their places**, decided by the owner 2026-10-04
      with the width of what went left recoverable from them, as it is from Acrobat
      (arXiv 2206.02285); moving the rest of the line to the region's edge was the
      other answer ([ADR-0111](docs/adr/0111-a-redaction-keeps-the-remaining-glyphs-where-they-were.md)).
      *Done when* a fixture carrying a marker string in text, in a form XObject, in an
      annotation and in `/ActualText`, and an image under the region, saved after one
      redaction, holds the marker in no decoded stream and none of the image's pixels
      inside the region; a marker in a text field's `/V` (merged with its widget, and as
      a parent with kids), in a choice field's `/Opt`, in `/XFA`, in a removed
      annotation's popup and reply, and in a pruned element's `/Alt`, likewise; a page
      `/Thumb`, gone; a `/Redact` annotation carrying `/IC` and one carrying none,
      applied, leave a fill of that colour and no fill, and no `/Redact` annotation; and
      taking out each of those parts fails the test.

- [x] **Y-11** — **a document changes only inside `apply`, and a write anywhere else
      fails a test** (Rule D). Measured 2026-10-03, with a counter put on the arena's
      seven writers and taken out again: sixteen of the facade's `&self` readers write
      nothing, and `apply_redaction_to_page` writes three times. `layering.py` counts
      `&mut self` methods, and the arena writes through `&self`, so it saw neither that
      nor `apply_physical_redaction_to_page`, a function the facade re-exports and
      `fepdf-mcp` calls through `doc.inner()`. A check on names reaches one call deep.
      *Goal*: the arena is sealed once a document is loaded, and `apply` alone unseals
      it; a write while sealed panics in a debug build, so every test that takes a path
      round `apply` fails, and a release build pays nothing. A save writes into a copy,
      which is a different arena.
      *Done when* the two redaction routes are gone from the facade (Y-10 gives them an
      `Operation`), and a probe writing to a sealed arena from a `&self` method fails
      the gate.
      **Sealed, 2026-10-03** ([ADR-0109](docs/adr/0109-a-document-changes-only-inside-apply.md)):
      the facade seals what it opens, and the probe in `sealed_document_test.rs` fails
      when the seal is taken out. The suite found 106 writes outside `apply`, from five
      sources and a sixth behind them; four were readers writing into what they read,
      and each now writes nothing. The two redaction routes were let through by name,
      `Document::redaction_until_y10`, until Y-10's first part made redaction
      `Operation::Redact` and took both routes and the name out, 2026-10-03.

**Open**: `fepdf-doc` is 18,187 lines and holds operations, auditing, measurement and
reading order. Whether it splits is not decided here. The Y-1d reading will show whether
there is a reason to.

---

*This file was cut to what each entry delivered twice: on 2026-09-22, from 4,924 lines
to 2,966, and on 2026-09-28, from 4,029 to the present length. Both cuts follow the
argument of
[ADR-0039](docs/adr/0039-the-design-document-was-narrating-its-own-corrections.md): an
account of how a thing went wrong is a record, and a record belongs in `docs/adr/`.
**Git holds the rest.** The full text before the second cut is at commit `406dd21`.
Every item ID and every phase name is kept, so a reference from code or a record still
lands.*
