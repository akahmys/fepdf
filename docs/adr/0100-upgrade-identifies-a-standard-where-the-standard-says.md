# ADR-0100: `Upgrade` identifies a standard where the standard says, and refuses one it cannot read

- **Status**: Accepted
- **Date**: 2026-09-28
- **Commit**: (see the commit that adds this file)
- **Rests on**: [ADR-0099](0099-the-xmp-packet-carries-what-the-engine-does-not-write.md), [ADR-0095](0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md)

## Context

`Operation::Upgrade` is how a caller says a document is PDF/UA-2, PDF/A-4 or PDF/X-6.
ROADMAP recorded that it wrote `pdfuaid` for UA-2. Measured on 2026-09-28, it wrote no XMP
at all. It put one key into the catalogue: `/PdfUA 2`, `/GTS_PDFA14` or `/GTS_PDFX`.
ISO 32000-2's catalogue (Table 29) defines none of them, and none is where its standard
looks. ISO 14289-2 clause 5 identifies a PDF/UA-2 file by `pdfuaid:part` 2 and
`pdfuaid:rev` in the catalogue's metadata stream, and PDF/A's identification is `pdfaid`
there too. `sdk_tests` asserted `/GTS_PDFA14`, so the test held the defect in place.

The specifications directory holds ISO 14289-2, but neither ISO 19005-4 (PDF/A-4) nor
ISO 15930-9 (PDF/X-6). The veraPDF PDF/A-4 files that pass state `pdfaid:part` 4 and
`pdfaid:rev` 2020. No file in the corpus states a PDF/X-6 identification.

## Decision

**`Upgrade` writes the identification into the XMP packet**, replacing what the packet
said under the same names:

- UA-2: `pdfuaid:part` 2 and `pdfuaid:rev` 2024, from clause 5.
- A-4: `pdfaid:part` 4 and `pdfaid:rev` 2020, as the conforming files state them.

It writes no catalogue key.

**PDF/X-6 is refused before anything changes.** This engine has no source for its
identification to write it from, and ADR-0095's rule applies to writing a claim as it
does to checking one. The window stops offering it, because a button that fails in front
of someone is worse than one that is absent. The CLI and `fepdf-mcp` accept the name and
return the refusal.

## Consequences

- An upgraded document identifies itself where a validator reads, and a PDF/UA-1 file
  upgraded to UA-2 says part 2 once, not both parts.
- PDF/X-6 comes back when a copy of ISO 15930-9 or a conforming file is here to write it
  from.
- The identification is a claim, and `Upgrade` still checks nothing of the standard. The
  UA-2 audit is how a caller asks whether the claim holds.
