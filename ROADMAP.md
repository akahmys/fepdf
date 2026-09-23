# fepdf Roadmap

> **Not a rule.** This is the work: what was measured, what was built, what is next.
> The rules are in [CODING.md](CODING.md); a claim here that contradicts the code is a
> defect in this file. Two colour defects sat in it as "open" for weeks after they were
> closed.

**Goal**: an engine that understands ISO 32000-2 (PDF 2.0) semantically — not merely
one that round-trips it. PDF 1.7 and earlier are **read-only** targets; output is
always 2.0.

That goal is not a predicate, and no run of `status.sh` can report it true or false;
every phase below states a completion condition, and the line they are all under had
none. Phase I gave it the nearest thing that can be measured: **the share of the
constructs a corpus actually presents whose contents this engine reads** — 96% over the
nine samples and 88% over all 524 files, after Phase K took the catalogue axis from 5 of
20 to 19 of 20 and Phase O-1 took it to **21 of 22** by fetching a corpus that presents
two more. What is left *on those axes* is 29 annotation entries, on subtypes that occur once or
twice; one filter; and `/Type`. The corpus doubled and the figure moved by one point,
which is the property it exists to have: a bigger denominator is the honest direction.

**And the axes are not the whole of it**, which Phase P is the record of. Three axes were
chosen because their denominators can be enumerated from a file; colour spaces, shadings
and functions cannot be, and are exactly where the engine was found rendering the wrong
picture. A number that only counts what it can count says nothing about the rest, and this
paragraph read as though it did. `fepdf inspect coverage` reports it and
[ADR-0019](docs/adr/0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md)
records what it is not: a proxy, silent about whether what was read was read correctly,
and bounded by a corpus that flatters an engine when it presents little.

That distinction sets the work. Round-trip fidelity already holds: the arena preserves
objects it has no typed view of. Measured across all nine samples by comparing
`inspect catalog` on the input with `inspect catalog` on the output — no catalogue key
is ever lost, and the entries the engine cannot read the contents of come back with the
same shape — `intel_sdm.pdf` carries eleven, of which nine are `Option<Object>` fields,
and its `/PageLabels` is a one-entry dictionary on both sides. Only two
differences appear, both by design: `/Metadata` is *added*
where the source had none, because output always carries XMP, and object numbers are
renumbered, because saving produces a new document
([ADR-0012](docs/adr/0012-saving-produces-a-new-document.md)). `bokutokitan.pdf`'s
page-tree root loses its `/MediaBox` and each page gains one, which is inheritance
resolved into the single normalised state
([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)), not a dropped key.
Understanding these entries is what remains. This paragraph used to assert the same
thing "verified key-for-key" while citing `ViewerPreferences` and `AcroForm` as untyped
examples, by which time both were typed and no test checked the verification — so
`crosscheck_selfread.sh` now makes the comparison, and the claim is the measurement
rather than a rewording of it.

---

## The subsets this processor has chosen

**PDF 2.0 has no "conforming reader", and says so.** 6.3.2.1 replaces it with a rule:
each PDF processor chooses which subsets of PDF functionality to support, and shall
comply with the applicable provisions for the ones it chose. The NOTE beside it explains
why — PDF/A, PDF/UA and the rest define a conforming reader *because* they restrict
processing, and 2.0 is too general for the notion to be useful.

That makes "full support" definable rather than meaningless: choose every subset and
comply with every provision. It is worth knowing what that costs before deciding not to.
The whole document carries **5,891 `shall`** (331 of them `shall not`), 414 `should` and
1,329 `may`; clause 2 pulls in **81 other documents** whose content *constitutes
requirements of this one* — among them ISO 14739-1 (PRC), ECMA-363 (U3D),
ISO/DIS 21757-1 (ECMAScript for PDF), XFA 3.3, the PostScript Language Reference,
SMIL 3.0, MathML 3.0 and the four ETSI CAdES/PAdES parts. "Implement PDF 2.0" is
"implement eighty-one documents", which is why 6.3.2.1 provides the escape and why every
implementation in the world takes it.

```bash
target/release/fepdf inspect text docs/specs/ISO_32000-2_sponsored-ec2.pdf > /tmp/iso.txt
grep -o '\bshall\b' /tmp/iso.txt | wc -l          # 5891
```

So this section is the escape, taken deliberately and written down. It is not new policy:
**"Not planned" and "Read broadly, write 2.0" below have been the subset declaration all
along**, and this only says so in the standard's vocabulary — which is what makes it
checkable rather than a matter of taste.

| Subset (6.3.1) | Chosen | What it commits this project to |
| :--- | :--- | :--- |
| **PDF reader** — interpret a document to display, print or extract data | **yes**, broadly | Reading 1.x as well as 2.0, including files that are wrong in recoverable ways (ADR-0003). This is the subset the coverage index measures. |
| **PDF processor providing rendering** (6.3.2.2) | **yes** | Two `shall`s: render the page contents as defined, and render the appearance stream of every annotation that has one unless its flags say otherwise; and respect the optional content definitions. Both are met — the second only since [ADR-0023](docs/adr/0023-a-renderer-that-skips-annotation-appearances-is-not-conforming.md). **Phase P is what is chosen and not yet complied with.** |
| **PDF writer** (6.3.2.1) | **yes**, for 2.0 only | Output shall conform, and nothing 2.0 deprecates is written — the rule that settled encryption at AES-256 R6 ([ADR-0015](docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)) and, later, made a field value build its appearance instead of setting `/NeedAppearances`. Writing 1.7, and amending a file this engine did not produce, are **not** chosen ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)). |
| **Interactive PDF processor** (6.3.2.3) | **yes** | 6.3.1 makes anything that interacts with a user while processing a file one of these, which `fepdf-gui` is; 6.3.2.3 requires support for all the interactive aspects of optional content and decision logs. Both are met — `/OCProperties` layer hierarchy and toggle controls redraw scenes interactively, and `doc.decisions()` surfaces reading/repair decisions in the GUI. |
| **ECMAScript actions** (12.6.4.17) | **yes**, for document and field scripting | Taken 2026-08-22 ([ADR-0026](docs/adr/0026-the-engine-takes-the-ecmascript-subset-because-it-already-owes-it.md)). Scope: 12.6.4.17's execution, and of ISO/DIS 21757-1 the objects form scripts reach for (`app`, `this`, `Field`, `event`, `util`, `color`). Engine: boa ([ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)). Shape: a fifth frontend over `Operation` ([ADR-0025](docs/adr/0025-a-script-processor-is-a-frontend-not-a-subsystem.md), [ADR-0032](docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md)). Phase R. |
| **Multimedia** (13.2–13.7) | **no** | 13.4 is deprecated in 2.0; PRC and U3D are two more standards. |
| **XFA** (Adobe XFA 3.3) | **no** | Deprecated in 2.0, and a second form model beside the one that works. |

**One constraint cuts across all of these**: nothing in this workspace may compile C
(RR-15 Rule 9, [ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)).
It is not a subset of PDF and the standard says nothing about it, but it decides *how* a
subset can be taken — which is why the ECMAScript question resolved to boa rather than
QuickJS, and why three dependencies left the engine before the rule could be written down.
`fepdf-model` went from 234 transitive crates to 149 in the process, and two of the three
were used by no line of code.

**What a declaration is for.** `/Requirements` (12.11) is how a *document* says which
subsets it needs, and `inspect actions` reports the ones this processor does not satisfy.
The table above is the other side of that exchange: without it, "this processor does not
do this" is an assertion in a source file; with it, the two can be compared. The one
requirement type named so far is `EnableJavaScripts`, in `actions::NOT_SATISFIED`.

**What this table is not.** It is not a claim that every provision of the PDF standard is
implemented — only that **all chosen subsets are met** (subsets not chosen, such as
multimedia and XFA, are formally declared as non-conformance boundaries per 6.3.1).
A subset declaration is what makes that distinction sayable: "not implemented" and
"chosen and not complied with" are different, and only the second is a defect.

This sentence read **three**, and named rendering as the third, until 2026-08-30. Phase P
met it — 6.3.2.2's two `shall`s are both kept, which the clause 10 row above states — and
the count was never brought down. `status.sh` counts the rows carrying the marker now, so
the sentence and the table cannot drift apart again without a row disagreeing.

**Taking a subset therefore creates a defect where a conforming refusal stood**, and the
ECMAScript row is the case. That is not an argument against taking it. It is what deciding
looks like when the declaration is honest: the alternative was to leave the question
undecided and call the resulting silence a choice, which is what
[ADR-0026](docs/adr/0026-the-engine-takes-the-ecmascript-subset-because-it-already-owes-it.md)
found had happened.

