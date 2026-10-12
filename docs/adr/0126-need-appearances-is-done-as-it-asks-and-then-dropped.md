# ADR-0126: `/NeedAppearances true` is done as it asks, and then dropped

- **Status**: Accepted
- **Date**: 2026-10-12
- **Commit**: (see the commit that adds this file)
- **Rests on**: [ADR-0120](0120-a-document-is-opened-with-every-annotation-given-an-appearance.md)

## Context

ROADMAP AB-5, row #21 of [pdf-2-0-deprecations.md](../specs/pdf-2-0-deprecations.md).
2.0 deprecates the interactive form's `/NeedAppearances` and requires every widget to
carry an appearance. Table 224 gives the entry's meaning: a flag "specifying whether to
construct appearance streams and appearance dictionaries for all widget annotations".

A producer sets it after changing a field's `/V` without changing its `/AP`. So the
appearance a file holds may show what the field held before. A reader that honours the
flag draws the new value; one that drops the flag and keeps the old appearance shows the
old one.

The owner decided on 2026-10-11 that a file's own `true` is dropped once every widget has
an appearance built. The plan said check boxes, radio buttons, choice and push buttons had
no builder. That was wrong: ADR-0120 already gives a widget with no `/AP` one from its
field, for all of them, at open. So the question left was not how to build an
appearance but which widgets to build one for. On 2026-10-12 the owner chose all
whose appearance depends on a value, over only those with none, because the second keeps
an appearance the flag says is stale.

## Decision

**Where the form says `/NeedAppearances true`, opening the document does what the entry
asks and then drops it.**

- **What depends on the value is rebuilt from it**, over any appearance the file has: a
  text or choice field is set from `/V` as filling it sets it (`set_text_appearance`),
  and a check box or radio button keeps its producer's appearances and has `/AS` set to
  the state `/V` names, or `/Off` where it names none of them.
- **What does not depend on a value is kept**: a push button's caption, a signature's
  appearance.
- **A widget with no appearance is given one** as ADR-0120 gives it, in the same pass.
- **The entry goes once every widget that needs an appearance has one**, and that is a
  `Decision` under 12.7.3. A widget whose `/Rect` is a point needs none (Table 166).
  Where one still lacks an appearance, or a rebuild fails, the entry stays and that is
  recorded: it is the only thing left saying the appearance is to be built.

It is done at open, beside ADR-0120's pass, rather than on save. Building an appearance
is how a reader reads the form, and the open document is the one the window draws.

## Consequences

- A form saved by this engine no longer carries `/NeedAppearances`, and a reader that
  ignores the entry, as 2.0 permits, draws the values the form holds.
- An appearance a producer drew for a text value is replaced by this engine's, even where
  it was current. The entry claimed it was not, and there is no telling which.
- `deprecations_test.rs` holds it: `need_appearances_rebuilds_from_the_values_and_goes`
  and `need_appearances_stays_while_a_widget_lacks_one`. Each failed with its part
  removed: the rebuild, the condition for dropping, and the drop.
