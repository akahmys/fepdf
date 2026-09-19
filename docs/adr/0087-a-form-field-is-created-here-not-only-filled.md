# ADR-0087: A form field is created here, not only filled

- **Status**: Accepted
- **Date**: 2026-09-19
- **Commit**: (see the commit that adds this file)

## Context

The engine read forms well and wrote them barely. `/Ch` choice fields were parsed with
`/Opt`, `/I` and `/TI` and their values set with appearance regeneration
([ADR-0048](0048-reading-and-setting-choice-fields.md)); `inspect interactive` reported
every field a document carried; `SetFormFieldValue` and `appearance::set_button_state`
changed values in a form that already existed. **No operation created a field.** On
2026-09-19 the window reached none of it either: `fepdf-gui` built neither
`SetFormFieldValue` nor anything else on that path, so a form could be read, audited, and
not touched.

Two scopes were defensible. Filling only is the smaller half by a wide margin — the
machinery exists, and the work is a panel. Creating fields brings nine widget types, an
`/AcroForm` that may not be there, a tab order, a calculation order, and an appearance
stream per widget per state.

The argument for stopping at filling was cost. The argument against it was what this
product is for: a document this engine declares PDF/UA-2 conforming has to have accessible
fields, and a field this engine did not create is one it can only complain about. An
auditor that cannot fix what it names is half a tool — the same reasoning that gave the
structure tree an editor rather than only a viewer.

## Decision

Forms are built through to creation: nine widget types, `/AcroForm` generation, tab order
and calculation order, each field carrying the `/TU` that makes it announceable.

Filling lands first (Phase W, W-F1) because creation is not testable without it.

## Consequences

- **The audit gains a defect it can now fix.** A field without a `/TU` is a Matterhorn
  failure this engine already reports; after this it is one the window can repair.
- **Nine widget types is nine appearance streams**, which is the same work as an
  annotation's `/AP` and the same font dependency underneath both
  ([ADR-0085](0085-editing-what-a-page-draws-is-in-scope.md)). Doing annotations first is
  not an ordering preference; it is the only way this is not written twice.
- **XFA stays refused.** It is deprecated in 2.0 and would be a second form model beside
  the one that works — unchanged by this decision, and worth restating because "forms,
  fully" is exactly the phrase that would quietly include it.
- **The check is the round trip, not the screen**: a form created here, reopened, reports
  every field through `inspect interactive` with the type it was given.
