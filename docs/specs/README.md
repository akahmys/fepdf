# Specifications

## The specifications themselves

The `.pdf` files beside these notes are the normative documents this engine is written
against. They are untracked — `.gitignore` excludes every `*.pdf` — so a clone does not
carry them and this list is what says which ones a working copy needs.

**All but one are free.** The PDF Association's members sponsor access: the
[ISO 32000-2 bundle](https://www.pdfa-inc.org/product/iso-32000-2-pdf-2-0-bundle-sponsored-access/)
since 2023-04-05 and the
[PDF/UA bundle](https://www.pdfa-inc.org/product/pdf-ua-bundle/) since 2024-08-12, each
at $0.00. The Matterhorn Protocol is a free PDF Association publication under CC BY 4.0.

**ISO 19444-1 is not.** It is a single-user licence bought from ISO, and it must never be
committed or published. `.gitignore` names it as well as matching it as a `*.pdf`, so
that removing the general rule would not expose it.

| File | What it is |
| :--- | :--- |
| `ISO_32000-2_sponsored_EC3.pdf` | **PDF 2.0, Errata Collection 3** — the core specification, 1023 pages |
| `ISO_32000-2_sponsored-ec2.pdf` | The same at Errata Collection 2, kept until the difference has been read |
| `PDF32000_2008.pdf` | **PDF 1.7, ISO 32000-1:2008**, as Adobe publishes it under agreement with ISO: "the technical material is identical … the page and sections numbers are also preserved". 756 pages. What PDF/UA-1 and Matterhorn cite |
| `ISO-14289-1-2014-sponsored.pdf` | **PDF/UA-1**, on PDF 1.7. 25 pages |
| `ISO-14289-2-2024-sponsored.pdf` | **PDF/UA-2**, on PDF 2.0, and not backward compatible with UA-1. 51 pages |
| `Well-Tagged-PDF-WTPDF-1.0.pdf` | **WTPDF 1.0**, beside UA-2 rather than under it. 57 pages |
| `Matterhorn-Protocol-1-1.pdf` | 31 checkpoints, 137 failure conditions — **for PDF/UA-1** |
| `ISO-TS-32005-2023-sponsored.pdf` | Structure namespaces |
| `ISO_TS_3200{1,2,3,4}-*.pdf` | The four other extensions to PDF 2.0 |
| `PDF20_AN00{1,2,3}-*.pdf` | Application notes: black point compensation, associated files, object metadata |
| `Tagged-PDF-Best-Practice-Guide.pdf` | Implementation guidance — **for UA-1**, as its cover says |
| `PDF-Declarations.pdf` | |
| `ISO19444-1-2019.pdf` | **XFDF 3.0**, the XML form of FDF (ISO 32000-2 12.7.8). **Paid, single-user: never commit it.** Placed 2026-10-10 for ROADMAP AA-4 |

**Three things this list exists to stop being got wrong**, each of which was got wrong on
2026-09-21 before the documents were read:

- **The Matterhorn Protocol is a PDF/UA-1 document.** Its own text: "31 checkpoints
  comprised of 136 failure conditions encompassing file format requirements specified in
  PDF/UA-1". It mentions PDF/UA-2 nowhere, and there is no Matterhorn 2.0. An engine
  declaring `PdfStandard::UA2` cannot measure that claim with it.
- **Its numbers are failure conditions, not checkpoints.** `14-003` is the third failure
  condition of checkpoint 14.

- **Its tables enumerate 137 of them and its own prose says 136.** The sentence quoted
  above is version 1.02's: 1.1's Document History records "Failure condition 13-008
  added", 13-008 is marked `H`, and counting the `How` column gives **87 `M`, 48 `H` and
  2 with no specific test** (23-001 and 27-001) — one more `H` than the sentence's 47.
  The tables are what a number can be looked up in, so 137 is what this engine counts
  against ([ADR-0093](../adr/0093-the-protocols-tables-enumerate-137-failure-conditions.md)).

**ISO 32000-1 is here, and eight `M` failure conditions want it.** Matterhorn cites
tables of PDF 1.7 rather than of 2.0 — 09-004 (Table 337), 09-005 (Table 336), 09-006
(Table 333), 09-007 and 09-008 (Table 338), 31-006 and 31-008 (Table 118), and 31-027
(Annex D); 02-001 and 10-001 cite it in their notes. ISO 32000-2's own Table 371 describes
the same table structure types in prose and is **not** Table 337, so a check reported
under 09-004 is written from `PDF32000_2008.pdf` and from nothing else
([ADR-0095](../adr/0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md)).
It is not in either sponsored bundle; Adobe publishes it.

For PDF/UA-2 there is no Matterhorn. veraPDF's
[validation profiles](https://github.com/veraPDF/veraPDF-validation-profiles) (CC BY 4.0)
formalise each "shall" of ISO 14289-2 as a rule named by its clause — `8.2.1-2` is the
second rule of clause 8.2.1 — and `PDFUA-2.xml` carries 91 of them. They are a second
reading to check against, not the source: the source is the standard, which is here.

---

## Beside them

[pdf-2-0-deprecations.md](pdf-2-0-deprecations.md) gives each feature ISO 32000-2
deprecates a rule — at load, in the interpreter, on save, kept, or never written — and
what the engine does now beside it. `ARCHITECTURE.md` §4.4 points at it.

What the engine does is in `ARCHITECTURE.md`; how it got there is in
[`docs/adr/`](../adr/README.md). The subsystem notes that once sat here were deleted
([ADR-0129](../adr/0129-the-subsystem-notes-are-deleted.md)).
