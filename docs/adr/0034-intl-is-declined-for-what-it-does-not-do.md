# ADR-0034: The locale is refused rather than ignored, and `intl` is declined for what it does not do

- **Status**: Accepted
- **Date**: 2026-08-23
- **Commit**: (this change)

## Context

A form that formats currency reaches ECMA-402, the internationalisation half of the
language ISO 32000-2 12.6.4.16 admits. This engine does not carry it. **Measured, its
absence looked like three different things:**

| called | answered |
|---|---|
| `new Intl.NumberFormat('de-DE')` | `ReferenceError: Intl is not defined` |
| `new Date(0).toLocaleDateString('de-DE')` | `Function Unimplemented` |
| `(1234567.891).toLocaleString('de-DE')` | **`"1234567.891"`** |

The third is the defect, and it is the shape this month keeps producing:
`fepdf-wasm::render_page` returning `Ok(())` having drawn nothing, a glyph run vanishing
without a `Decision`, `getField().value = 3` accepted and dropped. `toLocaleString` lives
on `Number.prototype` and exists with or without ECMA-402, so it **took the locale,
ignored it, and returned success** — a German invoice reading `1234567.891` where
`1.234.567,891` was asked for, and a script that believes it formatted the number.
`Array.prototype.toLocaleString` inherits the same answer, delegating to each element's.

ECMA-262 §21.1.3.4 permits the ignoring: the result is implementation-dependent and
returning what `toString` returns is allowed. **That is why this is not a bug report
against boa.** What no clause permits is Rule 20 — the engine knowing it answered a
different question than the one asked, and recording nothing.

Building it was the obvious alternative, and it was measured before it was declined.

## Decision

**ECMA-402 stays out, and its absence is made to look like one thing instead of three.**

* **A named locale is refused.** The call raises into the script with a sentence naming
  the clause, and a `Violation` records what was asked for. Unlocalised digits answer a
  different question than `de-DE` does.
* **No locale named is answered.** The script asked for *this host's default*, and this
  host's default really is unlocalised digits — the one case where the old answer was not
  a lie. An `Ambiguity` records which reading was taken, once per script execution: a
  loop formatting a column would otherwise write a thousand identical decisions into a log
  `inspect structure` prints in full. A calculation order that takes two passes over two
  formatting fields still records four, which is the shape 12.6.3's own `Violation`s have.
* `Date`'s three threw already, and still do — with the same sentence, and recording.

**`intl` was enabled, built and run before it was declined**, and what it answered is the
reason. Measured 2026-08-23 against `boa_engine 0.21.1`, feature `intl_bundled`,
`cargo check` green in 3m47s:

| a form asks for | boa 0.21.1 with `intl_bundled` |
|---|---|
| `new Intl.NumberFormat('de-DE').format(1234567.891)` | `1.234.567,891` ✅ |
| `new Intl.NumberFormat('de-DE', {style: 'currency', currency: 'EUR'})` | **`TypeError: unimplemented`** |
| `new Intl.DateTimeFormat('de-DE').format(new Date(0))` | **`TypeError: not a callable function`** |
| `Object.getOwnPropertyDescriptor(Intl.DateTimeFormat.prototype, 'format')` | **`undefined`** — the property does not exist |
| `new Date(0).toLocaleDateString('de-DE')` | `Function Unimplemented` — unchanged, and unchangeable |
| `'ä'.localeCompare('z', 'de')` | `-1` ✅ |
| `new Intl.PluralRules('de-DE').select(1)` | `one` ✅ |

**The two a PDF form reaches ECMA-402 for are the two that are missing.** Currency is why
`AFNumber_Format` exists; dates are the other half of an invoice. `Date.prototype`'s three
are not even feature-gated — `builtins/date/mod.rs:1621` returns the literal string
`"Function Unimplemented"` with no `cfg` on it, so the `intl` feature cannot reach them.
What arrives instead is decimal grouping, collation, plurals and list formatting.

The price for that: 30 crates, **10.2 MB** of ICU data compiled in (`icu_datetime.postcard`
alone is 5.0 MB, and it is the one whose consumer has no `format` method), and a full
re-lock of the workspace — `boa_engine` pins `icu_provider = "~2.0.0"` while this tree
holds 2.2.0, so the lockfile does not resolve incrementally and 636 packages move.

**The determinism hole is real and smaller than the sentence this ADR first carried.**
`DefaultLocale()` is `sys_locale::get_locale()` at `builtins/intl/locale/utils.rs:35`, and
`resolve_locale` consults it only when the requested locale list resolves to nothing — no
argument, or a tag with no data. A script naming `de-DE` never reads the machine.
Demonstrated on the machine that ran this, whose `AppleLocale` is `ja_JP`:
`new Intl.NumberFormat().resolvedOptions().locale` answered **`ja`**.

**And there is a floor, which this ADR was wrong to say there was not.** boa offers no
native seam — `Clock` replaces `SystemTime::now` and
`HostHooks::local_timezone_offset_seconds` replaces the machine's zone, both wired in this
same change after being declared and left dead, and the locale has no equivalent — but
this crate owns an interception layer of its own, the one built above. Substituting an
injected `ScriptEnvironment::default_locale` where a call names none would close it, at
the cost of wrapping the `Intl` constructors and `String.prototype`'s three as well.

So the decision does not rest on determinism. It rests on paying 10.2 MB and a re-lock for
a currency formatter that throws.

## Consequences

**A form that formats currency by locale now fails instead of printing the wrong number.**
That is the intended trade and it is a real loss: `run_calculations` stops the whole
calculation order at the first script that will not complete, so one `toLocaleString('de-DE')`
takes the form's other fields with it. It is preferred because a stopped run is visible
and a wrong invoice total is not. No corpus script calls any of these — 0 of 8 — so
nothing regressed today; the choice is about the first form that does.

**Three more methods ignore their locale as silently as the formatters did**, and are left
alone because a calculate action does neither collation nor Turkish casing. They are
measured and pinned in `locale_test.rs` rather than assumed, so the gap stays a fact:
`'ä'.localeCompare('z', 'de')` answers `1` where a viewer with ECMA-402 answers `-1`,
and `'i'.toLocaleUpperCase('tr')` answers `"I"` where it answers `"İ"`.

**The two gaps have different causes, and therefore different odds.** Currency is blocked
upstream of boa: `number_format/mod.rs:74` says *"Missing support from ICU4X for
Percent/Currency/Unit formatting"*, and line 329 names the missing piece — `CurrencyDigits(currency)`,
the per-currency fraction-digit table. ICU4X does have a `CurrencyFormatter`, but it lives
in `icu_experimental` rather than the stable surface boa depends on.
`Intl.DateTimeFormat` is blocked on nothing: `init()` registers no prototype methods at
all, `InitializeDateTimeFormat` is a bare `// TODO` at line 137, and **`icu_datetime` is
imported at exactly one place in the whole of boa — for the `HourCycle` enum**. The 5.0 MB
of date data ships and is never formatted with.

**What re-opens this**: an upstream boa that implements `Intl.DateTimeFormat.prototype.format`
and `NumberFormat`'s currency style. The first needs only boa's own time; the second waits
on ICU4X stabilising what is experimental today. That is the whole of it — the determinism hole has a
floor this crate can build, and the dependency cost breaks no rule and lands on no shipping
binary, since nothing in this workspace links `fepdf-script` at all. The feature is
declined for what it does not do, not for what it costs.

**ADR-0026's test is the one that was applied.** The question is whether the correctness
of work already undertaken depends on it: form editing depends on *the formatting being
right*, not yet on *the locale being available*. Refusing keeps the first true without
claiming the second.
