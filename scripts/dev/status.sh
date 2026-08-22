#!/usr/bin/env bash
# Measures the claims the documents make, rather than repeating them.
#
# AGENTS.md puts measurement above documentation in the hierarchy of truth. This
# re-derives the figures ROADMAP.md and docs/adr/ quote, so a claim that has gone stale
# shows up as a disagreement instead of being read as current.
#
#   ./scripts/dev/status.sh          state and figures
#   ./scripts/dev/status.sh --full   also runs the tests, the audit and the cross-check
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
row() { printf '  %-42s %s\n' "$1" "$2"; }

# Whether any row below stopped being about the code.
BROKEN=0

# The file that defines something, *found* rather than named.
#
# Naming it is what broke. `InspectSubcommands` moved from `main.rs` to `args.rs` when
# the CLI was split into `args.rs`, `commands/` and `formatters/`, and the row that
# hardcoded `main.rs` reported 0 subcommands against a truth of 8.
defining_file() {  # $1 pattern, $2 search root
    grep -rl "$1" "$2" --include="*.rs" 2>/dev/null | head -1
}

# The text between two anchors, left in `$ANCHORED`, or a BROKEN row.
#
# `sed -n '/a/,/b/p'` yields nothing both when the range is genuinely empty and when the
# anchor has moved, and every count below then reads 0 — a legal value for most of these
# rows. So a moved anchor does not produce a stale number, which this script exists to
# surface; it produces a *false* one, silently, in the one place the project reads its
# own figures from. Each extraction therefore states the anchor it needs, and a miss is
# reported rather than counted as zero.
#
# The result goes in a global rather than being returned on stdout, because the first
# version of this helper had the very defect it was written to remove: called as
# `$(anchored ...)`, its BROKEN row was captured into the variable instead of printed,
# and `BROKEN=1` was set in a subshell and discarded. Three rows vanished from the page
# with nothing to say they had.
ANCHORED=""
anchored() {  # $1 row label, $2 defining pattern, $3 search root, $4 sed range
    local file
    ANCHORED=""
    file=$(defining_file "$2" "$3")
    if [ -z "$file" ]; then
        row "$1" "BROKEN — nothing matching '$2' under $3"
        BROKEN=1
        return 1
    fi
    ANCHORED=$(sed -n "$4" "$file" 2>/dev/null)
    if [ -z "$ANCHORED" ]; then
        row "$1" "BROKEN — '$2' is in $file but the range $4 is empty"
        BROKEN=1
        return 1
    fi
}

bold "Position"
row "branch" "$(git rev-parse --abbrev-ref HEAD)"
row "HEAD" "$(git log --oneline -1)"
dirty=$(git status --porcelain | wc -l | tr -d ' ')
row "uncommitted files" "$dirty"
row "phase" "$(grep -m1 -oE '^## Phase [A-Z] — .*' ROADMAP.md)"

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
    crates/fepdf-cli/src crates/fepdf-gui/src crates/fepdf/src crates/fepdf-mcp/src \
    crates/fepdf-render/src --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "frontend log sites (not a defect)" "$frontend_logs"

