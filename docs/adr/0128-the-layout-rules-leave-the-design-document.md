# ADR-0128: The layout rules leave the design document

- **Status**: Accepted
- **Date**: 2026-10-13
- **Commit**: (see the commit that adds this file)

## Context

`AGENTS.md` says only the four phase documents state rules, and `ARCHITECTURE.md` says
what the design is. `ARCHITECTURE.md` §2.1 nevertheless carried a list of "MUST" rules
about where files go, under *Organisation*, *Governance* and *Maintenance*, and §4.5
carried two more: that a new capability belongs in one of four "domain namespaces", and
that no feature bypasses the `Operation` vocabulary.

A review on 2026-10-13 measured them against the tree. The root's permitted files named a
`VISION` that does not exist and omitted the phase documents that do. The exempt
infrastructure binaries were `verify_render.rs` and `bypass_decrypt.rs`, neither of which
exists. Three root directories (`fuzz/`, `dist_release/`, `release_assets/`) were
unregistered under a rule that every root directory be registered.
`scripts/visual_regression.py` sat outside the three script directories the rules allow.
The four namespaces no longer described the vocabulary, which had grown to 63 variants,
and the bypass rule restated Rule D.

None of them named what enforced it except Output Isolation, which `status.sh`
measures. `AGENTS.md`'s fourth writing rule says that an unchecked rule is a comment
unless its entry says "nothing".

## Decision

The rules move to `CODING.md` §5 as L1–L7, each with what enforces it. Six say
**nothing**. `ARCHITECTURE.md` §2.1 keeps the directory table, without its *Owner*
column, and points at them. §4.5 and its §4.5.1 are deleted. §4.5.1 described a DOCX
provider crate that `ROADMAP.md` lists under *Not planned*, and §4.2 already says what a
second source owes. The later subsections are renumbered.

The tree is brought into line: the three directories are listed, and the visual
regression script moves to `scripts/test/`.

## Consequences

- `CODING.md` gains six rows that say **nothing**. That cost is stated, not hidden.
- §4.6 and §4.7 are now §4.5 and §4.6. References to them outside `docs/adr/` were
  updated. Records keep the numbers they were written with.