**How a row gets decided.** Not by demand — a capability that does not exist has no users,
and Phase L's rule already says a corpus is grounds for building and never for declining.
The test is the one 6.3.2.1 supplies when read as a test: **a subset is required when the
engine has already undertaken work whose correctness depends on it.** Multimedia and XFA
survive that test as refusals — nothing here depends on them, and both are deprecated in
2.0. ECMAScript did not.

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
| **9** Text | Extraction loses **1,137 glyphs of 16,321,270**, from 85,982, and all of them are named. [↓](#9-text) |
| **10** Rendering | Both of 6.3.2.2's `shall`s met. `[/ICCBased …]` colours go through their profile (10.3); `/DeviceCMYK` without one follows 10.4.2.1, which 8.6.4.4 leaves device-dependent. [↓](#10-rendering) |
| **11** Transparency | Blend modes, constant alpha and soft masks all reach the backend. [↓](#11-transparency) |
| **12** Interactive features | Read and written. 15 entries have no reader: eleven are clause 13, which is declined, and four are `/Redact` keys written on a `/Stamp`, which no table defines. [↓](#12-interactive-features) |
| **13** Multimedia | Declined, and not a gap: 13.4 is deprecated in 2.0 and reading it would be building for a subsystem the standard is retiring. The corpus does carry it — `/3D` ten times, `/Movie` five, `/RichMedia` three — which changes the premise and not the refusal. |
| **14** Document interchange | Marked content and logical structure are read and acted on. [↓](#14-document-interchange) |
| **14.3** Metadata | Settled at load into one state. [↓](#143-metadata) |

The rows that link downward are expanded below, one section each. **They were table
cells until they were not**: clause 9's ran to 6,578 characters on a single line, which
is a paragraph no diff can show and no reader can scan — and is how two colour defects
stayed listed as open long after Phase P closed them.

### 7.4 Filters

**Nine of the ten** since Phase M built `CCITTFaxDecode`, `JBIG2Decode` and `JPXDecode`
— the tenth is `Crypt`, which the security layer handles (7.6). `inspect structure`
takes a census of which filters a file's streams name, so this row is re-derivable per
file rather than remembered. Across all 524 files: `/FlateDecode` 485 files,
`/DCTDecode` 21, `/XXXDecode` 8, `/JPXDecode` 3, `/LZWDecode` 3, `/CCITTFaxDecode` 2,
`/ASCIIHexDecode` 1, `/JBIG2Decode` **still none** — Phase O-1 doubled the corpus and
did not produce one, which is what leaves O-2 open. Every stream carrying a codec this
engine lacks is an image. Every filter that is a plain byte transformation decodes.
`FlateDecode`, `LZWDecode`, `ASCIIHexDecode`, `ASCII85Decode` and `RunLengthDecode`
decode, with Table 8's predictors reaching LZW as they do Flate, and `DCTDecode` reads
JPEG; `Crypt` is handled in the security layer (7.6). `ZstandardDecode` **was**
implemented and is not one of the ten — nor is it anywhere in ISO 32000-2, which is what
removing it turned up: zero occurrences in 1020 pages, no file of the 530 carrying its
magic number, and a heuristic testing every stream's first four bytes for it
([ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)). Table 6's
abbreviations are matched too — `/AHx` appears seven times in one external file, and
only `Fl` and `DCT` were recognised before. The three image codecs were absent until
Phase M, declined by Phase L on a measurement that could not speak to the use case that
reopened them; an image that still will not decode is skipped rather than aborting the
page, and says what it cost. **This row did not exist until a corpus this project did
not choose forced it.**

### 7.5 File structure

**Complete and in use.** Header scan, both cross-reference forms, `/Prev` chains, hybrid
references, object streams, incremental updates, and recovery by scanning.
`Document::open` reads the file itself; `lopdf` is gone. Recovery has **two** halves
since Phase N: a scan finds objects written `N G obj`, and an object stored inside a
`/Type /ObjStm` is not written that way — so every container in the file is expanded
too, filling holes and never overriding a section that read
([ADR-0006](docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md)). Having
only the first half cost `UnknownFilter-xrefstm.pdf` its page tree, which is object 5
inside object stream 2.

### 7.6 Encryption

Every password handler the standard defines now decrypts: RC4 (V1/V2), AES-128 (V4/R4)
and AES-256 (V5/R5, V5/R6) to Algorithms 1, 2, 2.A, 2.B and 4–6, with `/Perms` checked
and both password roles authenticating. Verified against PDFKit on fourteen files; all
of it was broken or absent
([ADR-0009](docs/adr/0009-permissions-are-thirty-two-bits-not-a-positive-integer.md)).
Writing is AES-256 at revision 6 and nothing else, because output is always 2.0 and this
edition deprecates the rest. Public-key handlers (7.6.5) are **read and written** — a
`/Adobe.PubSec` document opens with the certificate it was addressed to, which neither
Chrome nor Firefox will do, and `--encrypt-to` produces one. Unencrypted wrappers
(7.6.7) are recognised and reported. **Clause 7.6 is otherwise complete.**

### 7.7 Document structure

Every one of Table 29's 32 entries is a field of `PdfCatalog`, and **23 are modelled** —
the field's type says what the entry holds. Measured against all 524 files, 10 of the 32
keys occur in no file at all and are **declined a reader** for that reason, recorded in
the code as `catalog::ABSENT_FROM_BOTH_CORPORA`; of the twenty-two keys those files do
carry, **21 are modelled**, and the one that is not is `/Type`, whose value 7.7.2 fixes
at `/Catalog`. **Two keys left that list in Phase O-1** — `/AF` went from zero files to
seventeen when PDF/A-3 and PDF/A-4f arrived, and `/PieceInfo` to one — and both gained
readers in the same change, which is the rule working: a corpus justifies building,
never declining. That figure is qualified where it is printed: `inspect catalog`
reports, per entry, how much of the entry's *own* table its reader covers — `/AcroForm`
is modelled and reads 4 of Table 224's 8
([ADR-0020](docs/adr/0020-a-modelled-entry-reports-how-much-of-its-own-table-it-reads.md)).

### PDF 2.0 additions

**Re-derived against the 524, which this row said had not been done.**
`CatalogReport::survey` over every file of both corpora, 0 unreadable. The counts it
carried were from the 251-file corpus and four of the eight moved: `/OutputIntents` **64
→ 188**, `/OCProperties` **1 → 5**, `/PageLabels` **4 → 5**, `/AF` **0 → 17** (which the
row already knew), `/Threads` **1**, and `/Collection`, `/DSS` and `/DPartRoot` still
**0 of 524** — declined for want of a file, and that is now measured rather than
assumed. `/OutputIntents` nearly tripling is the one that matters: it is the second most
common non-required entry in the corpus after `/Outlines` and `/Metadata`, and the row
was quoting a figure a third of the truth. **The row's own description is stale in the
other direction too.** It said the six "each became an `Option<Object>` field … a field
whose contents are opaque"; every one of them that any file carries reports `Modelled`
now. The three the corpus does not carry are the three still waiting for one.

### 8 Graphics

**Optional content (8.11) is honoured while drawing.** `BDC` discarded its property
list, so an `/OC` section was painted whether its group was on or off, and
`/OCProperties` — read since Phase K — was consulted by nothing. The default
configuration's `/BaseState`, `/ON`, `/OFF`, `/Intent` and `/AS` are read, with
membership dictionaries, all four `/P` policies and `/VE` expressions. Thirteen
constructions were put to PDFKit and it honours **two**, so eleven are held to the
clause by 30 tests
([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md));
two of those exist because Phase O-1's corpus caught the reader out on a real file. A
pattern is painted, through `Paint::Pattern(PatternSpec)` — `scn` with a pattern name
(8.6.8.2) had been read as a grey component, which cost `fy05.pdf` six pages and the 718
after them. **The two colour defects this row called open are closed**, and it went on
saying they were open after Phase P closed them: `/Separation` and `/DeviceN` (8.6.6)
evaluate their tint transform, and a shading (8.7.4) samples its `/Function` at 33
points instead of reading `/C0` and `/C1`. The evaluator (7.10) carries all four
`/FunctionType`s, and the checklist entry that built it holds the PDFKit comparison — a
separation at full tint went from `254 254 254 254` to `0 254 254 254` against PDFKit's
`25 255 255 255`. Shading types **2, 3 and 4 to 7** are read, the mesh types into
triangles
([ADR-0030](docs/adr/0030-a-mesh-is-flattened-and-its-triangles-are-grown.md)). **Type
1, the function-based shading, is the one that is not**, and both ways into it — `sh`
and a shading pattern — record a violation naming the resource rather than painting
nothing quietly. **What none of this rests on is the corpus.** Across all 524 files:
`/FunctionType 0` seven times, `4` twice, `/Separation` twice, `/DeviceN` once,
`ShadingType 2` three times, `ShadingType 1` once, and **`/FunctionType 2` and `3` not
at all** — the stitching gradient this row used to describe as broken does not occur in
any file here. The evidence is the fixtures Phase P built for the purpose and PDFKit
beside them, which is worth saying plainly rather than leaving a reader to assume a
corpus stands behind it.

### 9 Text

**Re-measured, and then the cause was found two layers below where the row looked.** It
read *"every page of every sample yields its text"*; 228 of 7727 pages yielded none, and
counting pages hid the rest — a page that yields *some* text looked extracted. Counting
glyphs, the loss was **85,982 of 12,305,585**, `bokutokitan.pdf` losing 63% of its
65,102. **It is 43,123 now, and `bokutokitan.pdf` has no lossy page at all**: page 5
extracted as `濹東綺譚「絶好…猟奇的…` with every kana missing and now reads 「絶好のチャンスですぜ。猟奇的ですぜ」.
Three defects, each hiding the next. **A path**: the CID tables were found through a
*relative* default, so the same document extracted differently by working directory and
not at all for any consumer of the crate — one root now (`fepdf_font::resources`),
anchored to `CARGO_MANIFEST_DIR`, `FEPDF_RESOURCES` exclusive. **A repository**:
`external/adobe-cmaps` is Adobe's *CMap Resources*, which its own README calls
unidirectional — Unicode to CID. Extraction needs the other direction, which Adobe
publishes as `mapping-resources-pdf`; the code asked for `Adobe-Japan1-UCS2` **by that
exact name** and, not finding it, synthesised a table from the encoding CMaps' own
columns that reached 15,443 CIDs of 23,060. The 7,617 it missed were ordinary text: CID
12506 is の. **A parser**: `load_named` called `parse_recursive`, which chases `usecmap`
and nothing else, so **every CMap this engine loaded by name was empty** — 18 MB read
and parsed into nothing, which is why the synthesis was the only thing that ever
produced data. Fixing it also corrected multi-byte splitting: `fy05.pdf` decodes 688,388
glyphs where it decoded 806,252, and loses 3,913 where it lost 5,527. **What remains is
reported rather than guessed at**: of the 43,123, 39,233 reach no route, 2,801 are named
and then discarded by the private-use and circled-number filter — and 1,089 reach the
encoding and come back empty. **2,801 was 2,801 until the withholding was asked what it
was withholding**: 2,753 of them were circled numbers and 48 were private use.
Adobe-Japan1 defines 128 CIDs that *are* circled numbers and they carry no Shift-JIS and
no EUC encoding at all, so only a Unicode-based route reaches them — and every route
that can produce `U+2460` is authoritative, since the ASCII guess cannot reach past
`U+007E`. They are kept now. What they spell in `fy05.pdf` is 注⑵, a footnote marker that
was being deleted from the extracted text in three Japanese documents at about three a
page. An empty extraction no longer means two things. **Runs separated along a line are
words now**: the backend inserted a newline when text moved vertically and nothing when
it moved sideways, so a table's cells arrived glued — `RegionsLabels and
symbolsSpecification` where PDFKit reads `Regions Labels and symbols Specification`. A
`TJ` array delivers one call per element, so most gaps between calls are kerning and a
careless threshold puts spaces inside words; measured across a table page, two prose
pages and a page of vertical Japanese, kerning reaches 0.055 em at the 99th percentile,
every real separation is at least 0.15 em, and **nothing falls between 0.15 and 0.25**.
A quarter of an em sits in that empty band. Prose is unchanged and the table page now
matches PDFKit exactly. **`/ActualText` is read now (14.9.4), and it was the whole of
the `.notdef` row.** The 2,106 glyphs this engine could not name were one font on eight
pages of `volvo_xc90.pdf`, all of them character code `0x0000`: the file draws its
Chinese and Thai regulatory notices as `.notdef` and puts the real characters in the
`/Span <</ActualText …>>` around them. Page 389 carries 393 such spans — 169 CJK, 205
Thai, 17 punctuation — and PDFKit reads exactly 169 CJK scalars from it. The interpreter
had the property list all along and **flattened every inline dictionary to
`Object::Null`** on its way to the optional-content code, which is all `/OC` needs from
one and none of what 14.9.4 puts there. **volvo loses 0 glyphs of 718,262 now**, from
2,106, and the corpus 38,264 of 16,321,270 at that point. It also fixes a defect that was never
counted: codes whose `/ToUnicode` says `<0000>` were emitting `U+0000` into the
extracted text, so `R/7713/19` came out as `R⟨NUL⟩7713⟨NUL⟩19` — not an empty string, so
never tallied as a loss, and not a character anyone can read either. **The claim that 24
of `fugaku.pdf`'s 25 pages are legitimately blank was wrong in this row and is gone**:
the glyph count was right and the conclusion was not, because those pages carry 2,622
`/ActualText` spans. They extract now — badly, one character per span and mostly
punctuation, which is the document's own quality and not this engine's to improve. **The
base encodings of Annex D were reaching the CMap loader, which is a different kind of
thing.** A simple font's `/Encoding` may name one (9.6.6.1); every such name went to
`CMap::load_named`, which searches Adobe's *CMap Resources* — CJK character collections
that have never contained an Annex D table. `/WinAnsiEncoding` therefore resolved to
nothing and left the font with **no encoding at all**, so every code the ASCII guess
could not reach came back unnamed. That was 36,914 glyphs of `intel_sdm.pdf`, whose
1,600 font references all declare it, and the losses were ordinary punctuation: 8,563 em
dashes, 6,558 bullets, 12,295 curly quotes, 4,940 `®`. **`intel_sdm.pdf` loses 48 now**,
and the corpus 1,398 of 16,321,270 at that point. Adding the table also exposed a trap it had been
hiding: a mapping's value carried either the text or `/glyphname`, told apart by the
leading slash, and `U+002F` **is** a character — the first run of the new table lost
41,058 glyphs to its own solidus, and a `/ToUnicode` mapping a code to one would have
been losing it all along — the corpus carries two such mappings, in `volvo_xc90.pdf`,
and no drawn glyph reached either, which is why nothing showed until a table named every
ASCII character. Solidi now match PDFKit end to end, 878 against 878 there and 41,116
against 41,116 in `intel_sdm.pdf`. A name token has something after its slash, which is
the test now. `MacRomanEncoding` and `StandardEncoding` are deliberately still absent —
nothing in the corpus names one, and a table written from memory against no document is
how a wrong entry gets in unnoticed — but a document that names one now records a
violation instead of silently losing its text. **The 1,398 were two populations and not
one, and asking which route each reached separated them.** 261 reached *no route at all*,
and they were one font: `fy05.pdf` sets its title in `RyuminPr6N-Heavy`, which declares
`/Registry (Adobe) /Ordering (Japan1)` and got no Adobe-Japan1 table, because the engine
decided a font's character collection by looking for `mincho`, `gothic`, `koz` and six
more substrings in `/BaseFont` while the file said so outright. Two defects, the first
hiding the second: `/Registry` and `/Ordering` are **strings** (9.7.3, Table 114) and were
read with a name accessor, so 116 of 116 Type0 fonts across both corpora returned `None`;
and the collection was read only on the `Type0`, while the interpreter decodes through the
descendant `CIDFont`, which carries its own (Table 115) and was never asked. It is read
from the file now, on both, and the name heuristic is kept only where the file declares
`Identity` or nothing — 75 fonts, where it is the only thing to go on. Page 1 of
`fy05.pdf` reads 令和5年度決算検査報告 instead of nothing, the corpus loses **1,137 of
16,321,270**, and the `unmapped` route is 0 across the nine samples
([ADR-0041](docs/adr/0041-a-character-collection-is-declared-not-guessed.md)). **The guess
was also wrong in the other direction**: 19 fonts of the external corpus declare
`Adobe-Korea1` or `Adobe-China1` and carry `Gothic` in their name, so the *Japanese* table
was applied to them — Adobe-Japan1 puts `フ` at CID 16128 where Adobe-Korea1 puts `췎`. One
such glyph is drawn in the corpus; it is unnamed now, under a 9.7.3 violation naming the
collection, and PDFKit reads no text from that page either. **The other 1,089 will not go
down and the record says why.** They are `/Differences` names `c033`–`c039` in 127
subsetted Type1 fonts of `fy05.pdf`; PDFKit reads them as `!"#$%&'`, and rendered they are
the 圏点 above ため池 and the corner and extension pieces of brackets stretched over five
lines of a table. Adopting the second implementation's reading would put 1,089 spurious
ASCII characters into one document's text
([ADR-0042](docs/adr/0042-a-glyph-name-that-looks-like-a-character-code-is-not-one.md)).

### 10 Rendering

6.3.2.2 binds anything that draws a page with two `shall`s, and both are met: optional
content is honoured (8.11) and **annotation appearance streams are drawn** (12.5.5) —
the second was not done at all until Phase P found the clause, and a page whose only
mark was an annotation came out blank while every other reader painted it
([ADR-0023](docs/adr/0023-a-renderer-that-skips-annotation-appearances-is-not-conforming.md)).
What the clause's own subclauses ask for is thinner: the **PDF function evaluator
(7.10)** now exists — types 0, 2, 3 and 4 — which fixed clause 8's two measured colour
defects, though `/DeviceCMYK` to RGB is still not colour managed. **10.5 and 10.6 are
declined on their own clauses**: 10.6.1 exempts continuous-tone devices from halftoning
and this engine is one, and `/TR`/`/TR2` are deprecated in 2.0, which 3.15 defines as
"should be ignored by a PDF processor". 10.7's scan conversion is Vello's — including
the antialiasing seam between adjacent mesh triangles, which is a scan-conversion
artefact rather than a shading one and is answered by growing each triangle half a
device pixel. 10.8's separations follow the colour gap. Phase P.  **Two of
`volvo_xc90.pdf`'s 415 pages came back entirely blank, and they render now.** Pages 10
and 389 rasterised to a fully transparent buffer at 96 DPI while rendering correctly at
half that — all-or-nothing, and dependent on resolution rather than content. **The cause
was a clip drawn as a blend.** `VelloBackend::push_clip` called `Scene::push_layer`,
which asks for a blend layer, where `push_clip_layer` exists and asks for a clip; blend
memory is consumed per nesting level and is the one resource `Scene::bump_estimate` does
not model, so nothing warns before the GPU runs out. The correlation is exact: every
page that rendered has clip depth 1, and the two that did not have depth 6 and 7. Scene
size was not it — page 388 estimates 12.0 MB and renders, page 389 estimates 7.7 MB and
did not. A clip is a clip, and saying so is the whole fix. **The guard stays**:
`base_color` is opaque, so a completed render cannot leave a transparent pixel anywhere
and a fully transparent buffer is proof the rasteriser stopped — `render_to_texture`
reports `Ok` either way, and `render_page_to_file` wrote that buffer to a PNG and said
nothing.

### 11 Transparency

**Re-measured, and then built.** Blend modes (11.3.5) and constant alpha (`/ca`, `/CA`)
reached the backend and were pinned by `transparency_test.rs`; a soft mask did not.
`/SMask` in an `/ExtGState` was read into the interpreter's state and used by nothing —
the write had no reader and `RenderBackend` had no entry point — so a `/S /Luminosity`
mask whose group paints solid black, which 11.6.5.2 makes 0 everywhere, left the content
it covered at full strength with an empty log beside it. **160 of them in
`volvo_xc90.pdf` alone**, across its 415 pages. **They are applied now.** `SoftMaskSpec`
carries `/S`, `/BC` and `/TR` as one thing — a function from a position to an alpha — so
the interpreter brackets the content, replays the group in the matrix `gs` was executed
under, and hands the backend the spec; which parts of it can be honoured is the
backend's question. Vello's `push_luminance_mask_layer` takes the plain case exactly,
and **all 160 of `volvo_xc90.pdf`'s masks are the plain case** — 0 use `/S /Alpha`,
`/BC` or `/TR`, so the branch that cannot express them fires nowhere in the corpus and
is recorded rather than approximated where it would. **Transparency groups (11.6.6) are
still not read**: a form carrying `/Group << /S /Transparency /I true /K true >>`
produces backend calls identical to the same form without the entry, so isolation and
knockout are absent rather than approximate, and are recorded when `/I` or `/K` asks for
something — 0 firings on the samples.

### 12 Interactive features

Read through `inspect interactive`, and signature fields (12.7.5.5, 12.8) can be written
and checked. Named destinations (12.3.2) **resolve**, through both of 12.3.2.3's forms
and the name tree (7.9.6) one of them needs — which found a link in `intel_sdm.pdf` that
goes nowhere, `(G3.7717)`, referenced three times and declared in none of that file's
279,501 destinations. `samples/` exercises one annotation subtype — all 29,973 of its
annotations are `/Link` — and this row said that of *the corpus* for as long as there
was only one; the 515 external files carry 125 annotations across **18** subtypes and
twelve terminal form fields. `PdfAnnotation` held seven entries of Table 166 and no
`/AP`, which made a `/Redact` and a `/Watermark` the same object; Phase J took it to all
nineteen. **29 distinct entries across the remaining subtypes still have no reader**,
each on an annotation that occurs once or twice. All 30,098 annotations parse. **What a
document *does* is read** (`inspect actions`, 12.6): every place an action can hang,
what it lets the document do, and whether the reader has to touch anything first — which
found the only two files of 524 that run code on open
([ADR-0022](docs/adr/0022-what-a-document-does-is-a-settled-question-where-reads-an-action-is-not.md)).
**Setting a field value builds the appearance** (12.7.4.3) instead of writing
`/NeedAppearances`, which 2.0 deprecates; a form declaring a calculation order is told
its ECMAScript was not run — **a `Violation` of 12.6.3, and the sentence that decided
ADR-0026**, because it is this engine reporting that it undertook form editing and
cannot finish it — and `/Requirements` (12.11) reports the subsets a document asks for
that this processor does not deliver. `EnableJavaScripts` is the one name on that list,
and since 2026-08-22 it is there as **chosen and not yet met** rather than as a refusal.

### 14 Document interchange

**Re-measured against the 524.** Marked content (14.6) and logical structure (14.7) are
read and acted on, and the corpus says how much that covers: `/MarkInfo` and
`/StructTreeRoot` each in **142 files**, which is the same 142 — a tagged file carries
both. Associated files (14.13) **17**, page-piece dictionaries (14.5) **1**, both
`Modelled`. **Two subclauses are absent from every file of both corpora**: `/SpiderInfo`
(14.10, web capture) and `/Legal` (14.11) are **0 of 524**, so declining them is a
measurement now and not an omission — the same test ADR-0026 applies to everything else.
What this row still does not say is how deep the walk goes: presence and support level
were measured, the UA-2 audit's own coverage was not, and `fepdf inspect` reports per
entry how much of its own table it reads
([ADR-0020](docs/adr/0020-a-modelled-entry-reports-how-much-of-its-own-table-it-reads.md))
which is the figure to derive next.

### 14.3 Metadata

Settled at load into one state: `/Info` and the metadata stream are reconciled,
disagreements recorded, and the entries 14.3.3 deprecates moved to where that clause
puts them ([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)). Text
strings decode to 7.9.2.2 — PDFDocEncoding from Annex D, or a byte order mark — after a
Shift-JIS detector was found corrupting a conforming `/Title`. `--strip` removes every
metadata stream, not the catalogue's alone.

One measurement worth carrying forward: all **30** `Operation` variants are fully
implemented and verified — 24 until Phase Q enforced Rule D, which turned six of the
facade's mutating methods into operations and left `apply` as the only way to change a
document. In `fepdf-model` and `fepdf-syntax` the `log::warn!` count is
down from 14 to one, and that one is deliberate: it reports which fonts *this machine*
has, not anything the document says.

**That sentence used to say "in the engine", and the engine is bigger than those two
crates.** The figure across the whole engine is **16**, of which **3 are deliberate** —
each a property of the *host* rather than the document. The other thirteen sit in
`fepdf-content` (8, one discarding *"Unknown or unhandled operator"* to stderr),
`fepdf-font` (3) and `fepdf-render` (2), and each is a conclusion about the document, which
§4.3 makes a `Decision`. The `status.sh` row searched two crates and put the other three in
**neither** list, so it could not have said so and adding sites to them moved nothing; it
now derives the engine as every crate that is not a frontend. Phase P for the conversions,
Phase Q for the row. Frontends still log freely, which is their job.

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

## Phase B — Read before write *(complete; `inspect encryption` landed in Phase C)*

Semantic completeness starts with being able to *see* a feature. `inspect` began with
four commands — `info`, `audit`, `text`, `tree` — against roughly fifteen clauses, and
nothing reported encryption, interactive features, or file structure. It now has eight,
covering 7.5, 7.6, 7.7.2 and clause 12, with the decision log on all of them.

- [x] `inspect structure` — file layout: sections, updates, object streams, and the
      decisions taken while reading. Text, JSON and Markdown; reads the bytes rather
      than a normalised `Document`, so it reports the file as written

- [x] `inspect catalog` — every entry, typed or not, so gaps are visible. Which
      entries are *typed* is derived from `PdfCatalog`'s `#[pdf_key]` attributes
      rather than listed again, so the report cannot drift from the struct

- [x] `inspect interactive` — annotations by subtype, form fields walked through
      `/Kids`, actions by `/S`, and the outline as total, visible and declared. No
      sample carries a form field, so that walk is held by a hand-assembled fixture

- [x] `inspect encryption` — done in Phase C, once there was something correct to
      report on. Handler, revision, key length, cipher from `/CFM`, crypt filters,
      `/P` decoded bit by bit, and **what this engine does with it**

- [x] Surface `DecisionLog` in every output format, not only `audit` — and
      structured, not stringified: the audit had been flattening every decision to
      `Warning` regardless of the severity the engine assigned

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

- [x] AES-128 (V4/R4) actually decrypts. It did not: `/P` was read as a positive
      integer, failed to convert, and `unwrap_or(0)` fed a different key into
      Algorithm 2, so the one encrypted sample decrypted to noise and `publish
      upgrade` wrote that noise out
      ([ADR-0009](docs/adr/0009-permissions-are-thirty-two-bits-not-a-positive-integer.md))

- [x] User-password validation (Algorithm 6). A wrong password used to open the
      document and report 29,438 font failures; it is now refused

- [x] `inspect encryption` — handler, revision, key length, cipher, crypt filters,
      Table 22 decoded, and a conformance verdict per file rather than per declaration:
      a document can declare AES-256 and be unreadable, which is the case the report
      exists to make visible

- [x] RC4 (V1/V2), and `/V 4 /CFM /V2`. `build_handler` matched only `(4,4)` and
      `(5,5|6)`, so every pre-AES file was refused; `is_aes` was set `true` at both
      construction sites and no path could clear it, so a crypt filter naming RC4 was
      decrypted as AES. Test data comes from `scripts/test/make_encrypted.py`, which
      implements Algorithms 1–5 independently

- [x] AES-256 R5/R6 to Algorithm 2.A, with 2.B transcribed from 7.6.4.3.4. The old
      derivation invented salts from `/ID` and returned a handler for **any** password,
      so the file opened and decrypted to noise. `/Perms` is checked (step f), and both
      the user and owner passwords authenticate

- [x] Owner-password validation — Algorithm 2.A tries `/U` then `/O`, so an owner
      password opens a document whose user password is unknown

- [x] `/P` handling settled: **reported, never enforced**. It is readable without a
      password, is not cryptographically bound to any operation, and 7.6.4.1 puts
      obeying it at `should`. Refusing would over-read a soft declaration; the defect
      was that writing *erased* it in silence. Now recorded as a violation at write
      time, and only under user access — an owner password carries the right to change
      the permissions. The `save_*` methods return `Vec<Decision>` so the compiler asks
      every caller what it intends to do with them; the GUI shows them after saving,
      which is the only moment they are actionable

- [x] Owner-password authentication for revisions 2–4 (Algorithm 7), which the access
      distinction needs and which 7.6.4.1 requires regardless: either password should
      open the document

- [x] SASLprep (RFC 4013) on passwords, which 2.A step (a) requires — NFKC and the
      two mapping tables, applied in `fepdf-model` so the byte layer stays free of
      Unicode tables. Its prohibited-output and bidi checks are not implemented: they
      *refuse* passwords, and refusing one a conforming reader accepts is the failure
      being fixed. Measured on a fixture whose `/U` stores the normalised form of a
      ligature — PDFKit opened it and fepdf did not

- [x] Digital signatures (12.8), both directions. `publish sign` wrote
      `/SubFilter /adbe.pkcs7.detached` with 8,192 zero bytes for `/Contents` and a
      `/ByteRange` of four constants; `verify-signature` passed an empty slice to a
      validator that returned success for every document including unsigned ones. Both
      now do the work. Signing is a two-pass write — the signature covers the file
      except itself, so the writer reserves `/ByteRange` and `/Contents`, records where
      it put them, then states the range, hashes what it names and fills the hole.
      A caller may not supply either field: one that could state a byte range could
      state a wrong one. `/SubFilter` is `ETSI.CAdES.detached`, which required adding
      the `signing-certificate-v2` attribute ETSI EN 319 122-1 defines, because the
      subfilter is a claim to be CAdES. The field is invisible — `/Rect [0 0 0 0]`, no
      `/AP` — since a widget with a rectangle and no appearance stream is a box viewers
      draw empty. A signed file may also be encrypted; a signed or encrypted file may not
      be linearized, and says so. **Scope**: fepdf signs only what it wrote itself, so the byte range is
      over its own output ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)).
      Verification reports coverage apart from the verdict, because appending after a
      signature leaves it valid over the part it covers — and says what it did *not*
      check: no trust store, no validity window, no revocation.
      `scripts/test/crosscheck_signature.sh` requires openssl and fepdf to agree on all
      nine samples, and proves it can fail by changing a byte

- [x] Encrypting on write, **AES-256 revision 6 only**. `--password` claimed to encrypt
      and produced a plaintext file: nothing called `set_security_handler`, so
      `encrypt_stream` was unreachable. It encrypts now. This engine reads five schemes
      because files exist that use them, and writes one
      ([ADR-0015](docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)):
      output is always PDF 2.0, and 7.6.4.1 deprecates RC4 and the Algorithm 2
      derivation in that same edition.
      `SecurityHandler` could only authenticate against an `/Encrypt` that already
      existed; `encrypt_new` generates a key and runs Algorithms 8, 9 and 10 to make one.
      `--permissions` is un-hidden with it, taking the keywords `inspect encryption`
      prints, from one table so the two directions cannot disagree. Giving no owner
      password is recorded rather than defaulted in silence, because `/P` then restricts
      nobody who can open the file. Verified by `scripts/test/crosscheck_encryption.sh`:
      PDFKit opens all nine and reads the same text as the plain save

- [x] Public-key security handlers (**7.6.5**, not 7.6.4 as this line read until it was
      checked against the standard; 7.6.4 is the *standard* security handler) — **reading**.
      `--recipient-certificate` and `--recipient-key` open a `/Adobe.PubSec` document. The
      key is derived from a 20-byte seed unwrapped from a CMS `EnvelopedData`, digested
      together with every `/Recipients` entry in order, which is what binds the key to
      the recipient list. `/KDFSalt` is in the same dictionary and is *not* key material —
      it belongs to PDF 2.0's document MAC. `/Recipients` lives in the crypt filter for
      `/V` 4 and 5, not at the top of `/Encrypt`. Verified backwards, because there is
      nothing to compare against: pdf.js rejects any non-Standard `/Filter`, PDFium
      handles only Standard, and qpdf documents that it does not support this. So an
      independent producer makes the file and fepdf has to get the plaintext back —
      pyHanko's output and `make_pubsec.py`'s both read byte-identically to the plaintext
      they were made from. **Writing** is `--encrypt-to <cert.der>`, repeatable for more
      recipients; only the certificate is needed, since encrypting to someone uses their
      public half. Both directions share one derivation function, because two copies of a
      key derivation agree until somebody edits one and the failure is a document only
      this engine can open

- [x] Unencrypted wrapper documents (7.6.7) — recognised and reported, which is all
      the clause can ask of a reader: the payload is encrypted by a handler *this*
      standard does not define, so naming the missing filter is the service. Each of
      the clause's conditions is reported separately, met or not, because a producer
      that gets four of five right has still said what filter is needed

- [x] A corpus of encrypted files as regression tests — five, built independently:
      RC4 40- and 128-bit, AES-256 at revisions 5 and 6, and one with distinct user and
      owner passwords. `scripts/test/aes.py` is a pure-Python AES checked against
      FIPS-197, so the fixtures do not depend on the engine they test

- [x] Explain the 93 characters `fy05.pdf` loses through a round trip. It was 93
      *pages*, five of them losing all their text, because the refinement pass
      synthesised a `/ToUnicode` keyed on glyph ids for a `CIDFontType0`
      ([ADR-0010](docs/adr/0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md))

- [x] Explain what `fy05.pdf` gains through a save. Every operand was padded to six
      decimal places, so `1` went out as `1.000000`; PDFKit read the padded spelling to
      glyph origins a thousandth of a point away and moved its line breaks on 78 of 846
      pages. Trimming the zeros takes the whole corpus to a zero delta — the first time
      `crosscheck_roundtrip.sh` has reported no difference on any file

- [x] Output larger than input. Not two images, as this line first read: nothing was
      compressed, because `SaveOptions` derived its default and `compress` was `false`
      while `fepdf-gui` had always set it `true` — the same operation, two answers, which
      is what Rule D exists to stop. `fy05.pdf` goes from +76% to −46%

- [x] Write object streams (7.5.7), with the cross-reference streams (7.5.8) they
      require. `SaveOptions::obj_stm` was carried and read by nothing; `--obj-stm` now
      packs. `intel_sdm.pdf` keeps 323,066 of its objects in 8,044 containers and went
      from **+131% to +1%**; every other sample shrank too — `volvo_xc90` +1% to −13%,
      `unicode_16` −3% to −14%. The two are one switch because a classic cross-reference
      table has no type 2 entry, so it cannot say where a packed object lives. Four
      things stay loose: streams, generation-non-zero objects, `/Encrypt`, and this
      engine's own addition — the signature dictionary, whose `/Contents` is a hole at a
      byte offset. **Packed by default** since a second independent reader was obtained
      and agreed: PDFium — Chrome's engine, sharing no code with PDFKit — reads the same
      text out of the packed file page by page on all nine, and opens a packed *and*
      encrypted one with the password
      ([ADR-0016](docs/adr/0016-objects-are-packed-by-default.md)). `--no-obj-stm` writes
      the loose form, which is what to reach for when debugging the writer

- [x] Implement or delete every `SaveArgs` option that did nothing. All five are
      decided. `--permissions` came live with encryption on write; `--lang` writes
      `/Lang` (14.9.2.1) and `--copyright` writes `dc:rights`. `--image-quality` and
      `--diff` are deleted: the first is a feature wearing a flag — decode and re-encode
      every `DCTDecode` image, generation loss on something already lossy — and the
      second printed "Structural diff would be displayed here (M67 enhancement)" for an
      operation that is not an option on writing a file, and that
      `examples/compare_documents.rs` already does properly. ADR-0007 asked for exactly
      this audit and named `SaveArgs` as the place it had not been done

- [x] Make the content round trip a fixed point. It was not: `W n` came back as
      `W n n` and grew by 52 bytes on every pass, while `W f` came back as `W n f` and
      lost the fill outright
      ([ADR-0011](docs/adr/0011-the-content-round-trip-must-be-a-fixed-point.md))

- [x] Settle what a save *is*: it produces a new document derived from the input, not
      an edit of it ([ADR-0012](docs/adr/0012-saving-produces-a-new-document.md)). The
      revision chain is merged at load — fy05's three sections become one — so there is
      no history to preserve by the time anything is written. Origin is recorded in
      `xmpMM:DerivedFrom` and `xmpMM:OriginalDocumentID`; what the source carried and
      the output cannot is reported at write time

### The corpus is now three files, and that is why the defects surfaced

`scripts/test/make_encrypted.py` builds RC4 fixtures from `samples/sample.pdf`,
implementing Algorithms 1–5 from the standard with nothing but `hashlib`. Generating
them with fepdf's own cryptography would have tested it against itself; PDFKit reads
both fixtures and extracts the same 12,120 characters as the unencrypted source, so the
generator is right and any disagreement is the engine's.

Round-tripping the whole corpus through `publish upgrade` and reading the output with
PDFKit is now a standing check. It found the one thing internal comparison could not:
`fy05.pdf` was losing whole pages of text to a `/ToUnicode` the engine synthesised for
it ([ADR-0010](docs/adr/0010-a-synthesised-tounicode-keyed-on-glyphs-destroys-text.md)).
All seventeen files now come back with their text intact, at a zero delta.

### Why the corpus item is not optional

One encrypted file exercises the whole clause, and for as long as its content decrypted
to noise every internal check passed: it opened, its page count matched PDFKit's 1,140,
its objects counted the same, and `publish upgrade` reported success. The comparison in
`examples/compare_documents.rs` could not have caught it either — it compares two fepdf
reads, and both were the same noise.

What caught it was reading the file with something else. `scripts/dev/status.sh` now
asserts text comes out of that sample, because asserting it *opens* passed throughout.

## Phase C′ — The hole in the cross-checks

Not a clause. A method gap, found three times in one day, each time the same shape:
**an independent reader was asked and this engine was not.**

| Found | Independent reader said | This engine | Since |
| :--- | :--- | :--- | :--- |
| Encryption through object streams | PDFKit read it | could not read its own output | reachable for one day, latent longer |
| `inspect text` stopping at the first bad page | PDFKit read 846 | reported 127 and exited non-zero | at least the 2026-08-10 rename |
| `scn` with a pattern name (8.6.8.2) | PDFKit read the page | six pages failed | at least the 2026-08-10 rename |

"At least" because `git log -S` stops at the rename that touched every file; the repository
begins 2026-04-11, so the true answer is somewhere in that four months and is not worth
the archaeology. The first defect is different in kind: the reader has always expanded
object streams before decrypting, and packing by default is what made that reachable.

`crosscheck_roundtrip.sh` measures text with PDFKit on both sides of a save, so it
answers "did writing lose anything" and cannot answer "can this engine read what it
just wrote". The other three cross-checks inherited the shape: `crosscheck_objstm.sh`
asked PDFKit whether a packed *and encrypted* file was readable and PDFKit said yes,
correctly — the writer was right the whole time, and nobody asked the reader.

Three from one gap is enough to expect a fourth.

- [x] `scripts/test/crosscheck_selfread.sh` reads every produced file back with this
      engine and compares against the same engine's reading of the input — 21 states
      across packing, both encryption handlers and signing. No second implementation,
      so `status.sh --full` runs it

- [x] Combinations rather than features, which is what the matrix is for: injecting the
      encryption-through-object-streams defect fails exactly the four *packed and
      encrypted* states and leaves the loose ones green

- [x] **A comparison cannot see a symmetric defect**, which injection found rather than
      reasoning: with the `scn` defect put back, every combination still compared equal,
      because the reader loses the same pages on both sides. Two of the three defects
      were that shape. The check therefore also asserts that every page of every sample
      extracts at all — the exit status the comparison discards — and *that* is what
      catches them

## Phase D — The catalogue and PDF 2.0 features

Only now do the 19 stub operations become worth implementing, because reading exists
to verify them against.

- [x] Type the remaining catalogue entries (all 32 of ISO 32000-2 Table 29's entries
      are now strongly typed with `#[pdf_key]` mappings in `PdfCatalog`).

- [x] `PageMode`, `PageLayout` and `Lang` — 10 of 32 typed becomes 13. The two name
      entries are enums with an `Other(String)` arm: their value sets grew in 1.5 and
      1.6, so a file may carry a name newer than this code, and folding that to a default
      would invent an answer where keeping it loses nothing. `Lang` closes an asymmetry
      made in the same session it was created — `--lang` wrote it and nothing read it

- [x] `ViewerPreferences` — 13 of 32 typed becomes 14, and the largest of these entries:
      Table 147's eighteen keys plus four name enums. **Every field is an `Option`,
      including the five booleans the table defaults to `false`**, because a document that
      says nothing must not come back stating a viewer's policy as its own — and
      `fy05.pdf` carries an *empty* `/ViewerPreferences`, which under defaulting would
      read identically to a producer who had deliberately written those five. Only two of
      the eighteen keys occur in the corpus (`DisplayDocTitle` in four files, `Direction`
      in one); the rest are typed anyway because Table 147 is one dictionary of scalars,
      not a subsystem, which is exactly what `DSS`, `AF` and `DPartRoot` are not.
      `PdfDocument::viewer_direction` no longer walks the raw dictionary for one key, and
      `Document::catalog()` now exists so the next entry has somewhere to be read from

- [x] `Dests` — 14 of 32 becomes 15, and measuring first turned one catalogue entry into
      a feature. `ROADMAP.md` had it as "one file", true of the *entry*: only
      `volvo_xc90.pdf` carries a catalogue `/Dests`, 651 destinations. But 12.3.2.3 gives
      named destinations a second form, the `/Dests` name tree under `/Names`, and
      `intel_sdm.pdf` declares **279,501** there with 25,946 links resolving through it.
      Typing the entry alone would have covered 651 of 280,152. So: `Destination` over
      Table 151's eight forms, name-tree walking (7.9.6 — nothing in the workspace had
      any), and resolution of both forms, which are separate lookups because the standard
      keeps them in separate places and the corpus supplies one file of each

- [x] Found by it, which is the point: `intel_sdm.pdf` references `(G3.7717)` three times
      and declares it nowhere. One broken link in a 5,000-page manual, and nothing in this
      engine could have said so before. `inspect interactive` now names it

- [x] Implement operations in order of how much of the standard they unlock:
      catalogue edits (`UpdateOutlines`, `SetOutputIntent`, `UpdateLayers`,
      `SetPageLabels`) before page elements (`AddAnnotation`, `SetFormFieldValue`)
      before content synthesis (`ApplyBatesNumbering`, `AddPageDecoration`)

- [x] Un-hide each CLI subcommand as its operation lands

- [x] Decide the fate of the operations no frontend reaches; an unreachable operation
      is a maintenance cost without a user (all operations implemented and verified)

- [x] `color_policy` is the last ingestion option nothing reads, and `status.sh` counts
      it. ADR-0007's terms apply: implement the colour validation it was meant to govern,
      or delete the option and the enum. Clause 8.6 colour space validation in active
      refinement now actively reads `color_policy`, un-hiding `--relaxed-color`

### Tooling debt carried from Phase C

- [x] `scripts/test/make_pubsec.py` reads the PDF's cross-reference table directly to
      locate every in-use object, avoiding stream byte false positives and unreferenced objects.

- [x] `scripts/test/make_pubsec.py` accelerates AES encryption for large payloads via OpenSSL.

## Phase E — Structure, once the contents exist

Deferred deliberately. Splitting `fepdf-doc` out today would produce a crate that owns
the operation vocabulary while 79% of it is hollow — the shape of the mistake in
[ADR-0001](docs/adr/0001-resource-resolution-stays-in-the-model.md).

- [x] `fepdf-content`: move the interpreter beside the contract it already drives.
      The content stream interpreter and its operator handlers now live in `fepdf-content`
      alongside `RenderBackend`, with `fepdf` providing clean re-exports.

- [x] `fepdf-doc`: extracted and separated from `fepdf`.
      Owns the `Operation` vocabulary (all operations implemented and active),
      structural mutations, logical structure tree visitor, Matterhorn PDF/UA-2 auditor,
      and remediation engine.

- [x] `fepdf` as its own crate — renamed from `fepdf-sdk`, completing the target topology
      ([ADR-0005](docs/adr/0005-layering-rules-are-enforced-by-cargo.md)).

## Phase F — Deep Architectural & Structural Robustness Hardening

Addressing structural edge cases, resource exhaustion guards, and semantic cross-system invariants:

- [x] **Tagged PDF Structure Tree Integrity on Page Deletion** (`fepdf-doc`):
      Automatically decouple / prune dangling `/Pg` page handle references from `/StructTreeRoot`
      when pages are deleted, and verify `/Pg` validity in Matterhorn PDF/UA-2 audit.

- [x] **Content Stream Stack Depth Limits & DoS Defense** (`fepdf-content`):
      Enforce `MAX_GSTATE_STACK_DEPTH = 64` on `q`/`Q` and `MAX_MARKED_STACK_DEPTH = 64` on
      `BMC`/`BDC`/`EMC` to prevent recursion and heap exhaustion attacks.

- [x] **Upfront Precondition Validation for Mutation Operations** (`fepdf-doc`):
      Validate page indices, ranges, and target handles upfront before mutating arena state,
      guaranteeing operation atomicity.

- [x] **Fallback Font Metric Bounds Heuristics** (`fepdf`):
      Provide safe non-zero advance width heuristics for text spans when font `/Widths` are missing.

## Phase G — Measured against files this project did not choose

Every "zero occurrences in the corpus, so defer" judgement above is bounded by the nine
files in `samples/`, and this project picked all nine. `scripts/test/fetch_external_corpus.sh`
brings in 242 it did not — 37 from `pdf-association/pdf-differences`, where real
implementations legitimately disagree, and 205 Isartor files, each breaking one specific
clause with the clause in its filename. `scripts/test/measure_external_corpus.sh` runs
the engine over them, in release and in debug, and counts what fails.

The first run, on an engine whose every roadmap box was ticked, and where it stands now:

| | first run | now |
| :--- | ---: | ---: |
| files | 242 | 242 |
| opened | 240 | **242** |
| **panicked** | **1** | **0** |
| refused with a message | 1 | 0 |
| every page extracted | 233 of 240 | 241 of 242 |
| written back | 240 of 240 | 242 of 242 |

- [x] The panic. `get_index_item` in `fepdf-font` read a CFF INDEX with every offset
      unchecked, and `isartor-6-3-2-t01-fail-b.pdf` named an item one byte past the end:
      *"the len is 37458 but the index is 37458"*. The function already returned `Option`
      and every caller already handled `None`, so bounds-checking each read was the whole
      fix. That file now opens and reports what it actually is — a font program in no
      recognised format, skipped, with a system font substituted, which is what Isartor
      6-3-2 exists to test. **Phase F is titled "structural integrity and DoS stack
      limits" and is ticked; nine files could not produce a panic and 242 produced one
      immediately.**

- [x] Clause 7.4, the four that are plain byte transformations. Ordered by measurement
      rather than by the clause: across both corpora `ASCIIHexDecode` occurs in 3 files,
      `LZWDecode` in 3, `ASCII85Decode` in 1 and `RunLengthDecode` in **none**. The last
      is built anyway, and the departure is worth naming — the rule against building what
      nothing reaches is about *containers*, and this is a leaf function with a fixed
      definition, no dependants, and three siblings from the same clause that the corpus
      does exercise. Table 6's abbreviations are matched as well, which was a second gap:
      `/AHx` occurs seven times in one file and only `Fl` and `DCT` were recognised

- [x] The LZW test that was not a test. Injecting "ignore `/EarlyChange`" left every
      end-to-end case passing, because the worked example in 7.4.4.2 is nine bytes long
      and never reaches a code-width boundary — the clause's own vector is vacuous about
      the parameter most likely to be got wrong. The boundary logic is tested directly
      now and the injection fails it. Three of the hand-written expectations in the same
      test file were also wrong while the decoder was right, so the `ASCII85Decode` table
      is generated from an unrelated implementation instead

- [x] The three image codecs, **not built, and the line above them was wrong about why
      they mattered.** It read as though `CCITTFaxDecode` and `JPXDecode` were what stopped
      those files yielding text. Measured: in all four the filter is on an `/XObject
      /Subtype /Image`, never on a content stream — so decoding one produces pixels and no
      text at all. One of the four uses `/XXXDecode`, a filter invented for the test suite,
      which settles it: no codec will ever fix that file. What blocked the text was that a
      failing image aborted the content stream and took the page's real text with it, and
      an image is now skipped instead. Four files recovered without a line of codec.
      `JBIG2Decode` occurs zero times in either corpus. All three remain unbuilt and are
      still gated on rendering those images mattering — which is what the original line
      said, for a reason it did not have

- [x] `Object Handle<Object>(8) is not a dictionary`, from
      `UnknownFilter-Linearized.pdf` — the message Phase A closes by saying the reader no
      longer produces. The file is linearized and its **first** cross-reference stream is
      `/Filter /XXXDecode`, so the trailing section read fine and the leading one did not.
      A section that failed to read was dropped by an `if let Ok(..)` that said nothing,
      and the fallback scan only ran when the records were *empty* — which they were not.
      The file lost its catalogue and eleven other objects, all of them physically present
      in the bytes, and `inspect structure` reported "read without departing from the
      standard". The loss is now a `Decision` naming the offset and the filter, and a scan
      fills the holes the surviving sections do not cover. It **never overrides** a section
      that was read: a scan cannot tell a current object from a superseded one lying
      elsewhere (ADR-0006), so where a readable section has an answer that answer stands.
      The file opens, and PDFKit still cannot open it at all

- [x] `NegativeFontSize.pdf` extracted nothing and reported `Other("No font")`. Neither
      guess in this line was right: the negative size is read fine, and the font does not
      fail to resolve — there was **no font selected at all**. Six of the file's twelve
      runs choose their font through an `ExtGState` `/Font` (Table 57), which is
      `[font size]` with an indirect reference rather than a resource name, and the
      interpreter read `ca`, `CA`, `BM` and `SMask` from `gs` and ignored `/Font`. The
      reference now lives in the *text* state, so `q` and `Q` save and restore it as they
      must, and `Tf` and `gs` each clear the other. PDFKit read 327 characters from that
      page and this engine read none; it now reads all twelve runs

- [x] Decided. `measure_external_corpus.sh` exits non-zero **only on a panic**, and that
      is the whole verdict it is entitled to: most of this corpus is deliberately
      malformed, so refusing a file and saying why is a correct outcome and a refusal
      count is information. It sits in `TESTING.md`'s checklist to be run when the reader,
      the fonts or the filters are touched, against the debug binary as well as the
      release one, and it is **not** in `status.sh --full` because it needs a fetched
      corpus and the network. The counts it prints go in this table, where a regression
      shows up as a disagreement

## Phase H — A decision the interpreter takes is still a decision

`ARCHITECTURE.md` §4.3 says a departure from the standard is recorded rather than
logged, and one place in the engine cannot honour it. `ops/xobject.rs` skips an image
that will not decode and reaches `log::debug!`, because `Interpreter` holds `&Document`
and `DecisionLog::push` needs `&mut`; the comment there says so, which is better than
hiding it and is not the same as fixing it. The shape of that defect has already been
paid for once — `UnknownFilter-Linearized.pdf` lost its catalogue and eleven objects
while `inspect structure` reported "read without departing from the standard".

- [x] The log is reachable from `&Document`. `DecisionLog` holds
      `Mutex<Vec<Decision>>`, `push` takes `&self`, and `Document::record` is how the
      interpreter reaches it. `entries()` returns a snapshot rather than a borrow, because
      a caller holding a guard while a page is interpreted would deadlock against the
      interpreter recording into it. The alternative — returning the decisions from
      `render_page` and `extract_text` — was the SDK-wide signature change the comment
      declined to make, and it lands a content-level departure somewhere `inspect
      structure` will not print it ([ADR-0018](docs/adr/0018-interpreting-a-page-can-add-to-the-decision-log.md))

- [x] What that costs is recorded: `is_conforming` answers "no departure **in what has
      been examined**". It always did — `isartor-6-3-2-t01-fail-b.pdf` reports nothing
      under `inspect structure` and a 9.9 `Violation` under `inspect text`, because one
      of those loads fonts — and the change is that the partiality is stated instead of
      unnoticed. `inspect text` prints the two apart, reading before the text and
      interpreting after it

- [x] `status.sh` searches `fepdf-content` as well, and the row moved 53 → 56. It
      searched `fepdf-model` and `fepdf-syntax` only, so it would have reported this
      phase as having changed nothing — a measurement blind to what it measures

- [x] A `/Filter` census in `inspect structure`, taken by walking the arena's **streams**.
      grep cannot take it: searching both corpora for `CCITTFaxDecode` finds zero files
      where the census finds two, because the name sits inside a `/FlateDecode`d object
      stream. The first version walked dictionaries instead, on the reasoning that only
      a stream carries `/Filter`, and two files reported a filter called `/Standard` —
      the security handler of Table 20. Across 251 files: `/FlateDecode` 224 files,
      `/DCTDecode` 12, `/XXXDecode` 8, `/JPXDecode` 3, `/CCITTFaxDecode` 2,
      `/LZWDecode` 2, `/ASCIIHexDecode` 1, and **`/JBIG2Decode` none**. Every stream
      carrying a codec this engine lacks is an image: JPX 3 of 3, CCITT 2 of 2

## Phase I — Give the goal a completion condition

The line above every phase — "an engine that understands ISO 32000-2 semantically" — is
not a predicate, and `status.sh` closes by saying so. Phases A–G each stated what *done*
meant in terms a run could check; the sentence they sit under never did, so "is it met"
has no answer rather than a negative one.

What can be checked is narrower, and naming it as narrower is the point. Of the
constructs the two corpora actually present, how many does the engine read the
*contents* of? The denominator is measured rather than enumerated from the standard —
across 251 files, 20 of Table 29's 32 catalogue keys, 16 annotation subtypes, 2 of the
four field types and 13 action kinds — so a construct that never arrives can neither
raise the figure nor lower it. Arlington's machine-readable model
(`external/arlington/tsv/latest`, 613 object definitions) says what
each key is supposed to hold, which makes the numerator a comparison rather than a
self-assessment.

- [x] Defined, per axis, in `fepdf-model::coverage`, and reported by `fepdf inspect
      coverage`. Three axes have a denominator the engine can enumerate from a file
      without judgement — catalogue entries (7.7.2), annotation entries per subtype
      (12.5) and stream filters (7.4). Actions (12.6) are the obvious fourth and are
      left out on purpose: "reads an action" has no settled meaning here, and an axis
      whose numerator is a judgement call is one the figure can be argued into

- [x] [ADR-0019](docs/adr/0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md)
      records what the number is **not** — a proxy, silent about whether what was read
      was read *correctly*, and bounded by a corpus that flatters an engine when it
      presents little

- [x] `status.sh --full` prints it, naming which corpus it was measured over, and the
      `Next` section points at the command instead of explaining why there is none. Not
      in the default view: it is a minute over `samples/` alone, 47 seconds of which is
      `intel_sdm.pdf` surveyed three times, and that view is meant to be instant

- [x] The container test is a test, not a paragraph:
      `a_construct_no_file_carries_counts_in_neither_direction` asserts that a key the
      file does not carry is in neither the numerator nor the denominator. `/DPartRoot`
      has a field, occurs in none of the 251 files, and appears in neither

## Phase J — Read the interactive features the corpus does present

The premise this was deferred on has expired, and the row above said so for longer than
it was true. `samples/` carries 29,973 annotations of which every one is `/Link`; the
242 external files carry 82 across **16** subtypes — `Link` 29, `Popup` 18, `Circle` 12,
`Movie` 5, `Stamp` 4, `Widget` 4, and one each of `3D`, `Caret`, `FileAttachment`,
`PolyLine`, `Polygon`, `Redact`, `Screen`, `Sound`, `Watermark` and
`SomePrivateCustomAnnotationType`, which is not a subtype the standard defines and is
worth keeping visible for that reason. Four foreign files carry a terminal form field
each — `isartor-6-3-4-t01-fail-f` (`/Btn`), `isartor-6-9-t01-fail-a` (`/Tx`, with
`/NeedAppearances true`) and `isartor-6-9-t02-fail-a` and `-b` (`/Btn`) — so the form
walk is no longer exercised only by a fixture and by this engine's own signature field.

So the gap is not that nothing reaches this code. It is that `PdfAnnotation` reads seven
entries of Table 166 and nothing else: no `/AP`, no subtype-specific entry. A `Redact`
and a `Watermark` are the same object to this engine, distinguishable only by the name
it counted them under.

- [x] `PdfAnnotation` reads Table 166 entire — nineteen entries where it held seven —
      including `/AP` as a modelled [`Appearance`], which keeps "one appearance stream"
      apart from "a set of states with `/AS` selecting one". `/F` became the flags of
      Table 167 rather than an integer, `/C` a colour whose *length* decides its space,
      `/Border` the array of Table 168. One of the seven was not being read at all:
      `kind` carried no `#[pdf_key]`, so the macro looked for `/kind`, and every
      annotation in both corpora reported `/Type` as an entry with no reader

- [x] Subtype-specific entries, in the order the corpus presents them, **stopping where
      the corpus stops saying anything**: `/Link` (30,002), `/Popup` (18), `/Circle`
      (12), `/Movie` (5), `/Stamp` (4) and `/Widget` (4) are every subtype either corpus
      writes more than once, and each has a reader. The other ten occur exactly once
      each and get none — a sample of one is not a reason to build a type. Table 172 is
      read for all nineteen markup subtypes at once, which is where `/T`, `/Popup`,
      `/Subj` and `/CreationDate` live

- [x] The form walk reads `/V`, `/DA`, `/Ff`, `/T` and `/Kids`, and `/FT`, `/Ff`, `/V`
      and `/DA` are **inherited** down `/Kids` as 12.7.4.2 requires, so a kid stating
      none of them is no longer a field of no type. Fully qualified names are assembled
      on the way down. `/Ch` and `/Sig` occur zero times outside this engine's own output
      and get no reader

- [x] Written down, and it changes what more corpus would be *for*. Of Table 166's
      nineteen entries, **five are never written by any of the 30,055 annotations** —
      `/OC`, `/AF`, `/ca`, `/BM`, `/Lang`, which is every 2.0 addition plus optional
      content — and four of Table 172's nine are absent too (`/IRT`, `/RT`, `/IT`,
      `/ExData`). Thirteen of the 28 subtypes never appear at all: `/Text`, `/FreeText`,
      `/Line`, `/Square`, `/Highlight`, `/Underline`, `/Squiggly`, `/StrikeOut`, `/Ink`,
      `/PrinterMark`, `/TrapNetwork`, `/RichMedia`, `/Projection`. The four form fields
      are flat — no `/Kids` hierarchy exists in either corpus, so inheritance is
      exercised only by the fixture. `pdf-association/pdf20examples` stays the candidate
      and is **not fetched yet**: what it would buy is now a list rather than a hope

## Phase K — The catalogue's contents, in the order the corpus asks for them

Thirty-two keys are fields and six model their contents; the other 26 are the subject of
[ADR-0017](docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md). Modelling
all 26 is not the work, because 12 of them occur in no file of either corpus, and
building a reader for those is the container-before-contents shape Phase D was ordered
to avoid. Across 251 files, 20 of the 32 keys occur at all:

| Occurrences | Key | State |
| ---: | :--- | :--- |
| 251, 251 | `Pages`, `Type` | declared |
| 219, 182 | `PageMode`, `PageLayout` | modelled |
| 217, 210, 208 | `Outlines`, `Metadata`, `OpenAction` | declared |
| 64, 34 | `OutputIntents`, `Names` | declared |
| 7, 5, 1 | `ViewerPreferences`, `Lang`, `Dests` | modelled |
| 5, 4, 4, 4, 3 | `AcroForm`, `MarkInfo`, `PageLabels`, `StructTreeRoot`, `Version` | declared |
| 1, 1, 1 | `AA`, `OCProperties`, `Threads` | declared |
| **0** | `Extensions`, `URI`, `SpiderInfo`, `PieceInfo`, `Perms`, `Legal`, `Requirements`, `Collection`, `DSS`, `AF`, `DPartRoot` | declared, and reached by nothing |
| **0** | `NeedsRendering` | **modelled**, and reached by nothing |

So "6 of 32 modelled" is, against what the corpora actually contain, five of the twenty
keys that occur — plus one, `NeedsRendering`, that nothing reaches. The fifteen declared
keys that do occur split three ways, and the split is the plan:

- [x] **`Type` is a check, not a type.** It is the one key the corpora carry that stays
      `Declared`, and
      `every_key_the_corpora_carry_is_modelled_except_the_one_that_is_an_assertion`
      asserts it is the only one — so a second would be a failure rather than a drift

- [x] **The five that were wiring** needed less than a reader in one sense and more in
      another: the machinery existed, and none of it was reachable *from the entry*.
      `/Metadata` now decodes the XMP packet and reads what it says, `/Names` reports
      which of Table 31's ten trees a document declares and how many names each holds,
      and `/Pages` and `/StructTreeRoot` are `Located<T>` — the contents **and** the
      handle, because the page walk and the structure-tree visitor descend from the
      latter

- [x] **The nine that needed one**, all built: `/OpenAction` — both of its shapes, a
      destination array and an action dictionary, and the corpus writes both — `/AA`,
      `/OutputIntents`, `/AcroForm`, `/MarkInfo`, `/PageLabels`, `/Version`,
      `/OCProperties` and `/Threads`

- [x] The types in `document/extensions.rs` are not these readers, and the new module
      says so beside each entry where a same-named type sits in the other one:
      `OutputIntent` there carries `icc_profile_bytes`, because it is an argument to an
      `Operation` that *writes* one

- [x] The twelve that occur zero times are **declined in the code**, as
      `catalog::ABSENT_FROM_BOTH_CORPORA` with the measurement that justifies them, and
      `inspect catalog --all` marks each one "declined — no file of either corpus carries
      one". `the_keys_no_file_carries_did_not_gain_readers` is the container rule
      enforced from the other side: it fails if one of them is ever modelled.
      `/NeedsRendering` is the single exception and is named as such, because ADR-0017
      left it there

- [x] **And the figure was qualified before it could be quoted**
      ([ADR-0020](docs/adr/0020-a-modelled-entry-reports-how-much-of-its-own-table-it-reads.md)).
      19 of 20 is the shape of the number ADR-0017 exists to prevent, one level down, so
      `inspect catalog` gained an `own table` column: `/AcroForm` is modelled and reads
      **4 of Table 224's 8**, leaving `/Fields`, `/CO`, `/DR` and `/XFA` as objects. The
      expectation written into that test first was two; the measurement said four

## Phase L — The three image codecs, declined in writing rather than by omission

`CCITTFaxDecode` (2 files), `JPXDecode` (3) and `JBIG2Decode` (0) remain unbuilt, and
Phase G established that none of them blocks a single character of text. What is
missing is not the codecs but the record: a file whose image was dropped currently says
so to `log::debug!`, which Phase H fixes, and the decision not to build them is written
in this document rather than reported by the engine.

- [x] The judgement is measured. An image occupies the unit square transformed by the
      CTM (8.9.5.2), so **the determinant of that matrix is its area** — no rendering
      required, which is what makes this answerable on every file rather than on the ones
      a GPU is available for. The skip decision now names it: *"it covers 11.2% of the
      page"*. Across both corpora that is the whole bill for the two missing codecs.

- [x] **Not built**, and the measurement makes the refusal stronger rather than weaker.
      Four images, none covering more than an eighth of its page, is what the two codecs
      would buy. `JBIG2Decode` occurs zero times in either corpus. `JPXDecode` has no
      pure-Rust decoder worth depending on, and a C one has to clear
      `unsafe_code = "forbid"` and `deny.toml`'s licence allowlist before it is a
      candidate at all. `CCITTFaxDecode` is still the one that closes cleanly — T.4 and
      T.6 in roughly 600 lines with no dependency — and is still the first to build if
      those two pages ever matter

- [x] **And the measurement found something that is not about codecs at all.**
      `UnknownFilter-xrefstm.pdf` never reports a skipped image because it reports **no
      pages**: its `/Pages` names object 5, which was indexed only by a cross-reference
      stream written with `/XXXDecode`, and the recovery scan cannot find what is not in
      the bytes. `find_all_pages` swallowed both of its failures — `if let Ok(..)` on
      reaching the root and `let _ =` on walking it — so `inspect info` said "Pages: 0"
      about a file with a page in it, and `is_conforming` stayed true. That is the same
      shape as the catalogue lost to an `if let Ok(..)` in Phase G, and it is now a
      `Violation` of 7.7.3.2 naming the object. It fires on **one** file of 251

## Phase M — Scanned documents

Phase L declined the three image codecs on a measurement, and named the condition that
would reopen the question: *building them stays gated on rendering those images
mattering*. It matters — the engine is wanted for real scanned documents — and the
measurement that justified the refusal cannot speak to that. **Both corpora are
born-digital.** `JBIG2Decode` occurring zero times across 251 files is not evidence that
JBIG2 is rare; it is evidence that neither corpus contains a scan.

One of Phase L's stated reasons has also simply expired. It said `JPXDecode` had "no
pure-Rust decoder worth depending on"; `hayro-jpeg2000` is one, tested against 20,000
images scraped from real PDFs, and its sibling crates cover the other two. All three
forbid `unsafe` or are pure safe Rust, all three are `Apache-2.0 OR MIT`, and their
dependencies are optional.

- [x] **`CCITTFaxDecode`** (7.4.6), through `hayro-ccitt`. Table 12's parameters are read
      — `/K` chooses Group 4, Group 3 1D or Group 3 2D by its *sign*, `/Columns`
      defaults to 1728, `/BlackIs1` decides which bit is ink, `/EncodedByteAlign` and
      `/EndOfLine` and `/EndOfBlock` are honoured — and `/Rows` falls back to the image
      dictionary's `/Height`, which is the one fact a filter needs that its own
      parameters do not carry. Output is **one bit per pixel with each row starting on a
      byte boundary**, which is what 8.9.5.1 says image data is; the filter does not
      expand it and does not convert it to a colour. Both files of the corpus that carry
      a CCITT image now draw it, and the filter axis of the coverage index went 4 of 7
      to **5 of 7**

- [x] A behaviour change worth naming: a `/DCTDecode` stream whose bytes are a **PNG**
      is now refused rather than decoded. `image::ImageReader::with_guessed_format`
      sniffed the real format and decoded it anyway — leniency by accident, since it
      then returned RGB whatever the image dictionary said. Two files of the corpus do
      this, and both now report it, naming the bytes it found: *"Illegal start
      bytes:8950"*. The page's text is unaffected

- [x] Three defects found on the way, none of them about codecs:
      **`DCTDecode` was converting colour inside the filter** — `image`'s `DynamicImage`
      has no CMYK variant, so every JPEG came back as three components, and 160 of the
      178 JPEGs in the two corpora are `/DeviceGray`, described as one. **The image's
      component count was taken from the colour space *family*** — `[/ICCBased …]` is
      438 of 1,053 images and carries its count in `/N`, `[/Separation …]` is one
      component and was read as three. **A soft mask of a different size was skipped in
      silence**, where 8.9.5.4 says it is scaled to the image

- [x] **One contract for every filter, before two more arrive.** `DecodingFilter` covered
      the five byte transformations and neither image codec, because `CCITTFaxDecode`
      needs a fact its signature had no room for — that exception had already split the
      entry point into two functions. `FilterContext` carries the parameters, the arena
      and the image's `/Height`; `filter_for` maps a name to a unit and is the only place
      that mapping exists. Swapping a codec is now writing another unit and changing one
      arm, with nothing outside `filters/` aware of which crate decodes.
      **`is_decoded` is derived from that table**, so the hand-written second list is
      gone and with it the test that existed to catch the two disagreeing

- [x] **`JBIG2Decode`** (7.4.7), through `hayro-jbig2`. The unknown is settled: the
      mechanism is `Image::new_embedded(data, globals)`, which is Annex D.3's *embedded*
      organisation — the one PDF uses — and `/JBIG2Globals` is read from `/DecodeParms`
      and put through the filter pipeline first, since a globals stream is usually
      `/FlateDecode`d itself. **The two conventions are opposite and the filter inverts**:
      a JBIG2 codestream says 1 for black, a PDF image of one bit per component says 0,
      and a filter that passed the samples through would render every scan as its own
      negative. That is checked rather than reasoned — a JBIG2 page assembled segment by
      segment in the test, decoded both ways round, once over a white page and once over
      a black one

- [x] The dependency is trimmed to what is used: `default-features = false`, which drops
      a SIMD crate and an `image` bridge. There is no JBIG2 file in either corpus to
      benchmark against, and taking a dependency for an unmeasured gain is the shape this
      project keeps removing

- [x] The packing is shared. Both bilevel codecs pack one bit per pixel with each row on
      a byte boundary (8.9.5.1), and they arrive at it from opposite directions — CCITT
      reports whiteness, JBIG2 blackness — so `filters::bilevel` holds the packer and
      each adapter says which it has

- [x] **`JPXDecode`** (7.4.9), through `hayro-jpeg2000`, and the PDF-side rule with it:
      **7.4.9 makes `/ColorSpace` optional for a JPX image and for no other**, because
      the codestream carries its own. So the interpreter asks the dictionary first and
      the codestream only when the dictionary is silent, which is the order the clause
      gives. Without that a greyscale JPX would be read three bytes at a time — the
      defect `DCTDecode` was found committing on 160 images

- [x] Verified where it counts: **three JPX files of the external corpus**, which this
      project did not write. Two of them render, and `crosscheck_image.sh` puts our
      rendering beside PDFKit's — `252 244 245 188` against `253 245 245 189`, agreement
      within one part in 255. That is better evidence than any fixture, and it arrived
      because the corpus had files this phase could finally read

- [x] **Test material, without a sample to be had.** No scan exists in either corpus and
      none was available, so the material is *made* — and made so that nothing checks
      only itself:
      - `examples/make_scan_fixtures.rs` writes a page whose image is **encoded by a
        different implementation from the decoder under test**: `fax` for Group 4, and
        JBIG2 segments assembled by hand from T.88 §7.2
      - `scripts/test/crosscheck_image.sh` asks **PDFKit** what it sees in the same file,
        which is the standard the other five cross-checks hold to and the answer to "what
        is it compared against"
      - The comparator is four numbers — the mean luminance of each quadrant — because
        two renderers legitimately disagree about an edge and never about which quarter
        of the page is black. It also says *which way*: `fepdf 254 0 0 0` against
        `PDFKit 0 255 255 255` is an inversion, and that is what removing the JBIG2
        polarity flip produces
      - Verified to fail: with the inversion removed, `DISAGREE by 255`. Its own first
        run failed too, and the fault was the comparator's — it read a bitmap context's
        memory as if row zero were the bottom. An asymmetric fixture is what caught it

- [x] **Two defects the fixture found**, both of which a real scan would have found on
      the first day and neither corpus could:
      **a `/DeviceGray` image at one bit per component** — the commonest image in a
      scanned document — reached the GPU as a buffer eight times too short, and
      `Queue::write_texture` killed the process. Sub-byte samples are expanded to bytes
      now, as an indexed image already was. And **a buffer shorter than the dictionary
      describes is a `Violation` of 8.9.5.1** rather than a crash
*Done when*: **done.** The filter census reports `yes` for all three, a scanned page
renders, and what it is compared against is something this project did not produce. **All
three report `yes`**, a scanned page renders, and nine files agree with PDFKit within one part in 255
— five of them files this project did not write. Clause 7.4 is **nine of its ten**, with
`Crypt` handled in the security layer instead.

## The rule this list is now sorted by

Phases G to M were driven by a corpus, and it worked: measuring against files this
project did not choose found a panic, a lost catalogue, twelve false claims in a document
and a rendering defect on 160 images. It also failed once, in a way worth stating as a
rule.

Phase L declined three image codecs because they occurred in two, three and zero files of
251. That measurement was sound and the conclusion was wrong, because **both corpora are
born-digital test files** — 205 Isartor files each breaking one PDF/A clause, 37 targeted
at implementation differences, nine samples this project chose. A scanned page appears in
none of them. "Zero occurrences" measured the corpus, not the world.

> **A corpus can justify building something. Only a use case can justify not building it.**

Every refusal below now carries a reason of the second kind, or moves to a phase. The
entries that could only say "zero in the corpus" are the ones that moved, and they are
the same entries a business document would have exercised on its first day: attachments,
long-term signatures, choice fields, layers.

## Phase N — What the engine gets wrong *(complete)*

Not gaps. Five places where a file was read, drawn or written and the answer was wrong —
which is worse than refusing it, because nothing says so.

- [x] **Optional content is ignored while drawing.** `BDC` popped its property list and
      discarded it (`ops/marked.rs`, "Skeleton: just pop for now"), so content inside an
      `/OC` marked-content section was painted whether or not its group was on. Hidden
      layers became visible: the non-printing layer of a drawing, the other language of a
      bilingual page, a "draft" underlay. `fepdf-doc` can **write** `/OCProperties`
      through an `Operation`, so the engine created layers it then ignored.
      `/OCProperties` gained a reader in Phase K and nothing consulted it — which is what
      made the defect invisible, and is why the fix enters through that reader rather than
      walking the raw dictionary again.
      Recorded as [ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md).

- [x] **A page tree inside an object stream is not recovered.**
      `UnknownFilter-xrefstm.pdf` reported no pages while PDFKit rendered it. Its `/Pages`
      is object 5, which lives inside an object stream, and the cross-reference that says
      which container is written with `/XXXDecode`. The recovery scan looked for
      `N 0 obj` in the bytes and an object inside a compressed container is not there to
      be found.

- [x] **Headless rendering fails on a small page.** A 64×32 page produced *"Copy at
      offset 0 for 8192 bytes would end up overrunning the bounds of the Source buffer of
      size 1024"* from wgpu. Worked around by enlarging a fixture, which is not a fix and
      not an explanation — and, it turns out, not a workaround either.

- [x] **The layers the engine *writes* have no content in them.** Found by the optional
      content work above. `apply_update_layers` wrote the OCG dictionaries and a default
      configuration with `/ON`, `/OFF` and `/Order`, and **nothing was ever marked `/OC`**
      — no content stream, no XObject, no annotation — so every group it created was empty
      whatever its state. `LayerGroup::printable` was dropped on the floor with it, and
      `LayerGroup::id` was never used at all.

- [x] **`/SMaskInData` is not implemented.** Its default is 0 — ignore any alpha the
      codestream carries — and a JPX image asking for 1 or 2 got that treatment
      silently, so a transparent image was drawn opaque.

## Phase O — The holes in the checking *(complete)*

Phase M could not check its own work against anything it had not written, and said so.
That is not a scanned-image problem; it is the same hole in four places.

- [x] **Neither corpus contains a business document.** No attachment, no long-term
      signature, no choice field, no layer, no redaction — which is why every one of
      those read as "zero occurrences" and was declined on that basis.

- [x] **JBIG2 has never met an image this project did not assemble.** True, and still
      true: Phase O-1 doubled the corpus to 524 files and `/JBIG2Decode` occurs in
      **none** of them. No JBIG2 encoder is reachable here either, so the fixture cannot
      be made by somebody else the way `/SMaskInData`'s were made by OpenJPEG.

- [x] **`verify_visuals.sh` runs a test target that does not exist.** `visual_regression`
      is not in `fepdf-render`; the script could not pass and had not for as long as
      anyone had run it — `cargo test` exits 101 with *"no test target named
      `visual_regression`"*. **Deleted**, because nothing referenced it: `TESTING.md`, the
      `Makefile` and `AGENTS.md` all name `scripts/visual_regression.py`, which is a
      different suite and a working one.

- [x] **The rest of `docs/specs/` is unaudited.** `omissions.md` was checked and twelve
      of its claims were false, so it is archived. **Seven** documents remained, not the
      four this entry named — `core-pipeline.md`, `rendering.md` and `sdk-pipeline.md`
      were not on the list and are the newest and most accurate of them.

## Phase P — What the rendering subset owes *(complete)*

Phase N asked what the engine gets wrong and found five things by looking at the roadmap.
This phase found four by looking at **the standard**, read with this engine — `inspect
text` over the copy in `docs/specs/`, 1020 pages in 3.7 seconds — and then measuring the
clauses that turned up against PDFKit.

**The shape of the roadmap was hiding them.** Clause 7 had five rows in the table above
and clauses 8 to 14 had one between them, so Graphics, Text, Rendering and Transparency —
four clauses, 31 subclauses — shared a line that talked about annotations. Splitting that
row is the first half of this phase; the entries below are what became visible when it was
split. The coverage index did not help either, and could not: its three axes were chosen
because their denominators can be enumerated from a file, and colour spaces, shadings and
functions cannot be.

- [x] **There is no PDF function evaluator (7.10), and two visible defects come from it.**
      Types 0 (sampled), 2 (exponential), 3 (stitching) and 4 (PostScript calculator) are
      built, in `fepdf-model::function`, with 15 tests over values worked out from the
      clause rather than from this engine's output. Measured against PDFKit on the files
      built for the purpose.
      Recorded as [ADR-0027](docs/adr/0027-a-function-evaluator-and-two-divergences-it-pinned.md).

- [x] **`/DeviceCMYK` to RGB is not colour managed, and the module said it was.** The
      entry was right that the conversion was wrong and wrong about why. It is not that
      the conversion is uncalibrated — **the standard specifies one and this engine was
      not using it.**

- [x] **`/DefaultCMYK`, `/DefaultRGB` and `/DefaultGray` are a `shall` and nothing reads
      them.** They do now, and so does 8.6.5.3's `/CalRGB` transform, which is what makes
      the remapping observable at all.

- [x] **A font with no embedded program renders nothing.** Fixed. A minimal page setting
      `/Helvetica` and showing five characters read `254 254 254 254` — paper — where
      PDFKit read `233 239 217 230`. It now reads `235 240 219 233`.

- [x] **Two font-fallback faults that were not the cause, and are still faults.** Both
      fixed. `resource_dir("resources")` in `Document::load_system_fonts` and
      `VelloBackend::load_system_fonts` pointed at a directory that stopped existing on
      **2026-05-16**: `d71083d` renamed `resources/fonts` to `assets/fonts` as a pure
      `R100` rename and left the two defaults behind. The model falls through to platform
      paths and found real fonts anyway, which is why nothing noticed; the renderer has no
      such fallback, so its map was empty outright and it logged five warnings on every
      run for three months.

- [x] **A glyph that will not draw is silent.** `show_text` counts what it painted and
      records a 9.6 `Violation` when a run laid out glyphs and painted **none** of them:
      *"a run of 7 glyphs in /Helvetica yielded no outline at all"*. Verified by putting
      the standard-14 defect back and watching it fire, and by its silence on all nine
      conforming samples.

- [x] **Shading types 4 to 7 are not read.** All four are now, in
      `fepdf-model::graphics::mesh`, and all four agree with PDFKit.

- [x] **Halftones (10.6) have no code at all, and are declined — on the clause, not on
      the corpus.** The survey was the first step and it has been run;
      `crates/fepdf/examples/survey_extgstate.rs` is the command that re-derives it.

- [x] **`fepdf-gui` is an interactive PDF processor and does not meet 6.3.2.3.** It has a
      layer panel now, and the three rules of 8.11.4.3 that are about a *panel* rather
      than about what is drawn are enforced in the engine, where they can be tested.

- [x] **The engine logs thirteen conclusions about documents to stderr.** ARCHITECTURE
      §4.3 says a departure from the standard is recorded as a `Decision`, not logged,
      because a warning on stderr cannot tell a caller *this loaded* from *this was
      conforming*. The count is **three** again — the host-property ones — over a row
      that derives the engine as every crate that is not a frontend.
      Recorded as [ADR-0008](docs/adr/0008-an-indirect-length-is-not-an-ambiguity.md).

- [x] **The `Decision` row named five crates and `fepdf-render` was not one of them.**
      It read 82 where the truth was 84 the moment the renderer gained a site — the third
      time this figure had been wrong for that reason, and its own comment predicted it.
      Fixed by deriving it from the same partition the log row uses. **The pattern is
      fixed too now**: every row in `status.sh` was read for it, and two more had it.

## Phase Q — The rules the architecture asserts, and what actually checks them

`ARCHITECTURE.md` §7 opens with "architecture rules that are not checked become comments"
and then lists five rows, four of which name a tool. The fifth said Rule D was "enforced
by construction". Naming no tool is the tell, and re-deriving every checkable claim in the
document on 2026-08-22 found the rule broken, the crate sizes stale by up to 5.8×, and the
`Operation` listing — in the section that *defines* Rule D — a work of fiction.

**None of this was hidden. It was unmeasured, which is not the same as unknowable**: every
figure below came out of one command, and the commands are here.

- [x] **Rule D did not hold: eight frontend call sites mutated documents outside the
      vocabulary.** The facade exposed each mutation twice — as an `Operation` variant and
      as a plain `&mut self` method — so a frontend could leave the vocabulary without
      re-implementing anything, which is the failure Rule D was written to prevent in a
      form it did not anticipate. `fepdf-gui` called `remove_page` twice while
      `Operation::RemovePages` existed and the GUI never built it: **two ways to remove a
      page, with nothing comparing them**, which is §4's rotate divergence in its early
      form.
      Recorded as [ADR-0026](docs/adr/0026-the-engine-takes-the-ecmascript-subset-because-it-already-owes-it.md), [ADR-0007](docs/adr/0007-an-option-that-is-not-read-is-hidden.md).

- [x] **The engine/frontend log split was not a partition, so three crates were in
      neither half.** Fixed by deriving both lists from the workspace. The figure went
      from 1 to 16 without a line of engine code changing, and Phase P's entry above says
      what the thirteen non-deliberate ones are

- [x] **Forty-five dependency declarations were referenced by no line of code**, in `src`,
      `tests`, `examples` or `benches` — including six crypto crates in `fepdf-syntax`
      (`cbc`, `pbkdf2`, `hmac`, `x509-parser`, `ecdsa`, `p256`), four font crates in
      `fepdf-font` and `fepdf-model` (`skrifa`, `read-fonts`, `kurbo`), `pdf-writer` and
      `id-arena` in `fepdf-model`, and `tokio` in `fepdf-gui`. Verified by deleting all of
      them and running `cargo check --workspace --all-targets`: exactly one was real —
      `tokio` in `fepdf`, used by two examples with an async `main`, which is why the
      check has to be `--all-targets` — and it was put back with a comment saying so.
      `fepdf-model` fell from 149 transitive crates to 144 and `fepdf-font` to 14.
      Recorded as [ADR-0007](docs/adr/0007-an-option-that-is-not-read-is-hidden.md), [ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md).

- [x] **That audit has no row, so it will happen again.** It has one:
      `dependencies nothing references (expect 0)`. Two of the three C dependencies and
      forty-five of these were invisible to every check this project has, and both were
      found by hand, twice, four days apart.

- [x] **A frontend reached past the facade, and the rule that would have caught it was
      stated one notch too narrow.** `ARCHITECTURE.md` §7 claimed "no frontend declares
      `fepdf-model`", which was true; §2's topology puts every frontend above `fepdf` and
      nothing else, which was not. `fepdf-gui` declared `fepdf-render` directly and never
      enabled the facade's `render` feature — reaching the GPU crate around the opt-in
      [ADR-0004](docs/adr/0004-rule-b-makes-the-gpu-dependency-optional.md) exists to
      provide. It used two names, both re-exported by the facade, so the fix was one line
      of `Cargo.toml`. All four frontends now declare `fepdf` and nothing else, and
      `status.sh` counts the exceptions rather than asserting there are none

- [x] **`fepdf debug extract-font` wrote outside every registered directory.** A
      root-level `exports/`: unregistered in `ARCHITECTURE.md` §2.1, **not git-ignored**,
      and never created — so the write failed unless the user had made the directory, and
      left untracked files in the repository root when they had. Three defects in one
      line, none of which any check could see. It writes to `out/exports/` now. The
      `#[ignore]`d test that read from the same path had never run in either sense — the
      attribute stopped it, and the file was not there if the attribute had not — and was
      removed, which `TESTING.md` records happening once before for the same reason

- [x] **`fepdf-mcp` names 24 of the 30 operations as tools.** It names thirty now, and
      `status.sh` counts them: `operations named as MCP tools  30 of 30`.

- [x] **`fepdf-wasm::render_page` returns `Ok(())` having drawn nothing.** It returns an
      error now, naming the page it did not draw, the canvas it did not draw to, and the
      reason. Not being able to do something is a fact about this crate; reporting success
      for it is a fact about the caller's next hour.

- [x] **`fepdf-wasm` builds for WebAssembly.** It did not, and had not for as long as the
      crate existed: `cargo build --workspace` never touches the target, so the failure was
      invisible on a host where everything compiles. **The target was installed the whole
      time** (`wasm32-unknown-unknown`, and two WASI ones). It simply had never been run.

## Phase R — Running the document's code

**The subset is taken** ([ADR-0026](docs/adr/0026-the-engine-takes-the-ecmascript-subset-because-it-already-owes-it.md)),
so this phase is no longer a question about whether to build. It is a chosen subset that
is not met, which the table above makes a defect rather than a gap.

The decision rests on one sentence the engine had already written about itself. Setting a
value in a form that declares a calculation order records a **`Violation` of 12.6.3** —
"wrote the value and did not run the scripts; fields computed from it are now stale". Form
editing was undertaken; it cannot be finished without this. No corpus count and no user
appears anywhere in that argument, and none should: a capability that does not exist has
no users.

The engine is **boa** ([ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)),
and the shape is a **fifth frontend** translating into `Operation`
([ADR-0025](docs/adr/0025-a-script-processor-is-a-frontend-not-a-subsystem.md)) — reads
through the existing queries, writes through the vocabulary, no third path. What a script
may do is what the API may do, which is the bound every other frontend already has.

**Ordered on cost and dependency, not on doubt.** Phase P was broader and far cheaper —
one function evaluator (7.10) fixed a spot colour that rendered white — and Phase Q held
Rule D, which this design leans on and which had eight open breaches at the time. Both are
complete: the evaluator carries all four `/FunctionType`s and Rule D is enforced. A script
frontend routed entirely through `Operation` would be *more* conforming to Rule D than
`fepdf-gui` was, which is why Q came first rather than why R can wait.

**One clause of that argument was an overreach and is worth keeping visible.** "Every
print-oriented file" was written with no count behind it, and the count is two: across all
524 files, `/Separation` occurs twice and `/DeviceN` once. The work was still right to do —
a spot colour rendering white is wrong wherever it happens, and Phase P's own fixtures and
PDFKit are what establish the fix — but the breadth was asserted, not measured, which is
the same move Phase L's refusal made in the opposite direction.

- [x] **Establish that `&mut Document` can be held across boa calls.** Measured. **The
      requirement is met and the phrasing was wrong**, which is the more useful answer.

- [x] **Run the corpus's six `/JavaScript` scripts under `--features script`** with `app`
      and `this` and nothing else, and count how many complete. **Seven files, not six**,
      and between them only **two distinct scripts**: `app.alert("Hello World!")` and
      Adobe's stock "this document has file attachments" boilerplate, each repeated four
      times. Every one is a conformance-*failure* fixture.

- [x] **Rule 9's check looks at one target, and `cc` is reachable on another.** It reads
      four now — Linux, Windows, macOS and wasm — and naming them is strictly stronger
      than reading whichever machine happens to run the audit.

- [x] **Decide whether the Linux GUI keeps Wayland or Rule 9 keeps its exemption.**
      **Wayland stays** ([ADR-0033](docs/adr/0033-the-linux-gui-keeps-wayland-so-rule-9-names-one-exemption.md)).
      An X11-only Linux GUI in 2026 is a worse product than a rule kept clean, and Rule 9
      exists to keep unaudited C out of the *engine* — which compiles none on any of the
      four targets.

- [x] **Write the fixtures, because the corpus cannot validate this.** Written.
      `make_script_fixtures.rs` is the third sibling of `make_scan_fixtures.rs` and
      `make_colour_fixtures.rs`, and produces three forms carrying what no file in either
      corpus does — `/AA /C` on a field and `/CO` on the form.

- [x] **`Operation::RunDocumentScripts { trigger }`, and nothing implicit.** The second
      half holds and the first does not exist.
      [ADR-0032](docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md).

- [x] **Determinism is injected.** `ScriptEnvironment { now_ms, seed, viewer_version }`
      exists and `app` is built from it, never from the machine. Its default instant is a
      fixed one, so two runs of the same document agree.

- [x] **`/CO` supplies the calculation order** — the engine already reads it, at the one
      site that records the `Violation`. Running it is `fepdf_script::run_calculations`.

- [x] **Adobe's helpers may be `.js`, on two conditions.** Both are met.
      `crates/fepdf-script/scripting/aform.js` carries `AFMakeNumber`,
      `AFMakeArrayFromList`, `AFSimple` and `AFSimple_Calculate` — the last being the one
      a real form actually calls.

- [x] **`Intl` is absent, and its absence is now one behaviour instead of three.**
      `typeof Intl` is `undefined`, because ECMA-402 sits behind boa's `intl` feature and
      this build does not enable it. ICU crates arrive regardless — `icu_normalizer` and
      `icu_properties`, which the *language* needs for `String.prototype.normalize`,
      identifiers and `\p{…}` regex escapes.
      Recorded as [ADR-0034](docs/adr/0034-intl-is-declined-for-what-it-does-not-do.md).

## Phase S — What the text pass turned up in the renderer

Reading `fy05.pdf`'s remaining extraction loss
([ADR-0041](docs/adr/0041-a-character-collection-is-declared-not-guessed.md)) turned up two
defects that are not about text at all, and left one text question sized.

- [x] **The renderer does not draw the same page twice, and the engine is not why.**
      Eight renders of `samples/sample.pdf` page 1 with one binary give three distinct
      images, one isolated pixel apart; `fy05.pdf` page 304 and `fugaku.pdf` page 1 do the
      same. `examples/render_determinism` says which layer: it fingerprints the vello
      `Encoding` and then hands *one* scene to the rasteriser repeatedly. **The scene is
      byte-identical every time** — the interpreter and the backend keep Rule 10 — and
      vello's GPU pipeline turns that one scene into more than one image, while vello's
      CPU shaders give one. `Rasteriser::{Gpu, Cpu}` is a parameter now, `Gpu` the
      default, `publish render --cpu` the seam
      ([ADR-0043](docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md)). The GPU
      path is not made deterministic, because which stage of a compute pipeline reorders a
      reduction is vello's question and not one this level can answer.

- [x] **The `constitution.pdf` visual baseline was stale, and that was the whole failure.**
      28 pixels, of which 27 are the page number `1` at the foot of the page: the engine
      draws it, the frozen reference did not, and PDFKit reads it in the page's text.
      Refreshed; the suite passes 4 of 4 on three consecutive runs, and putting the old
      reference back fails it again. **The flaky pixel above is a channel delta of 1 and
      the suite already tolerates 1**, so it was never flapping — this row said otherwise
      when it was written, on the strength of "the suite fails and there is a flaky pixel"
      and without reading what the comparison does with that pixel. The tolerance now
      carries the measurement that justifies it.

- [x] **Four more character collections were on disk and unread, and are read now.**
      `fetch_font_resources.sh` already brought down `Adobe-CNS1-UCS2`, `Adobe-GB1-UCS2`,
      `Adobe-KR-UCS2` and `Adobe-Korea1-UCS2` beside the Japan1 table, and the loader asked
      for Japan1 **by name** whatever the file declared. It builds
      `{Registry}-{Ordering}-UCS2` from what the document says now, so a font declaring
      `Adobe-Korea1` gets the Korean table; a collection with no file — `Adobe-Japan2`, say
      — still records the 9.7.3 violation rather than borrowing one.

## Phase T — What is left, with the size of each measured rather than guessed

Everything here was carried in a handoff note as a one-line hunch. Each is now a figure.

- [x] **Extraction emits text in the order the producer wrote it, not the order it is
      read.** Measured against PDFKit over **7,727 pages of the nine samples**, comparing
      the multiset of non-space characters (spacing is a separate question, settled at a
      quarter em in §9) and then the sequence.
      Recorded as [ADR-0047](docs/adr/0047-text-extraction-sorts-runs-into-reading-order.md), [ADR-0049](docs/adr/0049-the-extraction-backend-was-not-tracking-the-ctm.md), [ADR-0050](docs/adr/0050-ruby-is-bound-to-the-base-it-reads.md).

- [x] **A font is built twice, by two different routes, and only the second one draws.**
      `ARCHITECTURE.md` §4.4 is called *normalisation-at-load* and says a `Document` is one
      normalised state by the time application code sees it. Three of the things it names
      hold; fonts do not. The cache was empty on every sample when `open` returned —
      `normalize_resources` cleared what the ingest pass built — and every font was rebuilt
      during rendering by `Interpreter::get_font` ([ADR-0045](docs/adr/0045-normalisation-at-load-does-not-reach-fonts.md)).
      Recorded as [ADR-0046](docs/adr/0046-unify-font-construction-paths-at-load.md).

- [x] **Eight wildcard arms still answer a file's value with silence.**
      `scripts/audit/silent_branches.py` lists them, down from eleven. Its own header says
      the number is a count and not a verdict — an unknown `/V` makes the document fail to
      open, which is loud enough — so each wants a judgement rather than a sweep:
      `/ShadingType` twice, a colour operand count, a CFF SID, an encryption version, a
      JPEG2000 channel count, a mesh shading type, and a key length.

- [x] **The interactive processor keeps nothing the engine decided.**
      6.3.2.3 is one of the two subset rows chosen and not met, and this is a measured part
      of what it owes: **`doc.decisions()` is called nowhere in `fepdf-gui`.** A document is
      opened, repaired and displayed, and the reading log that says what had to be decided
      to display it never reaches the window — the only decisions a user sees are the ones
      `save_with_options` returns. A page that fails to render collapses to
      `"Failed to render page {index}"`, discarding both the error and whatever the backend
      concluded on the way — the 9.6 violations Phase P added, among them.

- [x] **ECMAScript is chosen and not met**, which [Phase R](#phase-r--running-the-documents-code)
      owns and its own `Done when` states.
      **Resolved in [Phase R](#phase-r--running-the-documents-code)**: `fepdf-script` executes
      document and field ECMAScript via pure-Rust `boa`, evaluating calculation orders
      (`run_calculations`) over `Operation::SetFormFieldValue` to update computed fields
      without staling, with determinism injected and 0 C dependencies. All chosen subsets
      under 6.3.1 are now met.
      **This said so for the four phases before anything called it.** `fepdf-script` is a
      frontend and had no entry point — no binary depended on it, and `run_calculations`
      was reached only by its own tests — so every field write still recorded the 12.6.3
      `Violation` this phase exists to remove. `fepdf-mcp` calls it now, in
      `tools::operations::apply_and_calculate`, which is the one path its two
      field-writing tools share; `fepdf-cli` and `fepdf-gui` write no form field at all,
      so there is nothing there to wire. A frontend that runs the scripts says so with
      `Document::declare_script_processor`, and the `Violation` is what a frontend that
      does not still gets.

- [x] **Korean and Chinese document end-to-end extraction verified.** The four collections
      beyond Japan1 (`Adobe-Korea1`, `Adobe-GB1`, `Adobe-CNS1`, `Adobe-KR`) are read
      ([ADR-0044](docs/adr/0044-the-other-four-collections-were-already-on-disk.md)), and
      an end-to-end test suite (`tests/cjk_extraction_test.rs`) verifies full passage,
      multi-line, and cross-collection text extraction directly from Type0 and CIDFont
      documents against Adobe's character collections.

## Phase U — The work that stops at no crate boundary

The crate-by-crate pass over all thirteen crates and `crates/fepdf/tests/` is done. What
it could not take is here, because each item spans crates by construction. Every figure
below is re-derived on 2026-09-08 and the command that derives it sits beside it.

The order is not free. **The two gate items come first** — a large refactor under a gate
that cannot see part of the code puts violations in without saying so — and the parser
twin comes last, because everything above it strengthens the net that work needs.

- [x] **Rule 1 did not see `pub(crate) fn`.** Its `awk` detector matched
      `^[[:space:]]*(pub )?(async )?fn `, and `pub(crate) fn` does not match `(pub )?`.
      **86 functions across the workspace were invisible to the length limit, and 8
      exceeded it**.

- [x] **`scripts/audit/silent_branches.py` had a verdict nothing read.** This entry first
      said the tool was "measured and gated by nothing", and that was imprecise in two
      ways. `status.sh` already calls the tool rather than deriving its number a second
      time, so half the *Done when* was met before it was written. And the count is **not
      meant to gate**: the tool's own docstring says so — an unknown `/V` makes the
      document fail to open, which is loud enough without a `Decision`, so a new arm is a
      question rather than a defect.

- [x] **A PDF was assembled by hand in nineteen files.** `crates/*/src` carried 29
      occurrences across 13 files and `crates/*/examples` six more, on top of the three
      `tests/common/mod.rs` each crate had consolidated separately. **Six of the 29 were
      not fixtures at all** — `fepdf/src/lib.rs` holds the static empty document
      `create_empty` returns, and `fepdf-model/src/writer.rs` writes tables because that
      is its job. This entry counted both as duplication and neither was.
      Recorded as [ADR-0083](docs/adr/0083-a-fixture-crate-that-depends-on-nothing.md).

- [x] **`fepdf-mcp` links a GPU stack, and this entry was wrong about why.** It said
      `fepdf = { workspace = true, features = ["render"] }` "is the sentence Rule B uses
      as its own example". It is not. Rule B is about a crate that defines a contract
      depending on an implementation of it, and `fepdf-content` — which defines
      `RenderBackend` — depends on no vello and no wgpu. **The rule is kept.** Rule B's
      closing sentence describes the consequence it prevents; it does not name this.

- [x] **Two content-stream readers, and they were not peers.** This entry called them a
      semantic duplicate of 836 lines against 3,892, on the strength of a comment calling
      one "the twin" of the other — which is attached to a single function, not to the
      subsystems. Measured, the shape is different and worse: they handle **66 of the same
      operators**, and the interpreter's raw-byte path is not an equal reader but an
      incomplete one that only works because refinement normally runs first.
      `ops/marked.rs` said so in its own words: *`BMC`, `BDC` and `EMC` become
      `Command::BeginMarkedContent` and `Command::EndMarkedContent` in the parser and
      arrive through those arms.*

- [x] **`TESTING.md` quoted a test count from a run two phases old.** It said **754
      tests** where `cargo test --workspace` reports **800**, with the timings from that
      same stale run. `status.sh` re-derives neither, which is why it did not read as a
      disagreement. Every `expect 0` figure it *does* derive read 0 on 2026-09-08, so that
      sweep is finished.

- [x] **The MCP tool surface said less than it did, and the gap was not prose.** This
      entry expected two defects of documentation — two descriptions omitting that they
      execute the document's ECMAScript, and 35 of 36 descriptions being one sentence.
      Measured, the first was **31 of 36**, not two, and it was not a documentation defect.

### What Phase U did not close

Three entries, **all three now taken**. They stay struck through rather than deleted
because two of them were wrong about their own size — one about what it was worth, one
about how much of it was left — and that is the part worth keeping.

- ~~**`fepdf-model::color::ColorSpace` has no user outside its own module**~~ **— taken,
  2026-09-10.** The type was a twelve-variant enum duplicating `ColorSpaceKind`, differing
  from it only in that `ICCBased` carried an `Arc<ColorProfile>` — which its `transform`
  bound as `_profile` and discarded, under a comment reading "In a real implementation:
  map through ICC profile". Eight of its twelve arms returned `Color::Gray(0.0)`, so a
  `/Separation` or a `/CalRGB` came out black rather than unresolved. What rendering
  resolves is `ResolvedColorSpace`, which answers `None` instead and lets the caller
  record what it fell back to.

  This is the type the first attempt at 10.3 was written against: three unit tests passed
  and no page changed. `from_icc` was the only working code in it, eight lines, and
  `ResolvedColorSpace::from_icc` already does the same thing where a colour can reach it.

      ```text
      grep -rn "enum ColorSpace\b" crates --include='*.rs'   # empty
      ```

  The bare-name grep this entry used to carry is no longer the check: with the type gone,
  every remaining `ColorSpace` in the workspace is the PDF name `/ColorSpace` in a string,
  or one of the two `ColorSpace` enums `hayro_jpeg2000` and the JPEG decoder export. The
  exclusion list that made it read "empty" was doing most of the work.

- ~~**A calculating form is built by hand in five test files**~~ **— four of the five
  moved, and the fifth is not the same shape.** Re-derived 2026-09-10: `fepdf-mcp`'s
  `calculation_scope_test.rs` and `mcp_server_tests.rs`, `fepdf-script`'s
  `calculate_test.rs` and `fepdf`'s `calculation_order_test.rs` all call
  `fepdf_fixtures::acroform` now. `form_appearance_test.rs` still writes its own, and
  stays that way: it needs `/Q` quadding, a `/DR` carrying Helvetica, `/P` on the widget
  and a 300x100 `/MediaBox`, none of which `acroform` takes. Adding four parameters for
  one caller is generality with one user, and one instance is not duplication.

      ```text
      grep -rl '/CO \[' crates --include='*.rs'
      # crates/fepdf-fixtures/src/lib.rs          — the shared builder
      # crates/fepdf-model/examples/make_script_fixtures.rs
      # crates/fepdf/tests/form_appearance_test.rs — the one that stays
      ```

- ~~**`[profile.dev.package]` is not tuned**~~ **— taken, and it was worth more than the
  entry assumed.** `opt-level = 2` for dependencies takes `cargo test --workspace` from
  **45.9s to 27.5s**: the suite's cost is opening PDFs and the decompression under that is
  `flate2`. The one-off price is 212 seconds to rebuild the dependency graph, recovered by
  the eighth run, and `package."*"` leaves this workspace's own crates alone so the
  edit-and-rebuild loop is untouched.

      ```text
      time cargo test --workspace    # 27.5s, was 45.9s
      ```

## Read broadly, write 2.0

The seven capabilities this section used to list as open questions divide on one line,
and the line is a decision that has now been taken: **the faithful-copy path and general
PDF 1.7 output are out of scope.**

Three of the seven were about *writing*, and that settles them together:

- **PDF/A-3 output.** A-3 is PDF 1.7, so it is unreachable, and an e-invoice in the
  Factur-X or ZUGFeRD form cannot be produced. Where the recipient accepts **PDF/A-4f**
  — 2.0-based, and it does allow embedded files — the case is served; where a recipient
  requires A-3, that is a use case this engine does not serve.
- **Signing a document this engine did not write.**
  [ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md) already said the two
  were one question, so deciding the faithful copy decided this.
- **Encryption other than AES-256 R6.** "Do not write what 2.0 deprecates" and "do not
  write versions before 2.0" are the same rule
  ([ADR-0015](docs/adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)).

The remaining four are about *reading*, and reading is where this engine is meant to be
broad — it already reads five encryption schemes and writes one. They were declined on
corpus counts alone, which the rule above disqualifies as a reason, so they are open
questions rather than refusals:

| | What is not read | What makes it necessary | What the corpus says |
| :--- | :--- | :--- | :--- |
| **P1** | ~~**`/Ch` choice fields.**~~ **Built.** | Any government or business form | **12.7.4.4.** Choice fields are parsed (`/Opt`, `/I`, `/TI`), values updated with appearance stream regeneration ([ADR-0048](docs/adr/0048-reading-and-setting-choice-fields.md)) |
| **P2** | ~~**`/AF` associated files (14.13).**~~ **Built.** | Any document that carries another, and reading a PDF/A-3 even where one cannot be written | **17 files**, the moment PDF/A-3 and PDF/A-4f arrived. `FileSpecification` reads Table 43, `/AFRelationship` included — an e-invoice's `/Data` and its `/Source` are different facts |
| **P3** | **`/DSS` and `/Perms`.** Long-term validation data, and the permissions a signature sets (DocMDP) | *Reporting* on a signed document somebody else produced — which survives the decision above, where producing that data does not | **Still nothing.** Neither key occurs in any of the 524 files |
| **P4** | ~~**What a document *does* when opened.**~~ **Built** — `fepdf inspect actions` | Security screening. The coverage index excludes actions because "reads an action" has no settled meaning (ADR-0019); "what can this document do, and does the reader have to do anything first" has one ([ADR-0022](docs/adr/0022-what-a-document-does-is-a-settled-question-where-reads-an-action-is-not.md)) | **105 of 524 files** carry an action. Six `/JavaScript`, three `/Launch`, six naming an `/S` no edition defines. **Two run code with no interaction at all**, through a `/Names /JavaScript` tree the old census could not see |

Phase O-1 fetched the corpus and the column above is what it answered. **P1, P2 and P4 are
done.** P3 remains unsizeable, and the reason has changed: it is no longer "nobody
has looked" but "a corpus of 524 files assembled by three other projects contains none of
it", which is a much better argument for a *use case* being what justifies them, if
anything does.

P4 was built the moment it had files to be built against, and the files earned it: the
walk found two documents that run a script when they are opened, filed in a name tree
nothing points at, which every count this engine had taken reported as doing nothing.

## Not planned

What is left after the rule above: refusals resting on the nature of the thing rather
than on how often a corpus happened to contain it.

- **A DOCX converter.** The `DocumentSource` boundary exists so one has a place to go
  (`ARCHITECTURE.md` §4.2), but writing it means a layout engine — style resolution,
  line breaking, pagination — which shares almost nothing with reading PDF.
- **`fepdf-wasm` as a peer frontend.** Forty lines with an unimplemented renderer.
  Whether to build it is a product decision, not an architectural one.
- **Writing PDF 1.7, and the faithful-copy path.** Output is 2.0 and earlier versions
  are read-only; a file this engine did not write is not signed
  ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)). Decided rather
  than deferred, and what it costs is written out under "Read broadly, write 2.0" —
  PDF/A-3 output, and therefore an e-invoice for a recipient who will not take PDF/A-4f.
- **Reading an entry no corpus carries and no use case names.** **Ten** keys of Table 29
  are declined in the code (`catalog::ABSENT_FROM_BOTH_CORPORA`), down from twelve:
  Phase O-1 presented `/AF` and `/PieceInfo` and both were built. `/DSS` and `/Perms`
  were moved off the list by the rule above and are still carried by no file at all, so
  they remain open questions rather than refusals. A test holds the line from the other
  side.
- **Multimedia: `/Movie`, `/Sound`, `/Screen`, `/3D`.** Clause 13.4 is deprecated in
  2.0, and reading it would be building for a subsystem the standard is retiring. Phase
  O-1's corpus does carry them — `/3D` ten times, `/Movie` five, `/RichMedia` three,
  `/Sound` and `/Screen` once each — which changes the premise and not the refusal: this
  one rests on the standard retiring the clause, which is a reason a corpus cannot
  overturn.
- **XFA.** Deprecated in 2.0, and a second form model besides the one that works.
- **A faithful-copy path, and signing documents this engine did not produce.** The two
  are one question: byte fidelity buys nothing else that another route does not, and
  editing a signed file still reports as changed since signing whatever is preserved. A
  tool that never rewrites the file is the right place for that, and there are such
  tools ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)). Signing
  fepdf's *own* output was the part worth having, and it is done.
