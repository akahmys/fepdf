# ADR-0125: Form values come in by name, and make no field

- **Status**: Accepted
- **Date**: 2026-10-11
- **Commit**: (see the commit that adds this file)
- **Rests on**: [ADR-0117](0117-an-fdf-import-replaces-an-annotation-of-the-same-name.md)

## Context

ROADMAP AA-3b: a form's values exported and imported through FDF's `/Fields` (Table
249). It was kept apart from AA-3 because form values are matched by field name, and the
form's own rules decide what a value may be. XFDF carries the same values as `<fields>`
(ISO 19444-1 6.3). That standard says an import cannot create a field, nor change
anything but the value of one that exists (5.6.1).

Table 249 also lets an FDF field replace or change the field's flags (`Ff`, `SetFf`,
`ClrFf`) and its widgets' (`F`, `SetF`, `ClrF`). It can also carry appearances (`AP`,
`APRef`, `IF`), options (`Opt`), actions (`A`, `AA`) and rich text (`RV`).

## Decision

**A value fills the field of the same fully qualified name, and nothing else is made.**

- **The import takes what the file carries.** `ImportFdf` and `ImportXfdf` put its
  comments in by `/NM` (ADR-0117) and its values in by name. The whole file is read
  before anything changes.
- **Strings are decoded as `/Encoding` declares**, for every name, value and option that
  does not begin with U+FEFF. Every value Table 246 names is read: PDFDocEncoding,
  `utf_16`, `utf_8`, `Shift_JIS`, `BigFive`, `GBK` and `UHC`. The last four are decoded
  by `encoding_rs` as the WHATWG Encoding Standard does, so Shift_JIS includes Windows'
  extensions (髙, ①) as Windows writes them. A string invalid in its encoding is no text,
  rather than one with replacement characters. An encoding Table 246 does not name is
  refused, because there is no saying which characters its bytes are. This is not the
  guessing ADR-0124 refuses for plain text: the file declares its encoding.
- **A value is set as filling the form by hand sets it**, through `SetFormFieldValue`, so
  its appearance is drawn the same way. A name is a button's state. Of several choices,
  the first is set, because the form is filled one choice at a time; the rest are
  recorded.
- **Flags are applied as Table 249 says**: a whole `Ff` or `F` replaces, and otherwise
  bits are set and then cleared. `F` goes on each of the field's widgets.
- **What is not taken is recorded, not refused**: a field the form lacks, a value the
  field cannot take, and the entries above that are not values or flags.
- **Export writes values, not the form.** Each terminal field is written by its partial
  name, nested as the form nests it, with its `/V`. A signature's value is not written
  (ISO 19444-1 6.3.3).
- **Comments and values are exported separately**: `export_fdf` and `export_fdf_form`,
  and the same for XFDF. A reader asking for one does not want the other. The window
  reaches the form's export and import from the form drawer, as Acrobat reaches them
  from Forms. Both drawers' imports go through one function and one operation.

## Consequences

- An FDF or XFDF from another producer fills the fields it names, and says what it could
  not fill.
- A multi-select choice comes in with one choice. Holding several needs `FormValue` to
  hold them, which is a change to the form-filling operation, not to the import.
