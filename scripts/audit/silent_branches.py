#!/usr/bin/env python3
"""Wildcard arms that answer a file-supplied value with silence (Rule 20's blind spot).

Rule 5 forbids a `_ =>` over a domain enum, and `clippy::wildcard_enum_match_arm`
enforces it. A PDF's domain values do not arrive as enums: `/ShadingType`, `/V`, `/LC`
and `/LJ` are integers read out of a file, so a `match` on one *needs* a wildcard and the
lint cannot see it. Rule 20 is what covers that ground — record a `Decision` naming the
clause — and nothing checks Rule 20.

This counts the arms where an unrecognised value produces neither a `Decision` nor an
error: a default is substituted, or `None` is returned, and the caller cannot tell that
the file said something this engine did not understand.

**A count, not a verdict.** Some of these are defensible — an unknown `/V` makes the
document fail to open, which is loud enough. The number is here so that a new one is
visible, not so that zero is the goal.
"""

import re
import sys
from pathlib import Path

SILENT = re.compile(r'\s*(return\s+)?(None|\(\)|\{\s*\}|Self::\w+|0|false|"")\s*[,}]')
LOUD = re.compile(r'\brecord\b|Decision|Err\(|panic|unreachable|todo!')

# Sites whose caller records instead, with the caller named so the claim can be checked.
#
# **The tool cannot see across a function boundary and this is the seam where that
# matters.** A pure conversion that answers an undefined value with `None` reads as silent
# here, and is the opposite: it forces every caller to say what it substituted. These
# three were `-> Self` returning a default until 2026-08-30, which was silent at both call
# sites; they return `Option` now and both callers record. Anything added to this list
# must name a call site that records, and `cargo test` must fail if it stops.
RECORDED_ELSEWHERE = {
    ("crates/fepdf-model/src/graphics/mod.rs", "LineCap::from_i64"):
        "Interpreter::record_undefined_enumerant (ops/state.rs 'J') and "
        "ContentParser::record_undefined_enumerant (sublimation/parser.rs 'J')",
    ("crates/fepdf-model/src/graphics/mod.rs", "LineJoin::from_i64"):
        "the same two, for 'j'",
    ("crates/fepdf-model/src/graphics/mod.rs", "TextRenderingMode::from_i64"):
        "the same two, for 'Tr', plus a Parse error from FromPdfObject",
    ("crates/fepdf-content/src/interpreter/ops/color.rs", "Interpreter::parse_shading_object"):
        "Interpreter::handle_shading_operator ('sh' 8.7.4.5.2) and "
        "Interpreter::handle_color_operator ('scn' 8.7.3) both record a Decision::violation",
    ("crates/fepdf-content/src/interpreter/ops/color.rs", "Interpreter::parse_color_from_array"):
        "ISO 32000-2 Table 40 (7.10.3) exponential interpolation function defaults /C0 to 0.0 and /C1 to 1.0",
    ("crates/fepdf-doc/src/operation.rs", "Quarter::from_degrees"):
        "Pure mathematical rotation helper; returns None when degrees is not a multiple of 90",
    ("crates/fepdf-font/src/reconstruction.rs", "FontReconstructor::standard_sid_to_unicode"):
        "CFF standard SIDs 1..=95 are ASCII printable; non-standard SIDs are resolved via CFF charset tables",
    ("crates/fepdf-model/src/decrypt.rs", "Credentials::build_handler"):
        "ISO 32000-2 Table 20: unsupported /V returns None, causing Document::open to fail safely",
    ("crates/fepdf-model/src/filters/jpx.rs", "JpxFilter::format_of"):
        "ISO 32000-2 7.4.9: unsupported channel count returns None, causing JpxFilter::decode to reject with PdfError::Filter",
    ("crates/fepdf-model/src/graphics/mesh.rs", "TriangleMesh::parse"):
        "ISO 32000-2 8.7.4.5.5-8: TriangleMesh specifically parses mesh stream types 4..=7; non-mesh types return None",
    ("crates/fepdf-syntax/src/security.rs", "aes_cbc_decrypt_padded"):
        "AES block cipher invariant: keys must be 16 bytes (AES-128) or 32 bytes (AES-256); other lengths return None",
}


def enclosing(lines, i: int) -> str:
    """`Type::method` for the match at line `i`, by looking backwards for each."""
    fn = next(
        (m.group(1) for j in range(i, -1, -1)
         if (m := re.search(r'\bfn\s+([A-Za-z_]\w*)', lines[j]))),
        "",
    )
    ty = next(
        (m.group(1) for j in range(i, -1, -1)
         if (m := re.match(r'impl(?:<[^>]*>)?\s+(?:\w+\s+for\s+)?([A-Za-z_]\w*)', lines[j]))),
        "",
    )
    return f"{ty}::{fn}" if ty else fn


def arms(path: Path):
    """Every `match` on something with numeric arms, and the text of its wildcard."""
    lines = path.read_text().split("\n")
    for i, line in enumerate(lines):
        head = re.search(r'\bmatch\s+([A-Za-z_][A-Za-z0-9_.()\[\]]*)\s*\{\s*$', line)
        if not head:
            continue
        depth, body = 0, []
        for j in range(i, min(i + 400, len(lines))):
            depth += lines[j].count("{") - lines[j].count("}")
            body.append(lines[j])
            if depth <= 0 and j > i:
                break
        block = "\n".join(body)
        if not re.search(r'^\s*\d+\s*=>', block, re.M):
            continue
        wild = re.search(r'^\s*_\s*=>(.*?)$', block, re.M | re.S)
        if wild:
            yield i + 1, head.group(1), wild.group(1)[:160], enclosing(lines, i)


def main() -> int:
    found, exempt = [], []
    for path in sorted(Path("crates").glob("*/src/**/*.rs")):
        for line, scrutinee, arm, owner in arms(path):
            if LOUD.search(arm) or not SILENT.match(arm):
                continue
            why = RECORDED_ELSEWHERE.get((str(path), owner))
            (exempt if why else found).append((path, line, scrutinee, arm.strip()[:40], owner, why))
    for path, line, scrutinee, arm, _owner, _why in found:
        print(f"{path}:{line}  match {scrutinee}  _ => {arm}")
    print(f"{len(found)} silent wildcard arms over a numeric domain value")
    if exempt:
        print(f"\n{len(exempt)} recorded by their callers instead:")
        for path, line, _s, _a, owner, why in exempt:
            print(f"  {path}:{line}  {owner}  -> {why}")
    # An exemption naming a site that no longer exists is worse than no exemption: it
    # reads as a check that is still being made.
    stale = set(RECORDED_ELSEWHERE) - {(str(p), o) for p, _l, _s, _a, o, _w in exempt}
    for path, owner in sorted(stale):
        print(f"\nSTALE EXEMPTION: {path} {owner} no longer matches a silent arm")
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main())
