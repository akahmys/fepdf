#!/usr/bin/env python3
"""Rules A and D: what stands above the facade, what it declares, and what the facade lets in.

`CODING.md`'s Rule A says storage abstractions stop at the facade, and its enforcement row
says a frontend declares `fepdf` and nothing else. `status.sh` measured both and nothing
gated on either, so on 2026-09-07 a frontend gained a dependency on another crate above the
facade, the row read 1 where it expects 0, and `verify_compliance.sh` reported
`=== AUDIT PASSED ===`.

Two counts, both expecting 0:

* **Declarations.** A frontend may declare the facade and a library that stands above it
  ([ADR-0082](../../docs/adr/0082-the-script-crate-is-a-library-the-frontends-call.md)).
  Anything else is a frontend reaching around the facade, which is what `fepdf-gui` did to
  `fepdf-render` before ADR-0004's opt-in existed.
* **Arena leaks.** `PdfArena` and `Handle<T>` are how the object graph is stored and are
  not part of the caller's vocabulary. Above the facade they may not appear at all, in a
  frontend or in a library the frontends call.

* **Arenas made in the facade.** The facade holds a document; `fepdf-doc` builds new
  ones. Merging, extracting and the copy a save writes each called `PdfArena::new` in
  `crates/fepdf/src/lib.rs` until ROADMAP Y-5 moved them beside the cloner, which is the
  shape that let them carry their own copies of a page tree.

Rule E is a fifth, also expecting 0: **an operation does not reach into an audit.**
`fepdf-doc/src/apply/` changes documents and the audit modules judge them; an operation
that imported a helper from an audit module tied what a document may become to what the
audit happens to read ([ADR-0107](../../docs/adr/0107-an-operation-does-not-reach-into-an-audit.md)).

Rule D adds a fourth, also expecting 0: **a document is changed by `apply` and nothing
else.** `CODING.md` called that "enforced by construction" for four phases while the facade
exposed every mutation twice — as an `Operation` variant and as a plain method — and eight
frontend call sites took the method. The property lives in the facade's own type now: a
`&mut self` method on `crates/fepdf/src/lib.rs` that is not `apply`, and does not configure
saving rather than change the document, is what makes this fail.

The lists are here rather than in `status.sh`, which reports what this returns: written
twice, the two are free to disagree, and the one that is wrong is the one nobody re-reads.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# The frontends: each is something a person or another program drives, and each has an
# entry point. `fepdf-wasm` is a `cdylib` whose caller is JavaScript.
FRONTENDS = ["fepdf-cli", "fepdf-gui", "fepdf-mcp", "fepdf-wasm"]

# Above the facade and not a frontend: a library the frontends call. It has no entry point
# of its own, which is what ADR-0082 settled after four phases of it having no caller.
ABOVE_FACADE_LIBRARIES = ["fepdf-script"]

ABOVE_FACADE = FRONTENDS + ABOVE_FACADE_LIBRARIES
FACADE = "fepdf"
ARENA = re.compile(r"PdfArena|Handle<")

# The facade's one `&mut self` method that is neither `apply` nor a document change:
# `set_system_fonts` fills a cache the renderer reads, and `fepdf-render`'s examples call
# it. There were four. The other three configured a save, duplicating three fields of
# `SaveOptions` that `save_with_options` never read, and only this crate's own tests
# called them.
SAVE_SETTINGS = {"set_system_fonts"}
FACADE_SOURCE = ROOT / "crates" / "fepdf" / "src" / "lib.rs"


def FACADE_SOURCES() -> list[Path]:  # noqa: N802 — read as the constant it replaced
    """Every file of the facade's source. `PdfDocument`'s methods are in several since
    ROADMAP Y-7, and reading `lib.rs` alone would let a mutator in another pass."""
    return sorted((ROOT / "crates" / FACADE / "src").rglob("*.rs"))


def named_crates_exist() -> list[str]:
    """A name here that is not a crate silently drops that crate out of both counts."""
    return [
        f"  {name} is named above the facade and is not a crate"
        for name in ABOVE_FACADE
        if not (ROOT / "crates" / name / "src").is_dir()
    ]


def runtime_dependencies(manifest: Path) -> set[str]:
    """The workspace crates a manifest declares in a table that ships.

    **`[dev-dependencies]` is not one of them, and the distinction is the rule's.** Rule A
    is about what a frontend reaches in the product: a frontend that declares `fepdf-doc`
    can decide things the facade decides, and that is the whole objection. A crate its
    tests use to write a fixture reaches nothing at run time, and `fepdf-fixtures` is
    built so that it reaches nothing at all — it depends on nothing by default.

    This scanned the whole file until 2026-09-08, which was not a judgement about
    dev-dependencies; it was a regex over every line beginning `fepdf`, and no frontend
    had a dev-dependency on a workspace crate for it to be wrong about. The first one
    failed the audit the day it arrived.
    """
    table = None
    found = set()
    for line in manifest.read_text().splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            table = stripped.strip("[]")
            continue
        if table is None or table.endswith("dev-dependencies"):
            continue
        if not table.endswith("dependencies"):
            continue
        m = re.match(r"(fepdf[a-z-]*)", stripped)
        if m:
            found.add(m.group(1))
    return found


def stray_declarations() -> list[str]:
    """Workspace crates a frontend declares that are neither the facade nor a library."""
    allowed = {FACADE, *ABOVE_FACADE_LIBRARIES}
    out = []
    for name in FRONTENDS:
        declared = runtime_dependencies(ROOT / "crates" / name / "Cargo.toml")
        for stray in sorted(declared - allowed - {name}):
            out.append(f"  {name} declares {stray}, which is neither the facade nor above it")
    return out


def arena_leaks() -> list[str]:
    """`PdfArena` or `Handle<T>` reaching a crate that stands above the facade."""
    out = []
    for name in ABOVE_FACADE:
        for file in sorted((ROOT / "crates" / name / "src").rglob("*.rs")):
            for number, line in enumerate(file.read_text(errors="ignore").splitlines(), 1):
                if ARENA.search(line):
                    where = file.relative_to(ROOT)
                    out.append(f"  {where}:{number} names an arena type above the facade")
    return out


def facade_arenas() -> list[str]:
    """`PdfArena::new` anywhere in the facade's own source."""
    out = []
    for file in sorted((ROOT / "crates" / FACADE / "src").rglob("*.rs")):
        for number, line in enumerate(file.read_text(errors="ignore").splitlines(), 1):
            if "PdfArena::new" in line:
                where = file.relative_to(ROOT)
                out.append(f"  {where}:{number} makes an arena in the facade")
    return out


