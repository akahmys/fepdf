#!/usr/bin/env python3
"""Break one place in the code, run a test, and put the code back byte for byte.

    scripts/test/mutate_once.py FILE OLD NEW -- <cargo test arguments>

Prints one verdict, with FILE's OLD replaced by NEW:

  FIRES       a test reported FAILED; each one is named with the first line of its panic
  SURVIVES    the tests passed anyway — a test that does not guard what it is about
  NO-COMPILE  the edit does not build
  ERROR       cargo exited non-zero and no test reported FAILED — a lock, a link
              failure, a crashed test binary — with the tail of its output

OLD must occur exactly once in FILE. **A non-zero exit is not a test failing.** Until
2026-10-03 any non-zero exit printed FIRES, and in Y-F22 a mutation of
`main_xref_subsections` read FIRES while the same edit made by hand passed. FIRES now
needs a test named in cargo's output as failed; read the names, because a test in the
filter that fails for its own reason is still named there.

**The file is restored from a copy, not from git** (AGENTS.md, and the note that
`git checkout` of a file under test has wiped uncommitted work), and checked to be the
bytes it was before. This is how Y-2 showed each test file's central assertion fails with
the behaviour it guards broken (TESTING.md: prove a check fires by breaking the thing it
checks).
"""
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# libtest without -q: `test a::b ... FAILED`; with -q: `a::b --- FAILED`.
FAILED_LINE = re.compile(r"^(?:test (\S+) \.\.\.|(\S+) ---) FAILED$")
# libtest with or without -q: the names listed under a `failures:` heading.
FAILURES_HEADING = re.compile(r"^failures:$")
LISTED = re.compile(r"^    (\S+)$")
# `thread 'a::b' (tid) panicked at file:line:col:` then the message on the next line;
# older toolchains print no `(tid)`.
PANICKED = re.compile(r"^thread '([^']+)'(?: \(\d+\))? panicked at (.*)$")


def failed_tests(text: str) -> list[str]:
    """The tests cargo's output names as failed, in order, each once."""
    lines = text.splitlines()
    names = [m.group(1) or m.group(2) for line in lines if (m := FAILED_LINE.match(line))]
    for i, line in enumerate(lines):
        if not FAILURES_HEADING.match(line):
            continue
        for listed in lines[i + 1:]:
            if not listed.strip():
                if names:
                    break
                continue
            m = LISTED.match(listed)
            if not m:
                break
            names.append(m.group(1))
    return list(dict.fromkeys(names))


def first_panic_line(text: str, test: str) -> str:
    """The first line of `test`'s panic message, or what was found instead."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        m = PANICKED.match(line)
        if not m or m.group(1) != test:
            continue
        where = m.group(2)
        # Since Rust 1.73 the message follows on its own line.
        if where.endswith(":") and i + 1 < len(lines) and lines[i + 1].strip():
            return f"{lines[i + 1].strip()}  ({where.rstrip(':')})"
        return where
    return "(no panic message found)"


def verdict(returncode: int, text: str) -> str:
    if "error[" in text or "could not compile" in text:
        errors = [line for line in text.splitlines() if line.startswith("error")][:2]
        return f"NO-COMPILE: {errors}"
    failed = failed_tests(text)
    if failed:
        return "\n".join(["FIRES"] + [f"  {name}: {first_panic_line(text, name)}"
                                      for name in failed])
    if returncode != 0:
        tail = "\n".join("  " + line for line in text.splitlines()[-20:])
        return f"ERROR: cargo exited {returncode} and no test reported FAILED\n{tail}"
    return "SURVIVES"


def main() -> int:
    if "--" not in sys.argv or len(sys.argv) < 5:
        print(__doc__)
        return 2
    path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
    cargo_args = sys.argv[sys.argv.index("--") + 1:]
    source = open(path, encoding="utf-8").read()
    if source.count(old) != 1:
        print(f"BAD-MUTATION: {source.count(old)} matches in {path}")
        return 2
    keep = path + ".mutate-keep"
    shutil.copyfile(path, keep)
    try:
        with open(path, "w", encoding="utf-8") as out:
            out.write(source.replace(old, new))
        run = subprocess.run(["cargo", "test", "-q", *cargo_args],
                             capture_output=True, text=True, cwd=ROOT, check=False)
        print(verdict(run.returncode, run.stdout + run.stderr))
    finally:
        shutil.copyfile(keep, path)
        os.remove(keep)
        restored = open(path, encoding="utf-8").read()
        assert restored == source, f"{path} was not restored"
    return 0


if __name__ == "__main__":
    sys.exit(main())
