# ADR-0007: An option nothing reads is hidden, not removed

- **Status**: Accepted
- **Date**: 2026-08-15
- **Commit**: the pre-Phase-B reconciliation

## Context

`IngestionOptions` carries six fields. Auditing them before starting Phase B found
that two are stored, defaulted, `Debug`-printed and never consulted:

| Field | Read by | CLI flag |
| :--- | :--- | :--- |
| `active_refinement` | `ingest/mod.rs:137` | `--no-refinement` |
| `sublime_metadata` | **nothing** | `--no-metadata-recovery` |
| `color_policy` | **nothing** | `--relaxed-color` |
| `force_fallback` | 11 sites | `--force-fallback` |
| `password` | `ingest/mod.rs:237` | not exposed |
| `progress_callback` | `ingest/mod.rs:104` | not exposed |

Both dead fields reached the user as documented command-line flags —
`--relaxed-color` as "Use relaxed color validation policy", `--no-metadata-recovery`
as "Disable automatic conversion of Info to XMP". Neither did anything.

Absence of a reader was found by search, which is not evidence. It was confirmed by
injection: `samples/sample.pdf` upgraded with and without each flag, compared with
`examples/compare_documents.rs`.

| Run | Objects differing from the baseline |
| :--- | :--- |
| identical flags, second run | 1 — the XMP packet |
| `--relaxed-color` | 1 — the XMP packet |
| `--no-metadata-recovery` | 1 — the XMP packet, same 3,144 bytes |
| `--no-refinement` | content streams throughout |

The XMP packet carries a fresh `xmpMM:InstanceID` per run, so one differing object is
the noise floor, not a result. `--no-refinement` is the control: it proves the
comparison detects an effect when there is one to detect. The two flags sit exactly at
the floor.

`ColorPolicy` is named in `ROADMAP.md` as one of the completions that did not survive
measurement. It had been named and left wired.

## Decision

Hide both flags with `#[arg(hide = true)]`; keep the fields and the type, documented at
their definitions as unread.

This follows the treatment Phase A gave the nineteen stub `Operation` variants: report
honestly, hide the CLI surface, leave the vocabulary in place. The distinction that
matters is between *lying to a user* and *having an unfinished feature*. Hiding ends
the first without pretending to fix the second.

Removal was the alternative, and it is defensible: this codebase has repeatedly paid
for building a container before its contents existed — `fepdf-resource`, the nineteen
stubs, `open_repair`, and now these two. But `color_policy` and `sublime_metadata`
differ from `fepdf-resource` in that the option is the *right shape* for behaviour that
should exist; what is missing is the check, not the design. Deleting them would remove
the record that the check is owed.

## Consequences

- `fepdf --help` no longer offers a flag that does nothing. The flags still parse, so
  existing scripts do not break — they merely continue to have no effect, which is what
  they already had.
- Un-hiding is the last step of implementing either feature, not the first. A hidden
  flag that works is a smaller error than a visible flag that does not.
- The audit generalises: **an option is a claim, and a claim is checkable**. The
  method here — vary one flag, compare semantically, keep a control that must differ —
  applies to `SaveArgs` too, which was not audited.
- `open_repair` was found in the same sweep to carry a comment truncated mid-sentence
  (`document.rs`). Fixed in passing; noted because a comment explaining why a function
  is a delegation is the only thing standing between it and being read as a stub again.
