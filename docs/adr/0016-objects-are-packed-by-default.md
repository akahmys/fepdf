# ADR-0016: Objects are packed into object streams by default

- **Status**: Accepted
- **Date**: 2026-08-17
- **Commit**: this record

## Context

Object streams (7.5.7) and the cross-reference streams (7.5.8) they require were built
opt-in, behind `--obj-stm`. The measurement was immediate and one-sided: every sample in
the corpus came out smaller, none larger, and `samples/intel_sdm.pdf` — which keeps
323,066 of its objects in 8,044 containers and which this engine had been unpacking
every one of — went from **+131% to +1%** of its source.

The default was deliberately left alone at the time, for two reasons that were not the
same weight. One was real: only **one** independent reader had checked the result. The
other was caution about changing every output byte for every caller.

That caution has a specific precedent here. `SaveOptions::compress` defaulted to `false`
while `fepdf-gui` set it `true`, so the same operation had two answers depending on who
asked — which is what Rule D exists to stop. But the lesson there was about *divergent*
defaults, not about the value of any particular default, and it does not apply: the GUI
takes `..SaveOptions::default()` and cannot diverge.

So the question reduced to the first reason, and that is a measurement.

## Decision

**Objects are packed by default.** `--obj-stm` becomes `--no-obj-stm`, following
`--no-compress`.

A second independent reader was obtained and agrees. `pypdfium2` is PDFium — Chrome's
and Edge's engine, sharing no code with PDFKit — and it reads the same text out of the
packed file as out of the loose one, **page by page**, on all nine samples. It also
opens a packed *and* encrypted file with the password and refuses it without, which
independently confirms [ADR-0015]'s AES-256 work at the same time.

Both readers now run in `scripts/test/crosscheck_objstm.sh`, so the measurement that
decided this is repeatable rather than something someone once did.

## What this is not

It is **not** the same argument as [ADR-0015]. That one turns on deprecation: 7.6.4.1
tells readers to stop trusting RC4, so writing it into a file declaring itself PDF 2.0
puts a weaker guarantee behind a newer number. **A classic cross-reference table is not
deprecated.** It is perfectly valid in PDF 2.0 and always will be. There is no
conformance argument here at all — only size, and two readers saying the size costs
nothing.

Stating that plainly matters because the two decisions look alike and are not. If a
third reader is ever found that mishandles type 2 entries, this decision should be
revisited and ADR-0015's should not.

## Consequences

- **Every output byte changes** for callers who do not pass `--no-obj-stm`. Files get
  smaller; nothing else about them changes that two readers can detect.
- **A produced file is no longer readable in a text editor.** This is the real cost, and
  it falls hardest on whoever is debugging the writer. `--no-obj-stm` exists for exactly
  that, and `fepdf inspect` reads either form.
- **Four of the nine samples already arrived packed** — `bokutokitan`, `fy05`,
  `print_sample` and `intel_sdm`, the last with 323,066 objects in containers. Not a
  majority, which is worth stating accurately: the argument is the measurement, not an
  appeal to what other producers do.
- **`intel_sdm.pdf` stops being the outlier** that every size claim had to be qualified
  against.

[ADR-0015]: 0015-this-engine-reads-five-encryption-schemes-and-writes-one.md
