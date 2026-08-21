# fepdf Roadmap

**Goal**: an engine that understands ISO 32000-2 (PDF 2.0) semantically — not merely
one that round-trips it. PDF 1.7 and earlier are **read-only** targets; output is
always 2.0.

That goal is not a predicate, and no run of `status.sh` can report it true or false;
every phase below states a completion condition, and the line they are all under had
none. Phase I gave it the nearest thing that can be measured: **the share of the
constructs a corpus actually presents whose contents this engine reads** — 96% over the
nine samples and 88% over both corpora, after Phase K took the catalogue axis from 5 of
20 to 19 of 20. What is left is 23 annotation entries, on subtypes that occur once or
twice; three filters; and `/Type`. `fepdf inspect coverage` reports it and
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

## Where the engine actually stands

| ISO 32000-2 | State |
| :--- | :--- |
| **7.3** Objects | Complete. Every type in the clause. |
| **7.4** Filters | **Nine of the ten** since Phase M built `CCITTFaxDecode`, `JBIG2Decode` and `JPXDecode` — the tenth is `Crypt`, which the security layer handles (7.6). `inspect structure` takes a census of which filters a file's streams name, so this row is re-derivable per file rather than remembered. Across both corpora: `/FlateDecode` 224 files, `/DCTDecode` 12, `/XXXDecode` 8, `/JPXDecode` 3, `/CCITTFaxDecode` 2, `/LZWDecode` 2, `/ASCIIHexDecode` 1, `/JBIG2Decode` **none** — and every stream carrying a codec this engine lacks is an image, 3 of 3 and 2 of 2. Every filter that is a plain byte transformation decodes. `FlateDecode`, `LZWDecode`, `ASCIIHexDecode`, `ASCII85Decode` and `RunLengthDecode` decode, with Table 8's predictors reaching LZW as they do Flate, and `DCTDecode` reads JPEG; `Crypt` is handled in the security layer (7.6); `ZstandardDecode` is implemented and is not one of the ten. Table 6's abbreviations are matched too — `/AHx` appears seven times in one external file, and only `Fl` and `DCT` were recognised before. The three image codecs were absent until Phase M, declined by Phase L on a measurement that could not speak to the use case that reopened them; an image that still will not decode is skipped rather than aborting the page, and says what it cost. **This row did not exist until a corpus this project did not choose forced it.** |
| **7.5** File structure | **Complete and in use.** Header scan, both cross-reference forms, `/Prev` chains, hybrid references, object streams, incremental updates, and recovery by scanning. `Document::open` reads the file itself; `lopdf` is gone. Recovery has **two** halves since Phase N: a scan finds objects written `N G obj`, and an object stored inside a `/Type /ObjStm` is not written that way — so every container in the file is expanded too, filling holes and never overriding a section that read ([ADR-0006](docs/adr/0006-a-container-may-not-overwrite-a-newer-revision.md)). Having only the first half cost `UnknownFilter-xrefstm.pdf` its page tree, which is object 5 inside object stream 2. |
| **7.6** Encryption | Every password handler the standard defines now decrypts: RC4 (V1/V2), AES-128 (V4/R4) and AES-256 (V5/R5, V5/R6) to Algorithms 1, 2, 2.A, 2.B and 4–6, with `/Perms` checked and both password roles authenticating. Verified against PDFKit on fourteen files; all of it was broken or absent ([ADR-0009](docs/adr/0009-permissions-are-thirty-two-bits-not-a-positive-integer.md)). Writing is AES-256 at revision 6 and nothing else, because output is always 2.0 and this edition deprecates the rest. Public-key handlers (7.6.5) are **read and written** — a `/Adobe.PubSec` document opens with the certificate it was addressed to, which neither Chrome nor Firefox will do, and `--encrypt-to` produces one. Unencrypted wrappers (7.6.7) are recognised and reported. **Clause 7.6 is otherwise complete.** |
| **7.7** Document structure | Every one of Table 29's 32 entries is a field of `PdfCatalog`, and after Phase K **20 are modelled** — the field's type says what the entry holds. Measured against the 251 files of both corpora, 12 of the 32 keys occur in no file at all and are **declined a reader** for that reason, recorded in the code as `catalog::ABSENT_FROM_BOTH_CORPORA`; of the twenty keys those files do carry, **19 are modelled**, and the one that is not is `/Type`, whose value 7.7.2 fixes at `/Catalog`. That figure is qualified where it is printed: `inspect catalog` reports, per entry, how much of the entry's *own* table its reader covers — `/AcroForm` is modelled and reads 4 of Table 224's 8 ([ADR-0020](docs/adr/0020-a-modelled-entry-reports-how-much-of-its-own-table-it-reads.md)). |
| **PDF 2.0 additions** | `inspect catalog` reports **zero** `type only` entries, and the reason is not that the six gained readers. `PageLabels`, `Threads`, `OutputIntents`, `OCProperties`, `Collection` and `AF` each became an `Option<Object>` field, which moves them from "no field" to "a field whose contents are opaque" — the spec types (`PageLabelSpec`, `ArticleThread`, `OutputIntent`, …) are still not what the catalogue reads into. `DPartRoot` has no type at all, contrary to what this table said before it was checked. Phase K then gave `PageLabels`, `Threads`, `OutputIntents` and `OCProperties` real readers — the four of the six the corpora present, and `/OCProperties` was the one of those four that nothing read *from*, until Phase N made the renderer enter through it — and declined `Collection` and `AF`, which they do not. Of the six, `PageLabels` and `Threads` were the only ones to occur when the corpus was the nine samples — 2 files and 1. Across all 251 the order changes: `OutputIntents` 64, `PageLabels` 4, `OCProperties` and `Threads` 1 each, and `Collection` and `AF` still zero, as do `DSS` and `DPartRoot`, which have fields anyway — the container-before-contents shape Phase D was ordered to avoid. |
| **8–14** Content, text, interactive, tagged | Interpreter, fonts and UA-2 auditing exist; interactive features (12) can be *read* (`inspect interactive`), and signature fields (12.7.5.5, 12.8) can now be written and checked. Named destinations (12.3.2) **resolve**, through both of 12.3.2.3's forms and the name tree (7.9.6) one of them needs — which found a link in `intel_sdm.pdf` that goes nowhere, `(G3.7717)`, referenced three times and declared in none of that file's 279,501 destinations. Every page of every sample now yields its text: `scn` with a pattern name (8.6.8.2) was read as a grey component, which cost `fy05.pdf` six pages and — because extraction stopped at the first failure — the 718 after them. A pattern is now painted, through `Paint::Pattern(PatternSpec)`. `samples/` exercises one annotation subtype — all 29,973 of its annotations are `/Link` — and this row said that of *the corpus* for as long as there was only one. The 242 external files carry 82 annotations across **16** subtypes and four terminal form fields, so both walks now run on files this project did not choose. `PdfAnnotation` held seven entries of Table 166 and no `/AP`, which made a `/Redact` and a `/Watermark` the same object to this engine; Phase J took it to all nineteen, added Table 172 for the markup subtypes and readers for the six subtypes either corpus writes more than once, and the form walk now reads `/V`, `/DA`, `/Ff` and `/T` with 12.7.4.2's inheritance. **17 distinct entries across the remaining subtypes still have no reader**, each on an annotation that occurs once or twice, and `inspect interactive` names them per subtype. Every one of the 30,055 annotations parses. **Optional content is now honoured while drawing.** `BDC` discarded its property list, so an `/OC` section was painted whether its group was on or off, and `/OCProperties` — read since Phase K — was consulted by nothing. Clause 8.11 is read: the default configuration's `/BaseState`, `/ON`, `/OFF`, `/Intent` and `/AS`, membership dictionaries with all four `/P` policies and `/VE` expressions, and the `/OC` entry on an XObject. Thirteen constructions were put to PDFKit and it honours **two** of them, so eleven are held to the clause by 26 tests rather than to a second implementation ([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md)). Nothing is hidden on a doubt: an `/OC` that will not resolve draws and records a `Decision`. |
| **14.3** Metadata | Settled at load into one state: `/Info` and the metadata stream are reconciled, disagreements recorded, and the entries 14.3.3 deprecates moved to where that clause puts them ([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)). Text strings decode to 7.9.2.2 — PDFDocEncoding from Annex D, or a byte order mark — after a Shift-JIS detector was found corrupting a conforming `/Title`. `--strip` removes every metadata stream, not the catalogue's alone. |

