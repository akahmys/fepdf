#!/usr/bin/env python3
"""`let _ = <call>` where the call can fail (Rule 13's other half).

Rule 13 forbids swallowing an error in silence, and its check looks for
`filter_map(Result::ok)`. The same swallow written as a binding — `let _ = f();` where
`f` returns a `Result` — is invisible to it. `HeuristicEngine::infer_structure` carried
one for as long as it existed: a page whose content stream would not run contributed no
structural candidates and was reported identically to a page read perfectly (ADR-0073).

**Why not `clippy::let_underscore_must_use`.** It was tried. It has the type information
this script does not, and it reports 97 sites, of which 86 are a `write!` into a `String`
(infallible — `fmt::Write` for `String` never errors) or an `mpsc` `send` whose receiver
has hung up (the GUI is closing). A check whose output is 89% noise is a check nobody
reads, so the lint stays off and this names the benign shapes instead.

Each remaining site must either handle the error or appear in `ACCOUNTED_FOR` with the
reason. An entry pointing at a line that no longer discards anything fails, so the list
cannot rot into a permanent exemption.
"""

import re
import sys
from pathlib import Path

# The same ground the audit calls production: `crates/*/src`, not tests or examples.
TARGETS = sorted(str(p) for p in Path("crates").glob("*/src"))

# Discards that cannot lose an error, by the shape of the call.
#
# `write!`/`writeln!` into a String: `impl fmt::Write for String` is infallible, and the
# `Result` exists only because the trait is shared with `io::Write`.
# `.send(`: an `mpsc` send fails only when the receiver has been dropped, which in this
# codebase means the GUI thread has gone; there is nobody left to tell.
BENIGN = re.compile(r"\bwrite!|\bwriteln!|\.send\(|\.try_send\(")

# Sites that discard a real `Result` for a stated reason. Keyed by file and the text of
# the line, so moving the line does not silently re-approve a different one.
ACCOUNTED_FOR: dict[tuple[str, str], str] = {
    ("crates/fepdf-gui/src/speech.rs", "child.kill();"): (
        "stopping a reading: a synthesiser that has already finished cannot be killed, and "
        "either way it is not speaking, which is what the reader asked for"
    ),
    ("crates/fepdf-gui/src/speech.rs", "child.wait();"): (
        "reaping the process just killed, so it does not linger; how it ended is not a "
        "question anyone asked"
    ),
    ("crates/fepdf-gui/src/speech.rs", "std::fs::remove_file(&into);"): (
        "a test clearing the file its fake synthesiser writes, before and after reading "
        "it; one that was never there changes no result"
    ),
    ("crates/fepdf-gui/src/speech.rs", "std::fs::remove_file(&file);"): (
        "a test deleting the speech `say` wrote, after its size was read"
    ),
    (
        "crates/fepdf/src/lib.rs",
        "self.render_page(index, &mut backend, kurbo::Affine::IDENTITY);",
    ): (
        "a page read aloud as far as it interprets: `render_page` draws up to the "
        "operator it cannot run and records that failure itself, and the words drawn "
        "before it are words the page says"
    ),
    (
        "crates/fepdf-gui/src/worker.rs",
        "std::fs::remove_file(&path);",
    ): (
        "a test deleting the temporary file it saved; the answer was read before this "
        "line, and a file left behind in the temporary directory changes no result"
    ),
    (
        "crates/fepdf-gui/src/worker.rs",
        "std::fs::remove_dir_all(&folder);",
    ): (
        "a test deleting the temporary folder it wrote images into; the listing was read "
        "before this line, and a folder left behind changes no result"
    ),
    (
        "crates/fepdf/src/lib.rs",
        "inner.execute(stream);",
    ): (
        "an annotation appearance that will not execute is one annotation and not the "
        "page; the interpreter records what stopped it as a Decision on the document, so "
        "the failure is reported even though this caller carries on"
    ),
    (
        "crates/fepdf/src/lib.rs",
        "backend.take_decisions();",
    ): "returns the drained Vec, not a Result — the drain is the point",
    (
        "crates/fepdf-model/src/document.rs",
        "self.push_down_attributes_recursive(root_h, &mut inherited, 0);",
    ): (
        "normalization is best-effort at load: a page tree this cannot walk is reported "
        "by the reader that built it, and refusing to normalize the rest would leave a "
        "readable document less readable"
    ),
    (
        "crates/fepdf-model/src/font/mod.rs",
        "resource.init_collection_map();",
    ): (
        "returns Option<Decision>, not a Result, and `load` replaces `decisions` the "
        "moment this returns — the site's own comment says so"
    ),
    (
        "crates/fepdf-model/src/font/mod.rs",
        "self.perform_reconstruction();",
    ): (
        "the effect is checked instead of the Result: the next statement reads "
        "`reconstructed_data.is_some()` and keeps the raw bytes when it is None"
    ),
    (
        "crates/fepdf-model/src/parser.rs",
        "self.lexer.next_token();",
    ): "consumes the token a successful `peek()` on the line above already produced",
    (
        "crates/fepdf-model/src/object/sublimation/parser.rs",
        "lexer.next_token();",
    ): (
        "the same consume-what-peek-showed idiom; the two at the end of an array and a "
        "dictionary consume a closing bracket that may be absent, which leaves the "
        "structure already built"
    ),
}

CALL = re.compile(r"^\s*let _ = (?P<call>.+?);?\s*$")


def strip_comments(text: str) -> str:
    """Blank out `//` comments so a discard quoted in prose is not counted."""
    out = []
    for line in text.splitlines(keepends=True):
        marker = line.find("//")
        out.append(line if marker < 0 else line[:marker] + "\n")
    return "".join(out)


def main() -> int:
    findings: list[str] = []
    seen_keys: set[tuple[str, str]] = set()

    for root in TARGETS:
        for path in sorted(Path(root).rglob("*.rs")):
            if "/target/" in str(path):
                continue
            text = strip_comments(path.read_text(errors="replace"))
            lines = text.splitlines()
            for index, line in enumerate(lines):
                number = index + 1
                match = CALL.match(line)
                if not match:
                    continue
                # A call spanning lines is joined until its parentheses balance, so the
                # `?` that propagates it is seen. Without this, every multi-line
                # `let _ = f(..).map_err(..)?;` reads as a discard.
                call = match.group("call").strip()
                depth = call.count("(") - call.count(")")
                cursor = index
                while depth > 0 and cursor + 1 < len(lines):
                    cursor += 1
                    call += " " + lines[cursor].strip()
                    depth += lines[cursor].count("(") - lines[cursor].count(")")
                # A discard with no call on the line cannot fail; `let _ = x;` on a value.
                if "(" not in call:
                    continue
                # `let _ = f()?;` has already propagated; what it drops is the unwrapped
                # value, which is a deliberate "I want the effect, not the result".
                if "?" in call:
                    continue
                if BENIGN.search(call):
                    continue
                key = (str(path), call.rstrip(";").strip() + ";")
                if key in ACCOUNTED_FOR:
                    seen_keys.add(key)
                    continue
                findings.append(f"  {path}:{number}: {call}")

    stale = set(ACCOUNTED_FOR) - seen_keys
    for path, call in sorted(stale):
        findings.append(f"  STALE: {path} no longer contains `{call}`")

    if findings:
        print("  FAIL: a Result discarded with no reason given (Rule 13):")
        print("\n".join(findings))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