# The modules of `fepdf-doc` that judge a document rather than change it. A module is
# named here when it is an audit; one that is not, and that an operation needs, is moved
# out of this list's reach rather than added to an exception.
AUDIT_MODULES = re.compile(
    r"crate::(audit_[a-z_]+|matterhorn|glyph_map|glyph_select|glyph_widths|unicode_map"
    r"|formula_marks|page_languages)\b"
)
OPERATIONS = ROOT / "crates" / "fepdf-doc" / "src" / "apply"


def operations_reaching_audits() -> list[str]:
    """Lines of `fepdf-doc`'s operations that name an audit module."""
    out = []
    for file in sorted(OPERATIONS.rglob("*.rs")):
        for number, line in enumerate(file.read_text(errors="ignore").splitlines(), 1):
            if AUDIT_MODULES.search(line):
                out.append(f"  {file.relative_to(ROOT)}:{number} reaches an audit module")
    return out


def facade_mutators() -> list[str]:
    """`&mut self` methods on the facade that are neither `apply` nor a save setting.

    The signature is read to its opening brace rather than off the `fn` line: the first
    version of this check missed `reorder_pages_batch`, whose signature spans two lines.
    Every file of the facade is read, not `lib.rs` alone: `PdfDocument`'s methods are in
    several.
    """
    out, name, signature = [], None, ""
    lines = [line for file in FACADE_SOURCES() for line in file.read_text().splitlines()]
    for line in lines:
        opening = re.match(r"    pub (?:async )?fn ([a-z_]+)", line)
        if opening:
            name, signature = opening.group(1), line
        elif name is not None:
            signature += " " + line
        if name is not None and signature.rstrip().endswith("{"):
            if "&mut self" in signature and name != "apply" and name not in SAVE_SETTINGS:
                out.append(f"  the facade's `{name}` changes a document without an Operation")
            name, signature = None, ""
    return sorted(set(out))


def main() -> int:
    broken = named_crates_exist()
    if broken:
        print("Rule A: the list of crates above the facade is stale")
        print("\n".join(broken))
        return 1

    strays, leaks, mutators = stray_declarations(), arena_leaks(), facade_mutators()
    arenas, audits = facade_arenas(), operations_reaching_audits()
    print(
        f"declarations={len(strays)} leaks={len(leaks)} mutators={len(mutators)} "
        f"arenas={len(arenas)} audits={len(audits)}"
    )
    for line in strays + leaks + mutators + arenas + audits:
        print(line)
    if strays or leaks:
        print("Rule A: a frontend declares the facade, and a library that stands above it")
    if arenas:
        print("Rule A: the facade holds a document; fepdf-doc makes new ones")
    if mutators:
        print("Rule D: a document is changed by `apply` and nothing else")
    if audits:
        print("Rule E: an operation does not reach into an audit")
    if strays or leaks or mutators or arenas or audits:
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