One measurement worth carrying forward: all 24 `Operation` variants are fully
implemented and verified. In the engine (`fepdf-model`, `fepdf-syntax`)
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

*Done when*: for any PDF 2.0 feature the engine claims to support, there is a command
that shows it. Reading a feature is the precondition for writing it correctly.

`inspect encryption` moved to Phase C rather than being dropped, and landed there: a
report on a handler that could not then open a conforming file would have described the gap
rather than the feature. Two of the three defects Phase C then found were invisible
precisely because the file *opened*, so the command now states conformance per file —
against what the code implements, not what the dictionary declares.

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

*Done when*: an AES-256 document written by Acrobat round-trips, and one written by
fepdf opens in Acrobat.

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

*Done when*: **done.** All three defects that motivated this are caught by the script,
verified by putting each back. The third finding is the one worth carrying: a round-trip
comparison is blind to anything the reader gets wrong consistently, so "compare in with
out" needed "and check it worked" beside it.

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
      is a maintenance cost without a user (all 24 operations now implemented and verified)
- [x] `color_policy` is the last ingestion option nothing reads, and `status.sh` counts
      it. ADR-0007's terms apply: implement the colour validation it was meant to govern,
      or delete the option and the enum. Clause 8.6 colour space validation in active
      refinement now actively reads `color_policy`, un-hiding `--relaxed-color`

