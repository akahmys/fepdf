# ADR-0102: An error says whose it is, and `PdfError::Other` is removed

- **Status**: Accepted
- **Date**: 2026-09-28
- **Commit**: (see the commit that adds this file)

## Context

RR-15 Rule 11 forbids string-based errors in core APIs. `PdfError` is a `thiserror` enum,
so the grep that enforces the rule passed, and its last variant,
`Other(Cow<'static, str>)`, carried a string. On 2026-09-28 it was constructed at **199
sites** outside tests, the most in `fepdf-model/src/writer.rs` (21),
`fepdf-model/src/document.rs` (21) and `fepdf-doc/src/apply/text.rs` (18).

Reading the messages found three things.

1. **One condition had six spellings.** A page index past the end was "the page is not
   there" at nine sites, and elsewhere "Page index out of bounds", "there is no page {}",
   "page {} is not there", "Page 1 missing" and "this document has {count} pages and no
   page {page}". Only the last told the caller how many pages there were.
2. **The messages fell into four kinds**, which a caller answers differently:
   - the caller named something that is not there: a page, a run, a field, an object, a
     layer, an MCID;
   - the request is well formed and this document cannot take it: a run that is the last
     on its page has nothing to join to, a face does not draw the character, a scale of
     zero draws nothing;
   - the document is malformed where the operation needs it: no catalogue, a `/Kids`
     that is not an array;
   - the engine contradicted itself: "Stack underflow", "the signature object was never
     written".
3. **No frontend could tell them apart.** `fepdf-mcp` wraps every engine error as
   `McpError::Pdf(String)`, so an argument the model got wrong and a defect in this engine
   reached the client as the same thing. The window's notices were in the same position.

`fepdf` is not published to crates.io, and every caller of `PdfError` is in this
workspace.

## Decision

**`PdfError::Other` is removed.** Its sites move to variants that say whose the error is:

| Kind | Variant | Carries |
| :--- | :--- | :--- |
| The caller named something absent | `NotFound(Missing)` | `Missing` is an enum: `Page { index, count }` and one variant per other kind of name. The condition is in the type, not the text |
| The request cannot be taken by this document | `Refused { operation, why }` | The operation's name, and the reason as text for a person |
| The document is malformed where it is needed | `ClauseViolation`, which already exists | The clause |
| The engine contradicted itself | `Internal`, which already exists | |

The reason in `Refused` stays text. The caller's need is to know which kind of error it
is, and each reason is read by a person. Making every reason a type would add one
variant per reason, and no caller would match on any of them.

The six spellings of a missing page become `Missing::Page { index, count }`, and a single
`Display` implementation writes its message.

**`McpError::Pdf` carries the `PdfError`** instead of a string. The server answers
`NotFound` and `Refused` as errors in the call the model made, and `Internal` as a
server error.

**rustc enforces this rule.** Once the variant is gone, a new `PdfError::Other(` does not
compile, so no audit step is needed.

Keeping `Other` and gating a ratchet on its count was the alternative. It was rejected
because a ratchet allows the sites that already exist. Those 199 sites are what hid the
six spellings.

## Consequences

- The public error type of the facade changes. Nothing outside this workspace depends on
  it.
- The work is ROADMAP Phase Y's **Y-6**. It goes by file, starting with the most sites,
  and each commit leaves the tree building.
- A test that asserts on an error's text asserts on its variant instead, where the
  variant is the point. The cryptographic messages in `fepdf-syntax/src/cms.rs` are not
  `PdfError` and are unaffected.
