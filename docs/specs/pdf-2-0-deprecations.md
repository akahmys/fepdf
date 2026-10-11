# What PDF 2.0 deprecates, and what this engine does with each

The translation rule for every feature ISO 32000-2 deprecates or removes: where 2.0 puts
its replacement, what this engine should do with it, and what it does now. **A living
table** — when work changes how an item is treated, its row changes with it.

Where this disagrees with [ARCHITECTURE.md](../../ARCHITECTURE.md), that is right; where
it disagrees with a measurement of the code, the code is right and this row is corrected.

## Sources

| Source | What was read |
| :--- | :--- |
| `ISO_32000-2_sponsored_EC3.pdf`, body text | Every line saying deprecated, obsolete, removed or shall-not-be-used, with its clause and table |
| The same file's **annotations** | Errata Collection 3 is carried mostly as comments and markup, not in the text: 7,624 annotations, 16 of them about deprecation, each located on its rendered page |
| `PDF32000_2008.pdf` | What 1.7 had that 2.0 drops without saying so |
| `external/arlington` (commit `5a863942`, 2026-05-04) | Its `DeprecatedIn` column, compared row by row with the standard |

**A count depends on the unit.** 0.3 lists 23 features "including"; the table below has
34 at the level of a feature; Arlington marks 372 key rows deprecated in 2.0 (46 whole
objects, and keys of 47 more). All three counted 2026-10-11.

## The five treatments

| Treatment | When it applies |
| :--- | :--- |
| **Load** | The replacement means exactly what the deprecated form meant, and nothing downstream — the audit included — needs the original |
| **Interpret** | Equivalent, but it lives in a content stream: the interpreter reads it as its replacement and the writer emits the replacement |
| **Save** | No replacement, so it is dropped from what is written. The open document keeps it, so the audit and `inspect` still see what the file said |
| **Keep** | Rewriting it would break a signature, guess at a conversion, or substitute something not equivalent. Read, kept, not rewritten |
| **Never written** | Something this engine produces; it does not produce the deprecated form |

Every drop is a `Decision::repaired` naming its clause, one per kind per document with a
count. A move that loses nothing records nothing ([ARCHITECTURE.md](../../ARCHITECTURE.md) §4.3).

## The table

**Successor**: *stated* — the standard names it; *derived* — the standard has it but does
not connect the two; — — none. **Now** is what
[`deprecations_test.rs`](../../crates/fepdf/tests/suite/deprecations_test.rs) asserts: it
opens `fepdf_fixtures::deprecated_in_2_0()`, saves it, and reads each row in the open
document and, through the byte layer, in the saved file. A row that changes here changes
there. *Code* marks what was read and not run; *not measured*, what the probe does not
carry. A **gap** is a row whose Now is not its Rule.

