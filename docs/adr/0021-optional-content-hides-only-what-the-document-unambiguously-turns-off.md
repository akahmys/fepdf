# ADR-0021: Optional content hides only what the document unambiguously turns off

- **Status**: Accepted
- **Date**: 2026-08-21
- **Commit**: 09a51f7

## Context

`BDC` popped its property list and threw it away — the comment read "Skeleton: just pop
for now" — so content inside an `/OC` marked-content section was painted whatever the
state of its group. `/OCProperties` had gained a reader in Phase K and **nothing
consulted it**, and `fepdf-doc` writes an `/OFF` array through `Operation::UpdateLayers`,
so the engine created layers it then ignored. That is a wrong answer rather than a
missing feature: a non-printing underlay, a "draft" stamp and the other language of a
bilingual page all appear on a page that should not carry them, and nothing says so.

Two questions had to be answered before it could be fixed, and neither had an obvious
answer.

**What does a second implementation do?** Every other cross-check in this project has an
independent oracle, and clause 8.11 has many constructions. Thirteen probe pages were put
to PDFKit by the method `crosscheck_image.sh` uses — each 200×200, black in the top-left
quarter under one construction and black in the bottom-right under none:

| Construction | PDFKit | 8.11 says |
| :--- | :--- | :--- |
| group in the configuration's `/OFF` | **hides** | hide |
| `/BaseState /OFF` | **hides** | hide |
| `/BaseState /OFF` rescued by `/ON` | paints | paint |
| `/OC` on a form XObject | paints | hide (8.11.3.2) |
| `/OC` on an image XObject | paints | hide (8.11.3.2) |
| OCMD `/P /AllOn` with one group off | paints | hide (Table 97) |
| OCMD `/VE [/Not <on>]` | paints | hide (8.11.2.3) |
| `/Usage` `/View` applied through `/AS` | paints | hide (8.11.4.5) |
| a `/Span` section nested inside a hidden `/OC` | paints | hide |
| `/OC` naming an absent property, or with no `/OCProperties` | paints | paint |

PDFKit honours **two** of the thirteen. It appears to understand a direct reference to an
OCG in the default configuration and nothing else — not membership dictionaries, not the
`/OC` entry on an XObject, and not the nesting of marked-content sections, where its
first `EMC` ends the hiding whatever opened in between.

**What should happen when the `/OC` cannot be read at all?** A name that is not in the
page's `/Properties`, a group written in place rather than as an indirect object, a `/VE`
that contains itself, a `/P` that is not one of Table 97's four.

## Decision

**The standard decides, not the second renderer.** Where PDFKit and 8.11 disagree, this
engine follows 8.11 (`AGENTS.md`, Hierarchy of Truth). Eleven of the thirteen therefore
have no independent oracle, and they are held by
`crates/fepdf/tests/optional_content_test.rs` — twenty-six cases against the clause,
using a backend that records the calls rather than rasterising them. Only the two PDFKit
agrees about become fixtures for `crosscheck_image.sh`
(`examples/make_layer_fixtures.rs`), plus a control with the layer **on**, because
"hides the layer" and "draws nothing" otherwise produce the same four numbers.

**Nothing is hidden on a doubt.** Every reading that does not end in "this document
turned this group off" leaves the content visible and records a `Decision`. Painting a
layer that should be hidden shows something that was in the file; hiding one on a guess
removes something that was, and no reader of the output can tell that it happened. The
asymmetry is the argument — it is the same one that keeps an undecodable image from
aborting a page (ADR-0018), pointed the other way.

**The gate lives behind the trait, not at the call sites.** `fepdf-content::canvas`
wraps the backend and withholds the five methods that put marks on the page, forwarding
everything else — `q`, `Q`, `cm`, the clip stack, the colour and the font all still run
inside a hidden section, because the operators after `EMC` inherit the state the hidden
ones left. Five `if` statements in the interpreter would have worked until a sixth
painting site was added, and the symptom of forgetting one is a layer that should be off
appearing on the page, which is the defect this record exists about.

## Consequences

- **Text in a hidden layer is no longer extracted.** `show_text` is one of the five
  withheld calls, so `inspect text` and the remediation walks see what is drawn. That is
  deliberate: the alternative is a document whose text and whose page disagree. No file
  of either corpus is affected — one of the 251 carries `/OCProperties` at all, and its
  `/OCGs` array is empty.
- **`/Print` and `/Export` usages are read and not applied.** Nothing here prints or
  exports, and hiding a trapping layer from someone looking at a screen because a printer
  would omit it produces a different document from the one that was opened. The reader
  for them exists (`UsageDictionary`), so the place to act is named rather than absent.
- **`/RBGroups`, `/Locked`, `/Order` and `/ListMode` are named and not applied.** They
  constrain what a *user* may do next in a layer panel; this engine has no toggle to
  constrain. `/Configs` likewise: applying a configuration the document did not make
  default would be this engine choosing which layers a reader sees.
- **`/OCProperties`'s `/D` is now modelled**, which takes the entry from 0 of 3 to 1 of 3
  on `inspect catalog`'s inner coverage (ADR-0020) — and, more to the point, makes the
  Phase K reader load-bearing instead of reachable.
- **The corpus still cannot check any of this.** Phase O-1 remains the thing that would
  change that: a business document carries layers, and neither corpus contains one.
