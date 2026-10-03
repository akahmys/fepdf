#!/bin/bash
# Arlington PDF Model External Auditor Wrapper
# Usage: ./scripts/audit/arlington_audit.sh <target.pdf>
#
# Needs two things this repository does not carry, and says which is missing rather than
# failing on a bare "No such file or directory" as it did until 2026-09-06:
#
#   .arlington-venv      `make setup-arlington`
#   external/arlington   a submodule, pinned, since ROADMAP Y-F24 made the tests read it:
#                        `git submodule update --init`. It was cloned by hand before.
#                        https://github.com/pdf-association/arlington-pdf-model

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <target.pdf>"
    exit 1
fi

TARGET_PDF=$1
TSV_DIR="external/arlington/tsv/latest"
PYTHON_BIN="./.arlington-venv/bin/python3"
AUDITOR_SCRIPT="external/arlington/scripts/arlington.py"

echo "=== External Audit: Arlington PDF Model ==="
echo "Target: $TARGET_PDF"
echo "TSVs: $TSV_DIR"

if [ ! -f "$TARGET_PDF" ]; then
    echo "Error: Target PDF not found at $TARGET_PDF"
    exit 1
fi

if [ ! -x "$PYTHON_BIN" ]; then
    echo "Error: $PYTHON_BIN is not there. Run 'make setup-arlington' first." >&2
    exit 1
fi

if [ ! -f "$AUDITOR_SCRIPT" ] || [ ! -d "$TSV_DIR" ]; then
    echo "Error: the Arlington model is not in external/arlington." >&2
    echo "       It is a submodule: git submodule update --init external/arlington" >&2
    exit 1
fi

# Run the validation
$PYTHON_BIN $AUDITOR_SCRIPT --tsvdir $TSV_DIR --pdf "$TARGET_PDF" --validate

echo "=== External Audit Complete ==="
