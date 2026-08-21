#!/usr/bin/env bash
# Fetches a corpus this project did not choose.
#
# Every "zero occurrences, so defer" judgement in `ROADMAP.md` is bounded by the nine
# files in `samples/`, and those nine were picked by this project. They exercise one
# annotation subtype out of roughly 28, no form field at all, and no destination form
# beyond `/XYZ` and `/Fit`. A corpus assembled by somebody else, to be hard, is the only
# way to find out what that selection has been hiding.
#
# Three sources:
#
#   pdf-association/pdf-differences  37 files, Apache-2.0. Targeted PDFs where real
#                                    implementations legitimately disagree — the
#                                    "does this engine read what others read" question,
#                                    one construct per file.
#   pdf-association/pdf20examples     7 files, CC BY-SA 4.0. The PDF Association's own
#                                    PDF 2.0 examples, which is the version this engine
#                                    writes.
#   veraPDF corpus                   Isartor plus six sections. Isartor is the PDF/A-1
#                                    violation suite: each file breaks one specific
#                                    clause and the filename says which, so a failure is
#                                    diagnosable rather than a mystery. These are
#                                    *deliberately* invalid PDF/A; almost all are valid
#                                    ISO 32000, so refusing to read one is a finding, not
#                                    a pass.
#
# **Why the veraPDF sections beyond Isartor.** Phase O-1 asked for a corpus containing a
# business document — an attachment, a signature with validation data, a choice field, a
# layer — because four reading decisions were declined for want of one, and a corpus that
# presents nothing cannot justify either building or declining. `pdf20examples` was the
# roadmap's candidate and does not answer it: seven files demonstrating 2.0 syntax, with
# no attachment and no form among them. The veraPDF sections do carry some of it —
# `PDF_A-3b` and `PDF_A-4f` exist *because* PDF/A-3 and A-4f are the parts that embed
# other documents — so they are fetched and then measured rather than assumed.
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

# Several sections out of one repository, cloned once. Calling `fetch_sparse` seven times
# would pay for seven tree fetches of a repository whose trees are the expensive part.
fetch_sections() {  # $1 repo url, $2 branch, then `section=destname` pairs
    local url="$1" branch="$2"
    shift 2
    local work="$DEST/.multi.git" pending=() pair section dest
    for pair in "$@"; do
        dest="${pair##*=}"
        if [ -d "$DEST/$dest" ] && [ -n "$(find "$DEST/$dest" -name '*.pdf' -print -quit 2>/dev/null)" ]; then
            echo "  $dest: already present ($(find "$DEST/$dest" -name '*.pdf' | wc -l | tr -d ' ') files)"
        else
            pending+=("$pair")
        fi
    done
    [ ${#pending[@]} -eq 0 ] && return 0

    rm -rf "$work"
    if ! git clone --quiet --depth 1 --filter=blob:none --sparse --branch "$branch" "$url" "$work" 2>/dev/null; then
        echo "  CLONE FAILED — $url"
        return 1
    fi
    local patterns=()
    for pair in "${pending[@]}"; do patterns+=("${pair%%=*}/*"); done
    ( cd "$work" && git sparse-checkout set --no-cone "${patterns[@]}" >/dev/null 2>&1 )
    for pair in "${pending[@]}"; do
        section="${pair%%=*}"; dest="${pair##*=}"
        mkdir -p "$DEST/$dest"
        # Flattened, and the leaf name kept: veraPDF nests by clause and the names already
        # carry it, so the directories only make the loops that read this longer.
        find "$work/$section" -name '*.pdf' -exec cp {} "$DEST/$dest/" \; 2>/dev/null
        echo "  $dest: $(find "$DEST/$dest" -name '*.pdf' | wc -l | tr -d ' ') files"
    done
    rm -rf "$work"
}

echo "--- fetching a corpus this project did not choose ---"
fetch_sparse https://github.com/pdf-association/pdf-differences.git main '/*' pdf-differences ''
fetch_sparse https://github.com/pdf-association/pdf20examples.git master '/*' pdf20examples ''
fetch_sections https://github.com/veraPDF/veraPDF-corpus.git staging \
    'Isartor test files=isartor' \
    'PDF_A-3b=pdfa3' \
    'PDF_A-4f=pdfa4f' \
    'PDF_A-4e=pdfa4e' \
    'PDF_UA-2=pdfua2' \
    'TWG test files=twg' \
    'ISO 32000-2=iso32000-2'

total=$(find "$DEST" -name '*.pdf' 2>/dev/null | wc -l | tr -d ' ')
cat > "$DEST/README.md" <<EOMD
# External corpus — not this project's files, not committed

Fetched by \`scripts/test/fetch_external_corpus.sh\` on $(date -u +%Y-%m-%d).
$total PDFs.

| Directory | Source | Licence |
| :--- | :--- | :--- |
| \`pdf-differences/\` | github.com/pdf-association/pdf-differences | Apache-2.0 |
| \`pdf20examples/\` | github.com/pdf-association/pdf20examples | CC BY-SA 4.0 |
| \`isartor/\`, \`pdfa3/\`, \`pdfa4e/\`, \`pdfa4f/\`, \`pdfua2/\`, \`twg/\`, \`iso32000-2/\` | github.com/veraPDF/veraPDF-corpus | none stated at the repository root |

These exist to be measured against, not to be shipped. \`target/\` is ignored by git.

The Isartor files are **deliberately invalid PDF/A**. Nearly all are valid ISO 32000,
so this engine failing to read one is a finding; a PDF/A verdict is not what is being
measured here, because this engine does not claim to be a PDF/A validator.
EOMD

echo "  total: $total PDFs in $DEST"
