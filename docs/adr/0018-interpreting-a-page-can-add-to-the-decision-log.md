# ADR-0018: Interpreting a page can add to the decision log

- **Status**: Accepted
- **Date**: 2026-08-20
- **Commit**: this record

## Context

`ARCHITECTURE.md` §4.3 says a departure from the standard is **recorded, not logged**,
and one place in the engine could not obey it. `ops/xobject.rs` skips an image whose
filter it cannot decode — the change that recovered four files of the external corpus,
because a failing image used to abort the content stream and take the page's real text
with it. That skip reached `log::debug!` and a comment saying why it could go no
further: the interpreter holds `&Document` and `DecisionLog::push` needed `&mut`.

So a page whose picture had been discarded was indistinguishable from a page that never
had one. This engine has already paid for that shape once: `UnknownFilter-Linearized.pdf`
lost its catalogue and eleven objects to an `if let Ok(..)` that said nothing, while
`inspect structure` reported the file "read without departing from the standard".

Two ways out.

**Return the decisions.** `render_page` and `extract_text` yield them alongside their
result. Honest about the borrow, and it puts a content-level departure somewhere
`inspect structure` will not print — the caller now has two places to look, and the one
that answers "what did the engine decide about this file" is no longer either of them.
It also changes the signature of every extraction path across the SDK to carry a note
about a picture.

**Make the log reachable through `&Document`.** The log goes behind a
`parking_lot::Mutex`, `push` takes `&self`, and the interpreter records into the same
place the reader does.

## Decision

**The decision log is interior-mutable, and interpreting a page can add to it.**
`DecisionLog` holds `Mutex<Vec<Decision>>`; `Document::record(&self, Decision)` is how
the interpreter reaches it. `entries()` returns a snapshot rather than a borrow, because
a caller holding a guard while interpreting a page would deadlock against the
interpreter recording into it — the logs are small enough that copying one is not a cost
worth designing around (11 decisions across the 251 files of both corpora).

**`is_conforming` therefore means "no departure in what has been examined so far".**
This is stated rather than hidden, because it is the price. It was *already* true — a
document whose pages are never interpreted has never been fully read — and the same
files demonstrate it either way: `isartor-6-3-2-t01-fail-b.pdf` reports no decision
under `inspect structure` and a 9.9 `Violation` under `inspect text`, because one of
those commands loads fonts and the other does not. What changes is that the partiality
is now a property the documentation states, instead of one nobody had noticed.

`inspect text` reports the two apart: decisions taken **reading** before the text,
decisions taken **interpreting** after it. They answer different questions — what the
file needed in order to be read at all, and what this run of the interpreter gave up on.

## Consequences

Four files of the external corpus now say what they lost, naming the filter:
`/CCITTFaxDecode`, `/JPXDecode` and `/XXXDecode`, each on an image XObject. The clause
cited distinguishes two facts that had been one: **7.4** when the file names a filter
that is not in the filter table, **8.9.5** when the filter is one this engine has and
the image dictionary still could not be honoured.

`status.sh` searches `fepdf-content` for decision sites now. It searched `fepdf-model`
and `fepdf-syntax` only, so the row would have reported this change as having altered
nothing — a measurement blind to the thing it measures.

What is now harder: a `Document` can be observed to disagree with itself over time, and
a caller comparing `decisions()` from two moments will see the second superset. Nothing
depends on the log being stable, and a test asserting conformance must extract the text
before asking — which is the honest ordering anyway.

What this does **not** do is make the skipped image decodable. Phase L keeps the three
image codecs unbuilt; this makes the refusal something the engine reports about a
document rather than a sentence in a roadmap.