- ~~**Painting a pattern.**~~ **Built.** This said `scn` with a pattern name was
  consumed and the fill left unchanged, and that adding the variant first would be a
  container before its contents. `Paint::Pattern(PatternSpec)` and the interpreter's
  `handle_pattern_color` exist, so the entry is wrong rather than out of date — kept
  visible rather than deleted, because a "Not planned" list that quietly loses the items
  that got built cannot be trusted about the ones that did not.

---

## How this roadmap differs from its predecessor

The previous version marked Phases 1–27 complete against a goal of "the world's most
robust and ISO-compliant PDF 2.0 toolkit". Several of those completions did not
survive measurement: `open_repair` returned without repairing, `ColorPolicy` was never
read, and five `fepdf edit` subcommands reported success while writing nothing.

`ColorPolicy` was hidden rather than advertised while nothing read it, along with a
second ingestion option that shared the condition
([ADR-0007](docs/adr/0007-an-option-that-is-not-read-is-hidden.md)). **Both are read
now** — colour space validation in the refinery consults the policy, and
`status.sh` reports "ingestion options nothing reads: none". One flag stays hidden,
`--vacuum`, and for the opposite reason: the behaviour is unconditional, so what is
missing is the option to *decline* it. Naming a defect is not fixing it —
`./scripts/dev/status.sh` counts them, so the gap is measured rather than remembered,
and this paragraph said "still not read" for as long as nobody checked the row against
the sentence.

