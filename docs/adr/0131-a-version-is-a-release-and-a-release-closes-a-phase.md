# ADR-0131: A version is a release, and a release closes a phase

- **Status**: Accepted
- **Date**: 2026-10-14
- **Commit**: (see the commit that adds this file)

## Context

No rule said how a version was chosen. The workspace declared `1.0.0`, and two releases
were tagged: v1.0.0 on 2026-09-01 and v1.0.1 on 2026-09-05. The v1.0.1 tag named a commit
whose `Cargo.toml` still said `1.0.0`, so its binaries report a version the release does
not carry. Nothing compared the tag with the manifest. The version was also written twelve
times: once in `[workspace.package]` and once on each of the eleven path dependencies
`[workspace.dependencies]` declared. 450 commits followed v1.0.1 without a release.

The owner proposed numbering by ROADMAP phase: A as 0.1, B as 0.2, and so on. Two things
argued against tying the two directly. Phases do not close in order: Phase AA is open
while AB and AC are closed, and Y and Z ran together, so a tree has no single phase to
name it. And Phase C′ has no number. A phase names work, and a version names a release.

The v1.0.x releases were measured on 2026-10-14 before anything was decided: eight
downloads across both, and no stars or forks. The crates are not on crates.io. Renumbering
costs almost nothing outside this repository.

## Decision

The version is `0.MINOR.PATCH`. A release is cut when at least one phase has closed since
the last one, and it raises MINOR. A release that only fixes the last one raises PATCH. 1.0
is the owner's call. `TESTING.md` §5 states this as V1–V5.

`ROADMAP.md` keeps a release table, with each version, its date and the phases it closed.
That table is the one place where a phase and a version meet. Phase letters are never
renumbered.

The first release, 0.1.0, closes every phase closed before versions were kept: A–Z, AB
and AC. v1.0.0 and v1.0.1 are withdrawn, with their tags. This amends
[ADR-0057](0057-the-released-binaries-could-not-read-japanese.md), which kept v1.0.0's
`fepdf-macos-universal.zip` published because people might link to it. Measured, almost
nobody had.

## Consequences

- The version is written once: the path dependencies carry none, since nothing is
  published.
- The release workflow fails a tag that is not `v` and the workspace version. `status.sh`
  reads 1 when the release table's last row is not that version, and the audit gates the
  row. Both checks were run against a deliberate mismatch and failed.
- How often a release is cut (V2) is checked by nothing.
