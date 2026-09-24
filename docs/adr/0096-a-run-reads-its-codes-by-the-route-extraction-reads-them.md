# ADR-0096: A run reads its codes by the route extraction reads them

- **Status**: Accepted
- **Date**: 2026-09-24
- **Commit**: (see the commit that adds this file)

## Context

`fepdf::text` reads a page as runs — one show-text operator each — and says what each
code of a run reads in `RunInfo::pieces`. Those pieces are what a search over the runs
matches against (ROADMAP W-E3d, W-15). Extraction reads the same codes for
`extract_text` and `extract_spans`.

The two read by different routes. Extraction asks the font with `decode_next`, which
tries `/ToUnicode`, then the encoding, then the CID collection the font declares. The
runs asked `unified_map` — the table a `/ToUnicode` is *written from* when a document is
saved — turned round from text-to-code into code-to-text.

W-15 rebuilt the redaction studio's search on the runs, and compared it with the search it
replaced: for every word extraction read on the first five pages of each sample, whether
each search found it on the page. Measured 2026-09-24 with
`cargo run --release --example find_recall`:

| | words | extraction's spans | the runs |
| :--- | ---: | ---: | ---: |
| `bokutokitan.pdf` | 160 | 160 | 50 |
| `fy05.pdf` | 169 | 169 | 58 |
| `unicode_16.pdf` | 678 | 678 | 265 |
| `sample_02c.pdf` | 37 | 37 | 20 |
| `constitution.pdf`, `intel_sdm.pdf`, `print_sample.pdf` | 1,508 | 1,508 | 1,508 |

The words the runs missed were missing characters, not missing geometry.
`cargo run --release --example unread_codes` counted the codes the runs drew and read as
nothing: **1,721 of 3,080** on the first five pages of `fy05.pdf`, 652 of 1,011 in
`bokutokitan.pdf`, 1,258 of 15,385 in `unicode_16.pdf`. On `fy05.pdf`'s second page the
run reading 会計検査院、日本国憲法第 had lost the は and the の between them, so
会計検査院は could not be searched for.

`unified_map` was also where the time went. The table was turned round once for every
string a run showed, and a font's table is its whole mapping: reading every page of
`fy05.pdf` took 37 s, and `bokutokitan.pdf` 10 s (`--example find_timing`).

## Decision

**A run's codes are read with `FontResource::to_unicode`**, the route extraction reads
by, and nothing else. A fallback to `unified_map` after it was measured and decided
nothing: the unread codes over the samples were 4 with it and 4 without.

`unified_map` stays what it is — the table a written `/ToUnicode` comes from, and the
table `encode` uses to turn a replacement text into codes. Reading and writing are allowed
to know different things; what is not allowed is a search that cannot see what the page
reads.

## Consequences

- **The runs read what extraction reads.** Unread codes over the first five pages of
  every sample: 4, all in `fy05.pdf`, each a run of one code. The run search finds every
  word extraction reads except one on `sample_02c.pdf`, which extraction reads as
  きú選択ヵシィゲォわ and the runs do not.
- **Reading every page is the cost of a search, and it fell.** `fy05.pdf` 37 s to 0.43 s,
  `bokutokitan.pdf` 10 s to 0.05 s. `intel_sdm.pdf` is the slowest at 4.2 s for 5,057
  pages, and is not decoding.
- **A run read short is now rare enough that the samples have none to cut.**
  `a_run_this_engine_reads_short_is_still_cut_without_loss` used `unicode_16.pdf`'s first
  page, which lost 60 codes of 348; it uses a fixture drawing a code Helvetica's standard
  encoding does not have.
- **The W-E3d figures in ROADMAP were taken on the old reading.** Its median run of 0
  characters in `bokutokitan.pdf` was a run of codes that read as nothing.
