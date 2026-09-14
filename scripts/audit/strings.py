#!/usr/bin/env python3
"""UI-5: a user-facing string is a locale key, not a literal.

Thirty-nine sentences were written into the source in Japanese and twenty-three more in
English, on a window whose `LocaleManager` already held 260 keys and a test holding both
files equal. An English reader got Japanese on every control of the status bar; a Japanese
reader got English in the caliper, the structure tree and the password prompt.

Two of them did not reach the locale at all: `sidebar/document_info.rs` read
`if active_lang == "ja"` and picked a sentence, which is the mechanism `LocaleManager`
exists to replace, reimplemented beside it. A checker that fired only on CJK would have
passed both of those and every English sentence.

There are two checks here, and the second exists because the first reported zero while
fifteen strings were being shown to a Japanese reader in English.

**One: the sinks.** The calls that put a string in front of a reader, asked what they
were handed. This is what catches a literal in a language the checker cannot recognise as
prose — `ui.label("読み込み中")` is a sentence to a reader and a blob of codepoints here.

**Two: the prose.** Any literal anywhere in the crate that reads like a sentence. The
sink check reads the *first argument* of a call, and none of the fifteen were there: the
accessibility sub-tabs and the About window's credits were in arrays, the GPU failure
message was a second argument on its own line, the caliper's snap labels were struct
fields, and the structure tree's placeholder title was built in the worker thread. A
check is only as wide as the enumeration behind it, and enumerating call shapes was the
wrong enumeration — so this one enumerates the exceptions instead, which is a list that
can be read and argued with.

Exits non-zero with a line per literal. No arguments.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"
TESTS = ROOT / "crates/fepdf-gui/tests"
LOCALES = ROOT / "crates/fepdf-gui/assets/locales"

# A locale key, as it is written wherever one is named.
KEY = re.compile(r'"([a-z][a-z0-9_]{2,})"')

# The calls and the fields that take a locale key.
#
# **Named, rather than guessed from the shape of the string.** `side_drawer_scroll` and
# `properties_grid` are egui widget ids and look exactly like keys; a rule that read the
# name instead of who was handed it reported twenty-five of those and two real ones.
KEY_SINKS = {"tr", "Notice::done", "Notice::check", "Notice::failed", "fail"}
KEY_FIELD = re.compile(r'\b(?:key|description)\s*:\s*"([a-z][a-z0-9_]{2,})"')

# The calls that put a string in front of a reader.
#
# **The constructors as well as the methods.** This listed `ui.label(` and not
# `Label::new(`, so a drag handle reading `Drag` sat in the structure tree through two
# passes over the localisation — the same shape as UI-9 reading `Color32` and not
# `peniko::Color`, and as the `grep -v tests` that filtered by path.
SINKS = {
    "on_hover_text",
    "hint_text",
    "heading",
    "monospace",
    "colored_label",
    "selectable_label",
    "label",
    "button",
    "Label::new",
    "Button::new",
    "RichText::new",
    "haloed_text",
}

# Calls whose string argument is not shown to a reader, by what the call is for.
#
# A log line and an assertion message are written for whoever is reading the source; a
# pattern handed to `contains` or `split` is being matched against text, not displayed;
# a `label` on a wgpu descriptor names a resource in a graphics debugger.
NOT_SHOWN = {
    "log::warn", "log::info", "log::error", "log::debug", "log::trace",
    "assert", "assert_eq", "assert_ne", "debug_assert", "panic", "unreachable",
    "expect", "expect_err",
    "contains", "split", "split_once", "rsplit_once", "starts_with", "ends_with",
    "strip_prefix", "strip_suffix", "trim_end_matches", "trim_start_matches",
    "add_filter", "id_salt", "make_persistent_id", "from_id_salt", "new_persistent",
    "println", "eprintln", "print", "eprint", "write", "writeln",
}

# Calls whose strings are the engine's own vocabulary, which is English in every language
# on purpose.
#
# A `Decision` carries a clause number and the sentence the engine would have written for
# it; `app/mod.rs`'s `a_notice_keeps_the_engines_own_words` holds that rule. One
# translated decision sitting in a list of untranslated ones would read as a defect, and
# the clause it cites would no longer match what the standard says.
ENGINE_VOICE = {
    "fepdf::Decision::ambiguity",
    "fepdf::Decision::violation",
    "fepdf::Decision::repair",
    "Decision::ambiguity",
    "Decision::violation",
    "Decision::repair",
}

# The standard's own structure type names.
#
# Four buttons in the tag popup are labelled `H1`, `H2`, `P` and `Figure`, and the count
# below says so — a number that would drop to zero if that popup changed, which is what
# makes this an exemption rather than a belief. They are identifiers that appear in the
# file, not prose about it, and a translated `<H1>` would be a lie.
EXEMPT_SINK = {"H1", "H2", "H3", "P", "Figure", "Table"}

# Literals that read as prose and are not. Each is a name rather than a sentence: an
# identifier that means the same thing in every language, and would name something else
# if it were translated.
#
# Nothing goes in here because translating it is inconvenient. `Apache-2.0 License` names
# one licence and `Apache-2.0 ライセンス` names none.
EXEMPT_PROSE = {
    "Apache-2.0 License": "the licence's name",
    "Apache-2.0 / MIT": "the licences' names",
    "MIT / Apache-2.0": "the licences' names",
    "ISC License": "the licence's name",
    "Lucide Icons": "the project's name",
    "egui / eframe": "the crates' names",
    "fepdf wgpu device": "a wgpu label, shown in a graphics debugger",
    "Vello Target Viewport Texture": "a wgpu label, shown in a graphics debugger",
    "Vello Target Thumbnail Texture {page_index}": "a wgpu label, likewise",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc": "a path the platform chose",
    "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc": "a path the platform chose",
    "/System/Library/Fonts/Hiragino Sans GB.ttc": "a path the platform chose",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf": "a path the platform chose",
    "{:.2} pt  ({:.2} mm)": "two unit symbols, the same in every language",
    "This document": "stands in for an engine phrase, and is read in that voice",
}


# Long identifiers, exempt by what they begin with because quoting one of them exactly
# here would mean reproducing its escapes and its line continuations.
EXEMPT_PREFIX = {
    "GEOGCS[": "well-known text: a coordinate system's identity, not a sentence",
}

# Two runs of two letters or more, once the format placeholders are taken out — or any
# CJK at all, which nothing in this crate writes as an identifier.
PLACEHOLDER = re.compile(r"\{[^{}]*\}")
WORD = re.compile(r"[A-Za-z]{2,}")
CJK = re.compile(r"[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uff00-\uffef]")
CALLEE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)!?\s*$")
# A char literal, which may itself be a double quote: `head.rfind('"')` is in this crate,
# and a scanner that does not know about it reads the rest of the file as one string.
# A lifetime (`&'a str`) does not match, because the closing quote is not one character on.
CHAR_LITERAL = re.compile(r"'(?:\\.|[^'\\])'")


def prose(text: str) -> bool:
    """Whether `text` reads as something written for a person."""
    bare = PLACEHOLDER.sub(" ", text)
    if CJK.search(bare):
        return True
    return sum(1 for token in bare.split() if WORD.search(token)) >= 2


def literals(source: str) -> list[tuple[int, str, str]]:
    """Every string literal outside a comment or a test module, with its enclosing call.

    **The enclosing call and not the line.** A `log::warn!` or an `assert!` can open three
    lines above its message, and reading one line at a time cannot tell that message from
    one being shown to a reader. This walks back from each literal to the `(` that
    contains it and reads the name in front of it, which is the question actually being
    asked: who was handed this string.
    """
    code = blank_out_comments_and_tests(source)
    found: list[tuple[int, str, str]] = []
    i = 0
    while i < len(code):
        ch = code[i]
        if ch != '"':
            i += 1
            continue
        j = i + 1
        while j < len(code) and code[j] != '"':
            j += 2 if code[j] == "\\" else 1
        found.append((code.count("\n", 0, i) + 1, code[i + 1 : j], callee_before(code, i)))
        i = j + 1
    return found


def callee_before(code: str, at: int) -> str:
    """The name of the call whose parentheses contain the literal at `at`."""
    depth = 0
    i = at - 1
    while i >= 0:
        ch = code[i]
        if ch in ")]":
            depth += 1
        elif ch in "([":
            if depth == 0:
                match = CALLEE.search(code[max(0, i - 80) : i])
                return match.group(1) if match else ""
            depth -= 1
        i -= 1
    return ""


def blank_out_comments_and_tests(source: str) -> str:
    """`source` with comments and `#[cfg(test)]` modules replaced by blank lines.

    Blanked rather than removed, so the line numbers reported are the file's own.
    """
    lines = source.splitlines()
    depth = 0
    skipping = False
    kept: list[str] = []
    for line in lines:
        if not skipping and line.lstrip().startswith("#[cfg(test)]"):
            skipping, depth = True, 0
            kept.append("")
            continue
        if skipping:
            depth += line.count("{") - line.count("}")
            if depth <= 0 and "}" in line:
                skipping = False
            kept.append("")
            continue
        kept.append(strip_comment(CHAR_LITERAL.sub("'_'", line)))
    return "\n".join(kept)


def strip_comment(line: str) -> str:
    """The line up to a `//` that is not inside a string."""
    in_string = False
    escaped = False
    for i, ch in enumerate(line):
        if escaped:
            escaped = False
        elif ch == "\\" and in_string:
            escaped = True
        elif ch == '"':
            in_string = not in_string
        elif ch == "/" and not in_string and line[i : i + 2] == "//":
            return line[:i]
    return line


def keys_and_homes() -> tuple[list[str], list[str]]:
    """Keys nothing names, and names no key answers.

    **Both directions are silent at runtime.** `LocaleManager::tr` falls back to English
    and then to the key itself, so a key that no longer exists reaches the reader as
    `busy_opneing` and a key nothing names sits translated while the thing it names is
    shown in English — which is how `gpu_unavailable` and the three `acc_tab_*` came to be
    carrying Japanese nobody was being given. Sixty-eight of these were left behind by one
    removed file.
    """
    named: set[str] = set()
    asked: set[str] = set()
    for root in (GUI, TESTS):
        if not root.exists():
            continue
        for path in root.rglob("*.rs"):
            source = path.read_text()
            named.update(KEY.findall(source))
            asked.update(KEY_FIELD.findall(source))
            for _, text, callee in literals(source):
                if callee in KEY_SINKS:
                    asked.add(text)

    declared: dict[str, set[str]] = {}
    for path in sorted(LOCALES.glob("*.json")):
        declared[path.stem] = set(json.loads(path.read_text()))

    english = declared.get("en", set())
    # **Every literal for the first direction, only the asked-for ones for the second.**
    # A key with no home is a key no literal anywhere spells, which is a whole-file
    # question; a name with no key is a lookup that will fail, which is a question about
    # the call that makes it. `locale.rs` covers the keys an enum answers with, by asking
    # those functions rather than by reading them.
    homeless = sorted(key for key in english if key not in named)
    missing = sorted(name for name in asked if name not in english)
    return homeless, missing


def main() -> int:
    failures: list[str] = []
    exempted = 0
    tag_names = 0
    checked = 0

    for path in sorted(GUI.rglob("*.rs")):
        rel = path.relative_to(ROOT)
        for line_no, text, callee in literals(path.read_text()):
            if callee in SINKS:
                if text in EXEMPT_SINK:
                    tag_names += 1
                    continue
                failures.append(f'{rel}:{line_no}: handed to `{callee}`: "{text[:60]}"')
                continue
            if callee in NOT_SHOWN or callee in ENGINE_VOICE or not prose(text):
                continue
            checked += 1
            if text in EXEMPT_PROSE or any(text.startswith(p) for p in EXEMPT_PREFIX):
                exempted += 1
                continue
            failures.append(f'{rel}:{line_no}: prose: "{text[:60]}"')

    homeless, missing = keys_and_homes()
    for key in homeless:
        failures.append(f"locales: `{key}` is translated and nothing names it")
    for key in missing:
        failures.append(f"source: `{key}` is named and no locale declares it")

    for line in failures:
        print(line)
    print(
        f"UI-5: {len(failures)} user-facing literals, "
        f"{checked} prose-shaped literals read, {exempted} exempt by name, "
        f"{tag_names} structure names at a sink, "
        f"{len(homeless)} keys with no home, {len(missing)} names with no key"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
