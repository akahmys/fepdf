# ADR-0089: A face is embedded only where it permits it, and nothing is substituted

- **Status**: Accepted
- **Date**: 2026-09-19
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0085](0085-editing-what-a-page-draws-is-in-scope.md) put font embedding on the
critical path of everything Phase W builds, and left one question open: where the face
comes from when a document does not already carry a glyph. Writing 図面 through a
non-embedded `/Helvetica` was what the engine did instead, and it produced a row of Latin
glyphs and an extraction missing two characters.

**Mainstream tools do not choose between bundling a face and using the system's.** They
ask the font, through `OS/2.fsType` (ISO 14496-22), which states what its maker permits:
0 installable, bit 1 restricted, bit 2 preview and print only, bit 3 editable, bit 8 no
subsetting, bit 9 bitmaps only. The common shape is a ladder — the document's own face
first, the system's face of the same name second where `fsType` permits it, and a
substitution recorded third. Tools that cannot assume a system font, such as Ghostscript
and TeX, bundle a redistributable set instead.

**Nothing in this engine read that field.** On 2026-09-19 `grep -rn -i fstype` over
`crates/` returned one write — the `OS/2` table `reconstruction.rs` synthesises — and no
read at all.

Two properties of the corpus were measured before the ladder was designed, over the nine
samples, and the second changed the answer:

| | |
| ---: | :--- |
| 235 | embedded font programs |
| 64 | state a permission: 23 installable, 34 editable, **7 preview-and-print**, 0 restricted |
| 171 | **state nothing**: 153 CFF-based `FontFile3`, a format with no `OS/2` table, and 18 TrueType subsets whose producer dropped it |

**The first run of that measurement said 1 of 235 and was wrong.** It read the stream as
the arena holds it, which is still `/FlateDecode`d, so every table tag came out as noise
from a zlib header; printing the tags is what caught it. `cargo test -p fepdf-model --test
font_embedding_permission_test -- --nocapture` derives the table above.

## Decision

The ladder, and it ends in a refusal:

1. **The face the document already embeds**, re-subsetted to include the glyph.
2. **A face installed on this machine**, where its `fsType` permits an embedding that may
   be edited *and* permits subsetting — every face this engine embeds is a subset.
3. **Refuse.** The operation fails, naming the characters it could not write, and records
   a `Decision`. Nothing is substituted, and **no face is bundled with this engine.**

**Silence is not consent.** A program with no `OS/2` table has not permitted anything, so
it does not pass rung 2. That is 171 of the 235 programs measured above, and reading their
silence as permission would embed, by default, exactly the faces whose terms are unknown.

**Preview-and-print does not pass either.** This engine writes documents that are meant to
be worked on, so the bar is bit 3 rather than bit 2 — including for a face the document
itself embeds, where re-subsetting it for text the document did not have is a new
embedding rather than the one its producer made.

## Consequences

- **A Bates prefix in Japanese on a machine with no permitted CJK face now fails, loudly.**
  The present behaviour writes six unreadable glyphs and loses the characters from
  extraction, so the failure moves from silent and wrong to visible and refused.
- **The binary carries no font and the repository carries no font licence**, which was the
  alternative and is a cost this decision declines to pay: several megabytes for one face,
  and one face is not a CJK story for long.
- **A refusal is a `Decision`, not a log line**, which is principle 2 of `AGENTS.md`: a
  caller has to be able to tell *this was written* from *this was refused*.
- **The engine gained a reading it did not have**, and the corpus exercises the refusal
  rather than only the acceptance — 7 of the 64 faces that state a permission refuse one.
- **Whether PDF/A requires the embedding to be legally permitted is not checked** against
  the standard's text here, and is the first thing to read when W-E1 lands; ISO 32000-2
  and its parts outrank this record.
- **What this does not decide**: what the GUI offers a reader who hits rung 3. Naming a
  face by hand, and what that means for the licence, is a question for when the refusal
  exists and somebody meets it.
