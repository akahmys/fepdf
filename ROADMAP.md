# fepdf Roadmap

> **Not a rule.** This is the work: what was measured, what was built, what is next.
> The rules are in [CODING.md](CODING.md); a claim here that contradicts the code is a
> defect in this file.

**Goal**: an engine that understands ISO 32000-2 (PDF 2.0) semantically — not merely
one that round-trips it. PDF 1.7 and earlier are **read-only** targets; output is
always 2.0.

That goal is not a predicate. Its nearest measurable form is **the share of the
constructs a corpus presents whose contents this engine reads**: 88% over the ten
samples and 94% over all 525 files, with the catalogue axis at 21 of 22 (re-derived
2026-10-13 with `fepdf inspect coverage`,
[ADR-0019](docs/adr/0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md)).
It counts only what can be enumerated from a file. Colour spaces, shadings and functions
cannot be, and they are where Phase P found the engine drawing the wrong picture. Nor
does it say whether what was read was read correctly.

Round-trip fidelity holds: no catalogue key is lost through a save, and
`crosscheck_selfread.sh` makes that comparison rather than this sentence asserting it.
The differences are by design:
- `/SpiderInfo` is dropped, with the rest of what 2.0 deprecates and keeps nothing in
  place of ([docs/specs/pdf-2-0-deprecations.md](docs/specs/pdf-2-0-deprecations.md),
  AB-4).
- `/Metadata` is added where a source had none.
- Objects are renumbered, since saving produces a new document
  ([ADR-0012](docs/adr/0012-saving-produces-a-new-document.md)).
- Inherited attributes are resolved onto the pages
  ([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)).

