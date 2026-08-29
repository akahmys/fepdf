# ADR-0035: What a page shows and what it says are separate questions

- **Status**: Accepted
- **Date**: 2026-08-29
- **Commit**: (this change)

## Context

The roadmap carried a row reading *"2,106 glyphs this engine cannot name"*, filed under
the hardest remaining extraction work: characters drawn with no route to Unicode, where
any answer would be a guess at what the document declined to say.

Asked what they were, all 2,106 turned out to be one font, on eight pages of
`volvo_xc90.pdf`, at one character code:

```
   2106  unmapped   BAAAAA+VolvoNovum-SemiLight   F6   gid 0   code 0x0000
```

Code zero, glyph zero — `.notdef`. Not a character this engine failed to name, but a
glyph the document drew on purpose to show nothing. Page 389's content stream contains
the literal bytes `<0000> Tj` 414 times, confirmed against the decompressed stream rather
than through our own parser.

PDFKit reads 169 CJK characters from that page. The text is in the file, beside the
glyphs:

```
/Span<</ActualText (-) >> BDC   7 0 Td <0160> Tj   EMC
```

Page 389 carries 393 of these — 169 CJK, 205 Thai, 17 punctuation — and the corpus 6,080.
The CJK count matches PDFKit's exactly.

**The interpreter had the property list and threw it away.** On the way to the optional
content code, every operand that was not a name became `Object::Null`:

```rust
let operand = properties.as_ref().map(|ir| match ir {
    IrObject::Name(name) => Object::Name(/* … */),
    _ => Object::Null,
});
```

That is correct for `/OC` and documented as such — 8.11.2 requires a group to be an
indirect object, so an inline dictionary names nothing that `/OCProperties` could have
turned off. The flattening was written for the one caller that existed, and 14.9.4 puts
real text in exactly the place it discards.

## Decision

**`/ActualText` is read, and it replaces the glyphs of its section for extraction only.**

* **Read from the IR, before the flattening.** `IrObject::Dictionary` survives in the
  parsed command and nowhere after it. The `/OC` path keeps the `Object::Null` it was
  always given.
* **Both shapes of property list.** Written in place, which is what all 6,080 of the
  corpus's spans do, or a name into `/Properties`, which none of them does and which
  costs one lookup.
* **Extraction takes the text; rendering ignores it.** The glyphs are still what appears
  on the page. Only the reader's copy changes, which is the only place the difference
  exists.
* **The outermost section wins when they nest**, because an inner one describes part of
  what the outer already describes in full.
* **An empty `/ActualText` suppresses its glyphs and contributes nothing.** A section that
  stands for no text — a decorative glyph, a hyphen at a line break — is saying something,
  and it is not the same as saying nothing.
* **Replaced glyphs are counted apart from unmapped ones.** They are opposite failures: an
  unmapped glyph is text the document never gave, a replaced one is text the document did
  give, in the place the specification puts it.

## Consequences

`volvo_xc90.pdf` loses **0 glyphs of 718,262**, from 2,106. The corpus loses 38,264 of
16,321,270, from 40,370.

The Chinese and Thai regulatory notices extract in full. Ours is more complete than
PDFKit's, which drops the Thai combining marks: `เครื่องโทรคมนาคม` against `เคองโทรคมนาคม`.

**It also fixes a defect that was never counted.** Ten codes in that font have a
`/ToUnicode` of `<0000>`, and this engine was emitting `U+0000` into the extracted text
for them — `R/7713/19` came out as `R⟨NUL⟩7713⟨NUL⟩19`. Not an empty string, so it never
appeared in the loss tally; not a readable character either. The tally measured what it
knew how to measure, and this sat immediately outside it.

**One measured claim elsewhere turned out to be wrong.** Two files asserted that 24 of
`fugaku.pdf`'s 25 pages are legitimately blank because they draw no glyphs. The glyph
count was right; the conclusion was not. Those pages carry 2,622 `/ActualText` spans, and
they extract now — one character per span and mostly punctuation, which is the document's
own quality and not this engine's to improve. Reading a page's text and counting its
glyphs are separate questions, and the engine could not tell them apart until something
went looking.

**Two of the shapes decided above are not in the corpus.** All 6,080 spans are a `/Span`
tag with the property list written in place — none is empty, none nests (the deepest is
one), none uses another tag, and none names a list in `/Properties`. The named-list
lookup in particular was written because 14.6.2 allows it and it costs one lookup, which
is a reason to implement something and not evidence that it works. `actual_text_test.rs`
exercises those shapes directly, since no document here will.

**Nothing in this engine is measured against a corpus with `/ActualText` worth reading
other than `volvo_xc90.pdf`.** `fugaku.pdf` has 2,622 spans and their content is too poor
to check an implementation against.
