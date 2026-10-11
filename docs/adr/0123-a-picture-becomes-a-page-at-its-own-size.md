# ADR-0123: A picture becomes a page at its own size

- **Status**: Accepted
- **Date**: 2026-10-11
- **Commit**: (see the commit that adds this file)

## Context

The owner chose "a PDF made from images" (ROADMAP AA-5) on 2026-10-10. The engine had no
way to make a page from a picture, and no frontend could open one.

Several questions had more than one sound answer:

- what size the page is;
- whether a JPEG is decoded;
- how a phone photograph's EXIF orientation is honoured;
- what becomes of a PNG's transparency, a TIFF's thumbnails, and a scan whose zero is
  white.

## Decision

**`Operation::InsertImages` puts a page for each picture into a document**, and
`PdfDocument::from_images` is that operation applied to an empty document whose blank
page is then removed. Every picture is read before the document changes.

- **The page is the picture's size** at the resolution its file states: JFIF or EXIF for
  a JPEG, `pHYs` for a PNG, `XResolution` for a TIFF. Where none is stated, 72 dots to
  the inch. A page is shrunk to the 200 inches Annex C allows without `/UserUnit`.
  **Given a sheet**, every page is that sheet, with the picture fitted inside and
  centred. Frontends name sheets from `PageResize::SHEETS`.
- **A JPEG is carried as it is**, under `/DCTDecode`, when its coding is one that filter
  reads: baseline or progressive, eight bits. Any other JPEG is refused, not decoded and
  re-encoded. An Adobe `APP14` CMYK JPEG gets `/Decode [1 0 …]`, because Adobe writes its
  samples inverted.
- **EXIF and TIFF orientation turn the drawing matrix**, not the samples. Orientations 5
  to 8 swap the page's sides.
- **A PNG is decoded and expanded, and no further.** A palette becomes RGB, alpha becomes
  a soft mask, and sixteen bits stay sixteen. An alpha that is opaque everywhere is no
  mask.
- **Every page of a TIFF is a page**, except one `NewSubfileType` marks as reduced: a
  thumbnail. Fax-coded scans are decoded. Grey whose zero is white keeps its one-bit
  samples and is drawn the right way round by `/Decode`.
- **An ICC profile is kept as `/ICCBased`** where its header names the image's colour
  space. One that names another space is dropped, since it describes colours the samples
  do not hold.
- **The pages are untagged.** Alt text guessed from a file name would be a description
  nobody wrote.

The window opens a picture as the document made from it, so its history begins there and
not at a blank page with edits on it. It inserts pictures through the door it inserts
documents through, on a sheet the size of the page they go beside.

## Consequences

- A JPEG's bytes are the same in the output as in the file. `a_jpeg_is_carried_as_it_is`
  compares them through a save.
- A phone photograph at 72 dots to the inch makes a page as large as its pixels: 4032
  pixels is 56 inches. That is the picture's own size; a sheet is how to ask for a
  smaller one.
- Any other format — GIF, WebP, HEIC, BMP — is refused as not a JPEG, a PNG or a TIFF,
  with the picture's place in the list.