Phases Y and Z are closed (2026-10-10), Phases AB and AC on 2026-10-12, and Phase AA is open. Two items Phase Y found wait on Windows and
Linux, under [Held for another platform](#held-for-another-platform).
`./scripts/dev/status.sh` re-derives the figures this file leans on, so a stale one reads
as a disagreement rather than as current.

## Releases

How a version is chosen is [TESTING.md](TESTING.md) §5. The last row is the version
`Cargo.toml` declares, and `status.sh` holds the two equal.

| Version | Date | Phases closed |
| :--- | :--- | :--- |
| 0.1.0 | 2026-10-14 | A–Z, AB, AC: every phase closed before versions were kept |

v1.0.0 and v1.0.1 are withdrawn
([ADR-0131](docs/adr/0131-a-version-is-a-release-and-a-release-closes-a-phase.md)).

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
| **7.7** Document structure | 21 of the 22 Table 29 entries a file carries modelled; 10 of the 32 occur in no file. [↓](#77-document-structure) |
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

## Held for another platform

Work that can be neither done nor checked on this machine. Each keeps the ID of the phase
that found it, and is taken when it can be run where it applies; until then it holds no
phase open.

- [ ] **Y-F9** — reading aloud on Windows hands PowerShell the words on standard input
      and reads them with `[Console]::In`, whose encoding is the console's code page, not
      UTF-8; nothing sets it, so Japanese may arrive garbled (`speech.rs`). Unverified: no
      Windows here. Held until it can be run on Windows.
- [ ] **Y-F10** — stopping speech on Linux kills `spd-say`, and the speaking is done by
      the speech-dispatcher server; whether the passage under way stops is unverified,
      and `spd-say --cancel` is the call that says so (`speech.rs`). Held until it can be
      run on Linux.

## Open, outside a phase

- [x] **Two ways in for the font resources.** `external/` pins Adobe's CMap and
      CID-to-Unicode repositories as submodules, and `scripts/dev/fetch_font_resources.sh`
      cloned the same two at their tips, so the two disagreed about which data a working
      copy got. **Done, 2026-10-14**: the script is deleted and the submodules are the one
      way in, as CI and the release build already used.

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

The crate-by-crate cleanup ended at `c4331cf` (2026-09-10). Phases V, W and X added 54,355
lines after it, across 59 new source files and 81 changed ones, and none of it has been
read the way that cleanup read the rest.

**The net**
- [x] **Y-0** — `scripts/test/golden_outputs.sh` compares, between `HEAD` and the working
      tree, over `samples/` and `target/external/`: the bytes of a save (plain,
      linearised, encrypted), `inspect fonts`, `text` and `coverage`, and
      `doc.decisions()`.
- [x] **Y-0a** — its first run did not finish: linearising `samples/intel_sdm.pdf` (5,057
      pages) followed each article bead's `/T`, `/N` and `/V` from every page, so each
      page reached every bead, 24.7 million objects in all.
- [x] **Y-0b** — the same file linearises to 129 MB against a 25.6 MB plain save: 71 MB of
      it is the hint stream's reserve, zero-filled.
- [x] **Y-0c** — saves were not reproducible: the XMP `InstanceID` was salted with the
      clock's seconds.

**Found while building the net**, each to be taken on its own
- [x] **Y-F1** — `save_linearized` keeps its own copy of the metadata handling and ignores
      `password`, `strip`, `lang`, `copyright` and `obj_stm`, beside a comment saying it
      is consistent with `save_with_options`.
- [x] **Y-F2** — `SaveOptions::creation_date` is read by nothing.
- [x] **Y-F3** — `fepdf-cli` panics when a certificate named by `--encrypt-to` cannot be
      read (`args.rs`, `From<SaveArgs>`), where Rule 2 asks for an error.
- [x] **Y-F4** — the XMP `DocumentID` is `md5` of the title alone, so every untitled
      document shares one, and two documents of one title collide.
      ([ADR-0012](docs/adr/0012-saving-produces-a-new-document.md))
- [x] **Y-F5** — a document with no date is given `2026-05-26T06:00:00Z` as its
      `CreateDate`, `ModifyDate` and `MetadataDate`, with no `Decision`.
- [x] **Y-F6** — `ModifyDate` and `MetadataDate` are copied from the source.
      ([ADR-0012](docs/adr/0012-saving-produces-a-new-document.md))
- [x] **Y-F8** — `cargo doc --workspace --no-deps` prints 44 warnings, most of them
      intra-doc links that resolve to nothing, and nothing gates it.
- [x] **Y-F36** — **what the inspection of 2026-10-06 found**, each measured and fixed,
      among them three dependencies `fepdf-audit` declared and nothing in it names
      ([ADR-0112](docs/adr/0112-the-audit-is-a-crate-above-the-operations.md),
      [ADR-0080](docs/adr/0080-a-design-document-does-not-carry-a-number-that-moves.md)).
- [x] **Y-F11** — writing the bookmark panel's draft replaces the whole tree with what
      `OutlineNode` carries: a title, a page, children.
- [x] **Y-F12** — `PdfDocument::apply` states no atomicity, and the window's journal
      relies on it: a failed act is rebuilt from the history only when its second or later
      operation failed, so a first operation that changed the document and then failed
      would leave it differing from what undo replays.
      ([ADR-0110](docs/adr/0110-an-operation-that-fails-changes-nothing.md))
- [x] **Y-F13** — the window calls redaction 黒塗り (blacking out) and draws nothing black:
      `apply_physical_redaction_to_page` replaces every string of a show-text operator
      touching a rectangle with `[REDACTED]`, drawn in the page's own font, and leaves the
      page's images, vector graphics, annotations, form fields and form XObjects' text
      where they are.
- [x] **Y-F18** — **a page taken out of a document is still written into it.**
- [x] **Y-F21** — **a page was two objects in a merged or extracted document, and a direct
      dictionary is read as an object it is not.**
- [x] **Y-F32** — **a save broke every inline image.**
- [x] **Y-F34** — **a save took the font out of a form that reads the page's.**
- [x] **Y-F33** — **an inline image is drawn as RGB whatever it is.**
- [x] **Y-F22** — **a linearised file writes no object streams.**
      ([ADR-0108](docs/adr/0108-a-page-keeps-what-annex-f-gives-it-where-qpdf-counts-otherwise.md))
- [x] **Y-F20** — **what a crop cut off an image was still in the file.**
- [x] **Y-F19** — a page decoration and a Bates number are placed on the `/MediaBox`, with
      neither the `/CropBox` nor `/Rotate` taken into account
      (`annotations::calculate_decoration_coords`).
- [x] **Y-F14** — a composite font's codes are taken as two bytes each, whatever its
      CMap's codespace says (9.7.6.2).
- [x] **Y-F15** — ingestion fills a missing `/CIDToGIDMap` with `/Identity`
      (`refine/font.rs`) and records no `Decision`, so the audit cannot see what ISO
      14289-1 7.21.3.2 fails: 31-004's arm for an absent entry passes, and never runs.
- [x] **Y-F16** — 31-005 to 31-008, about a Type 0 font's CMap, are not asked.
      ([ADR-0105](docs/adr/0105-ingestion-never-rewrote-a-real-type-0-cmap.md))
- [x] **Y-F17** — 28-005 and 11-005 are left to a reader because "the enclosing structure
      element is reached through an `/OBJR`, which `struct_tree.rs` resolves to nothing"
      (`structure.rs`, `audit_form`).
- [x] **Y-F7** — `compare.rs`'s `to_f64` is used only under the `render` feature and is
      not gated with it, so `cargo build -p fepdf` warns.
- [x] **Y-F35** — **a linearised file writes part 8 directly.**
- [x] **Y-F23** — **a save writes `/Info`'s dates in the metadata stream's form.**
- [x] **Y-F24** — **nothing checks that what a save writes conforms.**
- [x] **Y-F25** — **a save keeps `/ProcSet`, which 14.2 deprecates**: 1,086 arrays in
      `fy05.pdf`'s output (measured 2026-10-03), while settling removes the `/Info`
      entries 14.3.3 deprecates.
- [x] **Y-F27** — **two operations have no MCP tool of their own**: `RemoveOutside` and
      `ResizePages`, of 52 (`status.sh`, 2026-10-03).
- [x] **Y-F28** — **a save keeps what its source departed by.**
- [x] **Y-F29** — **opening a file writes a packet of its own over the file's.**
- [x] **Y-F30** — **a linearised file puts before its first page what Annex F puts after
      it.**
- [x] **Y-F31** — **qpdf finds the hint tables wrong in nine samples of ten.**
      ([ADR-0108](docs/adr/0108-a-page-keeps-what-annex-f-gives-it-where-qpdf-counts-otherwise.md))
- [x] **Y-F26** — **the documents name the wrong thing as what fepdf is.**
      ([ADR-0112](docs/adr/0112-the-audit-is-a-crate-above-the-operations.md))

**Reading what was added**, in the order the last cleanup found defects
- [x] **Y-1a** — `fepdf-font`: `cff`, `subset`, `program_glyphs`, `metrics`, `embedding`,
      and the changes to `agl`, `annex_d`, `reconstruction` and `lib`.
- [x] **Y-1b** — `fepdf-gui`: `view`, `view/draw`, `worker`, `capture`, `annotate`,
      `speech`, `measuring`, `finding`, `printing`, the sidebars.
- [x] **Y-1c** — `fepdf-mcp` and `fepdf-cli`: each tool description against what the tool
      does.
      ([ADR-0103](docs/adr/0103-two-operations-that-wrote-what-no-reader-reaches-are-removed.md),
      [ADR-0104](docs/adr/0104-the-writer-makes-unencrypted-wrapper-documents.md))
- [x] **Y-1d** — `fepdf-doc`: `apply/*`, `audit_*`, `measure`, `reading`, `glyph_map`,
      `unicode_map`, `struct_tree`, `outline_tree`, `tagging`.
      ([ADR-0105](docs/adr/0105-ingestion-never-rewrote-a-real-type-0-cmap.md))
- [x] **Y-2** — the tests that were added: `fepdf/tests` (+12,112 lines) and
      `mcp_server_tests` (+540).

**Structure**
- [x] **Y-3** — finish the move into `fepdf-model/src/access.rs`, which already exists for
      this and calls itself "the destination, not the finished move".
- [x] **Y-4** — `fepdf-doc`'s operations stop importing helpers from its audit modules
      (`glyph_map`, `glyph_widths`, `unicode_map` and `formula_marks` import from
      `audit_objects`).
      ([ADR-0107](docs/adr/0107-an-operation-does-not-reach-into-an-audit.md))
- [x] **Y-5** — `merge` and `extract_pages` leave the facade for `fepdf-doc`, beside the
      cloner.
- [x] **Y-6** — `PdfError::Other` is removed, and its 199 sites take the variant for their
      kind ([ADR-0102](docs/adr/0102-an-error-says-whose-it-is.md)).
- [x] **Y-7** — no `impl` block in production code runs past 800 lines.
      ([ADR-0106](docs/adr/0106-an-impl-block-is-held-to-800-lines.md))
- [x] **Y-8** — `ARCHITECTURE.md` §3 carries no count that moves (ADR-0080).
      ([ADR-0080](docs/adr/0080-a-design-document-does-not-carry-a-number-that-moves.md),
      [ADR-0082](docs/adr/0082-the-script-crate-is-a-library-the-frontends-call.md))
- [x] **Y-9** — whether `fepdf/tests`' 82 files, each linking the GPU stack as its own
      binary, become one binary is decided by an A/B of the gate's own command, cold and
      warm.

**Taken from the findings**
- [x] **Y-10** — **redaction removes what it covers, and draws over it** (Y-F13, taken by
      the owner 2026-10-03).
      ([ADR-0111](docs/adr/0111-a-redaction-keeps-the-remaining-glyphs-where-they-were.md))
- [x] **Y-11** — **a document changes only inside `apply`, and a write anywhere else fails
      a test** (Rule D).
      ([ADR-0109](docs/adr/0109-a-document-changes-only-inside-apply.md))

**Decided 2026-10-06**: the audit splits out of `fepdf-doc` into `fepdf-audit`, a crate
above it, and Rule E is held by cargo
([ADR-0112](docs/adr/0112-the-audit-is-a-crate-above-the-operations.md)).

### Phase Z — What a hostile file does to the translator

A translator from any PDF has to finish on any PDF. `panic = "abort"` in the release
profile means a panic, a hang or an allocation the machine cannot meet takes the whole
process with it: the CLI, the MCP server and the window alike.

- [x] **Z-1** — **a fuzzer**, `cargo-fuzz` in a `fuzz/` directory outside the workspace:
      it builds on nightly, and the gate's toolchain is pinned to 1.98.1.
- [x] **Z-2** — **PrintCraft's findings, as inputs.**
      ([ADR-0060](docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md))
- [x] **Z-2a** — **a conforming embedded Type 1 program is never converted.**
- [x] **Z-3** — **Rule 2 held by clippy.**
- [x] **Z-4** — **pdf.js's test files in the external corpus.**
      ([ADR-0113](docs/adr/0113-another-projects-findings-cross-as-facts-not-code.md))
- [x] **Z-5** — **the window driven from outside.**

**Declined**: PrintCraft's parity manifest, a feature list where each entry must cite a
test.

### Phase AA — What an Acrobat user reaches for that is not here

[docs/reference/acrobat-features/](docs/reference/acrobat-features/README.md) was compared
with this engine on 2026-10-10, item by item, from the CLI's help, the operation
vocabulary, `fepdf-mcp`'s tools and the window's labels. The owner chose three gaps from it.
The order was set the same day: AA-1 first, because the window's history already holds
what it needs; then AA-2 and AA-3, since FDF carries annotations the model cannot yet
express; then AA-5 and AA-6.

- [x] **AA-1** — **autosave and crash recovery** (`core.autosave`, `core.crash-recovery`,
      `core.encrypted-autosave`, P0). The window journals each act and replays the
      journal after a crash; an encrypted document's journal is sealed with its password
      ([ADR-0114](docs/adr/0114-the-window-journals-its-acts-and-a-crash-replays-them.md)).
      *Done when* a journal written by a window that did not exit normally rebuilds the
      same document, with the same undo history, and a sealed journal holds none of an
      `EditRun`'s words in the clear.
      **Done, 2026-10-10.** `recovery.rs`'s tests hold each part, and each was shown to
      fail with its part broken: the seal, the lock, the trim of a torn record, and the
      undo replay. Driven with `--control`, on `constitution.pdf` and on an AES-256 copy
      of it: an edit, `kill -9`, a restart. The restart offered the session, a wrong
      password was refused, and the right one rebuilt the page with its edit. ⌘Z after
      the recovery took the edit back. A normal quit removed the session. The drive found
      two defects, both fixed. The recovered page never drew, because a render request
      queued before any document was open stayed queued; recovery now clears what an
      open clears (`forget_the_document`). And a lock this process had just released
      read as held in one test run of four, because a child process holds a copy of the
      lock's descriptor until it execs; `take_lock` now waits up to 200 ms.
- [x] **AA-2a** — **annotations that can be reviewed, in the vocabulary**: removed
      (with their pop-up and replies, through the code a redaction already removes them
      with), their words edited, replied to (`/IRT`), and given a state (`/State`,
      `/StateModel`, 12.5.6.3). Each names its annotation by page and place in `/Annots`
      ([ADR-0115](docs/adr/0115-an-operation-names-an-annotation-by-its-place-on-the-page.md)).
      A new annotation carries `/NM`, and `/T` only when an author is given
      ([ADR-0116](docs/adr/0116-an-annotation-carries-an-author-only-when-the-reader-named-one.md)).
      The facade lists a page's annotations, with each one's replies and states.
      **Done, 2026-10-10**: `RemoveAnnotation`, `EditAnnotation`, `ReplyToAnnotation` and
      `SetAnnotationState`; `PdfDocument::comments(page)`; and `list_comments` in
      `fepdf-mcp`, in a router of its own because the main one was at 802 lines of its
      800. Removal is the code a redaction removes annotations with (`remove_handles`).
      `annotation_review_test.rs` holds seven behaviours, and each of the state chain,
      the uniqueness of `/NM` and the refusal of a widget was shown to fail with its
      code broken; so was `AddAnnotation` JSON without `by` reading, and `list_comments`
      being served. The window's own author name and comment list are AA-2b.
- [x] **AA-2b** — **the comment list in the window**: every annotation, filtered by kind,
      author and state, sorted by page, author or date, and answered from the list. An
      author name in the settings.
      **Done, 2026-10-10**: a `Comments` drawer (`sidebar/comments.rs`), refreshed wherever
      the form is, after undo and redo included. Replies are listed under what they
      answer at any depth, and states show as `author: state`. Annotations made in the
      window are signed with the settings' author and the time. Driven with `--control`
      on a file with two notes by others: a reply showed under its note as having no
      author; a state with no author set was refused with a pointer to the settings; with
      one set, `Reviewer: 承認` appeared; and ⌘Z took it back off the list. The author name
      lasts as long as the window, since nothing in the window is persisted yet.
- [x] **AA-3** — **FDF annotations** (12.7.8): a document's markup annotations exported
      to FDF, each with its `/Page` and an `/NM`, and an FDF's annotations imported onto a
      document. An imported annotation whose `/NM` matches one on its page replaces it in
      place, and any other is added
      ([ADR-0117](docs/adr/0117-an-fdf-import-replaces-an-annotation-of-the-same-name.md)).
      *Done when* an export imported into the clean document reproduces its comments,
      replies and states, and a second import of the same file changes nothing.
      **Done, 2026-10-10**: `PdfDocument::export_fdf` and `Operation::ImportFdf`, reached
      from `fepdf edit fdf-export` / `fdf-import`, `fepdf-mcp`'s `export_fdf` /
      `import_fdf`, and the comment drawer's two buttons. `fdf_test.rs` holds both halves
      of *done when*, the in-place replacement keeping a document's reply, the refusal of
      a file that is not FDF, and the file's structure read from its bytes: each
      cross-reference entry at its own `n 0 obj`, generation 0, `/Root` the catalogue.
      Each was shown to fail with its code broken. The byte check is the one that caught
      shifted offsets, since this engine's reader recovers from them by scanning. qpdf
      could not be the second reader: it refuses a file with no page tree. Through the
      CLI, an export imported back into its own file left two annotations two.
- [x] **AA-3b** — **FDF form values** (`/Fields`, Table 249): a form's values exported and
      imported. Separate from AA-3, because the matching is by field name and the rules
      are the form's.
      **Done, 2026-10-11**
      ([ADR-0125](docs/adr/0125-form-values-come-in-by-name-and-make-no-field.md)), with
      XFDF's `<fields>` (ISO 19444-1 6.3) too:
      - `PdfDocument::export_fdf_form` and `export_xfdf_form`;
      - `ImportFdf` and `ImportXfdf`, which take values as well as comments, by name,
        setting flags as Table 249 says, through `fepdf-doc/src/form_data.rs`;
      - `fepdf edit fdf-export --form`, the MCP export tools' `form`, and two buttons in
        the form drawer.

      `fdf_form_test.rs` covers both formats' round trips, flags, what is not imported,
      the signature not exported, Shift_JIS with Windows' extensions and UTF-8 decoded
      by `/Encoding` (with `encoding_rs`), and an encoding Table 246 does not name
      refused. Each was shown to fail with what it guards removed. `sample_02c.pdf`'s Japanese-named fields filled from an
      XFDF draw their value.
- [x] **AA-4** — **XFDF** (ISO 19444-1:2019), the XML form of FDF. The standard is in
      `docs/specs/` since 2026-10-10, bought, and never to be committed
      ([docs/specs/README.md](docs/specs/README.md)). XFDF carries an appearance only
      for a stamp (6.5.2), and Table 166 requires one on almost every annotation, so it
      comes in three parts:
  - [x] **AA-4a** — replies and state changes carry an appearance that draws nothing.
        AA-2a wrote them without one, which Table 166 forbids
        ([ADR-0118](docs/adr/0118-a-reply-carries-an-appearance-that-draws-nothing.md)).
        **Done, 2026-10-10**, held by `replies_and_states_carry_an_appearance`, which
        fails with the appearance taken out.
  - [x] **AA-4b** — an appearance drawn from an annotation's own entries, for every kind
        XFDF admits that Table 166 does not exempt: seventeen subtypes
        ([ADR-0119](docs/adr/0119-an-annotation-without-an-appearance-is-given-one-from-its-own-entries.md)).
        **Done, 2026-10-10**: `apply/drawn/`, from `/QuadPoints` in either order the field
        writes, `/Vertices`, `/InkList` and `/Path`, `/L` with leader lines and Table 179's
        ten endings, `/IC`, `/BS` and `/Border`, `/RD`, `/CA`, `/DA` and `/Q`, `/Sy`; and
        icons for every name ISO 32000-2 and ISO 19444-1 list (fifteen for a note, four for
        an attachment, three for a sound), and a stamp's name set in a face and fitted to
        its frame. The FDF import and `AddAnnotation`'s stamp without a picture, which also
        had no appearance, use it. `drawn_appearance_test.rs` imports each subtype without
        `/AP` and checks it draws, and was shown to fail with the generator answering
        nothing. A page of twenty-two was rendered and looked at. **Not drawn**: a line's
        caption (`/Cap`), whose text needs a face and a place Table 178 describes in
        figures.
  - [x] **AA-4c** — XFDF export and import of annotations, by Tables 33 and 34. The import
        draws each annotation through AA-4b and matches by name as AA-3 does.
        **Done, 2026-10-10**: `PdfDocument::export_xfdf` and `Operation::ImportXfdf`,
        reached from `fepdf edit fdf-export` (to a `.xfdf`) and `fdf-import` (by the
        file's first character), `fepdf-mcp`'s `export_xfdf` / `import_xfdf`, and the
        comment drawer. `inreplyto` names an annotation in the file or already on the
        page. What XFDF has no place for — a free text's `/CL` and `/RD`, a stamp's
        picture, `/Path` — is a `Decision` on export. `xfdf_test.rs`: the round trip, a
        second import, a hand-written file by Table 34, a file attachment's bytes, the
        refusal; the round trip found `/IRT` read resolved, and so lost, which it now is
        not. Links and projections are left out, as from FDF.
- [x] **AA-4d** — **every annotation shown**: opening a document gives each annotation with
      no appearance the one it can be given, and records it
      ([ADR-0120](docs/adr/0120-a-document-is-opened-with-every-annotation-given-an-appearance.md)).
      Seventeen subtypes from their entries, widgets from their fields, Movie, 3D and
      RichMedia a frame and a sign; PrinterMark, TrapNet and Watermark none, recorded. A
      Screen with none is left so, as 12.5.6.18 says it shows nothing.
      Measured 2026-10-10: 11 widgets in `sample_02c.pdf` and 9 annotations in isartor and
      veraPDF files had none.
      **Done, 2026-10-10**: `fepdf_doc::apply::give_missing_appearances`, called by
      `PdfDocument::open_with_options` before the seal. `sample_02c.pdf` opens with eleven
      `REPAIRED 12.5.5` decisions, one a widget, and draws as it did, its fields being
      empty; an empty field is given an appearance with nothing in it and no face looked
      for. `opened_appearance_test.rs` holds a text value drawn, a check box's two states,
      the media stand-ins and the printer's mark said, and fails without the call.
      `parser_twin_test` now ignores the number of a face the engine made while opening,
      which differs between the two readers by how many objects each allocated first; and
      `an_annotation_with_no_appearance_is_skipped_quietly` became two tests, a link still
      skipped and a square now drawn.
- [x] **AA-4e** — **every subtype read**: the eleven of Table 171's twenty-eight with no
      reader of their own entries get one — Text, FreeText, Line, Ink, Sound, Screen,
      PrinterMark, TrapNet, 3D, Projection, RichMedia.
      **Done, 2026-10-10**: a reader each in `fepdf-model/src/annotation.rs`, Projection
      excepted, whose table adds nothing to the markup entries (12.5.6.24); and Stamp,
      Redact, Polygon and Caret, which read part of theirs, read all of it.
      `every_subtype_of_table_171_reads_its_own_entries` names one key of each and fails
      without the reader.
- [x] **AA-4f** — **every subtype written but the deprecated and the 3D**: the ten
      `AddAnnotation` cannot make yet — Polygon, PolyLine, Caret, FileAttachment, Screen,
      Popup, PrinterMark, Watermark, Redact, Projection. Sound, Movie and TrapNet are
      deprecated in PDF 2.0 and are not written; 3D and RichMedia are shown, not made
      (the owner, 2026-10-10).
      **Done, 2026-10-10**
      ([ADR-0122](docs/adr/0122-an-annotation-whose-appearance-is-its-content-is-drawn-only-when-made.md)):
      eight `AnnotationKind` variants and two `ShapeForm`s, written by
      `fepdf-doc/src/apply/kinds.rs`. They are drawn by `drawn` from their entries, or by
      `drawn::made` where the appearance is the content. MCP `add_annotation` names all
      ten. `made_annotation_test.rs` makes each, checks its entries, links a popup both
      ways and refuses what would draw nothing; it fails without the back-link and with a
      screen signed as markup.
- [x] **AA-4g** — **the ten new kinds as pens in the window**: a polygon clicked point by
      point, an attachment or a screen's clip chosen from disk, a popup for a comment
      picked in the drawer, and a printer's mark and a watermark from the drawer.
      **Done, 2026-10-11**: `fepdf-gui/src/annotate/more.rs`. A polygon or polyline is
      finished by a double-click, Enter or the drawer's button, and Escape starts again.
      Its points belong to the pen that clicked them. A watermark fills the box dragged.
      `scripts/dev/annotation-pens.txt` draws all of them with the pointer and keys, and
      was looked at. Its traces found three things:
      - Switching pens kept the last pen's points.
      - egui counts a brisk double-click on the last point as a triple-click, which
        finished nothing.
      - A 12-point watermark could hardly be seen.
      `dblclickat` was added to capture plans for the second, since `dblclick` sends no
      pointer event. Not checked by gesture: choosing a file, which is a system dialog.
- [x] **AA-5** — **a PDF made from images**: JPEG carried as it is, PNG with its alpha as
      a soft mask, and each page of a TIFF as a page. This is an operation, so it also
      inserts image pages into an existing document.
      **Done, 2026-10-11**
      ([ADR-0123](docs/adr/0123-a-picture-becomes-a-page-at-its-own-size.md)):
      `Operation::InsertImages` and `PdfDocument::from_images`, in
      `fepdf-doc/src/apply/pictures/`. Each page is its picture's size at its resolution,
      or a named sheet. Reached from `fepdf edit images` (`--into`, `--at`, `--sheet`), MCP
      `images_to_pdf`, and the window, which opens a picture as a new document and
      inserts pictures through its insert dialog. `image_pages_test.rs` makes its own
      JPEG, PNG and TIFF files. It checks a JPEG's bytes through a save, an EXIF-turned
      page, Adobe CMYK, a soft mask and an opaque alpha with none, sixteen bits and
      `pHYs`, a TIFF's pages without its thumbnail, a one-bit WhiteIsZero scan, a sheet,
      and the refusals. Each was shown to fail with what it guards removed.
      `PdfDocument::to_bytes` was added for the window, which opens what it has just
      made.
- [x] **AA-6** — **a PDF made from plain text**: lines broken by UAX #14 at the face's
      advances, pages broken where they fill, and each paragraph the input separates with
      a blank line tagged `/P`. Paragraphs are declared by the input, not inferred
      ([ADR-0091](docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
      **Done, 2026-10-11**
      ([ADR-0124](docs/adr/0124-plain-text-is-set-by-its-own-lines-and-paragraphs.md)):
      `Operation::InsertText`, `PdfDocument::from_text` and `PdfDocument::plain_text`, in
      `fepdf-doc/src/apply/typeset/`, with `unicode-linebreak`. Reached from
      `fepdf edit text`, MCP `text_to_pdf`, and the window, which opens a `.txt` as a new
      document and inserts one through its insert dialog. `text_pages_test.rs` covers:
      - paragraphs by blank lines, each `/P` in its language;
      - lines inside the margins, English, Japanese and an unbroken word;
      - a paragraph over two pages, and a form feed;
      - a second text joining the tree;
      - the refusals.
      Each was shown to fail with what it guards removed. The constitution's first page,
      set from its text, audits with no PDF/UA-2 error when given a title.

### Phase AB — What 2.0 deprecates, translated as the table says

[docs/specs/pdf-2-0-deprecations.md](docs/specs/pdf-2-0-deprecations.md) gives each of the
34 features ISO 32000-2 deprecates a rule — load, interpret, save, keep, never written —
and what the engine does now. AB-1 measured fifteen rows as gaps on 2026-10-11 (count the
table's **Gap** marks); AB-2 closed one, and with it a blend-mode array that was not
applied at all.

- [x] **AB-1** — **the probe, as a test.**
- [x] **AB-2** — **a blend-mode array** (11.6.3): `BlendMode::from_pdf_object` takes the
      first mode it recognises, or `Normal`; and at load an array, and `Compatible`,
      become one name.
- [x] **AB-3** — **at load**: an image or form XObject's `/Name` (#14), `checked` as
      `Checked` (#31), a structure element's `/R` and the integers in `/C` and `/A` (#30 —
      the two readers, `audit_tree.rs` and `apply/structure.rs`, skip them already), and
      `/Trapped` moved into the packet as `pdf:Trapped` (#1).
- [x] **AB-4** — **on save**, beside `drop_subset_claims`: web capture's five keys,
      `/AlternatePresentations`, the `/ViewArea` family, `/Suspects`, `/SeparationInfo`,
      OPI, the TrapNet annotation (taken out of `/Annots` too), a file specification's
      platform strings when `/F` or `/UF` is there, and a 3D stream's `/Resources`.
- [x] **AB-5** — **`/NeedAppearances` from the file** (#21): an appearance built for each
      widget that lacks one, then the entry dropped.
      ([ADR-0126](docs/adr/0126-need-appearances-is-done-as-it-asks-and-then-dropped.md),
      [ADR-0120](docs/adr/0120-a-document-is-opened-with-every-annotation-given-an-appearance.md))
- [x] **AB-6** — **the rows not measured**: signatures (#23–26), FDF encryption,
      Adobe-Japan2 and Adobe-Korea1, permission bit 10, and a Link element enclosing
      several annotations.
- [x] **AB-6b** — **permission bit 10, always set** (#4, Table 22).
- [x] **AB-7** — **a descendant font's decisions, recorded once.**
- [x] **AB-8** — **what Phase AB had left unmeasured, and its condition's second half.**
- [x] **AB-9** — **FDF encryption, measured** (#22).
      ([ADR-0125](docs/adr/0125-form-values-come-in-by-name-and-make-no-field.md))

### Phase AC — What 2.0 asks of a 1.x file beyond what it deprecates

Opened on 2026-10-12 by what Phase AB's Arlington audit found that was no deprecation.

- [x] **AC-1** — **a form XObject that draws with its caller's resources** (8.10.2).
      ([ADR-0127](docs/adr/0127-a-form-is-given-the-resources-its-callers-lend-it.md))
- [x] **AC-2** — **what a review of AC-1 found.**
      ([ADR-0127](docs/adr/0127-a-form-is-given-the-resources-its-callers-lend-it.md))

---

*This file was cut to what each entry delivered twice: on 2026-09-22, from 4,924 lines
to 2,966, and on 2026-09-28, from 4,029 to 2,231; and on 2026-10-13 the closed Phases Y,
Z, AB and AC were cut to one line an item. Both cuts follow the
argument of
[ADR-0039](docs/adr/0039-the-design-document-was-narrating-its-own-corrections.md): an
account of how a thing went wrong is a record, and a record belongs in `docs/adr/`.
**Git holds the rest.** The full text before the second cut is at commit `406dd21`.
Every item ID and every phase name is kept, so a reference from code or a record still
lands.*