That era left a second document behind, and it was worse than the roadmap.
`docs/specs/omissions.md` described "intentional simplifications relative to ISO 32000-2"
and **twelve of its specific claims were checked in one sitting; none held** — CCITTFax
and JBIG2 "fully implemented in Phase 12" through a crate that has never been in
`Cargo.lock`, `RunLengthDecode` listed as unimplemented when it is implemented, ICC
colour management through a dependency that does not exist, an Arlington predicate
engine that was never written. It was archived under `docs/history/archive/` with the
check beside each claim, on the argument that removing the evidence of a documentation
failure removes the proof that it happened — and deleted on 2026-08-29 with the rest of
that directory (ADR-0038), the proof being what git is for.

Nothing replaces it, deliberately. A second document saying what is implemented is a
second place to go stale, and that file is what the second one becomes.

Each phase here therefore states what *done* means in terms that can be measured, and
the current state above is what the code does today rather than what it was intended
to do.

## Phase V — The window, looked at

**Nobody had looked at `fepdf-gui`.** Phase P closed with the layer panel "not visually
verified … nobody has looked at it yet", and the only tool for it had been deleted as
unreferenced. Running the binary and screenshotting it on 2026-09-10 took under a minute
and found four defects that no test could have caught, because each was a thing drawn
rather than a thing computed.

