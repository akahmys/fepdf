# ADR-0057: The released binaries could not read Japanese, because the resources they look for were never built

- **Status**: Accepted
- **Date**: 2026-09-05
- **Commit**: (see the commit that adds this file)

## Context

`fepdf_font::resources` looks for Adobe's CMap collections in five places, in order: a root
named by `FEPDF_RESOURCES`, `<exe>/../share/fepdf/`, `<exe>/resources/`, the per-user and
system data directories, and last the source tree through `CARGO_MANIFEST_DIR`. It was
written for a program that ships its data.

**No release ever shipped any.** `fepdf-macos-arm64.tar.gz` holds two files: `fepdf` and
`fepdf-gui`. On any machine that is not a checkout of this repository, all five paths miss.

**Measured, on `samples/bokutokitan.pdf`:**

| | 9.7.3 violations | text extracted |
| :--- | ---: | ---: |
| built in the repository | 0 | **212,312 bytes** |
| the published binary, run outside it | one per CJK font | **4,554 bytes** |

The engine says so plainly, once per font: *"belongs to character collection Adobe-Japan1,
whose CID-to-Unicode table this engine does not carry … left its codes unnamed"*. Everything
[ADR-0041](0041-a-character-collection-is-declared-not-guessed.md) and
[ADR-0044](0044-the-other-four-collections-were-on-disk.md) established is unavailable in
the product.

**It was not an oversight in packaging. The build machine did not have the data either.**
`external/` is `.gitignore`d. `external/adobe-cmaps` is a submodule, and the workflow's
`actions/checkout@v4` had no `submodules:` key, so CI saw an empty directory.
`external/mapping-resources-pdf` was not a submodule at all — a manual clone that existed
on one developer's machine and nowhere else.

## Decision

**`external/mapping-resources-pdf` is a submodule**, pinned like its sibling, so the
CID-to-Unicode tables have a provenance and a version rather than a working copy.

**The build checks out submodules, and every archive carries `resources/`** — `cmaps`,
`cid2unicode` and `scripting` — beside the binaries. `<exe>/resources/<name>` is already
the third search path, so **no code changed**: the layout is the whole of the contract.

The staging step fails the build if `resources/cmaps/Adobe-Japan1-7` is not there. An
archive that silently omits the data is what this record is about, and a green build that
ships one is the same failure again.

**macOS ships as two architectures.** `lipo` produced an archive that was exactly the sum
of the two — 22MB where an Apple Silicon reader needs 10 — so every Mac downloaded the other
architecture as well. That trade made sense while Intel was the majority. It also packaged
macOS as a `.zip` where every other Unix target is a `.tar.gz`.

## Consequences

An archive is 17MB where it was 10. The data is 19MB uncompressed and compresses well; it
is the price of a reader that reads.

**Verified by rehearsing the workflow by hand**: staging the three directories, building the
archive, unpacking it elsewhere and running the binary from there — 0 violations, 212,342
bytes.

**`fepdf-macos-universal.zip` on v1.0.0 stays where it is.** A released asset that people
may have linked to is not renamed, and it is the only macOS archive that release has.

**`assets/fonts` is an empty directory**, and `Resource::Fonts` searches for it. Nothing
fills it in the repository either, so the fallback fonts are absent everywhere and not only
in the archives. That is a separate hole and is not fixed here.

**This was found by asking what two untracked directories were.** The gap had been in every
release since the first, and no test could see it: the suite runs in the repository, where
the fifth search path always hits.
