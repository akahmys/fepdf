# ADR-0116: An annotation carries an author only when the reader named one

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

A markup annotation's `/T` is, by convention, the name of the person who made it
(Table 172). Acrobat fills it from the operating system's login name unless the reader
has set another (`comment.author-identity` in
[the feature list](../reference/acrobat-features/README.md)). That puts a login name into
every document the reader comments on and then sends to someone else. The reader was
never asked.

12.5.6.3 makes `/T` mandatory in one place: a state change "shall specify the user".

## Decision

**An annotation carries an author only when the reader named one.** The window keeps an
author name in its settings, and it starts empty. While it is empty:

- a new annotation and a reply have no `/T`;
- setting a state asks for a name first, because 12.5.6.3 requires one, and
  `SetAnnotationState` refuses an empty author rather than inventing one.

The engine does not read the environment for a name. `author` in an operation is the
caller's, as everything else in an operation is.

## Consequences

- No document leaves this window with a login name in it that the reader did not type.
- A reviewer who wants their comments attributed has to set a name once. A list filtered
  by author shows comments without `/T` under no author.
