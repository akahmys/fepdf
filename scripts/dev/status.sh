#!/usr/bin/env bash
# Measures the claims the documents make, rather than repeating them.
#
# AGENTS.md puts measurement above documentation in the hierarchy of truth. This
# re-derives the figures ROADMAP.md and docs/adr/ quote, so a claim that has gone stale
# shows up as a disagreement instead of being read as current.
#
#   ./scripts/dev/status.sh          state and figures
#   ./scripts/dev/status.sh --full   also runs the tests and the compliance audit
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
row() { printf '  %-42s %s\n' "$1" "$2"; }

bold "Position"
row "branch" "$(git rev-parse --abbrev-ref HEAD)"
row "HEAD" "$(git log --oneline -1)"
dirty=$(git status --porcelain | wc -l | tr -d ' ')
row "uncommitted files" "$dirty"
row "phase" "$(grep -m1 -oE '^## Phase [A-E] — .*' ROADMAP.md)"

echo
bold "Figures the documents quote"

lopdf=$(grep -rl "lopdf" crates/ --include="*.rs" --include="*.toml" 2>/dev/null | wc -l | tr -d ' ')
row "files still referencing lopdf (expect 0)" "$lopdf"

# Scoped to the engine on purpose. A frontend logging for the user is doing its job;
# the engine logging a conclusion about the document is losing it (ARCHITECTURE.md 5.3).
engine_logs=$(grep -rn "log::warn!\|log::error!" \
    crates/fepdf-model/src crates/fepdf-syntax/src --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "engine log::warn!/error! sites (expect 1)" "$engine_logs"
frontend_logs=$(grep -rn "log::warn!\|log::error!" \
    crates/fepdf-cli/src crates/fepdf-gui/src crates/fepdf-sdk/src crates/fepdf-mcp/src \
    crates/fepdf-render/src --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "frontend log sites (not a defect)" "$frontend_logs"

stubs=$(grep -c 'PdfError::NotImplemented' crates/fepdf-sdk/src/lib.rs 2>/dev/null || echo 0)
row "Operation stubs in the SDK (expect 19)" "$stubs"

adrs=$(find docs/adr -name '0*.md' | wc -l | tr -d ' ')
row "decision records" "$adrs"

echo
bold "Corpus"
samples=$(find samples -name '*.pdf' 2>/dev/null | wc -l | tr -d ' ')
row "samples/*.pdf" "$samples"
if [ -d target/malformed ]; then
    row "target/malformed/*.pdf" "$(find target/malformed -name '*.pdf' | wc -l | tr -d ' ')"
else
    row "target/malformed/*.pdf" "absent — python3 scripts/test/make_malformed.py"
fi

if [ "${1:-}" = "--full" ]; then
    echo
    bold "Verification"
    passed=$(cargo test --workspace 2>&1 | awk '/^test result/ {p += $4; f += $6} END {print p "/" p + f}')
    row "tests passed" "$passed"
    if ./scripts/audit/verify_compliance.sh >/dev/null 2>&1; then
        row "compliance audit" "PASSED"
    else
        row "compliance audit" "FAILED — run ./scripts/audit/verify_compliance.sh"
    fi
else
    echo
    echo "  (--full also runs the tests and the compliance audit)"
fi

echo
bold "Next"
sed -n '/^## Phase B/,/^\*Done when\*/p' ROADMAP.md | sed 's/^/  /'
