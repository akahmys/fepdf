# ADR-0099: The XMP packet carries what the engine does not write

- **Status**: Accepted
- **Date**: 2026-09-28
- **Commit**: (see the commit that adds this file)
- **Bounds**: [ADR-0094](0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md), whose premise was a packet rewritten whole

## Context

W-22's last item is the claim itself: a PDF Declaration in the document's XMP metadata
whose `pdfd:conformsTo` names a WTPDF conformance level (WTPDF 6.1). Before writing one,
the question was whether a declaration already in a file survives this engine.

It did not. `metadata::settle` rebuilds the catalogue's packet at ingest from the nine
fields `MetadataInfo` models — title, author, subject, keywords, creator, producer, the
two dates and rights — and the save path rebuilds it again. Anything else in the packet
was gone from the document before a caller could ask for it. Measured 2026-09-28 by
opening and saving each of the 138 files in the veraPDF PDF/UA-2 corpus: every one lost
`pdfuaid:part` and `pdfuaid:rev`, and those that carried WTPDF declarations lost them.
The same held for PDF/A's `pdfaid`, and for any extension schema.

So a declaration this engine wrote would have been lost at the next save, and a file
that made a claim came out making none.

## Decision

**When the packet is rebuilt, what the packet in place says that the generator does not
own is carried into the new one**, in an `rdf:Description` of its own, with the
namespaces it was written in, each property copied as written. Only the descriptions
`rdf:RDF` holds are read: a description a property's value is written as belongs to that
property and travels with it.

**Each property is copied as written, prefix and all.** ISO 14289-2 clause 5 requires the
prefix `pdfuaid`, and two files of the corpus bind a second prefix to its namespace; a
prefix looked up from the namespace came out as the other one.

**What the engine owns it reads, in both of XMP's forms.** A simple property may be an
attribute of its `rdf:Description` (XMP Part 1, 7.9.2.2), which is how veraPDF writes
`xmp:CreatorTool`, `pdf:Producer` and the dates, and the reader looked only for elements;
`dc:rights` was written and never read. Each was lost at ingest, since what the generator
owns is not carried.

**What the generator owns is read from the generator**: one packet rendered with every
field it knows filled in, whose property names are the list. A property it owns is its
to write or to leave out, so a title a caller removed is not brought back from the old
packet. A second list kept by hand beside the generator would be two homes for one fact,
and would fall behind the first time the generator learned a field.

## Consequences

- A file's claims about itself — `pdfuaid`, `pdfaid`, `pdfd:declarations`, extension
  schemas — survive opening and saving. All 138 PDF/UA-2 files and all 42 PDF/A files in
  the corpus keep them, counted by namespace, not by prefix. Two of the PDF/UA-2 files
  declare two prefixes for the `pdfuaid` namespace, and come out with the other one.
- **A claim that was true of the input is carried into an output it may not be true of.**
  An edit can break PDF/UA-2 conformance and the `pdfuaid` claim still stands. This is
  what every editor that preserves metadata does. Whether a claim holds is a question for
  the auditor, and the engine never made the claim.
- ADR-0094 counted 06-002, a packet with no PDF/UA identifier, among the conditions
  ingestion answers. That held only while ingestion removed the identifier, so the answer
  for every file was that it broke 06-002. The ingested packet now holds what the file
  held, so the auditor could decide 06-002 on it. It does not yet.