*Done when*: `Operation` has no stubs, and `fepdf edit --help` lists only working
commands because they all work. *(complete)*

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
      Owns the `Operation` vocabulary (all 24 operations implemented and active),
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

*Done when*: the filters clause 7.4 lists either decode or are declined for a stated
reason, and a run over the external corpus panics zero times from a debug build.

## Phase H — A decision the interpreter takes is still a decision

`ARCHITECTURE.md` §5.3 says a departure from the standard is recorded rather than
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

*Done when*: **done.** Each of the four files that skips an image says so as a
`Violation` naming the filter — `/CCITTFaxDecode`, `/JPXDecode` and `/XXXDecode`, one
per file — citing **7.4** when the filter is not in the filter table and **8.9.5** when
it is one this engine has and the image dictionary still could not be honoured. The
decision-site row moved with it, and `measure_external_corpus.sh` panics zero times from
both builds.

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
(`external/arlington/tsv/latest`, 613 object definitions, already a submodule) says what
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

*Done when*: **done.** The goal line has a number — **61% over `samples/`** (17 of 28
constructs) and **82% over both corpora** (190 of 231), with the per-axis rows above it
because the total is weighted by how many constructs an axis presents. The measurement
immediately said something the prose had not: catalogue entries are 5 of 20 across both
corpora, which is the weakest axis by a distance and is what Phase K is for.

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

*Done when*: **done.** `inspect interactive` reports, per subtype, which entries the file
writes and which of them were read — `/Link`, `/Popup`, `/Circle` and even
`/SomePrivateCustomAnnotationType` now read every entry they carry, and 17 distinct
entries across the remaining subtypes have no reader and are named. The claim is checked
rather than derived: every one of the 30,055 annotations is parsed into `PdfAnnotation`,
0 fail, and injecting a defect into `/Border` takes `volvo_xc90.pdf` to 844 of 844.

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