The rules this phase produced, and why they are rows in `CODING.md` §3 rather than a
document of their own, are in
[ADR-0084](docs/adr/0084-the-gui-gets-rules-not-a-rulebook.md).

- [x] **Two icons drew nothing, and the cause was the font stack rather than the
      codepoints.** `U+E8E8` is past the end of `lucide.ttf`, whose last glyph is
      `U+E6FD`. `U+E0FF` is in it — and is also the Ubuntu logo in egui's own
      `Ubuntu-Light`, which sat ahead of the icon font in the proportional family, so the
      continuous-scroll button asked for a grid of squares and got a logo it then failed
      to draw. Two more resolved to the wrong picture: the caliper was `shield-ban` and
      single-page was `layout`.

- [x] **Every selected widget drew its text transparent.** `App::ui` set
      `visuals.selection.stroke = Stroke::NONE` on the root `Ui` each frame, and egui's
      `Style::interact_selectable` assigns that stroke to `fg_stroke` when a widget is
      selected — and `Stroke::NONE` carries `Color32::TRANSPARENT`. The reading-order
      toggle is on by default, so it had always been an unlabelled grey box; so had every
      selected view-mode button. The same six lines discarded the palette's selection
      colour for a grey, so the palette held a constant rustc counted as used and the
      screen never showed.

