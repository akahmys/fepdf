# ADR-0127: A form is given the resources its callers lend it

- **Status**: Accepted
- **Date**: 2026-10-12
- **Commit**: (see the commit that adds this file)

## Context

ROADMAP AC-1. ISO 32000-2's body text has a form XObject's `/Resources` "Optional but
strongly recommended" (Table 93). Errata Collection 3 changes that to "Sometimes required"
and adds that a form XObject is to be independent of the content streams it appears in,
"from PDF 2.0" as a requirement. In 1.x a form with no `/Resources` drew with its caller's:
the interpreter here does so too, pushing nothing and reading the page's or the outer
form's. So the drawing was right, and a save wrote a form that does not stand on its own.

The Arlington model found it while auditing the deprecation probe (ROADMAP AB-8), and is
stricter than the errata: it requires `/Resources` outright.

Measured on 2026-10-12 over 528 of the 531 files of `samples/`, `target/malformed/` and
`target/external/`: 316 form XObjects, 13 without `/Resources` in 12 files. Three of those
name resources, in two files, both test files: pdf-differences'
`DefaultRGBColourSpaces-inherit.pdf`, which tests the inheritance itself, and TWG's
`A002-pdfa2-fail-c.pdf`. The other ten name none.

The owner chose on 2026-10-12 to copy what a form names from what draws it, over giving
only an empty dictionary or only recording the form.

## Decision

**A form XObject with no `/Resources` is given the entries its content names, copied from
the page or form that draws it, at load.**

- **The names a form uses** are read from its content stream by operator: `Tf`, `Do`,
  `gs`, `sh`, `scn`/`SCN` with a pattern, `cs`/`CS` with a colour space that is not a
  family name, and `BDC`/`DP` with a properties name. An inline image's data is passed
  over.
- **What draws it** is found from the pages: a page's content, then each form it draws,
  and each form those draw. A form with no `/Resources` draws with its caller's, so a form
  it draws is looked up in those.
- **A form that names nothing is given an empty dictionary.**
- **A form is left as it was when it cannot be told what a name meant**: two callers lend
  the name for different objects, or no caller has it. Choosing one would be a guess, and
  a form given part of what it names would draw less than it does now. It still draws
  with its caller's resources. Each such case is a `Decision` under 8.10.2.
- **What is given is not recorded.** Copying an entry from a caller, or giving an empty
  dictionary, loses nothing, and a 1.x file whose forms draw with their caller's
  resources conforms. Recording it fired on PDF/A test files meant to pass, which is the
  case `ARCHITECTURE.md` §4.3 says makes a log a constant.

## Consequences

- On the 528 files, all 13 forms are given resources: ten an empty dictionary, three what
  they name. None is ambiguous or missing.
- A form drawn only from a glyph procedure, a pattern or an annotation's appearance has no
  page among its callers, so a name it uses is reported as missing and the form is left as
  it was. None occurs in the 528 files.
- `form_resources_test.rs` holds it. Each part was taken out and failed a test: the pass,
  the ambiguity check, the missing-name check, and the reading of a nested form through
  its caller's resources.
