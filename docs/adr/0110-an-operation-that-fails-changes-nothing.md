# ADR-0110: An operation that fails changes nothing

- **Status**: Accepted
- **Date**: 2026-10-03
- **Commit**: (see the commit that adds this file)

## Context

`PdfDocument::apply` stated no atomicity, and the window's journal relies on it. The
journal rebuilds a failed act from the history only when its second or later operation
failed. If a first operation changed the document and then failed, the document would
differ from what undo replays (ROADMAP Y-F12). No such operation was found. The contract
was what was missing.

Sealing the arena (ADR-0109) marked the span in which an operation writes: from `apply`
unsealing it to `apply` sealing it again. That span is also the one to undo when the
operation fails.

## Decision

**While unsealed, the arena keeps a journal, and a failed operation is put back from
it.** The journal holds each pool's length when the operation began, and the value each
slot that existed then held before its first write. `PdfArena::transaction` truncates the
pools and restores those slots in reverse order. It also restores the version, and drops
the reverse object index so that it is rebuilt.

`Document::change` wraps the transaction and also puts back the page list. It truncates
the decision log to its length before the operation, and clears the font and colour-space
caches, which are keyed by arena handles. `apply_operation` routes every `Operation`
through it. A transaction inside a running one is part of the outer one.

Names an operation interned are kept. An interned name is the same name whoever asked for
it.

## Consequences

- `sealed_document_test.rs` changes a document and then fails. The change allocates an
  object, rewrites the page dictionary, empties the page list and records a decision, and
  the test asserts each one is put back. Removing the rollback failed it, and so did
  removing the page list's restore.
- Each write inside `apply` to a slot that existed before costs a copy of what it
  replaces.
- Only a failed operation is put back this way. Undo still replays the history from the
  bytes that were opened (`ARCHITECTURE.md` §4.1), and nothing inverts an operation.