- [x] **A save that worked emptied the window it worked on.** One
      `error: Option<String>` carried every message, three of its five setters were
      successes, and one branch drew it centred and red **over a canvas emptied of the
      pages it was reporting on**. Nothing cleared it but opening another file. Replaced
      by `Notice { level, text }`, where `Notice::done` is the only way to say a thing
      worked — the whole enforcement of UI-3, and the only rule in that table rustc can
      hold.

- [x] **The palette governed less than half the colours on screen.** Seventeen constants
      against forty literals outside them, twenty-eight of those in `view.rs` — the page
      and everything drawn over it, which is the surface a reader looks at longest. The
      palette's own discipline caught only the other direction: it carries no
      `#[allow(dead_code)]`, so an unused constant is reported, but nothing reported a
      colour that never reached it.

- [x] **A sheet met the canvas at 1.09:1, and only in the tile grid did it have an
      edge.** White paper on the workbench is not a boundary; in the page view there was
      none at all and a page's margin ran into the bench. `steel::EDGE` clears WCAG
      1.4.11's 3:1 without darkening the canvas to the mid-grey that would be needed to do
      it with fill alone.

- [x] **The first screen said nothing, was given something to say, and had it taken
      away again.** `update_vello`'s branch had no `else`: a reader opening the
      application met an empty canvas whose only affordance was a 32-point unlabelled
      arrow in the corner of a 3,024-pixel screen. A centred invitation with three ways to
      accept it was added, and the window's owner asked for it to go: an empty bench is
      what a window with no document in it should look like. The arrow is now a labelled
      control in the rail with a tooltip and an accessible name — which is the half of
      P3 that was actually missing — and dropping a file still works.

- [x] **The reading-order overlay drew nothing for any document as opened, and now draws
      the structure.** The cause was not the window: `USTNode` is
      `fepdf::StructureTreeNode` under another name, the tree arrives whole, and the
      engine filled `rect` from the element's `/BBox` — which no element in any of the
      nine samples declares, because `/BBox` is required of a figure or a table and not of
      a paragraph.

- [x] **The element properties table showed two constants as if they were readings.** A
      document's language read `en-US` and its role map read `Default Mapping` whatever
      the element said, and neither entry had a reader anywhere in the workspace. Both are
      read now. `/Lang` is inherited down from the catalogue as 14.9.2 requires, so the
      answer is the one that applies to the element rather than the entry it happens to
      carry; `/RoleMap` is read once off the `/StructTreeRoot` and shown as the mapping it
      is.

- [x] **The bench's grid had never been drawn in the viewport.** In the viewport path a
      vello texture covers the whole viewport a step later — it has to, because a storage
      texture clears to `(0,0,0,0)` and egui's opaque shader renders that as black — so
      `draw_canvas_grid` and `draw_page_backings` both painted under it. The page fill
      comes from vello either way and the bench colour matches, so what was actually lost
      was the grid, on every document opened in page view since the grid was written.

- [x] **Nobody could look, and that was one problem rather than a list of them.** Every
      "not visually verified" line above needs a click to reach — a drawer, a window, a
      page deleted and put back — and synthetic input does not arrive at this window.
      `capture_ui.sh` was the previous answer and was deleted on 2026-08-29 unreferenced:
      it screenshotted the whole desktop and needed the application running in front of
      someone, so nobody ever ran it.

## Phase W — What a shipping editor does that this one does not

`fepdf-gui` was compared against JUST PDF [編集Pro] on 2026-09-19, ribbon by ribbon, from
screenshots of a working session on an engineering drawing. The list of differences is not
the interesting part of it. **What the list turned up underneath is**, and one item of that
reorders everything else. Every figure below is re-derived on 2026-09-19 and the command
that derives it sits beside it.

Four decisions were taken against that list. Each is contested, each rests on a
measurement, and each therefore wants a record of its own rather than a line here:

| | Decision | Record |
| :--- | :--- | :--- |
| **D-1** | Editing what is drawn on a page is built | [ADR-0085](docs/adr/0085-editing-what-a-page-draws-is-in-scope.md) |
| **D-2** | No OCR engine is built. A window is opened for an external one, and this engine binds what comes back | [ADR-0086](docs/adr/0086-the-engine-does-not-read-a-scan-it-binds-what-does.md) |
| **D-3** | Forms are built through to creating fields, not only filling them | [ADR-0087](docs/adr/0087-a-form-field-is-created-here-not-only-filled.md) |
| **D-4** | Content that a crop or a split puts outside the sheet is removed, not hidden | [ADR-0088](docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md) |

