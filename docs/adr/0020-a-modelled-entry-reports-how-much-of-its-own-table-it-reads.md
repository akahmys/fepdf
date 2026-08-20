# ADR-0020: A modelled entry reports how much of its own table it reads

- **Status**: Accepted
- **Date**: 2026-08-20
- **Commit**: this record

## Context

Phase K gave a reader to each of the fourteen catalogue entries the two corpora present,
taking `Support::Modelled` from 6 of Table 29's 32 to 20 — and, against the twenty keys
those 251 files actually carry, from 5 to 19. The one that stays `Declared` is `/Type`,
whose value 7.7.2 fixes at `/Catalog`: a reader for it is an assertion, not a type.

That figure is the problem this record is about. **19 of 20 is exactly the shape of the
number ADR-0017 was written to prevent**, one level down. `/AcroForm` is now modelled,
and its reader reads `/NeedAppearances`, `/SigFlags`, `/DA` and `/Q` while leaving
`/Fields`, `/CO`, `/DR` and `/XFA` as objects — four of Table 224's eight. A caller told
"modelled" and nothing else would conclude the form is understood.

The same is true wherever a subsystem hangs off an entry: `/StructTreeRoot`'s `/K` is the
structure tree, `/OCProperties`'s `/D` is a configuration dictionary, `/AcroForm`'s `/DR`
is a resource dictionary. Reading those means building the subsystem, not the entry.

## Decision

**A catalogue entry is `Modelled` when its reader reads that entry's own scalars, and
`inspect catalog` reports separately how much of the entry's own table the reader
covers.** `CatalogEntry::inner` carries "modelled / total" for every entry whose table is
a fixed set of keys, derived from the reader type's `PdfSchema::pdf_key_types` — the same
mechanism, and the same classifier, that produced the headline figure.

Entries whose contents are not a table of keys report nothing there: a number tree
(`/PageLabels`), ten name trees (`/Names`), an XMP packet (`/Metadata`), an array
(`/OutputIntents`, `/Threads`), a value that is one of two shapes (`/OpenAction`), and
the scalars. `every_entry_with_a_fixed_table_is_listed` names those exceptions and fails
if a modelled entry gains a fixed table and nobody wires its reader in.

**Twelve keys are declined a reader**, and the refusal lives in the code as
`catalog::ABSENT_FROM_BOTH_CORPORA` with the measurement that justifies it: they occur in
no file of either corpus. `the_keys_no_file_carries_did_not_gain_readers` is the
container rule enforced from the other side — it fails if one of them is ever modelled.
`/NeedsRendering` is the single exception and is named as such, because it was already
modelled when the rule arrived (ADR-0017 records why).

## Consequences

`inspect catalog` prints an `own table` column. `intel_sdm.pdf` reads 10 of its 11
entries as modelled, and **26 of the 36 keys** those entries' own tables define — two
numbers that say different things, where before there was one that said the flattering
one.

The coverage index (ADR-0019) moved with the phase: catalogue entries went from 5 of 20
to 19 of 20 across both corpora, and the total from 82% to 88%. Read together with the
column above, that is honest; read alone, it is the claim this record exists to qualify.
The index deliberately does **not** fold the nested figure in — an axis whose numerator
is "sum of nested fractions" would weight `/ViewerPreferences`, with eighteen scalars,
above `/StructTreeRoot`, which is a whole clause.

The check that guards it all is `crosscheck_selfread.sh`, which now compares what the
catalogue *says* across a round trip rather than which keys survived — it could only ever
compare `dictionary[3]` with `dictionary[3]` while the entries were opaque. On its first
run it found one difference, and it was the one ADR-0013 predicts: `bokutokitan.pdf`'s
page-tree root carries an inheritable `/MediaBox` that the writer resolves onto each
page. That is normalised away, and finding it is the evidence that the check can see
contents.
