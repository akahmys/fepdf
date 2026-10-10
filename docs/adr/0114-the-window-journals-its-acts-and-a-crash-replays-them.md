# ADR-0114: The window journals its acts, and a crash replays them

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

Acrobat saves changed documents periodically and offers the work back after a crash
(`core.autosave`, `core.crash-recovery` and `core.encrypted-autosave`, all P0 in
[the feature list](../reference/acrobat-features/README.md)). `fepdf-gui` had none of
the three. A window that was killed, or crashed, lost every edit since the file was
opened, and the only warning was the close dialog, which a crash never reaches.

The window already holds what a recovery needs. `History` in `worker.rs` keeps the bytes
the open read and every act applied since, and undo is the same replay: open the bytes
again and apply what remains. `Operation` derives `Serialize` and `Deserialize`. So the
question was not *how* to rebuild a document but *what to put on disk*, and there were
two answers:

1. **Write the document out**, every so often, as a PDF. This is what Acrobat does. But
   here a save is a translation ([ADR-0012](0012-saving-produces-a-new-document.md)). It
   takes as long as an export, and it rewrites the whole file. And a recovered PDF has
   lost its history: an undo after a crash would have nothing to take back.
2. **Journal the acts.** Copy the origin once, at the first edit. Then append each act, undo
   and redo as it happens. Recovery replays the journal onto the origin, and that is the
   path an undo already takes.

An encrypted document adds a third question. The origin is the file's own ciphertext,
but an act is not. `EditRun` carries the words the reader typed, and `Redact` carries the
regions they meant to hide. A plaintext journal would leave those on disk beside a file
that was protected so they would not be.

## Decision

**The window journals its acts, and a crash replays them.**

- A session's directory holds `origin.pdf` and `journal`. The directory is created at
  the first act, not at the open, because a document that is only read leaves nothing
  behind. It lives under the platform's per-user data directory, in `fepdf/recovery/`.
- Each act, undo and redo is appended as a length-prefixed record and synced before the
  worker answers. A record cut short by the crash is dropped, and the window says the
  last act was lost.
- The directory is removed when the document is closed or replaced, or when the window
  exits normally. One that remains at the next start belongs to a window that did not
  exit normally. The test is a lock on a file inside the directory: its owner holds the
  lock while it runs, and the operating system releases it when the process ends,
  however it ends. So a second window's session is never offered as a crash.
- **An encrypted document's journal is sealed with the password it was opened with.**
  The seal reuses the AES-256 revision 6 code in `fepdf-syntax`. `encrypt_new` makes the
  `/U`, `/UE`, `/O` and `/OE` strings, and those go in the journal's header. Each record
  is encrypted as a stream would be. Recovery asks for the password again, and
  `new_aes256` checks it. The password itself is never written.

## Consequences

- A crash loses at most the act that was being written. An undo after a recovery takes
  back what it would have taken back before the crash.
- No new dependency. The seal is the code that already encrypts the output, with the
  same SASLprep gap: a password outside ASCII is taken as given
  (`SecurityHandler::new_aes256`).
- The journal is JSON, so `InsertFrom` writes its source document as an array of
  numbers, about four bytes for each byte of the source. This is accepted until a measured
  journal shows it matters.
- An encrypted document that opened with an empty user password has its journal sealed
  under the empty password. That protects the journal as much as the file is protected,
  and no more.
- Only the window journals. The CLI and the MCP server finish each command or tool call
  before returning, so they have no unsaved work to lose.