| # | Feature (clause) | Successor | Rule | Now |
| :--- | :--- | :--- | :--- | :--- |
| 1 | `/Info` entries other than `CreationDate`, `ModDate` (14.3.3) | XMP metadata stream — stated | Load | `Title`, `Producer` move to XMP (`metadata.rs`). **Gap**: `/Trapped` stays in `/Info` |
| 2 | Encryption `V` 1–4; standard handler `R` 2–5; Algorithms 1–7; RC4; MD5 key derivation (7.6) | `V` 5 / `R` 6 — derived | Never written | Read; AES-256 R6 is the only scheme written ([ADR-0015](../adr/0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)). Not re-measured |
| 3 | Public-key RC4, DES, 3DES, RC2, AES-128/192; crypt filters `V2`, `AESV2`; crypt filter `Length` (7.6.5, 7.6.6) | `AESV3` — derived | Never written | As #2 |
| 4 | Permission bit 10 (Tables 22, 24) | — readers ignore it, writers set 1 — stated | Never written | Not measured |
| 5 | `NeedsRendering` (Table 29), `/XFA` (Table 224), Annex K | AcroForm — derived | Keep | Kept |
| 6 | Web capture (14.10): `/SpiderInfo` (Table 29), page `/ID` (Table 31), image `/ID` (Tables 87, 143), `/IDS` and `/URLS` (Table 32) — the five keys by errata | — | Save | **Gap**: all kept |
| 7 | `/AlternatePresentations` (Table 32, 13.5) | — | Save | **Gap**: kept |
| 8 | `/ProcSet` (Table 34, 14.2) | — | Load | Dropped at load (`discovery.rs`, `drop_procsets`) |
| 9 | File specification `/DOS`, `/Mac`, `/Unix`; embedded file `/Mac` (Tables 43, 45) | `/F`, `/UF` — derived | Save when `/F` or `/UF` is present; otherwise keep, since making `/F` from a platform string is a guess | **Gap**: kept beside `/F` and `/UF` |
| 10 | Array of blend modes (Table 57, 11.6.3) | Reader takes the first it recognises — stated | Load | **Gap**: written unchanged, and *code*: `BlendMode::from_pdf_object` accepts only a name, so an array is not applied at all — contrary to 11.6.3 |
| 11 | Blend mode `Compatible` (Table 134) | `Normal` — stated | Load | *Code*: drawn as `Normal` (`graphics/mod.rs`); written unchanged |
| 12 | `/TR`, `/TR2` in a graphics state (Table 57, 10.5) | Halftone transfer functions — derived, **not equivalent** | Keep | Kept; *code*: not applied (only a soft mask's `/TR` is read) |
| 13 | Operator `F` (Table 59) | `f` — stated | Interpret | A saved content stream carries `f` |
| 14 | `/Name` of fonts (Table 109), image and form XObjects (Tables 87, 93) | — | Load | Font: dropped at load (`conform.rs`). **Gap**: image and form `/Name` kept |
| 15 | `/CharSet`, `/CIDSet` (Tables 120, 122) | — subset tag | Save | Both dropped from what is written, and kept in the open document (`drop_subset_claims`) |
| 16 | Adobe-Japan2, Adobe-Korea1 (2020) | Adobe-Japan1-7, Adobe-KR — derived, with no mapping | Keep | Not measured |
| 17 | `/ViewArea`, `/ViewClip`, `/PrintArea`, `/PrintClip` (Table 147) | — | Save | **Gap**: kept |
| 18 | Sound and Movie annotations and actions; 13.3, 13.4 | 13.2 Multimedia — stated, but see [Inconsistencies](#inconsistencies-in-the-standard) | Keep | Kept; not made by `AddAnnotation` (`apply/kinds.rs`) |
| 19 | TrapNet annotation, trapping (14.11.6), `/TrapRegions` | — | Save, the annotation taken out of `/Annots` as well. No other kind means what it means: a PrinterMark would read as a production mark and could not tell that a later edit had left the traps stale (the owner, 2026-10-11) | **Gap**: kept |
| 20 | Launch `/Win`, `/Mac`, `/Unix` (Table 207) | — | Keep | Kept, and read when `/F` is absent (`actions.rs`) |
| 21 | `/NeedAppearances` (Table 224) | Appearance streams — stated | Never written; a file's own `true` is dropped once an appearance is built for every widget lacking one (the owner, 2026-10-11) | Not written by a field edit, which builds the appearance (`apply/appearance.rs`). **Gap**: a file's own `true` is kept |
| 22 | FDF encryption (12.7.8.3.1, Table 247) | — | Keep | Not measured |
| 23 | SubFilter `adbe.x509.rsa_sha1`, `adbe.pkcs7.sha1`; PKCS #1 signatures (12.8.3) | `adbe.pkcs7.detached`, `ETSI.CAdES.detached` — derived | Keep | Not measured |
| 24 | MD5, SHA1 digests; `/DigestMethod` of a signature reference, by errata (Tables 237, 256) | SHA-256 and up — derived | Keep | Not measured |
| 25 | Signature `/R` (Table 255) | `/Prop_Build` — stated | Keep — it lies inside the signed byte range | Not measured |
| 26 | UR transform method, `/UR3`, usage rights signatures (12.8.2.3, Table 263) | — | Keep | Not measured |
| 27 | `/MovieActions`, `/SoundActions`, `/ExternalOPIdicts` (Table 264) | — | Keep — part of a legal attestation | Not measured |
| 28 | 3D stream `/Resources` (Table 311), by errata | — | Save | **Gap**: kept |
| 29 | `/Suspects` (Table 353) | — | Save | **Gap**: kept |
| 30 | Attribute revision numbers (14.7.6.3): structure element `/R`, and the integers in `/C` and `/A` arrays — both by errata | — | Load, once the structure tree code is shown to read an array without them | **Gap**: kept |
| 31 | PrintField attribute key spelled `checked` (Table 383) | `Checked` — stated | Load | **Gap**: kept |
| 32 | Separation dictionaries (14.11.4), page `/SeparationInfo` | — | Save | **Gap**: kept |
| 33 | OPI (14.11.7) on images, forms and trap networks | — | Save | **Gap**: kept |
| 34 | One Link element enclosing several link annotations of a page (14.8.4.7.3), by errata | One annotation to each Link — derived | Keep | Not measured |

## Removed, not deprecated

| Feature | Where it went |
| :--- | :--- |
| Public-key encryption dictionary `/P` (Table 23) | Removed for errors that prevented interoperable implementations (NOTE beside Table 23) |
| Clipping marked-content tags (1.7, 14.6.3) | "Intentionally removed in PDF 2.0" (errata, Table 352a) |
| `TagSuspect` (1.7, 14.8.2.3) | The same note |
| PostScript XObjects (1.7, 8.8.2: "should not be used") | Absent from 2.0's text, without a note. Found only by reading 1.7 beside it |

A removed feature takes the rule of a deprecated one with no successor: **Keep** while it
is read, and not written.

## Inconsistencies in the standard

- **Sounds and movies have two successors.** 12.5.6.16, 12.5.6.17, 12.6.4.9, 12.6.4.10,
  13.3 and 13.4 each say "superseded by … 13.2"; 13.1 says 13.7, Rich media. The errata
  repair 13.1's list formatting and leave the disagreement.
- **Two keys of deprecated features are not marked.** A page's `/PZ` belongs to web
  capture and its `/SeparationInfo` to separation dictionaries; neither entry says
  deprecated, after the errata as before.
- **NOTE 3 in Table 237** named `adbe.pkcs7.detached` as deprecated; the errata replace
  it with `adbe.x509.rsa_sha1`. Read from the body text alone, the note is wrong.

## Where Arlington differs

`MODEL_NOTES.md` says the model does not distinguish removal from deprecation. Beyond
that, it marks as deprecated in 2.0 rows the standard does not:

- **Adobe Extension Level 3**, which was never ISO: RichMedia `/Params` and `/CuePoints`,
  `Navigator`, `PaperMetaData` and a widget's `/PMD`, a collection's `/Resources`.
- **Adobe's PDF 1.7 reference**, not ISO 32000-1: `/DigestValue`, `/DigestLocation`.
- **A cross-reference stream's `/F`, `/FFilter`, `/FDecodeParms`**, citing pdf-issues #246;
  EC3's text does not.

It also dates `Compatible` from 1.4, where Table 134 says 2.0. The standard ranks above
the model, so an Arlington report on these rows is not a finding about 2.0.
