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

# ARCHITECTURE.md 5.3 quotes this. It read "one site" for as long as it took Phase A to
# convert the other eleven, which is the drift this row exists to make visible.
decisions=$(grep -rn "Decision::ambiguity\|Decision::repaired\|Decision::violation" \
    crates/fepdf-model/src crates/fepdf-syntax/src --include="*.rs" 2>/dev/null \
    | grep -vc "interpretation.rs" | tr -d ' ')
row "Decision sites in the engine" "$decisions"

# ARCHITECTURE.md 4 justified Rule A with "9 references and 2". Cargo has since made
# both impossible (5.7), so anything but 0 means a frontend gained a direct dependency.
leak=$(grep -rn "PdfArena\|Handle<" \
    crates/fepdf-cli/src crates/fepdf-gui/src crates/fepdf-mcp/src crates/fepdf-wasm/src \
    --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "Rule A leaks: arena types in frontends (expect 0)" "$leak"

# ROADMAP.md quotes "10 of ~30 catalogue entries typed". Both halves are derived:
# the numerator from `PdfCatalog`'s `#[pdf_key]` attributes, the denominator from the
# Table 29 list `catalog.rs` reports against.
typed=$(sed -n '/^pub struct PdfCatalog/,/^}/p' crates/fepdf-model/src/document.rs \
    | grep -c '#\[pdf_key(' | tr -d ' ')
table29=$(sed -n '/^const TABLE_29/,/^\];/p' crates/fepdf-model/src/catalog.rs \
    | grep -c '^    ("' | tr -d ' ')
row "catalogue entries typed (of Table 29)" "$typed of $table29"

# ROADMAP.md Phase B counts these. `help` is clap's own and is not one of them.
inspect_cmds=$(sed -n '/^enum InspectSubcommands/,/^}/p' crates/fepdf-cli/src/main.rs \
    | grep -c '^    [A-Z][A-Za-z]* {' | tr -d ' ')
row "inspect subcommands" "$inspect_cmds"

# An option that no code reads is a claim the CLI makes and the engine does not keep
# (ADR-0007). Counts IngestionOptions fields with no reader outside the plumbing.
# Field *access* only: `options.foo`. A declaration, a struct literal, a `.field("foo")`
# in a Debug impl and a doc comment all mention the name without reading it, and the
# first version of this check counted them — reporting a field that is read and missing
# one that is not. Asserts are excluded too: `assert!(opts.sublime_metadata)` observes
# that the flag arrived, which is what the passing test in fepdf-cli asserts about a
# field the engine never consults.
inert=""
for field in $(sed -n '/^pub struct IngestionOptions/,/^}/p' crates/fepdf-model/src/ingest/mod.rs \
        | grep -oE '^    pub [a-z_]+' | awk '{print $2}'); do
    uses=$(grep -rhn "\.$field\b" crates/*/src --include="*.rs" 2>/dev/null \
        | grep -vE '^\s*[0-9]+:\s*//' | grep -v '\.field("' | grep -vc 'assert' | tr -d ' ')
    [ "$uses" -eq 0 ] && inert="$inert $field"
done
row "ingestion options nothing reads (expect 2)" "${inert:-none} "

# The one encrypted sample, which is the only thing exercising clause 7.6. It read as
# "1,140 pages, no errors" for as long as its content decrypted to noise (ADR-0009), so
# the check has to be that text comes out, not that the file opens.
if [ -f samples/unicode_16.pdf ]; then
    chars=$(cargo run -q --release -p fepdf-cli -- inspect text samples/unicode_16.pdf \
        --pages 3 2>/dev/null | tail -n +3 | wc -c | tr -d ' ')
    if [ "${chars:-0}" -gt 5000 ]; then
        row "encrypted sample decrypts (chars on p3)" "$chars"
    else
        row "encrypted sample decrypts (chars on p3)" "$chars — FAILED, expected >5000"
    fi
fi

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
