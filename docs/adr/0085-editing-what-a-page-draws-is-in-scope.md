# ADR-0085: Editing what a page draws is in scope

- **Status**: Accepted
- **Date**: 2026-09-19
- **Commit**: (see the commit that adds this file)

## Context

Thirty-two operations acted on pages, structure, metadata, annotations and security. None
of them changed what a content stream drew. Comparing `fepdf-gui` against JUST PDF
[編集Pro] on 2026-09-19 put that at the head of the list, above annotations and forms.

`ROADMAP.md`'s "Not planned" had already refused a DOCX converter, and the refusal rested
on the nature of the work rather than on a corpus count: writing one meant style
resolution, line breaking and pagination — a layout engine, which shares almost nothing
with reading PDF. The question this raised was whether replacing a word already drawn on a
page fell under that same refusal.

It did not, and the reason was measurable rather than argued. Content streams were already
held parsed, as `SublimatedData::Commands { items: Vec<Command> }`, with `ShowText`,
`ShowTextArray` and the TJ offsets that vertical Japanese needs; `serialize_commands`
wrote them back on the save path. ADR-0079 had settled that this form stays eager. **The
round trip an edit needs therefore already ran on every document this engine wrote.**

The half that did not exist was the font. On 2026-09-19, `grep -rn "FontFile"` over
`crates/fepdf-doc/src` and `crates/fepdf/src` returned nothing, and `subset.rs` held one
function — `subset_tag`, which reads the `ABCDEF+` prefix off a `/BaseFont` name. This
engine had never embedded a font.

That was not a hypothetical gap. Bates numbering, reachable from the window's document
tools, put Japanese on a page through a non-embedded `/Helvetica`:

```bash
fepdf edit bates samples/constitution.pdf -o /tmp/x.pdf --prefix "図面-" --digits 4
fepdf inspect text /tmp/x.pdf     # -0001
```

The two kanji were gone from the extraction, and the engine's own reader reported both
causes: `9.6.2`, the content stream selecting a `/Helvetica` its resources did not define,
and `9.10.2`, six of sixteen glyphs with no Unicode value — six being the number of UTF-8
bytes in 図面, each having become a character code of its own.

## Decision

Editing what a page draws is built. The line the DOCX refusal drew is **redrawn rather
than crossed**:

| | |
| :--- | :--- |
| **Editing** | replacing the text of a run already on the page, spacing it within its line, and moving, scaling or replacing a drawn object |
| **A layout engine** | re-flowing a paragraph across its line breaks, and repaginating |

The first is built, as Phase W's W-E3 and W-E5. The second is a separate decision, to be
taken when the first runs and its cost is measured rather than guessed.

Font embedding is built first, because it is what every other item waits on.

## Consequences

- **`ARCHITECTURE.md` stops describing an engine that leaves content streams alone.** The
  present tense there follows the code, so it changes when W-E3 lands, not now.
- **The critical path became one part.** `fepdf-font` gains real subsetting and
  `fepdf-doc` gains a writer for `/FontFile2`, `/FontDescriptor`, `/ToUnicode` and
  Identity-H. Annotations' appearance streams, form field values, watermarks and a text
  layer from an OCR engine all wait behind the same work.
- **What ships gets repaired before the product gets bigger.** The Bates defect above, and
  every watermark and header `overlay_text_on_page` would write, are fixed by W-E1 rather
  than by a patch of their own.
- **The granularity of ADR-0064 is inherited.** Redaction works one text-showing operator
  at a time; editing wants a glyph. Splitting a run needs advance widths, and that
  requirement now has two callers — this decision and [ADR-0088](0088-what-a-crop-puts-outside-the-sheet-is-removed.md).
- **What is still open**: whether a paragraph re-flows. Stating it here is what keeps the
  answer from arriving by accident, one commit at a time.
