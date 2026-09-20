# ADR-0091: Paragraphs are not inferred; the reader marks the range, and overflow shows

- **Status**: Accepted
- **Date**: 2026-09-20
- **Commit**: (see the commit that adds this file)
- **Extends**: [ADR-0085](0085-editing-what-a-page-draws-is-in-scope.md)

## Context

ADR-0085 decided that editing what a page draws is in scope and left one question open:
whether a paragraph re-flows across its line breaks. Answering it needs a paragraph, and a
PDF does not usually say where one is.

**What shipped first matched nothing.** `EditTextRun` replaced a run whose text read
exactly what was asked for, which worked on a hand-written fixture and nowhere else.
Measured over the samples, characters per show-text operator: the median is 1 to 2, from
26% to 100% of runs are a single character, and `volvo_xc90.pdf` is entirely
single-character runs. Replacing `日本国憲法` on the first page of `constitution.pdf`
changes nothing, because those five characters are five runs.

**Inferring the container was measured and does not hold.** A line-gap rule was compared
against what two tagged documents declare, over adjacent pairs of lines:

| | gap rule | always "same block" | always "different" |
| :--- | ---: | ---: | ---: |
| `print_sample.pdf`, a slide deck | **30%** | 0% | **100%** |
| `volvo_xc90.pdf`, a manual | **83%** | **75%** | 25% |

The rule beats a constant answer by eight points on the manual and loses to one by seventy
on the slides. The problem is not its average but its variance between kinds of document,
and nothing tells a processor in advance which kind it has. A better rule — indentation,
alignment, font changes, sentence-final punctuation — moves the numbers and not the shape
of the difficulty: on a slide, a line *is* the block, and on a manual it usually is not.

Six of the nine samples carry no structure tree at all, so "reflow only where the document
declares a paragraph" refuses on two thirds of this corpus.

## Decision

**Nothing about a paragraph is inferred.**

- The engine draws the runs the file declares, and no grouping of them.
- **A caller names one run.** Runs are not grouped, not even by adjacency: the first
  attempt joined runs with nothing between them that moves the text, on the ground that
  the operators say they are contiguous. They do — and contiguous is not the same as *one
  phrase*. A label and its value, two columns, or the cells of a row can be drawn one
  after another with nothing between them, and joining those is a processor deciding what
  a document means. Most PDFs are unstructured and many have a drawing order unrelated to
  their sense, so that decision is wrong often and invisibly.
- The replacement is set in the font the run is set in, and the glyphs after it on that
  line are re-placed by their advance widths, which is arithmetic rather than inference.
- **Text that no longer fits is drawn anyway.** It is not refused, not re-wrapped, and not
  silently shortened.

## Consequences

- **The open question of ADR-0085 stops being on the path.** Re-flowing a paragraph is not
  needed to edit text, so the decision is not taken here and is not blocking anything.
- **Overflow can collide with what follows it**, and a reader who makes text longer will
  see it overlap. That is the cost of not re-wrapping, and it is visible at once rather
  than discovered later — which is the argument for it: a processor that re-wrapped would
  move things the file placed deliberately, and a processor that refused would stop a
  reader who knows there is room.
- **A selection that falls inside a run needs the run split**, at a position the advance
  widths give. That is W-E4b and is arithmetic too.
- **A word is several edits.** A run is one or two characters in a real file, so changing
  a word means changing each of its runs. That is the price of not grouping, and it is
  paid in effort rather than in surprise — which is the trade this decision makes. What
  makes it workable is the reader doing the grouping themselves, by merging runs they
  know belong together and splitting ones that do not; neither needs the engine to guess.
- **This is not what other tools do**, and the difference is deliberate. Acrobat and the
  editors like it infer a block, re-wrap inside it, and draw the box so the reader can see
  what moved — accuracy bounded by showing the guess rather than by being right. This
  engine does not put a guess on the screen at all.