*Done when*: **done.** `status.sh` reports 19 of the 20 keys a corpus carries, beside the
12 it declines and the 32 it declares — three numbers where one used to stand.
`crosscheck_selfread.sh` compares what the catalogue *says* across a round trip rather
than which keys survived, and on its first run found the one difference ADR-0013 predicts
— `bokutokitan.pdf`'s inherited `/MediaBox` — which is how a check earns the claim that
it can see contents. The catalogue axis of the coverage index went **5 of 20 to 19 of
20**, and the index as a whole from 82% to **88%** over both corpora, 96% over
`samples/`.

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
      page"*. Across both corpora that is the whole bill for the two missing codecs:

      | File | Filter | Cost |
      | :--- | :--- | :--- |
      | `382252…` | `/CCITTFaxDecode` | 3.9% of its page |
      | `4387ba…` | `/CCITTFaxDecode` | 3.9% of its page |
      | `UnknownFilter-Linearized` | `/JPXDecode` | 11.2% |
      | `UnknownFilter-objstm` | `/JPXDecode` | 11.2% |
      | `UnknownFilter-xrefstm` | `/JPXDecode` | **never reached** — see below |
      | `JBIG2Decode` | — | nothing; it occurs in no file |

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

*Done when*: **done.** The engine reports the codec it lacks, on the file that needs one,
with what it cost — and the answer is small enough that "not built" is now a measurement
rather than a preference. Building them stays gated on those pages mattering, which is
what the Phase G entry said and what this makes checkable.

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

What remained open was not a codec, so it was not here: `/SMaskInData`, the file
`crosscheck_image.sh` was red on, and the visual suite that does not run moved to Phases N
and O — sorted by what they *do* rather than by which phase happened to find them. The
first two are done; the third is Phase O-3.

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

      Built as `fepdf-model::optional_content`, with the gate behind the backend trait
      (`fepdf-content::canvas`) so a painting site cannot be added without it. Marks are
      withheld; `q`, `Q`, `cm`, the clip stack and the colour still run, because the
      operators after `EMC` inherit what the hidden ones left. **Thirteen constructions
      were put to PDFKit and it honours two** — a group in `/OFF` and a `/BaseState /OFF`
      — painting an `/OC` on an XObject, every OCMD policy, a `/VE` expression, a
      `/Usage` applied through `/AS`, and a section nested inside a hidden one. Those two
      are fixtures for `crosscheck_image.sh` with a control; the other eleven are 26 tests
      against the clause
      ([ADR-0021](docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md))
- [x] **A page tree inside an object stream is not recovered.**
      `UnknownFilter-xrefstm.pdf` reported no pages while PDFKit rendered it. Its `/Pages`
      is object 5, which lives inside an object stream, and the cross-reference that says
      which container is written with `/XXXDecode`. The recovery scan looked for
      `N 0 obj` in the bytes and an object inside a compressed container is not there to
      be found.

      Fixed as the entry said: every `/Type /ObjStm` the file contains is found and what
      it carries is adopted, filling holes and never overriding a section that read
      (ADR-0006). The adoption adds `InObjectStream` records rather than writing objects,
      so the expansion goes through the ordinary path and the existing guard covers it
      unchanged. Containers are found by searching the bytes for `/ObjStm` and attributing
      each hit to the nearest preceding `N G obj` — a false positive is rejected by the
      parse, and parsing every object to ask would cost `intel_sdm.pdf` 332,814 parses on
      a path that exists for damaged files. **`crosscheck_image.sh` is now green on every
      file it can compare**, and `UnknownFilter-Linearized.pdf` recovered three more
      objects on the way
- [x] **Headless rendering fails on a small page.** A 64×32 page produced *"Copy at
      offset 0 for 8192 bytes would end up overrunning the bounds of the Source buffer of
      size 1024"* from wgpu. Worked around by enlarging a fixture, which is not a fix and
      not an explanation — and, it turns out, not a workaround either.

      **It was neither a small page nor a rendering defect, and the arithmetic names it.**
      A 64×32 image at *one bit per component* decodes to 256 bytes; a backend reading one
      byte per pixel makes 256 pixels of RGBA out of them — 1024 bytes — against a texture
      of 64×32 that wants 8192. Eight times too short, which is the same `/DeviceGray`
      scan defect Phase M records fixing in the commit that filed this entry; nobody
      connected the two. The page size never entered into it, and enlarging the fixture
      did not help: 256×128 fails identically, by the same factor, which is how the
      "workaround" was shown to be a coincidence.

      Reproduced by reverting Phase M's two fixes and getting that message back **byte for
      byte**, then closed by the thing that was actually missing: neither
      `expand_sub_byte_gray` nor the short-buffer guard beside it had a single test.
      `crates/fepdf/tests/image_sample_count_test.rs` covers both, at the level the defect
      lives at — the bytes handed across the backend contract, which needs no GPU — and
      each half fails when the other is removed
