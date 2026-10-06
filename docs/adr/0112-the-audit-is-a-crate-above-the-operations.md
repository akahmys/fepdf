# ADR-0112: The audit is a crate above the operations

- **Status**: Accepted
- **Date**: 2026-10-06
- **Commit**: (see the commit that adds this file)

## Context

`fepdf-doc` held 22,617 lines: the `Operation` vocabulary and its interpreter, the
structure tree, reading order, measurement, remediation, and the Matterhorn audit.
ROADMAP left open whether it should split, until the Y-1d reading showed a reason.

The owner defined fepdf on 2026-10-03 as a translator from any PDF to ISO 32000-2,
operations on what it translated, and frontends for those operations. Reporting what was
done serves that. It is not what fepdf is.

Measured on 2026-10-06, the audit was 16 modules and 5,368 lines (`audit_*`, `structure`,
`matterhorn`, and the glyph and language readers the font audit uses). It reached into the
rest of `fepdf-doc` in four places: the content reader `apply::text::Content`,
`struct_tree`, `parent_tree` and `tagging`. Nothing outside the audit reached into it.
`layering.py` held Rule E
([ADR-0107](0107-an-operation-does-not-reach-into-an-audit.md)) by counting lines of
`apply/` that named an audit module.

The owner was asked on 2026-10-06 whether to split, not to, or to split the readers out
as well. They chose to split the audit out.

## Decision

**The audit is `fepdf-audit`, a crate that depends on `fepdf-doc`.** It judges a document
and changes nothing. It reads through the four readers above, which `fepdf-doc` makes
public. The facade depends on both crates.

## Consequences

- Rule E is held by cargo. No module of `fepdf-doc` can name an audit, because the audit
  is not among its dependencies. Declaring the dependency back is a cycle, and cargo
  refuses it: it was tried, and `cargo check` stops. `layering.py` no longer counts lines.
  The check is now stronger, too. It covered `apply/`, and the crate boundary covers all
  of `fepdf-doc`.
- The crate graph matches the owner's definition. The translator and the operations sit
  below, and the report that judges them sits above.
- `Content::iter` is `Content::commands`. As a public method named `iter`, it would have
  promised an `IntoIterator` it does not have.
- `fepdf-doc` still holds reading order, measurement and remediation. Splitting those too
  was the third answer, and was not chosen.