stubs=$(grep -rho 'PdfError::NotImplemented' crates/fepdf/src crates/fepdf-doc/src \
    --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "Operation stubs in the engine (expect 0)" "$stubs"

adrs=$(find docs/adr -name '0*.md' | wc -l | tr -d ' ')
row "decision records" "$adrs"

# ARCHITECTURE.md 5.3 quotes this. It read "one site" for as long as it took Phase A to
# convert the other eleven, which is the drift this row exists to make visible.
#
# `fepdf-content` is searched too, and was not until Phase H put a decision there. The
# interpreter is as much the engine as the reader is — it decides to skip an image whose
# filter cannot be decoded — and a row that could not see it would have reported that
# phase as having changed nothing.
# The crate list has had to grow twice, and both times the row under-reported until it
# did. Phase H added `fepdf-content` — an image skipped mid-page is a decision. This adds
# `fepdf-doc` and `fepdf`, which gained sites when a form field learnt to say its scripts
# had not run and an annotation appearance learnt to say `/AS` named no state. A row that
# names the places it looks will keep missing new ones, so the list is the thing to check
# when the figure looks too round.
decisions=$(grep -rn "Decision::ambiguity\|Decision::repaired\|Decision::violation" \
    crates/fepdf-model/src crates/fepdf-syntax/src crates/fepdf-content/src \
    crates/fepdf-doc/src crates/fepdf/src --include="*.rs" 2>/dev/null \
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
catalog_row="catalogue entries typed (of Table 29)"
if anchored "$catalog_row" 'pub struct PdfCatalog' crates/fepdf-model/src \
        '/pub struct PdfCatalog/,/^}/p'; then
    catalog_src="$ANCHORED"
    if anchored "$catalog_row" 'const TABLE_29' crates/fepdf-model/src '/const TABLE_29/,/^\];/p'
    then
        typed=$(printf '%s' "$catalog_src" | grep -c '#\[pdf_key(' | tr -d ' ')
        table29=$(printf '%s' "$ANCHORED" | grep -c '^    ("' | tr -d ' ')
        row "$catalog_row" "$typed of $table29"
        # Declaring a key and modelling what it holds are different achievements, and
        # the first number alone overstates the second: it went 15 -> 32 in one session
        # while the entries whose contents the engine can read moved by one. A field
        # typed `Option<Object>` hands back what the arena already had.
        #
        # `catalog.rs::models_contents` is the authority and `inspect catalog` reports
        # per file; this mirrors its list so the headline figure carries its own caveat.
        passthrough=$(printf '%s' "$catalog_src" \
            | grep -cE 'pub [a-z_]+: (Option<)?(Object|Handle<Object>|Handle<PdfName>|Handle<Vec<Object>>|Vec<Object>)>?,' \
            | tr -d ' ')
        row "  of which model their contents" "$((typed - passthrough))"
        # Against 32 the figure flatters: twelve of those keys occur in no file of
        # either corpus, so a reader for one would be a container before its contents.
        # `catalog.rs::ABSENT_FROM_BOTH_CORPORA` is the measured list and this counts it,
        # so the two cannot drift.
        if anchored "  of which no corpus file carries" 'ABSENT_FROM_BOTH_CORPORA' \
                crates/fepdf-model/src '/ABSENT_FROM_BOTH_CORPORA/,/^\];/p'; then
            declined=$(printf '%s' "$ANCHORED" | grep -cE '^    "' | tr -d ' ')
            row "  of which no corpus file carries" "$declined — declined a reader"
            # Some modelled keys are carried by no file, and they have to come out of the
            # numerator or it counts them against a denominator they are not in.
            # `/NeedsRendering` is the exception ADR-0017 left behind; the rest are in
            # `BUILT_FOR_A_USE_CASE`, built on a reason that is not a corpus count.
            #
            # **Derived, not written down.** This was `- 1` for `/NeedsRendering` alone,
            # under a comment saying a second one would fail the test suite before it
            # reached this row. That stopped being true the moment the suite learned about
            # `BUILT_FOR_A_USE_CASE`, and the row went quietly wrong — 22 of 22, with
            # `/Type` counted as read. A number a test no longer guards has to be derived.
            use_case=0
            # The range stops at the *declaration's* `];`, not at the file's next one:
            # `/BUILT_FOR_A_USE_CASE/,/^\];/p` ran past the end of the const and counted
            # `ABSENT_FROM_BOTH_CORPORA` too, which read 13 of 22.
            if anchored "" 'BUILT_FOR_A_USE_CASE: ' crates/fepdf-model/src \
                    '/BUILT_FOR_A_USE_CASE: /,/\];/p'; then
                use_case=$(printf '%s' "$ANCHORED" | grep -cE '\("[A-Za-z]+", *"' | tr -d ' ')
            fi
            row "  modelled, of the keys a corpus carries" \
                "$((typed - passthrough - 1 - use_case)) of $((table29 - declined))"
        fi
    fi
fi

# ROADMAP.md Phase B counts these. `help` is clap's own and is not one of them.
if anchored "inspect subcommands" 'enum InspectSubcommands' crates/fepdf-cli/src \
        '/enum InspectSubcommands/,/^}/p'; then
    row "inspect subcommands" \
        "$(printf '%s' "$ANCHORED" | grep -cE '^    [A-Z][A-Za-z]* [{(]' | tr -d ' ')"
fi

# An option that no code reads is a claim the CLI makes and the engine does not keep
# (ADR-0007). Counts IngestionOptions fields with no reader outside the plumbing.
# Field *access* only: `options.foo`. A declaration, a struct literal, a `.field("foo")`
# in a Debug impl and a doc comment all mention the name without reading it, and the
# first version of this check counted them — reporting a field that is read and missing
# one that is not. Asserts are excluded too: `assert!(opts.sublime_metadata)` observes
# that the flag arrived, which is what the passing test in fepdf-cli asserts about a
# field the engine never consults.
inert=""
options_src=""
anchored "ingestion options nothing reads (expect 0)" 'pub struct IngestionOptions' \
    crates/fepdf-model/src '/pub struct IngestionOptions/,/^}/p' && options_src="$ANCHORED"
for field in $(printf '%s' "$options_src" | grep -oE '^    pub [a-z_]+' | awk '{print $2}'); do
    uses=$(grep -rhn "\.$field\b" crates/*/src --include="*.rs" 2>/dev/null \
        | grep -vE '^\s*[0-9]+:\s*//' | grep -v '\.field("' | grep -vc 'assert' | tr -d ' ')
    [ "$uses" -eq 0 ] && inert="$inert $field"
done
[ -n "$options_src" ] && row "ingestion options nothing reads (expect 0)" "${inert:-none} "

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

# The goal line, made into a figure. `ROADMAP.md` opens with "understands ISO 32000-2
# semantically", which is not a predicate; this is the nearest thing that can be
# reported (ADR-0019). Not run by default because it is a minute over `samples/` alone —
# 47 seconds of that is `intel_sdm.pdf`, surveyed three times — and this view is meant
# to be instant. `--full` runs it.
row "semantic coverage (ADR-0019)" "fepdf inspect coverage samples/*.pdf"

echo
bold "Corpus"
samples=$(find samples -name '*.pdf' 2>/dev/null | wc -l | tr -d ' ')
row "samples/*.pdf" "$samples"
if [ -d target/malformed ]; then
    row "target/malformed/*.pdf" "$(find target/malformed -name '*.pdf' | wc -l | tr -d ' ')"
else
    row "target/malformed/*.pdf" "absent — python3 scripts/test/make_malformed.py"
fi
if [ -d target/encrypted ]; then
    row "target/encrypted/*.pdf" "$(find target/encrypted -name '*.pdf' | wc -l | tr -d ' ')"
else
    row "target/encrypted/*.pdf" "absent — python3 scripts/test/make_encrypted.py"
fi
if [ ! -f target/encrypted/wrapper.pdf ]; then
    row "target/encrypted/wrapper.pdf" "absent — python3 scripts/test/make_wrapper.py"
fi

if [ "${1:-}" = "--full" ]; then
    echo
    bold "The goal, as far as it can be measured"
    # Over `samples/` alone unless the external corpus has been fetched, and the row
    # says which — a coverage figure whose corpus is unstated is not a measurement.
    coverage_files="samples/*.pdf"
    coverage_of="samples/ only"
    if [ -n "$(find target/external -name '*.pdf' -print -quit 2>/dev/null)" ]; then
        coverage_files="samples/*.pdf target/external/*/*.pdf"
        coverage_of="samples/ and the external corpus"
    fi
    # shellcheck disable=SC2086
    cargo run -q --release -p fepdf-cli -- inspect coverage $coverage_files 2>/dev/null \
        | grep -E '^  (catalogue|annotation|stream|[0-9]+ of)' \
        | while IFS= read -r line; do printf '  %s\n' "$line"; done
    row "measured over" "$coverage_of"

    echo
    bold "Verification"
    passed=$(cargo test --workspace 2>&1 | awk '/^test result/ {p += $4; f += $6} END {print p "/" p + f}')
    row "tests passed" "$passed"
    if ./scripts/audit/verify_compliance.sh >/dev/null 2>&1; then
        row "compliance audit" "PASSED"
    else
        row "compliance audit" "FAILED — run ./scripts/audit/verify_compliance.sh"
    fi
    # Release-only verification missed a regression that made seven subcommands panic
    # before parsing anything: clap's duplicate-argument check is a `debug_assert`.
    if ./scripts/test/cli_smoke.sh >/dev/null 2>&1; then
        row "CLI starts (debug build)" "every subcommand"
    else
        row "CLI starts (debug build)" "FAILED — run ./scripts/test/cli_smoke.sh"
    fi
    # Three defects have been found only by reading the output with something else
    # (ADR-0006, ADR-0009, ADR-0010). None was visible to the engine's own comparison.
    if ./scripts/test/crosscheck_roundtrip.sh >/dev/null 2>&1; then
        row "round trip vs PDFKit" "no page lost its text"
    else
        row "round trip vs PDFKit" "FAILED — run ./scripts/test/crosscheck_roundtrip.sh"
    fi
    # And three found only by reading it back with *this* engine, which the check above
    # cannot do: it measures both sides with PDFKit. Needs no second implementation, so
    # it runs here rather than being one more script somebody has to remember.
    if ./scripts/test/crosscheck_selfread.sh >/dev/null 2>&1; then
        row "reads back its own output" "every combination"
    else
        row "reads back its own output" "FAILED — run ./scripts/test/crosscheck_selfread.sh"
    fi
else
    echo
    echo "  (--full also runs the tests, the compliance audit and the PDFKit cross-check)"
fi

echo
bold "Next"
# Found, not named. This printed Phase C long after Phase C was complete, because the
# range was written when Phase C was the next thing and nothing re-derived it.
next_phase=$(awk '/^## Phase /{p=$0} /^- \[ \]/{print p; exit}' ROADMAP.md)
if [ -n "$next_phase" ]; then
    echo "  $next_phase"
    grep -A3 '^- \[ \]' ROADMAP.md | head -20 | sed 's/^/  /'
else
    echo "  Every box in ROADMAP.md is checked."
    echo
    echo "  That is not the same as the goal being met, and it is no longer the case that"
    echo "  nothing can be said about it. The goal — \"an engine that understands"
    echo "  ISO 32000-2 semantically\" — is still not a predicate; what can be reported is"
    echo "  how much of what a corpus presents this engine reads the contents of:"
    echo
    echo "      fepdf inspect coverage samples/*.pdf target/external/*/*.pdf"
    echo
    echo "  That figure is a proxy and ADR-0019 says what it is not. Phases A-L each had a"
    echo "  completion condition that could be checked; the line above them now has one"
    echo "  too, and it is a number rather than a yes."
fi

echo
if [ "$BROKEN" -ne 0 ]; then
    bold "A row above stopped being about the code"
    echo "  Every counter here returns a number, and 0 is a legal value for most of them,"
    echo "  so an anchor that moves reports a false figure rather than no figure. Fix the"
    echo "  anchor before believing anything else on this page."
    exit 1
fi