- [x] **The layers the engine *writes* have no content in them.** Found by the optional
      content work above. `apply_update_layers` wrote the OCG dictionaries and a default
      configuration with `/ON`, `/OFF` and `/Order`, and **nothing was ever marked `/OC`**
      — no content stream, no XObject, no annotation — so every group it created was empty
      whatever its state. `LayerGroup::printable` was dropped on the floor with it, and
      `LayerGroup::id` was never used at all.

      `Operation::AddPageDecoration` gained a `layer`, naming a group by its `/Name`, and
      wraps the overlay in `/OC … BDC`/`EMC` with the group reached through the page's
      `/Properties`. `printable` reaches the file as a `/Usage` `/Print` state **and** the
      `/AS` entry that applies it — 8.11.4.5 puts the acting in the application, so
      without the second the first is a description nothing consults. `LayerGroup::id` is
      gone: Table 96 gives a group `/Name` and nothing else, so a second identifier had no
      slot in the file to reach and no operation could have referred to a layer by it.
      Naming a layer the document does not declare is refused rather than drawn
      unconditionally.

      Held by a **round trip**: the file is written, opened again and drawn, and the
      decoration reaches the backend exactly when its layer is on. Found on the way and
      fixed with it — the decoration path reached for `/Resources` on the page dictionary
      alone, so a page whose resources are indirect had them *replaced* and a page that
      inherits them had them *shadowed*; adding a header could blank the page it was
      added to
- [x] **`/SMaskInData` is not implemented.** Its default is 0 — ignore any alpha the
      codestream carries — and a JPX image asking for 1 or 2 got that treatment
      silently, so a transparent image was drawn opaque.

      All three of Table 89's values are read now: 1 keeps the fourth channel as the
      image's soft mask, and 2 does the same after dividing the alpha back out of the
      colour, because the backends here take straight alpha. A value the table does not
      define, a file claiming a mask its codestream does not carry, and a file carrying
      both `/SMaskInData` and `/SMask` — which 8.9.5.2 forbids together — are each read
      the safe way and **recorded**, which is the half that was missing more than the
      decode was. Checked against codestreams encoded by **OpenJPEG**, not by the decoder
      under test, following `make_scan_fixtures.rs`: neither corpus carries a
      `/SMaskInData` at all and its one JPX image is plain RGB, so there was nothing here
      to check against

*Done when*: **done.** `crosscheck_image.sh` is green on every file it can compare — 13
compared, 1 without a second opinion, where it was red on `UnknownFilter-xrefstm.pdf`. A
page with a hidden layer renders without it. The small-page failure is explained in a
sentence that names the cause, and the two fixes that had already closed it without anyone
noticing now have tests. A layer this engine writes contains something, read back by the
reader that honours it. And a transparent JPX image is no longer drawn opaque.

Five entries, and the last two were found by fixing the first three — an engine that
honours a construct correctly is the thing that can tell you it never wrote one, and
reproducing a failure is the thing that can tell you it was filed under the wrong cause.

## Phase O — The holes in the checking

Phase M could not check its own work against anything it had not written, and said so.
That is not a scanned-image problem; it is the same hole in four places.

- [ ] **Neither corpus contains a business document.** No attachment, no long-term
      signature, no choice field, no layer, no redaction — which is why every one of
      those reads as "zero occurrences" and was declined on that basis. Fetching a corpus
      that contains them is what turns the decisions in "What the engine promises" from
      guesses into measurements. `pdf-association/pdf20examples` remains the candidate,
      and the rule from Phase G holds: it lands in `target/`, never in `samples/`
- [ ] **JBIG2 has never met an image this project did not assemble.** CCITT and JPX were
      confirmed against real files of the external corpus; JBIG2's only evidence is a page
      built segment by segment in a test, which checks the decoder against its author's
      reading of T.88 and nothing else. `/SMaskInData` was the same shape and is no longer
      — its fixtures come out of OpenJPEG — but **no file of either corpus carries one**,
      so what has never been measured is how producers actually write it
