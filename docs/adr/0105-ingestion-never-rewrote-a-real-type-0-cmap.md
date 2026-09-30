# ADR-0105: Ingestion never rewrote a real Type 0 font's CMap

- **Status**: Accepted
- **Date**: 2026-09-30
- **Commit**: (see the commit that adds this file)

## Context

`refine::font::normalize_type0_font` replaced a Type 0 font's `/Encoding` with
`Identity-H` or `Identity-V` and left the content stream as it was. Three places relied
on it:

- `audit_fonts.rs` said this was why 31-005 to 31-008 were not checked;
- `glyph_map.rs` read the CMap from the loaded font because "the dictionary cannot say";
- `audit_scope_test.rs` held 31-006 and 31-008 out of the checked set, with an
  assertion meant to fail if ingestion stopped rewriting.

Reading Y-1d measured it instead. The rewrite ran only when the font's loaded
`FontResource::subtype` was `Type0`. For a Type 0 font with a descendant that loads,
that field holds the descendant's subtype, `CIDFontType0` or `CIDFontType2`.
`samples/sample_02c.pdf` loads with `UniJIS-UTF16-H` and `UniJIS-UCS2-H` as the file
wrote them, and saves with them. The one fixture that saw a rewrite has
`/DescendantFonts []`, a font no reader can draw with.

The rewrite would also have been wrong had it run. Content coded through `90ms-RKSJ-H`
or `UniJIS-UTF16-H` does not become CIDs when the dictionary says `Identity-H`. It
becomes other characters.

## Decision

**The rewrite is removed.** It changes nothing for a font that loads, and it corrupts a
font that does not.

The three places are corrected to what was measured. The test now asserts that the
`/Encoding` is kept as written. It also records that 31-006 and 31-008 are still not
checked, now for want of the work (ROADMAP Y-F16) rather than because ingestion answers
them.

## Consequences

- 31-005 to 31-008 are checkable on the document the auditor reads. Until Y-F16 builds
  them, they stay outside the checked set, and the report says so as it does for every
  condition it does not ask.
- Filling a missing `/CIDToGIDMap` is a separate repair of the same pass, and it does
  happen. It is recorded as Y-F15, not decided here.
