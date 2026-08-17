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
| **7.6** Encryption | Every password handler the standard defines now decrypts: RC4 (V1/V2), AES-128 (V4/R4) and AES-256 (V5/R5, V5/R6) to Algorithms 1, 2, 2.A, 2.B and 4–6, with `/Perms` checked and both password roles authenticating. Verified against PDFKit on fourteen files; all of it was broken or absent ([ADR-0009](docs/adr/0009-permissions-are-thirty-two-bits-not-a-positive-integer.md)). Writing is AES-256 at revision 6 and nothing else, because output is always 2.0 and this edition deprecates the rest. Public-key handlers (7.6.5) are **read and written** — a `/Adobe.PubSec` document opens with the certificate it was addressed to, which neither Chrome nor Firefox will do, and `--encrypt-to` produces one. Unencrypted wrappers (7.6.7) are recognised and reported. **Clause 7.6 is otherwise complete.** |
| **7.7** Document structure | **14 of Table 29's 32** catalogue entries typed, measured by `status.sh` from `PdfCatalog`. Untyped entries survive a round trip but cannot be reasoned about; `inspect catalog` names which ones, per file. |
| **PDF 2.0 additions** | **Six** catalogue entries have a spec type but no read or write path — `PageLabels`, `Threads`, `OutputIntents`, `OCProperties`, `Collection`, `AF`. `DPartRoot` has no type at all, contrary to what this table said before it was checked. `inspect catalog` reports the six as `type only`. Of the six, only `PageLabels` (2 files) and `Threads` (1) occur in the corpus at all; `DSS`, `AF` and `DPartRoot` occur zero times, which is why Phase D types by measured occurrence and not by which clause added them. |
| **8–14** Content, text, interactive, tagged | Interpreter, fonts and UA-2 auditing exist; interactive features (12) can be *read* (`inspect interactive`), and signature fields (12.7.5.5, 12.8) can now be written and checked. Every page of every sample now yields its text: `scn` with a pattern name (8.6.8.2) was read as a grey component, which cost `fy05.pdf` six pages and — because extraction stopped at the first failure — the 718 after them. A pattern is consumed but not painted. The corpus exercises one annotation subtype of ~28 — all 29,973 are `/Link` — and no form field at all, so the form walk is exercised by a fixture and by this engine's own signed output. |
| **14.3** Metadata | Settled at load into one state: `/Info` and the metadata stream are reconciled, disagreements recorded, and the entries 14.3.3 deprecates moved to where that clause puts them ([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)). Text strings decode to 7.9.2.2 — PDFDocEncoding from Annex D, or a byte order mark — after a Shift-JIS detector was found corrupting a conforming `/Title`. `--strip` removes every metadata stream, not the catalogue's alone. |

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

- [ ] Type the remaining catalogue entries. **In the order the corpus uses them, which
      is not the order this line first gave.** It said `DSS`/`AF`/`DPartRoot` first
      because they are the 2.0 additions; counting occurrences across the nine samples
      says those three appear **zero** times, and typing them first would be a container
      built before its contents — the shape this codebase keeps paying for. Measured
      order: `ViewerPreferences` (6 files, done), `PageMode` (4, done), `Lang` (4, done),
      `PageLayout` (2, done), `Dests` (1). `Type` is in all nine and is the constant
      `/Catalog`, so it wants validating rather than typing
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
- [ ] Implement operations in order of how much of the standard they unlock:
      catalogue edits (`UpdateOutlines`, `SetOutputIntent`, `UpdateLayers`,
      `SetPageLabels`) before page elements (`AddAnnotation`, `SetFormFieldValue`)
      before content synthesis (`ApplyBatesNumbering`, `AddPageDecoration`)
- [ ] Un-hide each CLI subcommand as its operation lands
- [ ] Decide the fate of the operations no frontend reaches; an unreachable operation
      is a maintenance cost without a user
- [ ] `color_policy` is the last ingestion option nothing reads, and `status.sh` counts
      it. ADR-0007's terms apply: implement the colour validation it was meant to govern,
      or delete the option and the enum. It is not "find the code that reads it" — there
      is none

*Done when*: `Operation` has no stubs, and `fepdf edit --help` lists only working
commands because they all work.

### Tooling debt carried from Phase C

- [ ] `scripts/test/make_pubsec.py` rewrites PDF syntax with regular expressions. Two of
      its bugs looked like engine defects before being traced — it left dictionary
      strings unencrypted, and it found the end of a stream by searching for `endobj`,
      which compressed data contains by chance. Reading the file's own cross-reference
      table instead would be exact, and is not the "real parser" it was dismissed as
      needing: the table already says where every object begins
- [ ] The same script does AES in pure Python, so `intel_sdm.pdf` is skipped by size.
      Only worth fixing if a defect is ever suspected to be size-dependent

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
- **A faithful-copy path, and signing documents this engine did not produce.** The two
  are one question: byte fidelity buys nothing else that another route does not, and
  editing a signed file still reports as changed since signing whatever is preserved. A
  tool that never rewrites the file is the right place for that, and there are such
  tools ([ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md)). Signing
  fepdf's *own* output was the part worth having, and it is done.
- **Painting a pattern.** `scn` with a pattern name is consumed and the fill left
  unchanged. Painting one needs `Color` to be able to say "a pattern" and the backends
  to render it; adding the variant first would be a container before its contents.
  Text extraction, which is what the corpus exercises, does not depend on it.

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