- [ ] **`verify_visuals.sh` runs a test target that does not exist.** `visual_regression`
      is not in `fepdf-render`; the script cannot pass and has not for as long as anyone
      has run it. `crosscheck_image.sh` covers images against PDFKit, and text, layout
      and colour are covered by nothing
- [ ] **The rest of `docs/specs/` is unaudited.** `omissions.md` was checked and twelve
      of its claims were false, so it is archived. Four documents of the same era —
      `sdk_design.md`, `app_design.md`, `refinery_engine.md`, `charter_redesign_*.md` —
      have not been read against the code

*Done when*: every check in `TESTING.md` can pass, a corpus that contains a business
document has been measured against, and no document under `docs/specs/` makes a claim
that a command contradicts.

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

| | What is not read | What makes it necessary |
| :--- | :--- | :--- |
| **P1** | **`/Ch` choice fields.** A dropdown or list box is walked, and its options and value are not read | Any government or business form. Declined from a corpus containing four form fields in total |
| **P2** | **`/AF` associated files (14.13).** An attached file — the XML of an e-invoice, a source document — is reachable and not read | Any document that carries another, and reading a PDF/A-3 even where one cannot be written |
| **P3** | **`/DSS` and `/Perms`.** Long-term validation data, and the permissions a signature sets (DocMDP) | *Reporting* on a signed document somebody else produced — which survives the decision above, where producing that data does not |
| **P4** | **What a document *does* when opened.** `/JavaScript`, `/Launch` and embedded-file payloads are counted by kind and never read | Security screening. The coverage index excludes actions because "reads an action" has no settled meaning (ADR-0019); "does this run code" has one |

None of the four can be sized until Phase O fetches a corpus that contains a business
document, which is the same sentence in both directions: they were declined for want of
one, and they cannot be planned without one.

## Not planned

What is left after the rule above: refusals resting on the nature of the thing rather
than on how often a corpus happened to contain it.

- **A DOCX converter.** The `DocumentSource` boundary exists so one has a place to go
  (`ARCHITECTURE.md` §5.2), but writing it means a layout engine — style resolution,
  line breaking, pagination — which shares almost nothing with reading PDF.
- **`fepdf-wasm` as a peer frontend.** Forty lines with an unimplemented renderer.
  Whether to build it is a product decision, not an architectural one.
- **Writing PDF 1.7, and the faithful-copy path.** Output is 2.0 and earlier versions
  are read-only; a file this engine did not write is not signed
  ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)). Decided rather
  than deferred, and what it costs is written out under "Read broadly, write 2.0" —
  PDF/A-3 output, and therefore an e-invoice for a recipient who will not take PDF/A-4f.
- **Reading an entry no corpus carries and no use case names.** Twelve keys of Table 29
  are declined in the code (`catalog::ABSENT_FROM_BOTH_CORPORA`) and ten annotation
  subtypes occur once each. `/AF`, `/DSS` and `/Perms` were on that list until the rule
  above moved them; the rest stay, and a test holds the line from the other side.
- **Multimedia: `/Movie`, `/Sound`, `/Screen`, `/3D`.** Clause 13.4 is deprecated in
  2.0, and reading it would be building for a subsystem the standard is retiring.
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
engine that was never written. It is archived under `docs/history/archive/` with the
check beside each claim, rather than deleted: removing the evidence of a documentation
failure removes the proof that it happened.

Nothing replaces it, deliberately. A second document saying what is implemented is a
second place to go stale, and that file is what the second one becomes.

Each phase here therefore states what *done* means in terms that can be measured, and
the current state above is what the code does today rather than what it was intended
to do.

*Updated 2026-08-22. The figures above come from the sample corpus, a set of
deliberately malformed files, and the 242 external files of Phase G; the catalogue,
annotation and form-field counts in Phases J and K were taken by running `inspect
catalog` and `inspect interactive` over all 251 and aggregating the JSON.*
