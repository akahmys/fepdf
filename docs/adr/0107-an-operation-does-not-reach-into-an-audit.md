# ADR-0107: An operation does not reach into an audit

- **Status**: Accepted
- **Date**: 2026-10-03
- **Commit**: (see the commit that adds this file)

## Context

ROADMAP Y-4 asked that `fepdf-doc`'s operations stop importing helpers from its audit
modules. Measured on 2026-10-03, one did: `apply/artifacts.rs` read a page's
`/Properties` through `audit_fonts::names_in`. The helper moved to
`fepdf_model::access::names_in`, and Y-4's entry closed with "nothing checks that it
stays so".

The cost of the import is not its size. An audit's helpers are shaped by what the audit
reads: `names_in` leaves out an entry written directly, because the font audit compares
objects. An operation reading through it inherits that choice without having made it, and
a change made for the audit changes the operation.

## Decision

**Rule E: no line of `fepdf-doc/src/apply/` names an audit module** — `audit_*`,
`matterhorn`, `glyph_map`, `glyph_select`, `glyph_widths`, `unicode_map`,
`formula_marks` or `page_languages`. `scripts/audit/layering.py` counts such lines as
`audits=`, expects 0, and fails the audit on any. What an operation and an audit both
need goes below them, in `fepdf-model`'s `access` or a module neither owns.

The list names modules rather than a prefix because four of the audit's helpers are not
called `audit_*`; they were the four Y-4 named.

## Consequences

- An audit module added under another name is not covered until it is listed.
- The check was shown to fire: `artifacts.rs` put back on `crate::audit_fonts::names_in`
  read `audits=1` and failed it.
