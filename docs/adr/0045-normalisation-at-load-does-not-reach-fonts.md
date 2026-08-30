# ADR-0045: Normalisation-at-load does not reach fonts, and the step meant to finish them runs on an empty map

- **Status**: Accepted
- **Date**: 2026-08-30
- **Commit**: (see the commit that adds this file)

## Context

`ARCHITECTURE.md` §4.4 is titled *normalisation-at-load* and says: **"A `Document` is
therefore one normalised state, not the file. Everything above happens before application
code sees anything."** Asked whether that still holds, three of its claims do and one does
not, and the one that does not is fonts.

**What holds**, checked rather than recalled:

* The three stages run before `Document::open` returns — ingest, `normalize_resources`,
  `normalize_page_tree`, page-tree rebuild, all above the `Ok(doc)`.
* There is no remapping table. `reader.rs` builds `Object::Reference(Handle::new(n))` from
  `n 0 R` and nothing renumbers.
* The byte layer is a separate entry point, as the table in §4.4 says: `inspect structure`,
  `catalog`, `encryption` and `interactive` all go through `survey(&data)`.

**What does not.** `cargo run -p fepdf --example load_state_probe -- samples/*.pdf`:

```
file                         at open     after    pages
bokutokitan.pdf                    0         5       40
constitution.pdf                   0        12       13
fugaku.pdf                         0        36       25
fy05.pdf                           0        11       40
intel_sdm.pdf                      0        10       40
```

**The font cache is empty on every sample when `open` returns.** The ingest pass builds a
resource for every font object and copies them in (`document.rs`), and six lines later
`normalize_resources` calls `self.font_cache.write().clear()`. Every font is then rebuilt
lazily, during rendering, by `Interpreter::get_font` — which does not repeat what ingest
did: it merges a Type0's descendant `CIDFont` into it, propagates the encoding, `wmode`
and `/ToUnicode` down, rebuilds the unified map and runs reconstruction again. Then it
writes the result back into the document's cache.

So the resource that decodes a document's text is built after `open`, by a different
route, and the `Document`'s own state changes the first time a page is drawn.

**This is not hypothetical and it has already cost a session.**
[ADR-0041](0041-a-character-collection-is-declared-not-guessed.md) fixed `/CIDSystemInfo`
on the load path and **no measured number moved**, because the copy that decodes is the
descendant one the interpreter builds. Only after that branch was fixed did 261 glyphs come
back. The decisions in `inspect text`'s "DECISIONS TAKEN READING" block are harvested from
the ingest resources before they are discarded, so **they describe font resources that no
longer exist**, and the ones that draw can reach different conclusions with nothing said.

**And one step of the normalisation is inert.** `normalize_resources` ends with
`resolve_missing_font_data()`, which walks `font_cache.write().values_mut()` substituting a
system face for any font with no `/FontFile` — over the map cleared three lines above it.
It is the only caller, so that loop has never had an entry to visit. The substitution it
was written for happens anyway, later and elsewhere, in the interpreter's own
`is_sfnt` fallback; nothing failed, and nothing said the step was doing nothing.

## Decision

**Recorded, not fixed.** The fix is to give fonts one construction path instead of two,
which means deciding where a Type0's descendant is merged and making both the ingest pass
and the interpreter use that answer. That is ADR-0041's size again and it touches every
route that names a character, so it belongs in a phase with a measurement in front of it —
[ROADMAP Phase T](../../ROADMAP.md).

**§4.4 states the exception**, because the section's own claim is what a reader will
otherwise believe. The three things it names — revision chain, ciphertext, metadata — are
true; the sentence around them reads wider than the code, and a reader trusting it will
fix the wrong copy, which is exactly what happened.

`examples/load_state_probe` is kept so the figure can be re-derived rather than quoted from
here.

## Consequences

**Nothing changes in what the engine does.** This records what it does.

**Two things follow for anyone reading a font decision.** A `Decision` recorded at load
came from a resource that was thrown away; a font's behaviour under the interpreter has to
be measured through the interpreter, not through `FontResource::load`. Both are what made
ADR-0041 take two attempts, and both are now written down instead of being rediscovered.

**The inert step is left in place rather than deleted**, because deleting it would remove
the only statement that fallback substitution is meant to happen at load. It is named here
so that whoever unifies the two paths knows there is a third thing to place, not two.
