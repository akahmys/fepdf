# fepdf

A PDF engine in Rust that tells you what it did to your file.

Most PDF tools read a document, silently repair whatever was wrong with it, and hand
you something that looks fine. fepdf records every one of those choices and reports
them, because the ones it made in silence turned out to be where its worst bugs lived.

```
$ fepdf inspect info report.pdf

--- [ DECISIONS TAKEN READING (5.3) ] ---
  1 ambiguities, 1 repairs, 0 violations
  [AMBIGUITY] ISO 14.3.3 : /Info /ModDate is "D:20241114200008+09'00'" and the
    metadata stream says "2024-11-08T09:08:18+09:00" -> took the metadata stream
  [REPAIRED] ISO 14.3.3 : /Info carries Title, Subject, Keywords, Creator,
    Producer, deprecated in PDF 2.0 -> moved to the document's metadata stream
```

Every line names the clause of ISO 32000-2 it rests on, what was found, and what was
done about it — enough to disagree with.

> **This is experimental software.** It is a personal project, not a product. Read
> [What it cannot do](#what-it-cannot-do) before pointing it at anything you care
> about, and keep your originals.

## Building

Needs Rust 1.94 or later. There are no published binaries.

```bash
cargo build --release
```

That gives you `target/release/fepdf` (command line) and `target/release/fepdf-gui`
(desktop). The GUI wants a GPU; the CLI does not.

## What you can do with it

### Look at a file without changing it

```bash
fepdf inspect structure report.pdf   # revisions, cross-reference form, object storage
fepdf inspect catalog report.pdf     # every catalogue entry, and what the engine makes of it
fepdf inspect encryption report.pdf  # what protects it, and how far the engine conforms
fepdf inspect interactive report.pdf # annotations, form fields, actions, outline
fepdf inspect info report.pdf        # metadata and a font summary
fepdf inspect text report.pdf        # extracted text
fepdf inspect audit report.pdf       # compliance audit against ISO 32000-2 and UA-2
fepdf inspect tree report.pdf        # the logical structure tree
```

`structure`, `catalog`, `encryption` and `interactive` report **the file as written** —
they read the bytes and decrypt, and nothing else. The rest report **the document the
engine made of it**, which is not the same thing and is not meant to be
([ADR-0013](docs/adr/0013-a-document-is-one-normalised-state.md)).

### Change one

```bash
fepdf publish upgrade in.pdf out.pdf              # rewrite as PDF 2.0
fepdf publish upgrade in.pdf out.pdf --strip      # …and remove all descriptive metadata
fepdf publish render in.pdf page.png --page 1     # render a page
fepdf edit merge a.pdf b.pdf -o out.pdf
fepdf edit split in.pdf --pages 1-10 -o out.pdf
fepdf edit rotate in.pdf -o out.pdf --pages 1 --angle 90
```

Encrypted files open with `--password`. Give any command `--help` for its own options.

**Saving produces a new document, not an edited one.** fepdf normalises a file as it
reads it, so it cannot write your original back byte for byte, and anything that
depends on those exact bytes does not survive. It says so when that costs you
something:

```
[VIOLATION] ISO 7.6.4.2 : the document was opened with user access and its /P
  (-1068) permits no modification and no assembly and no annotation -> this engine
  normalises at load and has no path that writes a faithful copy, so the output is
  modified; /Encrypt cannot survive decryption either, so it declares no permissions
  at all
```

The output records where it came from, in `xmpMM:DerivedFrom` and
`xmpMM:OriginalDocumentID`, so a reader can trace it back without having watched.
The reasoning is in [ADR-0012](docs/adr/0012-saving-produces-a-new-document.md).

## What it cannot do

| | |
| :--- | :--- |
| **Write encrypted files** | It reads every password-based scheme the standard defines — RC4, AES-128, AES-256 — but writes none. Output is unencrypted. |
| **Sign a file it did not write** | It signs its own output — `publish sign` and `publish verify-signature` — and a signature it made covers the whole file. It cannot add one to someone else's file without rewriting it first, and a signature already in a document does not survive a save. [ADR-0014](docs/adr/0014-the-faithful-copy-path-is-not-built.md) is why. |
| **Decide whether to trust a certificate** | `verify-signature` says whether the signature covers the bytes and is bound to the certificate it carries. It has no trust store, checks no validity window, and reads no revocation list — and says so in its output. |
| **Preserve a file byte for byte** | There is no faithful-copy path. See above. |
| **Edit interactive features** | Annotations, form fields and outlines can be read and reported. The only one it writes is a signature field, and only as part of signing. |
| **Public-key security handlers** | 7.6.5 is recognised and reported, not implemented. |
| **Run usefully in a browser** | `fepdf-wasm` opens a document and counts pages. Its `render_page` does nothing. |
| **Write object streams** | 7.5.7 containers are read but not written. A file that relies on them heavily grows: `intel_sdm.pdf` comes out 132% larger. Everything else in the corpus is within 1% of its source or smaller. |

Nineteen SDK operations report `NotImplemented` rather than pretending. Ten of Table
29's thirty-two catalogue entries have a typed representation; the rest survive a round
trip but cannot be reasoned about, and `inspect catalog` names which ones for your file.

[ROADMAP.md](ROADMAP.md) has the measured state clause by clause, and
`./scripts/dev/status.sh` re-derives those numbers so a stale one shows up as a
disagreement rather than reading as current.

## The crates

| Crate | |
| :--- | :--- |
| `fepdf-model` | The engine: objects, file structure, encryption, fonts, refinement |
| `fepdf-syntax` | Lexer and cryptographic primitives, and nothing else |
| `fepdf-sdk` | The API most callers want |
| `fepdf-cli` | The `fepdf` command |
| `fepdf-gui` | Desktop application (egui, wgpu, Vello) with CAD measurement and redaction |
| `fepdf-render`, `fepdf-font`, `fepdf-content` | Rendering, font programs, content streams |
| `fepdf-mcp` | Model Context Protocol server for structural diagnostics |
| `fepdf-wasm` | WebAssembly bindings — a stub, see above |

## Contributing

The project runs on a few rules that are enforced rather than aspired to: no `unsafe`,
functions under fifty lines, no non-deterministic collections in the core, and no
wildcard match over a domain enum. `make audit` checks all of them.

```bash
make audit                        # RR-15 rules, clippy, cargo-deny, betterleaks
cargo test --workspace
./scripts/test/cli_smoke.sh       # every subcommand starts in a debug build
./scripts/test/crosscheck_roundtrip.sh   # text preserved, compared against PDFKit
python3 scripts/visual_regression.py     # GPU rendering
```

Two habits matter more than the rules:

- **Measure before designing.** This codebase has repeatedly paid for building a
  container before its contents existed.
- **Prove a check fires by breaking the thing it checks.** Several tests here passed
  against the very defect they were written for, and were only found to be inert when
  someone put the bug back.

| | |
| :--- | :--- |
| [AGENTS.md](AGENTS.md) | Truth hierarchy and operating principles — start here |
| [ARCHITECTURE.md](ARCHITECTURE.md) | System design, layering rules, the pipeline |
| [CODING.md](CODING.md) | The RR-15 rules in full |
| [TESTING.md](TESTING.md) | Testing strategy |
| [AUDITING.md](AUDITING.md) | Static audits, licences, secret scanning |
| [PLANNING.md](PLANNING.md) | Planning and codebase discovery |
| [docs/adr/](docs/adr/README.md) | Why things are the way they are, including reversals |

## Licence

MIT.
