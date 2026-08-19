#!/usr/bin/env bash
# Fetches a corpus this project did not choose.
#
# Every "zero occurrences, so defer" judgement in `ROADMAP.md` is bounded by the nine
# files in `samples/`, and those nine were picked by this project. They exercise one
# annotation subtype out of roughly 28, no form field at all, and no destination form
# beyond `/XYZ` and `/Fit`. A corpus assembled by somebody else, to be hard, is the only
# way to find out what that selection has been hiding.
#
# Two sources, 242 files, about 9 MB:
#
#   pdf-association/pdf-differences  37 files, Apache-2.0. Targeted PDFs where real
#                                    implementations legitimately disagree — the
#                                    "does this engine read what others read" question,
#                                    one construct per file.
#   veraPDF Isartor test files      205 files. The PDF/A-1 violation suite: each file
#                                    breaks one specific clause and the filename says
#                                    which, so a failure is diagnosable rather than a
#                                    mystery. Note these are *deliberately* invalid
#                                    PDF/A; almost all are valid ISO 32000, so refusing
#                                    to read one is a finding, not a pass.
#
# Not committed and not in `samples/`. `samples/*.pdf` is nine files and several
# measurements quote that number; adding to it would silently change them. This lands in
# `target/` beside the other generated corpora (`make_malformed.py`, `make_encrypted.py`).
#
# The veraPDF repository carries no licence file at its root and the GitHub API reports
# none, which is why these are fetched into an ignored directory and never redistributed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

DEST="target/external"
mkdir -p "$DEST"

# $3 is the sparse pattern git is given; $5 is the directory to copy from once the
# checkout exists. They differ: a pattern of `*` is not a path `find` can be pointed at,
# which is how the first version of this fetched nothing while reporting it clearly.
fetch_sparse() {  # $1 repo url, $2 branch, $3 sparse pattern, $4 dest name, $5 subdir
    local work="$DEST/.$4.git"
    if [ -d "$DEST/$4" ] && [ -n "$(find "$DEST/$4" -name '*.pdf' -print -quit 2>/dev/null)" ]; then
        echo "  $4: already present ($(find "$DEST/$4" -name '*.pdf' | wc -l | tr -d ' ') files)"
        return 0
    fi
    rm -rf "$work" "$DEST/$4"
    # A blobless sparse clone: the trees come down, the file contents only for the one
    # path asked for. Pulling the whole veraPDF corpus would be 160 MB for 7.9 MB of it.
    if ! git clone --quiet --depth 1 --filter=blob:none --sparse --branch "$2" "$1" "$work" 2>/dev/null; then
        echo "  $4: CLONE FAILED — $1"
        return 1
    fi
    ( cd "$work" && git sparse-checkout set --no-cone "$3" >/dev/null 2>&1 )
    mkdir -p "$DEST/$4"
    # Flattened: the names carry the clause, and nesting only makes the loops longer.
    find "$work/${5:-}" -name '*.pdf' -exec cp {} "$DEST/$4/" \; 2>/dev/null
    rm -rf "$work"
    local n
    n=$(find "$DEST/$4" -name '*.pdf' | wc -l | tr -d ' ')
    if [ "$n" -eq 0 ]; then
        echo "  $4: NOTHING FETCHED — '$3' checked out no PDF under ${5:-the repository root}"
        return 1
    fi
    echo "  $4: $n files"
}

echo "--- fetching a corpus this project did not choose ---"
fetch_sparse https://github.com/pdf-association/pdf-differences.git main '/*' pdf-differences ''
fetch_sparse https://github.com/veraPDF/veraPDF-corpus.git staging 'Isartor test files/*' \
    isartor 'Isartor test files' 

total=$(find "$DEST" -name '*.pdf' 2>/dev/null | wc -l | tr -d ' ')
cat > "$DEST/README.md" <<EOMD
# External corpus — not this project's files, not committed

Fetched by \`scripts/test/fetch_external_corpus.sh\` on $(date -u +%Y-%m-%d).
$total PDFs.

| Directory | Source | Licence |
| :--- | :--- | :--- |
| \`pdf-differences/\` | github.com/pdf-association/pdf-differences | Apache-2.0 |
| \`isartor/\` | github.com/veraPDF/veraPDF-corpus, \`Isartor test files\` | none stated at the repository root |

These exist to be measured against, not to be shipped. \`target/\` is ignored by git.

The Isartor files are **deliberately invalid PDF/A**. Nearly all are valid ISO 32000,
so this engine failing to read one is a finding; a PDF/A verdict is not what is being
measured here, because this engine does not claim to be a PDF/A validator.
EOMD

echo "  total: $total PDFs in $DEST"
