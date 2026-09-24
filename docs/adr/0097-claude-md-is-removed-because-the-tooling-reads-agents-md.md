# ADR-0097: `CLAUDE.md` is removed, because the tooling reads `AGENTS.md`

- **Status**: Accepted
- **Date**: 2026-09-24
- **Commit**: (see the commit that adds this file)
- **Reverses**: the `CLAUDE.md` part of [ADR-0081](0081-the-writing-rules-had-nothing-behind-them.md)

## Context

[ADR-0081](0081-the-writing-rules-had-nothing-behind-them.md) added a `CLAUDE.md` because
the agent's tooling loaded that file without being asked and did not load `AGENTS.md`. It
carried three writing rules, a pointer to `AGENTS.md`, and the two gate commands.

Claude Code v2.1.277, released 2026-09-18, reads `AGENTS.md` as a project's instructions —
but by default only where there is no `CLAUDE.md` in the working directory or above it
([the tooling's documentation](https://code.claude.com/docs/en/memory)). With both present
it read `CLAUDE.md` alone, so the file added to get `AGENTS.md` read had become what kept
it from being read.

The file had also broken two of the rules it carried. It gave `AGENTS.md` as 73 lines when
it was 81, a figure with no date and no derivation beside it; and two of its rules restated
`AGENTS.md`'s own.

## Decision

**`CLAUDE.md` is deleted.** What it held that `AGENTS.md` did not moved there: "one ADR,
one decision" became writing rule 6, and the gate became a "Before you start" section that
names `status.sh` and links to `TESTING.md` and `AUDITING.md`, which state the two
commands.

## Consequences

- **One instructions file, read by every tool that reads the convention.**
- **A tooling version before v2.1.277 reads neither file.** The documentation lists the
  first session after an upgrade from an earlier version among those that read `CLAUDE.md`
  only. The audit stops a commit whether the rules were read or not, which ADR-0081 already
  named as the stronger of its two fixes.
