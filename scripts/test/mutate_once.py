#!/usr/bin/env python3
"""Break one place in the code, run a test, and put the code back byte for byte.

    scripts/test/mutate_once.py FILE OLD NEW -- <cargo test arguments>

Prints FIRES when the test fails with FILE's OLD replaced by NEW, SURVIVES when it
passes anyway — a test that does not guard what it is about — and NO-COMPILE when the
edit does not build. OLD must occur exactly once in FILE.

**The file is restored from a copy, not from git** (AGENTS.md, and the note that
`git checkout` of a file under test has wiped uncommitted work), and checked to be the
bytes it was before. This is how Y-2 showed each test file's central assertion fails with
the behaviour it guards broken (TESTING.md: prove a check fires by breaking the thing it
checks).
"""
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


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
        text = run.stdout + run.stderr
        if "error[" in text or "could not compile" in text:
            errors = [line for line in text.splitlines() if line.startswith("error")][:2]
            print("NO-COMPILE:", errors)
        elif run.returncode != 0:
            print("FIRES")
        else:
            print("SURVIVES")
    finally:
        shutil.copyfile(keep, path)
        os.remove(keep)
        restored = open(path, encoding="utf-8").read()
        assert restored == source, f"{path} was not restored"
    return 0


if __name__ == "__main__":
    sys.exit(main())
