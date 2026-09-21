# Subsystem Notes

## The specifications themselves

The `.pdf` files beside these notes are the normative documents this engine is written
against. They are untracked — `.gitignore` excludes every `*.pdf` — so a clone does not
carry them and this list is what says which ones a working copy needs.

**All of them are free.** The PDF Association's members sponsor access: the
[ISO 32000-2 bundle](https://www.pdfa-inc.org/product/iso-32000-2-pdf-2-0-bundle-sponsored-access/)
since 2023-04-05 and the
[PDF/UA bundle](https://www.pdfa-inc.org/product/pdf-ua-bundle/) since 2024-08-12, each
at $0.00. The Matterhorn Protocol is a free PDF Association publication under CC BY 4.0.

| File | What it is |
| :--- | :--- |
| `ISO_32000-2_sponsored_EC3.pdf` | **PDF 2.0, Errata Collection 3** — the core specification, 1023 pages |
| `ISO_32000-2_sponsored-ec2.pdf` | The same at Errata Collection 2, kept until the difference has been read |
| `ISO-14289-1-2014-sponsored.pdf` | **PDF/UA-1**, on PDF 1.7. 25 pages |
| `ISO-14289-2-2024-sponsored.pdf` | **PDF/UA-2**, on PDF 2.0, and not backward compatible with UA-1. 51 pages |
| `Well-Tagged-PDF-WTPDF-1.0.pdf` | **WTPDF 1.0**, beside UA-2 rather than under it. 57 pages |
| `Matterhorn-Protocol-1-1.pdf` | 31 checkpoints, 137 failure conditions — **for PDF/UA-1** |
| `ISO-TS-32005-2023-sponsored.pdf` | Structure namespaces |
| `ISO_TS_3200{1,2,3,4}-*.pdf` | The four other extensions to PDF 2.0 |
| `PDF20_AN00{1,2,3}-*.pdf` | Application notes: black point compensation, associated files, object metadata |
| `Tagged-PDF-Best-Practice-Guide.pdf` | Implementation guidance — **for UA-1**, as its cover says |
| `PDF-Declarations.pdf` | |

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
  Quoting the sentence's 136 is the third thing this list exists to stop being got wrong,
  and it was got wrong here until 2026-09-21.

**ISO 32000-1 is not here, and eight `M` failure conditions want it.** Matterhorn cites
tables of PDF 1.7 rather than of 2.0 — 09-004 (Table 337), 09-005 (Table 336), 09-006
(Table 333), 09-007 and 09-008 (Table 338), 31-006 and 31-008 (Table 118), and 31-027
(Annex D); 02-001 and 10-001 cite it in their notes. ISO 32000-2's own Table 371 describes
the same table structure types in prose and is **not** Table 337, so a check written from
it and reported under 09-004 would be a finding against a requirement nobody here has
read ([ADR-0095](../adr/0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md)).
Neither sponsored bundle carries it: the ISO 32000-2 bundle is 2.0, and the PDF/UA bundle
is 14289. Where a copy comes from is open.

For PDF/UA-2 there is no Matterhorn. veraPDF's
[validation profiles](https://github.com/veraPDF/veraPDF-validation-profiles) (CC BY 4.0)
formalise each "shall" of ISO 14289-2 as a rule named by its clause — `8.2.1-2` is the
second rule of clause 8.2.1 — and `PDFUA-2.xml` carries 91 of them. They are a second
reading to check against, not the source: the source is the standard, which is here.

---

## Notes

Background on individual subsystems. **Not authoritative.**

Most of this predates the current design and sits at the bottom of the hierarchy of
truth (`AGENTS.md`). Where it disagrees with `ARCHITECTURE.md`, `ARCHITECTURE.md` is
right; where it disagrees with a measurement of the code, the code is right.

Read it for context on *why* a subsystem was shaped a certain way, not for what the
code does today.

**Audited 2026-08-22 (Phase O-4).** Every claim in these files that a command could
check was checked, and three of the documents did not survive it:

| Was here | Why it went | |
| :--- | :--- | :--- |
| `sdk_design.md` | Named four source files, a `serialize/` directory and five dependency versions that do not exist, and an Arlington predicate engine that has never existed | |
| `app_design.md` | Named five crates and four types that do not exist, and called the CLI binary the GUI | |
| `charter_redesign_2026-04-13.md` | A dated deliberation record, which belonged with the history rather than the specifications | |

All three were archived under `docs/history/`, which was deleted with
`docs/retrospectives/` on 2026-08-29 ([ADR-0038](../adr/0038-one-hierarchy-of-truth-and-the-parallel-rulebook-is-deleted.md)):
both existed for a self-improvement loop that is no longer run, and git holds what
happened.

The three that remain were corrected in place rather than archived, because most of
each was true: `refinery_engine.md` claimed generation bits on `Handle`, a
`SafetyBitmask`, a text-encoding detector that was **removed** for corrupting a
conforming `/Title`, and Zstd compression of exactly the two stream kinds that are
excluded from it; `core-pipeline.md` claimed a non-recursive decryption walk that
recurses; `rendering.md` claimed a `.notdef` fallback that does not log, in an
engine that held sixteen log sites at the time — three now, after ADR-0028 turned the
conclusions among them into `Decision`s. Each correction says what was checked and when, because a
line that is silently right today is indistinguishable from one that is silently stale.

**Audited again 2026-08-22 (Phase Q).** The first audit checked every claim a command
could check and corrected three files in place. Four months of drift was not the reason it
had to be repeated the same day — a wider re-derivation was, and it found that
**`rendering.md`'s correction was itself wrong**: it verified "the engine holds exactly one
`log::warn!`" against `status.sh`, and `status.sh` searched two crates, neither of them the
ones that file describes. The real figure is sixteen, and seven of those sit in the
rendering and font code.

A claim checked against a tool that cannot see its subject is indistinguishable from a
claim nobody checked. That is the one lesson worth carrying out of this directory.

| File | What the second pass found |
| :--- | :--- |
| `rendering.md` | "exactly one `log::warn!` by design" — measured against a row that could not see the crates in question. Sixteen sites, three deliberate |
| `core-pipeline.md` | A **"Structural Bar Suppression"** heuristic that deleted fills at `y > 700`. It is gone from the code, having fired 1,738 times on one file and 902 on another, deleting table rules. Also a reader "remapping table" that has never existed |
| `sdk-pipeline.md` | "Wildcards are prohibited in the primary dispatch loop" — the primary dispatch matches a `&str` and its `_` arm logs *"Unknown or unhandled operator"*. Also a "Zero-Fallback Policy" beside a working system-font fallback |
| `refinery_engine.md` | Zstd, removed by Rule 9; `encoding_rs`, no longer in the tree at all; and a GUI described as using Tokio, which it declared and never called |

Nothing here was archived this time. Each file now says at the point of each claim what
was checked and when, because **a line that is silently right today is indistinguishable
from one that is silently stale** — which is the same sentence the first audit ended with,
and it applied to that audit too.

To learn what the engine does now, read `ARCHITECTURE.md`; to learn how it got there,
read [`docs/adr/`](../adr/README.md).
