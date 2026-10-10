# ADR-0117: An FDF import replaces an annotation of the same name, and adds the rest

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

AA-3 imports the annotations of an FDF file (12.7.8) onto a document. 12.7.8.3.4 gives
each FDF annotation a `/Page` and says nothing about what happens when the page already
holds an annotation of the same `/NM`. The same comment can arrive twice, because a
reviewer sends the file again, or sends it after editing a comment.

Other implementations, as their documentation says on 2026-10-10:

- **Apryse** offers three merges. `FDFMerge` adds everything and duplicates what is in
  both. `FDFUpdate` makes the document match the file and removes what the file lacks.
  `MergeXFDF`, the one it recommends for a partial file, matches by name: it replaces an
  annotation whose `/NM` matches, adds one that does not, and leaves the rest. It also
  names every annotation it exports, so that matching works.
- **Acrobat** imports FDF and XFDF ("Import Data File"), and its documentation does not say
  what a matching name does.
- **Foxit**'s SDK imports both, and does not say either.

## Decision

**An imported annotation whose `/NM` matches one on its page replaces that one. Any other
is added, and an annotation the file does not mention is left alone.** This is
`MergeXFDF`'s rule.

- The replacement is made **in place**: the existing annotation's dictionary takes the
  imported entries, and keeps its own `/P`, `/Popup` and `/StructParent`. Replies in the
  document that answer it go on answering it, and the structure tree goes on pointing at
  it.
- An imported annotation without `/NM` cannot be matched, so it is added. Apryse aborts
  instead. Here the reader gets the comment, and a second import of the same file
  duplicates it.
- The export names every annotation it writes. One without an `/NM` in the document is
  written as `fepdf-p<page>-<index>`, so that the file it makes can be matched when it is
  imported again.

## Consequences

- Importing the same file twice leaves the document as one import did, wherever the file
  names its annotations.
- A change made in the document to an annotation the file also carries is overwritten by
  the import. It is one act in the history, so an undo takes it back.
- The rule is about annotations. Form values in an FDF (`/Fields`) are a separate item.