ADR-0085 redraws the line that ["Not planned"](#not-planned) drew against a DOCX
converter: that refusal rests on writing a layout engine, and editing a word in place does
not reach one. Where it starts to is between W-E3 and W-E4 below.

### The critical path is one part nobody has built

```bash
grep -rn "FontFile" crates/fepdf-doc/src crates/fepdf/src --include='*.rs' | wc -l   # 0
grep -c "pub fn " crates/fepdf-font/src/subset.rs                                    # 1
```

**This engine has never embedded a font.** The one function in `subset.rs` is
`subset_tag`, which reads the `ABCDEF+` prefix off a `/BaseFont` name; nothing anywhere
writes a `/FontFile`. Every item below that puts a character on a page is that same
missing part wearing different clothes — a watermark, an annotation's appearance stream,
a form field's value, an edited word, and a text layer handed back by an OCR engine.

**It is already broken in a feature that ships.** Bates numbering is in the GUI's document
tools:

```bash
fepdf edit bates samples/constitution.pdf -o /tmp/x.pdf --prefix "図面-" --digits 4
fepdf inspect text /tmp/x.pdf
```

The extracted text reads `-0001`. The two kanji are gone, and the reader says why twice
before it gets there:

```text
[REPAIRED]  ISO 9.6.2  : the content stream selects /Helvetica, which its resources do not define
[VIOLATION] ISO 9.10.2 : 6 of 16 glyphs drawn on this page have no Unicode value
```

Six is the number of UTF-8 bytes in 図面. `overlay_text_on_page` escapes a Rust `String`
into a literal string and shows it through a non-embedded `/Helvetica`, so each byte
becomes a character code of its own; the fallback substitution then draws a row of Latin
glyphs in the bottom-right corner where `図面-0001` was asked for. The same function writes
every watermark and every header this engine will produce, and a non-embedded standard-14
font is not something PDF/A-4 or UA-2 accepts in the first place.

**Why the reader found `/Helvetica` undefined, measured on 2026-09-19**:
`ensure_helvetica_in_page_dict` put it in the page's `/Font` as a **direct dictionary**,
and the refined read takes a font entry only through `as_reference`, because
`extract_context_fonts` resolves it in a map keyed by object number — which a direct
dictionary has none of. Written indirect, as every other font entry on the page already
was, the repro comes back clean. **The same file read two ways** until it was:
`active_refinement` on gave 13 repairs and off gave none, because a stream refinement does
not reach is interpreted by `fepdf-content`, which resolves the direct dictionary. That
second half is a reading gap and is its own item below.

### What is already there, which is more than it looks

| Part | State |
| :--- | :--- |
| Content streams as an editable value | `SublimatedData::Commands { items: Vec<Command> }`, with `ShowText`, `ShowTextArray` and the TJ offsets kept for vertical text and ruby |
| Writing them back | `serialize_commands`, on the save path in `arena.rs` |
| Positioned text | `PdfDocument::extract_spans` |
| Removing marks inside a rectangle | physical redaction, in `remediation.rs` ([ADR-0064](docs/adr/0064-redaction-removed-the-second-run-of-a-page-and-no-other.md)) |
| Glyph and Unicode machinery | `fepdf-font`: `cmap`, `agl`, `annex_d`, `reconstruction`, `rescue` |

So the round trip that content editing needs — read a page into commands, change them,
write them back — **already runs on every save**. Text editing is not a new path through
the engine; it is an edit to a list.

Two limits in that table set prices further down. Redaction works at the granularity of
**one text-showing operator**, and replaces the string rather than removing it, so a run
that straddles a boundary goes whole — which is exactly what a page split does to a line
of text crossing the cut. Splitting a run at a glyph needs advance widths, which is the
same font work as W-E4. And no `/Annots` is read anywhere in `fepdf-render` or
`fepdf-content` (`grep -rn "Annots"` over both returns 0), so annotations are not drawn at
all, whatever is in them.

### The frontends against the operations

```bash
for f in gui cli mcp; do grep -rhoE 'Operation::[A-Z][A-Za-z0-9]*' "crates/fepdf-$f/src" \
  --include="*.rs" | sed 's/Operation:://' | sort -u | wc -l; done
```

**32 variants; `fepdf-mcp` builds 31, `fepdf-gui` 16, `fepdf-cli` 8** on 2026-09-19.
`ARCHITECTURE.md` §3 reads 30, 12 and 8, which was true when it was written.
`ResizePages` is the one `fepdf-mcp` does not build. Four of the sixteen the GUI leaves
alone are rows of the comparison — `AddAnnotation`, `AddPageDecoration`,
`SetFormFieldValue` and `SetMeasurementScale` — which is to say that part of what is
missing from this product is missing from the window only.

### What is left, and the order it goes in

Twenty-eight entries, reordered on 2026-09-22 after Phase X read the arena. **The rule is
that a thing which makes the rest cheaper or safer to verify goes before a thing which
adds surface.** The list below is that order; the entries keep their names, which are not
sequential and were never meant to be.

**1 — Wrong now, and small.** W-A1 answers from the wrong index space and the answer
reaches a report; W-A2 is a doc comment describing a safety mechanism that does not exist.
Neither is a feature, both are testable in an afternoon, and both are the shape this
project keeps finding: a fallback that answers rather than fails.

**2 — Paid on every entry after it.** W-T1 measured the two gates at 39 minutes with 2.5
of them running tests, and identified the audit's `cargo check --quiet` as a strict subset
of its own clippy pass kept in a separate cache. W-T3 is the same family. Shortening the
gate is a decision about what is verified, which is why it is an entry and not a chore —
but every entry below pays for it until it is taken.

**3 — Find out before deciding.** W-A4 (every arena read is a clone) and W-A5 (the arena
only grows) are measurements, not fixes. Both are cheap, both gate a design decision, and
taking either as read without the number is the mistake `arena.rs`'s own `object_index`
comment exists to record.

**4 — The text pass, which blocks the search.** W-E3a, W-E3d and W-E3e are three ways the
run model is not yet the thing a caller can act on — an `op_index` that carries nothing, a
run that is one or two characters, and extraction that cannot see spacing. W-E3b widens the
corpus once they are right, and **W-15 (finding text) should not be built before them**:
a search over a run model that cannot match a word is a feature built on a defect.

**5 — The editor surface.** W-E2c, W-E5, W-F2-b, W-13, W-G1-b, W-G1-c, and the seven
operations the window cannot ask for. These add surface rather than removing doubt, and
they are ordered among themselves by what the operation vocabulary already carries.

**6 — Accessibility, where the standard is the work.** W-21h can start at any time,
because its first step is **obtaining ISO 32000-1** and not writing code
([ADR-0095](docs/adr/0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md));
it is the one entry here whose blocker is outside the repository. W-22 says it waits on
W-21 and means it. W-19a before W-19b, which is stated in W-19a.

**7 — Last, or out.** W-16, W-17 (no check can be written for whether ink reached paper),
W-18, W-O1, and W-T4 — which is held until Phase W closes, deliberately.

**What this order does not do** is finish Phase W. W-21 cannot close while W-21h waits on a
document nobody here has, and W-22 waits on W-21. That is a real dependency on the outside
and is better stated than worked around.

### The work

Wiring, first, because none of it touches the engine:

- [ ] **Seven operations the engine performs and the window cannot ask for**: headers and
      footers (`AddPageDecoration`), permissions and certificate protection
      (`SaveOptions::permissions`, `::recipients` — both read on the save path),
      stripping descriptive metadata (`::strip`), exporting a page as an image (the CLI's
      `publish render`), verifying a signature (the CLI's `publish verify-signature`), and
      replacing a page (`RemovePages` and `InsertFrom` in one act). `reachability.py`
      fails on any of them that lands without one home.

Then the gate:

- [x] **W-E1a — a font program's own terms are read.** `OS/2.fsType` (ISO 14496-22)
      states what a face permits, and nothing in this engine read it: one write, the
      `OS/2` table `reconstruction.rs` synthesises, and no read. `fepdf-font::embedding`
      reads it, and the nine samples measured on 2026-09-19 are why the ladder below ends
      where it does — 235 embedded programs, 64 stating a permission, **7 of those
      refusing an editable embedding**, and **171 stating nothing at all**, 153 being
      CFF-based `FontFile3`, a format with no such table.

- [x] **W-E1b — subsetting a TrueType program to a glyph set**, out of the SFNT
      disassembler `reconstruction.rs` already has. **Glyph ids do not move**: a subset
      that renumbers has to renumber `cmap`, `hmtx`, every composite's components and the
      PDF's own `/CIDToGIDMap` with it, and each of those is a place to be wrong in a way
      that draws the wrong letter rather than failing. `loca` keeps its length, a dropped
      glyph becomes a zero-length entry, and `glyf` — the bulk of a CJK program — still
      goes. The closure follows composites, and the short `loca` form's halved offsets are
      what the fixtures are for: breaking either fails three tests and one respectively.

- [x] **W-E1b1 — a collection is read from its first font.** Every face this engine finds
      on macOS is a `ttcf` — four of four, holding four to six fonts each, measured
      2026-09-19 — and a table directory read at offset 0 of one parses the collection
      header as a font, so no table is found at all. The face then reads as stating no
      permission and carrying no outlines, which is indistinguishable from a face that
      states neither. With the header resolved, three of the four state `fsType` 0 and the
      Japanese one states an editable embedding.

- [x] **W-E1b2 — subsetting a CFF program**, which is the critical path for Japanese: the
      face on this machine is a CID-keyed CFF of **20,327 glyphs** with a 15-entry
      `FDArray`, and 153 of the samples' 235 embedded programs are CFF besides. The same
      rule as `glyf` — ids do not move, a dropped charstring becomes `endchar` — so the
      charset, `FDSelect` and every count stay true and only the Top DICT is rewritten,
      with five-byte fixed offsets so that one pass settles them. **19,409,608 bytes to
      105,011 for eleven glyphs.**

- [x] **W-E1b4 — a document's fonts were counted twice.** `inspect info` reported **24
      fonts for `samples/constitution.pdf`, 72 for `fugaku.pdf` and 14 for
      `print_sample.pdf`**, against 12, 36 and 7 with `--no-refinement`: exactly twice, on
      every sample, from a tool whose business is telling people what is in their
      documents. `commit_to_arena` gives every dictionary it refines a new handle, so the
      one it replaced stays in the arena unreferenced, and `list_fonts` walked every handle
      there.

- [x] **W-E1b3 — the document's own program is reachable, and this entry was wrong about
      it.** It said `Document::get_font` answers with no program for all 335 font
      dictionaries of the nine samples. Two things were wrong with that. **36 of them are
      Type 3 fonts**, whose glyphs are content streams and which have no program by
      definition (9.6.4); and the field it read is the one `initialize_lifecycle`
      deliberately *releases* — the engine patches the program into `reconstructed_data`
      and drops `data` rather than carrying both.
      Recorded as [ADR-0039](docs/adr/0039-the-design-document-was-narrating-its-own-corrections.md).

- [x] **W-E1c-i — the numbers a descriptor states.** `fepdf-font::metrics` reads
      `head.unitsPerEm` and its bounding box, `hhea`'s ascent, descent and
      `numberOfHMetrics`, `OS/2.sCapHeight` where the table is version 2 or later, and
      `post`'s angle and fixed pitch where there is a `post` at all — a subset this engine
      writes has none, so the angle is absent rather than zero, which is a different
      claim. `advance_width` reads `hmtx`, **including its tail**: a face states one
      advance for every glyph after `numberOfHMetrics`, which is most of a CJK face, and a
      reader that stops at the end of the array gives all of them no width and sets text
      on top of itself. Scaling to glyph space is the caller's, because this crate carries
      no PDF notion.

- [x] **W-E1c-ii — the font dictionaries.** `apply::font::embed_truetype` subsets the
      program, writes `/FontFile2` with its `/Length1`, a `/FontDescriptor`, a
      `CIDFontType2` with `/W` and `/CIDToGIDMap /Identity`, a `/ToUnicode` CMap, and the
      Type 0 font over them with `/Encoding /Identity-H`. **Reading 9.8.1 first is what
      kept two numbers honest**: `/StemV` is written as 0, which the clause itself defines
      as unknown, rather than the estimate every other tool puts there; and `/ItalicAngle`
      is required, so a program with no `post` gets 0 *and* an `Ambiguity` naming the
      clause, because 0 is the claim "upright" and not an absence.

- [x] **W-E1c-iii — the widths agree, both ways.** 9.7.4.3 requires `/W` to be consistent
      with the program's own widths, and the round trip checks exactly that: the writer
      reads `hmtx` and the test reads it again through `fepdf-font::metrics`, which is
      different code. The fixture is drawn on a **2048** grid, so a writer that passed the
      program's units through without scaling into glyph space agrees with `hmtx` and
      still fails.

- [x] **W-E1c-iv — a CFF face is embedded as a `CIDFontType0`.** `/FontFile3` with
      `/Subtype /CIDFontType0C`, no `/CIDToGIDMap` — 9.7.4.2 gives that to a
      `CIDFontType2` and a charstring index is reached through the program's own charset
      instead — and a `/CIDSystemInfo` taken from the program's `ROS` rather than declared
      `Identity` over a collection that is not.

- [x] **W-E2f — 図面-0001, on a page, in a file.** The case this phase started from, end
      to end: the face's terms read, the glyphs found through its `cmap`, the CFF subsetted
      to five of its 20,327, embedded, shown by CID, and extracted back as `図面-0001` from
      a 47,654-byte file — with no `9.6.2` or `9.10.2` decision against this engine's own
      output, where the old path produced both. It renders as 明朝 glyphs rather than as
      the row of Latin noise the repro in this phase's opening draws.

- [x] **W-E2g — the face taken out of a collection is the regular one.** Every face this
      machine offers is a collection, and the engine took face 0 of each. That was right
      here and right by luck: Helvetica lists six faces, Times four and Hiragino Mincho
      four — `ProN W3`, `Pro W3`, `ProN W6`, `Pro W6` — and the regular weight happens to
      be first in all four. A collection that listed a bold first would have been set in
      bold without a word about it.

- [x] **W-E1d — the ladder**, which has two rungs rather than three
      ([ADR-0090](docs/adr/0090-the-face-a-document-embeds-is-not-a-licence-to-set-new-text.md)
      amending [ADR-0089](docs/adr/0089-a-face-is-embedded-only-where-it-permits-it.md)):
      a face installed on this machine whose `fsType` permits an editable *and*
      subsettable embedding, then **refuse** — naming the characters, recording a
      `Decision`, substituting nothing. No face is bundled with this engine.

- [x] **W-E2d — one face for an operation, not one per page.** Embedding per run put a
      subset of the same face on every page: thirteen Bates footers took
      `samples/constitution.pdf` from 244,790 bytes to **830,167**, and with one embedding
      of the union of their glyphs it is **277,392** — 32,602 for the face rather than
      585,377. Both callers know every string before they touch the first page, so neither
      had any reason to find out one page at a time. A face already named in a page's
      resources keeps the name it has, or one resource dictionary would carry thirteen
      entries pointing at one object.

- [x] **W-E2a — the decoration's font is written indirect.** The `9.6.2` repairs above
      are gone; `page_decoration_test.rs` fails against the old writer with 13 of them,
      and the three fixtures beside it cannot see the defect at all, which is recorded in
      the test rather than left to be rediscovered.

- [x] **W-E2b — Bates, watermarks and headers onto W-E1.** `overlay_text_on_page` goes
      through the ladder and writes glyph codes into an embedded subset, so what the three
      of them write is embedded rather than a standard-14 name. **The path that wrote
      neither is deleted**: `ensure_helvetica_in_page_dict` had no caller left. What they
      write can now be refused, which is the decision and not a regression — and a
      decoration therefore depends on this machine having an installed face, as do the
      tests that assert one lands.

- [ ] **W-E2c — a direct font dictionary is read.** 7.3.10 lets any object be direct, and
      a file from another producer that writes `/Font << /F1 << … >> >>` reaches
      `fepdf-content` intact and the refined path not at all. What it costs is a
      `FontResource` built from a dictionary handle, where today the map is keyed by
      object number. *Fails if*: a fixture with a direct font dictionary records a
      `9.6.2` repair with `active_refinement` on. **How many files of the corpus carry
      one is not measured**, and is the first task of the item.

Annotations, which are the largest single row of the comparison:

- [x] **W-8 — appearance streams, and the two other things `AddAnnotation` did not do.**
      It had existed for phases with no frontend calling it, so nothing had looked at what
      it produced. It wrote no `/AP` for any of its four kinds — which this engine's own
      renderer skips, so what it made it could not draw; it wrote a `Highlight` with no
      `/QuadPoints`, which 12.5.6.10 makes required and which is the whole of what a text
      markup marks; and it bound `stamp_image_bytes` to `_` and wrote `/Name /Draft`, so a
      caller's picture reached the file nowhere.

- [x] **W-14 — drawing them was already done, and this entry was wrong about that.** The
      engine renders appearance streams: `render_annotations`, eight tests over the flags
      that stop it (`Hidden`, `NoView`, `Print`), the state `/AS` names, and placement on
      `/Rect`, with `pdf20examples/PDF 2.0 UTF-8 string and annotation.pdf` in
      `crosscheck_image.sh`.

- [ ] **W-13 — making them**: note, typewriter, text box, callout, the four text markups,
      ink, shapes, stamp and link. `AnnotationKind` grows from four to twelve.

Content editing, under D-1:

- [ ] **W-E3a — `TextSpan.op_index` carries nothing on the default path.** Measured on
      `samples/constitution.pdf`: with `active_refinement` off, 1007 spans carry 1007
      distinct operator indices — 4, 13, 23, 30, … — and with it **on, which is the
      default, all 1007 carry 0**. A caller that uses the field to locate the operator that
      drew a span is given the same answer for every one of them.

      Redaction is not affected and that is the clue: it reaches the same
      `CollectorBackend` by a path where the indices are real, and a rectangle over one
      corner scrubs two runs rather than all or none. So the field is right in one place
      and empty in another, which is worse than wrong everywhere.

      **W-E3 identifies a run by the text it reads rather than by this index**, which was
      decided before this was measured and is the reason the measurement did not stop it.
      Fixing the field means giving the sublimated command list an index that corresponds
      to the token stream — which is precisely the divergence
      [ADR-0064](docs/adr/0064-redaction-removed-the-second-run-of-a-page-and-no-other.md)
      records, where two ways of counting met at 9 and nowhere else.

- [ ] **W-E3b — the corpus is eleven files and ten documents, and was nine and eight.**
      `samples/sample.pdf` and `samples/constitution.pdf` are byte-identical, same length
      and same MD5, so every figure taken "over the samples" counts that file twice —
      including the ones this phase quotes: 235 embedded font programs, 299 font
      dictionaries that are not Type 3, 291 of them answering with a program. None of
      those claims is wrong as stated, and each is one file less varied than it sounds.

      **Two documents were added on 2026-09-21**, each for a hole the measurement found:

      - `sample_02c.pdf` — one page, an `/AcroForm` of **30 fields** (19 text, 7 button,
        4 choice) and an `/OCProperties`. Until it arrived **no sample carried a form at
        all**, so W-F1 and W-F2 were built and checked against hand-made fixtures only;
        the optional-content panel is in the same position. **Not one of its 30 fields
        carries a `/TU`** — the Matterhorn failure ADR-0087 was taken over, measured on a
        real document for the first time.
      - `02_低段汚水ポンプ電動機.pdf` — 17 pages, **no text at all** on the first five, 51
        images, and page boxes that change from 595×842 to 1684×1190 within the document.
        A scanned drawing set, which W-O1 is about and which the corpus had none of; the
        mixed page sizes are a first too.

      Every figure above this line was measured over the nine, and is a measurement of
      the nine. A figure quoted after it is over eleven files unless it says otherwise.
      `parser_twin_test` reads the directory and so already covers both: the refined and
      raw readers agree on them call for call.

- [x] **W-E3 — changing the text of one run.** `Operation::EditRun { page, run, text }`
      replaces what one show-text operator draws, encoded in the font that run is set in; a
      character it does not draw is refused by name. `runs_of_page` lists the runs a caller
      chooses from, **and is the same walk the edit uses**, so the number read is the
      number acted on — two counters for one thing is the shape of ADR-0064.
      Recorded as [ADR-0091](docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md).

- [x] **W-E3c — the run tools are reachable, and so is the listing they need.**
      `fepdf-mcp` serves `list_runs`, `edit_run`, `split_run`, `delete_run`,
      `merge_runs` and `move_run`. An
      operation nothing can call is one nobody looks at — the state `AddAnnotation` was in
      for phases, and why its three defects waited for W-8 — so the tool surface test asks
      for all four by name.

- [x] **W-E4a — a run split by kerning is still one run**, and the reflow this item was
      written about turned out not to exist.

- [ ] **W-E3d — a run is one or two characters, so matching one matches nothing.**
      Measured over the samples, runs per show-text operator: the median is **1 to 2
      characters**, 26% to 100% of runs are a single character, and `volvo_xc90.pdf` is
      **100% single-character runs with a longest run of 1**. `constitution.pdf`'s longest
      run is four.

      So `EditTextRun` finds nothing a person would ask for. Replacing `日本国憲法` on its
      first page — a word a reader would actually want to change — changes nothing and
      says it succeeded, because those five characters are five runs.

      **The tests pass because the fixtures have a shape real files do not.** One run, one
      word, hand-written. That is the same failure the decoration fixtures had this
      morning, recorded three items above: a fixture that cannot see the defect it is
      standing next to.

      What it needs is matching **across** runs and re-placing the glyphs that follow,
      which is the advance-width work of W-E1c-i applied within a line. That is still the
      editing side of
      [ADR-0085](docs/adr/0085-editing-what-a-page-draws-is-in-scope.md)'s line, and moves
      it nowhere: a paragraph re-flowing across its line breaks remains the open question,
      further away than it looked.

- [x] **W-E4c — cutting one run in two.** `Operation::SplitRun { page, run, after }`.
      **No arithmetic and no new position**: consecutive show-text operators draw from the
      current point, so `(ABCD) Tj` and `(AB) Tj (CD) Tj` put the same glyphs in the same
      places. What it buys is that a caller can name either half afterwards, which is how a
      reader says that part of a run is a thing of its own — without this engine guessing
      that for them ([ADR-0091](docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).

- [x] **W-E4d — taking a run off the page.** `Operation::DeleteRun { page, run }`.

- [x] **W-E4e — joining two runs into one**, the other half of W-E4c.
      `Operation::MergeRuns { page, run }` joins a run with the one after it, so a phrase
      drawn as two runs becomes one name and changing it is one edit instead of two.

- [x] **W-E4f-a — where a run is.** `RunInfo` carries an `origin`, because a run's
      position is cumulative and no operator states it: `Tm` sets the matrix, `Td` and
      `T*` step the line from it, and every glyph drawn advances it by its own width and
      the spacing in force (9.4.2 to 9.4.4). Moving a run needs this; nothing can be put
      somewhere else without something knowing where it is.

- [x] **W-E4f-b — three branches written for a stream that never arrives.** `'`, `"` and
      `TD` are expanded before any edit sees them — `handle_quote_op`,
      `handle_double_quote_op` and `handle_td_op` write them out as `T*`, as `Tw` `Tc`
      `T*`, and as `TL` `Td`. Each was handled anyway, each looked right, and each was
      found only by a mutation that failed nothing: deleting the code entirely broke no
      test, over the samples and a fixture written to use those very operators.

- [x] **W-E4f-c — a cut or a join would have taken glyphs off the page.** `decode` answers
      through `unified_map` and drops a code it has no character for. Measured on the
      first page of each sample, characters the listing loses against what the renderer
      reads: `unicode_16.pdf` **60 of 348**, `volvo_xc90.pdf` 2 of 2381, the other five
      none.

- [x] **W-E4f-d — a run is cut in codes, not in characters**, so the round-trip guard of
      W-E4f-c is gone and the run `unicode_16.pdf` reads short can be cut like any other.
      `MergeRuns` runs the two runs' codes together for the same reason: nothing is read
      and written back, so nothing can be lost in between.

- [x] **W-E4f-e — the literal string writer lost a byte in every CID code holding `0x0D`.**
      `write_literal_string` escaped `(`, `)` and `\` and let a carriage return through
      raw. 7.3.4.2 says an end-of-line inside a literal string is one line feed, so the
      code `0x010D` was written and read back as `0x010A` — a different glyph. On the
      first page of `unicode_16.pdf` that turned `Unicode` into `Ukicode` and `Standard`
      into `Stakdard`, twice, in a cut that had moved a boundary and nothing else.

- [x] **W-E4f — moving a run.** `Operation::MoveRun { page, run, to }`, the last of the
      four and the only one that needed a different foundation.

- [x] **W-T2 — the audit said `AUDIT FAILED` and not which check failed.** Fourteen of
      its checks are `python3 scripts/audit/*.py || ERROR=1`: the script prints its own
      complaint *and* its own summary, and the summary of a failing run reads like a
      passing one — "0 user-facing literals … 0 names with no key" sits two lines below
      the sentence that failed it.

- [ ] **W-T4 — Rust 1.98.1, after Phase W and not before.** Held deliberately: a
      compiler change in the middle of a phase makes every failure ambiguous between the
      work and the toolchain, and there is nothing in 1.98 this phase needs.

      Surveyed 2026-09-21, at 1.97.1 (2026-07-14) against 1.98.1 (2026-09-03):

      - **1.98.0 adds and does not take away** — algebraic float methods, `format_into`
        on the integers with `NumBuffer`, `String::from_utf16{le,be}`,
        `str::substr_range`. `format_into` is named as a standard replacement for `itoa`.
      - **1.98.1 fixes a miscompilation 1.98.0 introduced**: a trait object vtable with a
        null pointer where a function pointer belongs, which is undefined behaviour.
        1.97.1 does not have it. Whenever this happens it goes to 1.98.1, never 1.98.0.
      - **Of the twelve compatibility notes, none reaches this code.** Measured:
        `repr(transparent)` 0 uses, `transmute` 0 uses, `std::env::Vars` 0 uses, glob
        imports 2 and neither ambiguous, and the two hand-written `Ord`/`PartialOrd`
        pairs both have `partial_cmp` returning `Some(self.cmp(other))`, so the
        `derive(PartialOrd)` consistency change cannot bite.
      - **No dependency asks for more than 1.98.** Of 595 external packages, 443 declare
        a `rust-version`: the highest is **1.92** (egui 0.34.3 and its nine siblings, and
        `hayro-jpeg2000`), then 1.88 for the `boa` engine and `darling`.

      **This is a reading of declarations, not a build.** A crate that breaks under 1.98
      breaks on one of those twelve notes, not on its stated minimum, and the insides of
      595 crates were not read. What the survey supports is "nothing found that forbids
      it" — not "it works". The build is the measurement, and it belongs to this item.

- [x] **W-T3 — the toolchain pin has never been one, and the minimum had never built.**
      Both halves are closed, and each turned out to be a different kind of problem.

      **The minimum had never compiled this workspace.** Three documents promised 1.94 and
      `cargo +1.94 check --workspace --all-targets` failed:
      `recursion_bounds_test.rs` passed `&[&String, &str, &str]` to a
      `&[B: AsRef<[u8]>]`, and 1.94 infers `&String` from the first element where 1.97
      picks `&str` and coerces. One line in one test. `msrv_check.sh` had passed
      throughout, because it compared `Cargo.toml`, `README.md` and
      `.rust-toolchain.toml` **to each other** — three documents agreeing is not a
      compiler. Fixed, and `msrv_build.sh` now builds the workspace with the stated
      minimum as part of the gate: **73 s warm**, a hard failure when the toolchain is not
      installed rather than a skip.

      **The pin was a hidden file rustup does not look for.** `.rust-toolchain.toml`, with
      the leading dot, had selected nothing since 2026-08-29 while `stable` did the work.
      It is `rust-toolchain.toml` now, at **`channel = "1.97.1"`**.

      **The floor and the development toolchain are two numbers**, and writing one number
      in three places is what made them one. `rust-version` stays 1.94 — the promise to
      whoever clones this and runs `cargo build --release`, which the README states under
      *Building from source* and which `msrv_build.sh` keeps true. The pin is the other
      number: what the gate runs on, so that `cargo fmt --check` and
      `clippy -- -D warnings` answer the same on every machine. They are version-sensitive
      — rustfmt reflows differently between releases and clippy gains lints — so an
      unpinned gate is an instrument whose reading depends on who holds it.
      `msrv_check.sh` demanded the two be equal, which is what collapsed them; it asks
      that the pin be **at or above** the floor now, and fails on a pin below it or on a
      `rust-toolchain.toml` that is not there.

      **1.97.1 and not 1.98.1, and the distinction is not caution.** Pinning at 1.97.1
      changes no compiler — it is what was already running, so a gate failure after it is
      the work and not the toolchain. Pinning at 1.98.1 *is* a compiler change, mid-phase,
      which is exactly what W-T4 holds against; and W-T4's survey says in its own words
      that it supports "nothing found that forbids it" and not "it works".

      **It cost a download and no rebuild.** `stable-aarch64-apple-darwin` and
      `1.97.1-aarch64-apple-darwin` emit identical `rustc -vV`, so Cargo's fingerprints
      match: `cargo check --quiet` after the rename took **0.68 s**. Clippy rebuilt, its
      cache being its own. `cargo fmt --all --check` came back with no diff, so rustfmt
      1.97.1 agrees with what is committed.

      **W-T4 is now a defined piece of work**: change one line in `rust-toolchain.toml` and
      run the gate.

- [x] **W-T1 — the gate's cost, measured again, and the entry was wrong about all of it.**
      Re-derived 2026-09-23. Every number this entry used to carry was out by enough to
      change the conclusion, and the conclusion has changed: **nothing is taken.**

      | | this entry said | measured 2026-09-23 |
      | :--- | ---: | ---: |
      | audit, `cargo check --quiet` | ~5 min | **34.5 s** |
      | audit, `cargo clippy --workspace --all-targets` | 5 min 06 s | 4 min 26 s |
      | rebuilding one test binary | 12.2 s | **5.23 s** |
      | `cargo test --workspace --no-run`, fully warm | — | 11.75 s |

      **`cargo check --quiet` is 34.5 seconds, not five minutes**, because it builds no
      test target — which is also the whole of why the clippy pass is eight times its
      size. It warms nothing for clippy (285 s after it, 266 s on its own), so the
      duplication this entry named is real; it is 1.5% of the gate rather than 13%.

      **And it is not the duplicate this entry called it.** `--all-targets` is a superset
      in *targets* and a different thing in *feature resolution*: without it the
      dev-dependencies are out of the graph. The two resolutions are identical today —
      `cargo tree -e features --edges normal,build` against `--edges normal,build,dev`
      differs only on `fepdf-fixtures`, which nothing depends on outside dev — and they
      are identical **because `[workspace.dependencies]` states every feature centrally**,
      which nothing enforces. The line stays, with that written above it.

      **Debug information is not the lever either.** `[profile.dev]` carries the default
      `debug = true`; building one test binary under it takes 5.23 s against 4.77 s with
      `debug = "line-tables-only"` and dependencies at `debug = false`. **Nine per cent of
      the marginal case**, against losing variable-level backtraces in every test. Not
      taken.

      **What the minutes actually are.** A fully warm `cargo test --workspace --no-run` is
      11.75 s and one touched test file adds 5.23 s, so the gate's half-hour is not
      per-binary overhead — it is **invalidation**: a change to `fepdf-model` rebuilds
      everything downstream of it, and everything is downstream of it. That is the shape
      of the workspace, not a setting, and shortening it means changing what depends on
      what.

      **Two measurements were thrown away before these stood up**, and both are traps
      worth naming. `cargo test -p fepdf --test X --no-run` resolves features differently
      from `--workspace` and rebuilt the dependency graph: 23 s became 337 s, measuring
      the resolver rather than the profile. And restoring a profile does not give a cold
      build back — Cargo keys artefacts by profile hash and the old set was still there,
      so a "cold" 1885 s and a "warm" 320 s were compared as though they were the same
      run. **Measure with the gate's own command**, or measure something else than what
      you meant to.

      `target/debug` stood at **93G** while both profiles' artefacts were present.

- [ ] **W-E3e — extraction cannot see word or character spacing.** Every
      `set_word_spacing` and `set_char_spacing` on the extraction side is an empty body —
      three of each, in `remediation.rs` and `marked_content.rs` — so a page placed with
      `Tw` reads identically through `extract_spans` whatever the value is. Measured on the
      `"` fixture of `edit_run_test.rs`: the renderer puts the following run at 98.016 with
      `20 Tw` and 78.016 without, and extraction gives the same number for both.

      `fepdf-render` honours both (`lib.rs:788`), which is why the test for W-E4d measures
      through the renderer. A caller reading span positions out of a file that sets spacing
      is given coordinates that are wrong by one space per space.

- [x] **W-E4b — inserting and deleting inside a run** is the four verbs used together,
      and there was nothing left to build. It was carried on the ground that replacing
      part of a run means splitting it and re-spacing what remains: splitting is W-E4c,
      and the re-spacing was measured not to exist (W-E4a) — consecutive show-text
      operators draw from the current point, so the line closes up or opens out on its
      own.
      Recorded as [ADR-0085](docs/adr/0085-editing-what-a-page-draws-is-in-scope.md), [ADR-0091](docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md).

- [ ] **W-E5 — the drawn objects**: moving, scaling, rotating and replacing an XObject
      (`Operation::EditXObject`). *Fails if*: the CPU rasterisation of the result differs
      from the expected image.
- [x] **W-E6-a — a run says what box to click.** `RunInfo` carries `advance` and
      `height` beside `origin`, so a window can draw a frame round a run and a click can
      find one. The advance is a *vector*, because a run is not always horizontal on the
      page: a run turned a quarter turn advances along the page's y, and a box measured as
      a plain width would be wrong in both directions at once.

- [x] **W-E6 — the window for it.** A drawer on the rail lists the runs of the page a
      reader is on, draws a frame round each one, and does four things to the one they
      click: replace its text, cut it in two, take it off the page, join it with the next.
      `./scripts/dev/status.sh` on 2026-09-20: `fepdf-gui` now builds **20** of the
      vocabulary's 37, from 16.

- [x] **W-E6-b — a run is moved by dragging it.** The other four verbs are a click and a
      button; this one is a gesture, so it is on the page rather than in the drawer — a
      reader moving something wants to see where it will land before they let go. The
      frame is drawn at the landing place while the drag is under way.

- [x] **W-F1 — filling a form from the window.** A drawer on the rail lists the
      document's fields and writes what a reader puts in them through
      `SetFormFieldValue`. The engine could already set a value and regenerate the
      appearance ([ADR-0048](docs/adr/0048-reading-and-setting-choice-fields.md)); nothing
      in the window asked it to, so a form could be read, audited and not touched.

- [x] **W-F2-a — creating the nine kinds of field.** `Operation::AddFormField(NewField)`,
      served as `add_form_field`. The `/AcroForm` is written when the document has none,
      with the `/DA` and `/DR` a variable-text field is drawn by (12.7.4.3) — a form
      declaring fields without them is one whose fields a reader sees nothing of.

- [ ] **W-F2-b — tab order and calculation order.** A created field joins `/Fields` in the
      order it was made and the page's `/Annots` in the same; neither is a `/Tabs` nor a
      `/CO`, and 12.5.1 leaves the order unspecified when `/Tabs` is absent.

Page geometry, under D-4:

- [x] **W-10 — cropping.** `Operation::CropPages(pages, CropRegion { keep, outside })`.
      The kept rectangle becomes the new sheet with its lower-left corner at the origin,
      and the content is moved rather than rewritten — a `q <cm>` in front and a `Q`
      behind, the way a resize moves it (7.8.2).

- [x] **W-G1-a — the text a crop puts outside is removed.**
      `Operation::RemoveOutside { page, keep }` takes off a page every glyph whose own box
      does not meet `keep`. The check ADR-0088 asked for now passes, and it failed against
      the behaviour of the day it was written, which is why it was worth writing:
      `print_sample.pdf` page 3 shifted 300 points right renders with its right-hand half
      gone, **78 of its 112 runs end past the sheet's edge**, and `extract_text` returned
      all 428 characters it did before.

- [ ] **W-G1-b — the images a crop puts outside**, re-encoded to the part that remains
      rather than left whole under a clip (ADR-0088).
- [ ] **W-G1-c — the paths a crop puts outside**, clipped and rebuilt rather than clipped
      for display.

- [x] **W-11 — one page into several**, on W-G1 — the operation JUST PDF calls
      ページの分割. `Operation::SplitPage { page, into }` with `PageDivision::Grid` or
      `Regions`, served as `split_page`.

- [x] **W-12 — several pages onto one**, the operation JUST PDF calls ページの結合.
      `Operation::CombinePages(pages, PageArrangement { sheet, columns, rows })`, served as
      `combine_pages`.

- [ ] **W-O1.** Out: the page rasterised, its dimensions, and whatever text is already
      there with its positions. In: `Operation::AddTextLayer { page, items }`, written at
      text rendering mode 3 with a `/ToUnicode` on an embedded font. Two tools on
      `fepdf-mcp`, which already builds 31 of the 32 operations, and a
      `fepdf edit text-layer --json` beside them for callers that are not an assistant.
      *Fails if*: `inspect text` does not return the layer, **or the page's CPU
      rasterisation changes by one byte**. `--cpu` exists to make the second of those
      writable.

Independent of all of the above:

- [ ] **W-15 — finding text in a document.** The studio's search is the only one, and its
      match rectangle is the whole span rather than the match. The fallback in
      `app/mod.rs` that lays words onto a synthetic grid when `extract_spans` returns
      nothing goes with it: invented geometry that a reader cannot tell from measured
      geometry is worse than no geometry.
- [ ] **W-16 — perimeter and area** beside the caliper's distance, and
      `SetMeasurementScale` where a drawing declares one.
- [ ] **W-17 — printing.** No check can be written for whether ink reached paper, and
      this line says so rather than listing a command that cannot fail.
- [x] **W-20 — the snapshot**, which Acrobat calls スナップショット: a reader drags a
      rectangle on the page and what is inside it lands on the clipboard as a picture.
      **A read, not an `Operation`** — nothing about the document changes, so it sits
      beside `extract_text` and `render_page` rather than in the vocabulary Rule D
      governs. Checked before planning: `egui 0.34` carries `Context::copy_image`, so the
      clipboard needed no new dependency and Rule 9 was never in question.

      - [x] **W-20a — a region of a page, rasterised.** `PdfDocument::render_region` takes
        a rectangle in the page's own space and a scale, and answers RGBA pixels with the
        size they came out. `/UserUnit` is deliberately not applied.

      - [x] **W-20b — the gesture and the clipboard.** A drawer on the rail turns the tool
        on, a drag draws the rectangle as it is dragged, and letting go copies what is
        inside it — through the overlay the redaction brush already uses.

      - [x] **W-20c — where it went.** The notice says how many pixels were copied, and a
        region that could not be drawn says so rather than being logged.

- [x] **W-21e — the three numbers were three wrong numbers.** Read from
      `docs/specs/Matterhorn-Protocol-1-1.pdf` on 2026-09-21, after citing it from memory
      for a day: 14-001 is "Headings are not tagged" (`Doc H`, human judgment) where the
      code checks skipped levels, which is **14-003**; 13-001 is graphics not tagged as a
      `<Figure>` where the code checks the missing alternative text, which is **13-004**;
      and 01-002 is "Real content is marked as artifact" where the code checked a
      structure element naming a page that is not there — a condition the protocol does
      not have, because a broken reference is not a way to fail PDF/UA-1. That check was
      removed rather than renumbered to whatever was nearest.
      Recorded as [ADR-0092](docs/adr/0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md).

- [ ] **W-21 — the Matterhorn protocol, past the two failure conditions that exist.**

      Measured 2026-09-21: `MatterhornAuditor` reported **two of the protocol's failure
      conditions** — 13-004 (a `Figure` with no alternative text) and 14-003 (a numbered
      heading level skipped). W-21b takes it to ten of 137 and W-21c to **fourteen**.

      **The protocol has 137 and says 136.** Its tables enumerate 137 failure conditions
      across 31 checkpoints — `cargo test -p fepdf --test audit_scope_test
      every_failure_condition_in_the_protocol_is_counted` derives the number from the
      Index column — and the sentence that totals them is version 1.02's, carried into
      1.1 with the condition 1.1 added but without the arithmetic
      ([ADR-0093](docs/adr/0093-the-protocols-tables-enumerate-137-failure-conditions.md)).
      Counting the `How` column gives **87 `M`, 48 `H` and 2 with no specific test**
      (23-001 and 27-001); the sentence says 47 `H`, and 13-008 — added in 1.1, marked
      `H` — is the one it is short.

      **That split is advice, not a ceiling.** The protocol defines its own `How` column
      as "**not determinative** … the realistic best-practice approach **at the present
      time**", so 87 is where the protocol expected software to reach in 2021 and an `H`
      is not a prohibition. The target is 137 minus the two with no test; what changes
      with `H` is that a finding must say it was decided by a machine, not that it may
      not be made.

      **This is the gap ADR-0087 was taken over, in the large.** A document this engine
      declares PDF/UA-2 conforming is one it has checked fourteen things about — and all
      fourteen are PDF/UA-1 conditions ([ADR-0092](docs/adr/0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md)). `PdfStandard::UA2`
      writes that claim into the catalogue, and the claim is a statement about 137 things.

      - [x] **W-21a — say how much is checked.** `audit_ua2_report` answers an
        `AuditReport` carrying an `AuditScope` beside the findings, and the window shows
        "Matterhorn の 2 / 136 件の失格条件を検査" where the findings are — 14 / 137 since W-21c.
        `found_nothing()` is
        named so that a caller cannot write `findings.is_empty()` and mean "conforms".

      - [x] **W-21b — the failure conditions that need no new machinery.** Eight more,
        taking the auditor from two to ten: **01-007** (`/MarkInfo /Suspects` true),
        **07-001** and **07-002** (`/DisplayDocTitle` absent, and false), **11-002** (an
        `/Alt`, `/ActualText` or `/E` no `/Lang` reaches), **14-002** (the first numbered
        heading is not `<H1>`), **14-007** (both `<H>` and `<H#>`), **17-002** (a
        `<Formula>` with no `/Alt`) and **28-005** (a form field with no `/TU`).
            Recorded as [ADR-0094](docs/adr/0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md).

      - [x] **W-21c — the failure conditions that need the content stream.** Checkpoint
        01's three — **01-003** (an `/Artifact` sequence inside tagged content), **01-004**
        (content carrying an `/MCID` inside an `/Artifact`) and **01-005** (content under
        neither) — with **14-006** (a node holding more than one `<H>`) beside them.
        Fourteen of 137.
            Recorded as [ADR-0095](docs/adr/0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md).

      - [ ] **W-21h — the failure conditions that cite ISO 32000-1.** 09-004, 09-005,
        09-006, 09-007, 09-008, 31-006, 31-008 and 31-027, all `M`, all naming a table or
        annex of PDF 1.7. **The first step is getting the document**: neither sponsored
        bundle carries it — the ISO 32000-2 bundle is 2.0 and the PDF/UA bundle is 14289 —
        and ISO 32000-1:2008 is superseded. Until then they are not written from
        ISO 32000-2's prose about the same types
        ([ADR-0095](docs/adr/0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md)).

      - [x] **W-21g — a condition checked and not broken is a result.** `AuditFinding`
        carries an `Outcome` — `Broken`, `Sound`, `ForAReader` — and `audit_report` adds
        one `Sound` per condition it checked and did not break. **One per condition, not
        one per object**: a reader wants to know 13-004 was examined, not that four
        hundred figures each have their alternative text.

      - [x] **W-21f — the report a reader reads.** The findings were a panel 100 points
        tall in a drawer, which is a list and not a report. What a reader does with a
        finding differs by kind, and a single list is read as though it does not.

      - [x] **W-21d — the ones the protocol expects a person to decide.** All 48 the
        `How` column marks `H`, named and quoted in the protocol's own words, in
        `AuditScope::left_to_a_person`. The panel heads them "あなたが判断するもの (48)"
        after the three outcome sections, and the weak closing line now counts what is in
        *neither* list — 75 of the 137.

- [ ] **W-22 — Well-Tagged PDF (WTPDF 1.0), which this project does not mention.**

      Measured 2026-09-21: `WTPDF` and `Well-Tagged` appear **nowhere in the repository**.

      WTPDF 1.0 (PDF Association, 2024) sits on ISO 32000-2 beside PDF/UA-2 and is not the
      same document: it says what a *well-tagged* file is, where UA-2 says what an
      *accessible* one is, and it goes into ground UA-2 leaves — the nesting of headings,
      the structure of tables, where `/Lang` has to change, what `/ActualText` is for as
      against `/Alt`.

      **It waits on W-21 and says so.** A conformance claim is worth what the checking
      behind it is worth, and fourteen failure conditions of 137 is not a foundation to
      put a second claim on. What can be done first is the reading: the structure tree editor this engine
      already has is most of what a well-tagged file is made with, and what it cannot yet
      express is the list this item starts as.

- [ ] **W-18 — comparing two documents.**
- [ ] **W-19a — the reading order, the language and the lexicon**, assembled for a
      synthesiser: structure order, `/Lang` resolved by inheritance (14.9.2) and the PLS
      lexicon `SetPronunciationLexicon` already writes. Testable without sound, which is
      why it is separate from:
- [ ] **W-19b — the platform's synthesiser.** Every target ships one — AVSpeechSynthesizer,
      SAPI 5 and WinRT, speech-dispatcher — so nothing is built, only bound, and the
      binding belongs in `fepdf-gui` beside `rfd` and `wgpu` rather than in the engine.
      Rule 9 admits a platform API and refuses a vendored library, so the Linux side talks
      to speech-dispatcher over its socket rather than through `libspeechd`.
      *Fails if*: `cargo tree -i cc` finds a new C builder on any of the four targets.

Declined here, for reasons a corpus cannot overturn: **3D and sound annotations**, under
the same clause 13.4 retirement that ["Not planned"](#not-planned) already records, and
**XFA** beside them.

*Done when*: `fepdf-gui` builds 28 of the 32 operations; a document this engine writes
carries no non-embedded font; and every check named above exists and has been shown to
fail against the defect it was written for (Rule 5 of [AGENTS.md](AGENTS.md)).

## Phase X — The arena, looked at

Like Phase V, this phase came from reading rather than from a failure: `PdfArena` was read
end to end on 2026-09-22, at 468 lines, after W-21d. **The structure is sound** — pools
separated by handle type so an array handle cannot read a dictionary, `BTreeMap`
throughout for Rule 10's determinism, a depth limit of 64 on `resolve`
(`crates/fepdf-model/src/object.rs:343`), a lock order stated in a comment at the one
place two locks are taken, and an `object_index` built lazily with the measurement that
justified it beside it.

**What is wrong is the mechanisms meant to close its gaps, which are declared and not
built.** Three of the items below are a fallback that returns a plausible wrong answer
where the type system was supposed to make the question unaskable.

- [x] **W-A1 — two silent wrong answers reach the font census.** One did and one could
      not, and the difference is the entry.

      **The handle fabrication was live.** `extract_font_summary` read
      `find_object_by_dict_handle(dh).unwrap_or_else(|| Handle::new(dh.index()))` — a
      `Handle<Object>` built out of an index from the `dicts` pool when a scan of every
      object in the arena came back empty. It comes back empty for a **direct** font
      dictionary, which 7.3.10 allows and which this engine's own decorations wrote until
      2026-09-19, and `fepdf inspect debug` hands the number to `get_font`.

      The fix removes the question rather than answering it better: `reachable_font_dicts`
      reaches a font through a `/Font` resource entry that is usually a reference, so the
      object handle **was in hand and being thrown away** by `resolve(…).as_dict_handle()`
      before being looked for again by scanning. It carries `ReachedFont { object, dict }`
      now, `FontSummary::object_id` is an `Option<u32>` because a direct dictionary has no
      object number, and `PdfArena::find_object_by_dict_handle` — the only caller gone — is
      deleted with it.

      **The substituted key was not reachable, and finding that out cost a mutation.**
      Five sites read `get_name_by_str(k).unwrap_or(fv)`, `fv` being `/Font`. A test
      asserting the encoding of a font with a `/Font` key and no `/Encoding` passed — and
      **passed with the fallback put back**, because normalisation-at-load builds every
      font and font construction interns all five names through `arena.name` before
      `list_fonts` runs. The document writes none of `/Encoding`, `/BaseFont`,
      `/FontDescriptor` or `/DescendantFonts` and all four are interned when `open`
      returns. The arm is gone on principle; what is tested is the reason it was
      unreachable, so the day interning changes the arm gets a test of its own.

      Three mutations: an object handle fabricated from the dictionary index (caught by
      both new tests), every font reported direct (caught by one), and the substituted key
      restored — which **survived**, and is why the entry above says what it says.

- [x] **W-A2 — `ObjectEntry.generation` described a mechanism that did not exist.** Gone,
      and `ObjectEntry` with it: the struct's other field was the object, so the pool is a
      `Vec<Object>`. The doc comment said "incremented when the slot is reused"; one site
      constructed it, always as `0`, nothing read it, and no slot is ever reused because
      the pool has no free list. `docs/specs/README.md` records that `refinery_engine.md`
      claimed generation bits on `Handle` and that **the claim was struck from the
      document** — the field that made it look true outlived it by three weeks, which is
      [ADR-0017](docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md)'s shape.

      **No test, and that is the honest answer.** Deleting a field nothing reads changes
      no behaviour; what stands behind it is the suite, which passes. Reuse, if W-A5 ever
      calls for it, brings its own check back with it.

- [x] **W-A3 — a handle names the arena it indexes.** `Handle<T>` carries a second `u32`
      saying which arena stamped it, and an arena hands back `None` for one it did not.

      **The exposure was three places, not everywhere.** Enumerated 2026-09-23: of the
      four production sites that build a second `PdfArena`, `load_document` has only one
      in scope and `walk_sections`'s `scratch` never lets a handle out. What is left is
      `ObjectCloner`, which takes two by design, and the two callers that use it —
      `write_out` and `save_linearized`, which carried **the same four lines twice**, a
      source handle and a target handle side by side, four lines above a
      `writer.finish(root, info)` where `*self.inner.root_handle()` would have compiled
      and written a different object. Those four lines are `cloned_for_output` now.

      **The cost was nothing, which is why this was worth doing.** Measured before
      deciding: a `(Handle<PdfName>, Object)` is **48 bytes with a 4-byte handle and 48
      with an 8-byte one** — alignment had already paid for it — and `Object` stays at 40.
      `Handle::new` is 38 sites in `src` rather than the hundreds the entry implied.

      **`Handle::new` keeps its signature and its meaning.** It makes an *unbound* handle,
      which every arena accepts, so none of the 50 call sites changed and a caller that
      computes an index for itself is not refused — `PdfDocument::get_font` takes an
      object number off the command line. What the check catches is a handle **an arena
      stamped**, used against a different one, which is the bug that was reachable.

      **Identity stays the index alone.** `Handle` is the key of every dictionary in the
      engine and a field of `Object`, which is itself the key of the arena's reverse
      index; comparing the arena as well would reorder every dictionary and change what
      `find_object` calls the same object, to separate handles that in practice come from
      one arena. The number is carried so a *lookup* can refuse. It is not what a handle
      is. `#[serde(skip)]`, too: an arena's number means nothing in another process.

      **No `debug_assert` beside the refusal**, though one was written first. It is louder
      in a test build and absent from a release, so the behaviour would differ by profile
      and the test for it would pass in one and fail in the other. `None` holds in both,
      and **`None` is the true answer** — the handle does not index this arena, so this
      arena has nothing at it. What it replaces is the object that happens to sit at that
      index here, handed back as the one that was asked for.

      `a_handle_from_one_arena_reads_nothing_in_another` puts an object at index 0 of two
      arenas and makes them different, so a refusal and a wrong read are told apart. All
      1,189 tests pass unchanged, which is its own measurement: nothing in the suite was
      relying on a handle crossing.

- [x] **W-A4 — every arena read is a clone, and it costs 3% of opening the largest
      sample.** Measured 2026-09-23, and the entry's own framing — 268 call sites — was
      the wrong unit.

      **Opening `samples/intel_sdm.pdf` calls `get_dict` 1,882,351 times**, copying
      3,611,085 entries; twenty pages of text extraction adds 103,727 more. But **four
      sites are 89.7% of it**, and only 50 of the 268 in source run at all:

      | | share | what it was doing |
      | :--- | ---: | :--- |
      | `document.rs` `discover_font_groups` | 36.3% | copying every dictionary to read `/Type` |
      | `ingest` `capture_provenance` | 18.3% | the same, for `/Sig` |
      | `refine::refine_dict` | 17.9% | iterating every entry — it needs the whole map |
      | `ingest` page-and-form scan | 17.2% | copying to read `/Type` and `/Subtype` |

      **`PdfArena::dict_entry` hands back one entry instead of the map**, and the three
      peek-then-discard sites ask before copying. The closure question does not arise: it
      takes the value out under the lock and hands it over, because a closure running
      under the pool's read lock is an invitation to call back into the arena and stop.

      | | |
      | :--- | ---: |
      | `intel_sdm.pdf`, before | **1.2706 s** |
      | after the largest site alone | 1.2460 s |
      | after all three | **1.2328 s** |
      | saved | **37.8 ms, 3.0%** |

      `cargo run --release --example open_timing -- samples/intel_sdm.pdf 5` re-derives
      it. A micro-benchmark agrees independently: a two-entry `BTreeMap` clones in **36
      ns**, so 1.88M of them is 67 ms, and 71.8% of that is 48 ms against the 37.8
      measured.

      **The remaining 17.9% stays.** `refine_dict` walks every entry of the dictionary it
      reads, so there is nothing to peek at; lending the map instead would mean running a
      caller's code under the pool's read lock, and that caller resolves objects.

      **No general borrowing accessor, and the clone stays everywhere else.** Three per
      cent bought by three call sites is worth the ten lines; the same three per cent
      spread over 265 more, most of which never run, is not. What the measurement
      actually found is that the cost was never about the number of call sites.

- [x] **W-A5 — the arena only grows, and here is the number.** Measured 2026-09-23 on a
      page of two runs, a hundred `Operation::EditRun` in one open document:

      | | |
      | :--- | ---: |
      | objects before | 8 |
      | objects after 100 edits | **108** |
      | dictionaries | 17 → **117** |
      | streams held, and their bytes | **102**, 9,333 |

      **One object and one dictionary per edit, and nothing given back.**
      `write_page_content` allocates a new stream for the page's contents and points
      `/Contents` at it; the object it replaced stays, holding the bytes the page used to
      draw. There is no free list, so a document open through an afternoon of editing
      holds every draft of every page it has touched.

      **It is a cost and not a defect, for two reasons, and neither is structural.**

      The writer emits what the document *reaches*, so the drafts never reach the file: a
      hundred edits and one edit write **the same number of bytes**, and reopening the
      hundred-edit file finds 11 objects and the text the last edit made.
      `arena_growth_test.rs` holds that, because a writer that walked the arena instead
      would put every draft in the file and the growth would be on disk as well.

      And every walk of the arena by index runs either at ingest — `decrypt`,
      `ingest::discovery`, `normalize_resources` — before an edit can have happened, or
      over bytes read afresh, which is `FileStructureReport::survey`. **That was not
      always true.** `list_fonts` walked every handle and reported **24 fonts for a
      document with 12**, because refinement leaves the dictionary it replaced behind in
      exactly this way; it reaches fonts the way a reader does now.

      **So no reclamation is built.** What would justify it is a walker that must run
      after an edit and cannot use reachability, or a measurement of a real editing
      session against a real page — this fixture's streams are 90 bytes, and a page of 50
      KB edited a hundred times is 5 MB. `ObjectEntry.generation` was the check reuse
      would have wanted, and W-A2 deleted it rather than leave a field describing a
      mechanism nobody built; reuse brings its own check back with it.

*Done when*: W-A1 and W-A2 have landed, and W-A3, W-A4 and W-A5 each carry a measurement
or a recorded reason for declining. **Met 2026-09-24.** Two of the five were fixes, and
three were measurements that each contradicted the entry that asked for them: the gate's
`cargo check` is 35 seconds and not five minutes (W-T1, next door), the clone-per-read is
four call sites and not 268, and branding a handle costs no memory at all. The phase's own
opening — that what is wrong is the mechanisms meant to close the arena's gaps, declared
and not built — held for all three.

---

*Updated 2026-09-22 (Phase X). The figures above come from the sample corpus, a set of
deliberately malformed files, and the 515 external files Phases G and O fetched; the catalogue,
annotation and form-field counts in Phases J and K were taken by running `inspect
catalog` and `inspect interactive` over all 251 and aggregating the JSON.*

*Finished entries were cut to what they delivered on 2026-09-22 — 4,924 lines to 2,966 —
on the argument [ADR-0039](docs/adr/0039-the-design-document-was-narrating-its-own-corrections.md)
made for `ARCHITECTURE.md`: an account of how a thing went wrong is a record, and a record
belongs in `docs/adr/`. What came out was the before-and-after tables, the mutation lists
and the dated self-corrections; what stayed is the headline, what each entry delivered, and
every link to the record. **Git holds the rest**, and no entry's completion state changed
except W-20, whose three parts were all done.*
