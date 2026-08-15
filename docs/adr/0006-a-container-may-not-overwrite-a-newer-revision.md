# ADR-0006: An object stream may not overwrite a newer revision of what it carries

- **Status**: Accepted
- **Date**: 2026-08-15
- **Commit**: the Phase A reader switch

## Context

The first assembly pass placed objects in two rounds: every object written directly in
the file, then every object carried by an object stream. The order was chosen for a
practical reason — a container has to be parsed before it can be expanded — and looked
harmless, because the merged cross-reference had already been resolved oldest-section
first, so the newest definition of each object number had won.

It was not harmless. In a file with incremental updates (ISO 7.5.6), an object may be
written into an object stream in one revision and **replaced by a direct object** in a
later one. Expanding the container after placing the direct objects put the superseded
version back.

The bug did not announce itself. Every sample loaded, every object count matched, and
the reader read *more* than the path it replaced. It surfaced only on a cross-check:

| Reader | Pages in `samples/fy05.pdf` |
| :--- | :--- |
| macOS PDFKit | 846 |
| the path being replaced | 846 |
| the new reader | **847** |

`fy05.pdf` has three revisions. The page tree root is object 4: revision 1 puts it in
object stream 4509, revision 2 replaces it with a direct object declaring `/Count 846`.
The extra page came from resurrecting revision 1's copy.

A second, quieter case appeared in the same file. Object 3684 is marked free with
generation 1 by the newest section — deleted. Expanding its old container brought it
back too.

## Decision

When a container is expanded, an object it carries is stored **only if the merged
cross-reference still places that object in that container**. Anything else means a
later revision moved or deleted it, and the container no longer speaks for it.

```rust
if current_container(records, object.number) == Some(container) {
    arena.set_object(Handle::new(object.number), object.object);
}
```

## Consequences

- Object numbering and deletion in incremental updates are honoured. The replaced path
  did not honour deletion: it kept object 3684, which the file had removed.
- Reading more than another implementation is not evidence of reading better. The
  disagreement was the signal; the count alone looked like a win.
- Two tests hold the rule: one that a later direct object beats the container that
  first held it, one that a freed object is not resurrected. Both are built from a
  hand-assembled two-revision file rather than a sample, so the case stays explicit.
- The cross-check is worth keeping as a habit. `examples/page_tree_probe.rs` prints
  what the trailer's page tree declares, which is what made the disagreement visible.
