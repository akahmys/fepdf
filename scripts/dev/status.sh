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
# The *first unfinished* phase, not the first heading. Reading the first heading printed
# "Phase A" forever, which is the same defect the "Next" block below carries a comment
# about having fixed there and not here.
phase_now=$(awk '/^## Phase /{p=$0} /^- \[ \]/{print p; exit}' ROADMAP.md)
row "phase" "${phase_now:-every box checked}"

echo
bold "Figures the documents quote"

lopdf=$(grep -rl "lopdf" crates/ --include="*.rs" --include="*.toml" 2>/dev/null | wc -l | tr -d ' ')
row "files still referencing lopdf (expect 0)" "$lopdf"

# Scoped to the engine on purpose. A frontend logging for the user is doing its job;
# the engine logging a conclusion about the document is losing it (ARCHITECTURE.md 5.3).
#
# The engine is *derived* rather than listed, for the reason the Decision row above had
# to learn twice: a row that names the places it looks keeps missing new ones. It read
# "1" for three phases while `fepdf-content` held eight sites and `fepdf-font` three,
# because those two crates were in neither list and nothing noticed the gap — a crate
# absent from both lists is invisible, which is worse than being in the wrong one.
# So: the engine is every crate that is not a frontend, and the two lists are complements
# by construction. Adding a crate to the workspace puts it in one of them.
FRONTEND_CRATES="fepdf-cli fepdf-gui fepdf-mcp fepdf-script fepdf-wasm"
# The one list that is written down, so the one that can go stale. A name here that is
# not a crate silently moves that crate into the *engine* half — the partition still
# covers everything, and covers it wrongly. Checked rather than trusted.
for crate_name in $FRONTEND_CRATES; do
    [ -d "crates/$crate_name/src" ] || {
        row "frontend list" "BROKEN — FRONTEND_CRATES names $crate_name, which is not a crate"
        BROKEN=1
    }
done
engine_dirs=""; frontend_dirs=""
for crate_dir in crates/*/; do
    name=$(basename "$crate_dir")
    [ -d "$crate_dir/src" ] || continue
    case " $FRONTEND_CRATES " in
        *" $name "*) frontend_dirs="$frontend_dirs $crate_dir/src" ;;
        *)           engine_dirs="$engine_dirs $crate_dir/src" ;;
    esac
done
# Three are deliberate and each says so where it is: which fonts *this machine* has
# (`fepdf-model`), the GPU failing to initialise so the CPU renderer takes over, and a
# system fallback font that would not load from its path. All three are properties of the
# host, which is what a log is for; a conclusion about the document is a `Decision`.
engine_logs=$(grep -rn "log::warn!\|log::error!" $engine_dirs --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "engine log::warn!/error! sites (expect 3)" "$engine_logs"
frontend_logs=$(grep -rn "log::warn!\|log::error!" $frontend_dirs --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "frontend log sites (not a defect)" "$frontend_logs"

# Rule D (ARCHITECTURE.md 5.1): every document mutation is an `Operation`, and only
# `fepdf-doc` interprets it. Section 7 called that "enforced by construction" while
# nothing enforced it — the facade exposed each mutation twice, as a variant and as a
# plain method, and eight frontend call sites used the method.
#
# The first version of this row grepped the four frontends for each facade mutator's
# name. It was wrong twice over: it missed `reorder_pages_batch`, whose signature spans
# two lines, and it counted `app.duplicate_page` in `fepdf-gui`, which is the GUI's own
# method that happens to share a name. A check that greps call sites cannot tell those
# apart.
#
# So the property moved into the type instead. The mutators are gone from the facade, and
# what is counted is the facade itself: `&mut self` methods that are not `apply` and not
# the four that configure saving rather than change the document. One file, no receivers
# to disambiguate, and a frontend cannot bypass a vocabulary that is the only way in.
# Adding a mutating method to `crates/fepdf/src/lib.rs` is what makes this fail.
facade_mutators=$(awk '
    /^    pub fn [a-z_]+/ { sig = $0; name = $0; sub(/^.*pub fn /, "", name); sub(/[(<].*$/, "", name); collecting = 1 }
    collecting && !/^    pub fn / { sig = sig " " $0 }
    collecting && /\{[[:space:]]*$/ { if (sig ~ /&mut self/) print name; collecting = 0 }
' crates/fepdf/src/lib.rs | sort -u | grep -vx "apply" \
    | grep -vE '^set_(vacuum|strip|password|system_fonts)$' | wc -l | tr -d ' ')
row "Rule D: document mutators on the facade besides apply (expect 0)" "$facade_mutators"

# Over every engine crate, not the two this named. The label said "in the engine" while
# the search said `fepdf` and `fepdf-doc`, so a stub anywhere else was invisible — the
# same shape that let the `Decision` row read 82 for a truth of 84, and the reason that
# row now derives its list too.
stubs=$(grep -rho 'PdfError::NotImplemented' $engine_dirs \
    --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
row "Operation stubs in the engine (expect 0)" "$stubs"

# A dependency declared and referenced by no line of code. Phase Q found forty-five of
# these by hand, and ADR-0024 found three more the same way four days earlier — two of
# which were the ones dragging in a C compiler. Both searches were one-offs, so the shape
# that produced them survived; this is the figure, which is what a shape needs.
#
# **The command ROADMAP.md gave for this did not work.** It named `src`, `tests`,
# `examples` and `benches` unconditionally, and `grep` exits *2* — an error, not "no
# match" — when a directory it is given does not exist. Most crates here have only `src`,
# so `|| echo unused` fired for every dependency of every crate: it reported 100% unused
# and would have done so just as loudly on a tree with nothing wrong. Only the
# directories that exist are searched now.
#
# Package name to Rust identifier is `-` to `_`, which is the whole translation. A crate
# reached only through another's re-export is *not* found here and should not be: the
# declaration is what is being asked about.
#
# **It can miss one, and the way it misses is worth knowing.** The search is textual, so a
# dependency whose name is an ordinary word passes on a coincidence: `hex` was the first
# probe used to test this row and it reported nothing, because `fepdf-cli` has a local
# variable called `hex`. A name nothing could collide with reports correctly. So a zero
# here means "nothing obviously unused", and the audit that removed forty-five was
# `cargo check --workspace --all-targets` with them deleted — which is the check this row
# points at rather than replaces.
#
# **It read two sections of six.** The awk started only on `^[dependencies]` and
# `^[dev-dependencies]`, so `[build-dependencies]` and every
# `[target.'cfg(...)'.dependencies]` block was invisible: a dependency declared there
# could be referenced by nothing at all and this row would still say 0. Verified by
# feeding it a manifest with a target section — it returned the two ordinary
# dependencies and not the target one. It starts on any header ending `dependencies]`
# now, which is the six of them, and skips `[workspace.dependencies]` because this loop
# reads member manifests rather than the root.
#
# The third blind spot is a different question and has its own row below.

# Declarations that exist to select a feature rather than to be called, as
# `crate:dependency` — Rule 9's shape, which names the cause and not the member.
#
# `fepdf-wasm:getrandom` is declared for `features = ["js"]` and nothing else. `getrandom`
# reaches that build through `rsa`'s default -> `std` -> `rand_core/std`, so the signing
# stack pulls it whether or not anything signs, and on `wasm32-unknown-unknown` it refuses
# to compile until it is told where randomness comes from. No line of `fepdf-wasm` calls
# it; the declaration exists so that Cargo's feature unification turns the flag on.
#
# **Exempting `fepdf-wasm` would forgive whatever it declares next.** Naming the pair
# forgives this one declaration, as ADR-0033 named `wayland-backend` rather than the GUI.
UNUSED_DEPS_EXEMPT="fepdf-wasm:getrandom"
unused_deps=0
for crate_dir in crates/*/; do
    dep_dirs=""
    for sub in src tests examples benches; do
        [ -d "$crate_dir$sub" ] && dep_dirs="$dep_dirs $crate_dir$sub"
    done
    [ -n "$dep_dirs" ] || continue
    crate_name=$(basename "$crate_dir")
    declared=$(awk '/^\[workspace\.dependencies\]/{f=0;next} /^\[.*dependencies\]/{f=1;next} /^\[/{f=0} f' \
        "$crate_dir/Cargo.toml" | grep -oE '^[a-zA-Z0-9_-]+' | sort -u)
    for dep in $declared; do
        case " $UNUSED_DEPS_EXEMPT " in
            *" $crate_name:$dep "*) continue ;;
        esac
        ident=$(echo "$dep" | tr '-' '_')
        grep -rqE "\b${ident}\b" $dep_dirs --include="*.rs" 2>/dev/null \
            || unused_deps=$((unused_deps + 1))
    done
done
row "dependencies nothing references (expect 0)" \
    "$unused_deps (exempt, recorded: $UNUSED_DEPS_EXEMPT)"

# A dependency in the runtime section that only a test, example or benchmark references.
#
# The row above searches `examples/` when deciding whether a `[dependencies]` entry is
# used, so a dependency in the wrong section counts as used and it says nothing. That is
# not a hypothetical: `fepdf` declared `tokio` with `features = ["full"]` in
# `[dependencies]` for the sake of three examples with an async `main`, under a note
# saying nothing in `src/` touched it. Cargo builds `[dependencies]` for every consumer
# and `[dev-dependencies]` for none, so `mio` came with it and `fepdf-wasm` stopped
# building for `wasm32-unknown-unknown` — while the host target compiled fine and this
# script read 0.
#
# The question here is sharper than the one above: not "does anything reference it" but
# "does anything that ships reference it".
misplaced_deps=0
for crate_dir in crates/*/; do
    [ -d "$crate_dir/src" ] || continue
    dev_dirs=""
    for sub in tests examples benches; do
        [ -d "$crate_dir$sub" ] && dev_dirs="$dev_dirs $crate_dir$sub"
    done
    [ -n "$dev_dirs" ] || continue
    runtime=$(awk '/^\[dependencies\]/{f=1;next} /^\[/{f=0} f' \
        "$crate_dir/Cargo.toml" | grep -oE '^[a-zA-Z0-9_-]+' | sort -u)
    for dep in $runtime; do
        ident=$(echo "$dep" | tr '-' '_')
        grep -rqE "\b${ident}\b" "$crate_dir/src" --include="*.rs" 2>/dev/null && continue
        grep -rqE "\b${ident}\b" $dev_dirs --include="*.rs" 2>/dev/null \
            && misplaced_deps=$((misplaced_deps + 1))
    done
done
row "runtime dependencies only a test or example uses (expect 0)" "$misplaced_deps"

# `fepdf-mcp` is the frontend whose whole job is to expose the `Operation` vocabulary — a
# tool is the serialised form of an operation (ARCHITECTURE 5.1). It named all of it until
# Rule D turned ten facade methods into six new operations, and it sat at 24 of 30 for a
# phase because nothing counted. Every variant is *reachable* through the generic
# `apply_operation` tool whatever this says; what a missing one lacks is a schema, so a
# caller has to already know it exists to ask for it.
if anchored "operations named as MCP tools" 'pub enum Operation' crates/fepdf-doc/src \
    '/^pub enum Operation {/,/^}/p'; then
    op_count=$(printf '%s' "$ANCHORED" | grep -cE '^    [A-Z][A-Za-z0-9]*' | tr -d ' ')
    op_named=$(grep -rhoE 'Operation::[A-Z][A-Za-z0-9]*' crates/fepdf-mcp/src --include="*.rs" \
        | sed 's/Operation:://' | sort -u | wc -l | tr -d ' ')
    row "operations named as MCP tools" "$op_named of $op_count"
fi

# JavaScript that ships with the engine and that RR-15 does not read.
#
# Adobe's AF* helpers are written in the language they are specified in, which every other
# implementation also does — and `verify_compliance.sh`'s fifteen checks all run over
# Rust. Not the function-length limit, not the error types, not determinism, not the
# `unsafe` ban. ADR-0025 made two conditions for that: each helper carries a test that
# fails when it breaks, and this figure exists so it cannot grow quietly.
#
# **Zero would be the wrong target.** The number is not a defect to drive down; it is a
# quantity of code standing outside the audit, and the question it answers is "how much"
# rather than "is there any". `docs/specs/` held twelve false claims for the want of
# exactly that kind of visible figure.
unaudited_js=$(find crates -name '*.js' -not -path '*/target/*' 2>/dev/null \
    | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
row "JavaScript lines no RR-15 check reads" "${unaudited_js:-0}"

adrs=$(find docs/adr -name '0*.md' | wc -l | tr -d ' ')
row "decision records" "$adrs"

# Output isolation (ARCHITECTURE.md §2.1). Every generated file goes under `out/`, and a
# root-level output directory is git-ignored, so nothing goes red when one appears. Four
# writers used a root `artifacts/` until 2026-08-29 for exactly that reason.
stray_out=$(grep -rnE '"(artifacts|exports|renders)/|OUT(PUT)?_DIR="(artifacts|exports)' \
    --include="*.rs" --include="*.sh" --include="*.py" crates scripts 2>/dev/null | wc -l | tr -d ' ')
row "writers outside out/ (expect 0)" "$stray_out"

# Markdown links, anchors included. A checker that skipped `#fragment` targets reported
# zero while four links pointed at sections deleted the same day — the anchors are the
# half that goes stale when a document is renumbered.
dead_links=$(python3 - <<'PYEOF' 2>/dev/null
import re, pathlib
def anchors(p):
    return {re.sub(r'[^\w\s-]','',h.lower()).strip().replace(' ','-')
            for h in re.findall(r'^#{1,6} (.+)$', p.read_text(), re.M)}
files=list(pathlib.Path(".").glob("*.md"))+sorted(pathlib.Path("docs").rglob("*.md"))
cache={f.resolve():anchors(f) for f in files}
bad=0
for f in files:
    for m in re.finditer(r'\[([^\]]*)\]\(([^)]+)\)', f.read_text()):
        t=m.group(2)
        if t.startswith(("http","file:")): continue
        path,_,anc=t.partition("#")
        dest=(f if not path else f.parent/path)
        if path and not dest.exists(): bad+=1; continue
        if anc and anc not in cache.get(dest.resolve(), set()): bad+=1
print(bad)
PYEOF
)
row "dead markdown links (expect 0)" "${dead_links:-?}"

# Rule 20's blind spot. Rule 5 stops a wildcard over a domain *enum*; a PDF's domain
# values arrive as integers, so the lint cannot see them. This counts the arms where an
# unrecognised value produces neither a Decision nor an error. Not a defect count — an
# unknown /V failing the open is loud enough — but a new one should be visible.
silent=$(python3 scripts/audit/silent_branches.py 2>/dev/null | tail -1 | grep -oE '^[0-9]+')
row "silent branches on a file's value" "${silent:-?}"

# An index maintained by hand is an index that goes quietly wrong, which is the failure
# the log exists to make visible. Every record has a row and every row has a record, or
# this says which.
adr_files=$(ls docs/adr/0*.md 2>/dev/null | xargs -n1 basename | sort)
adr_indexed=$(grep -oE '\]\(0[0-9]{3}-[a-z0-9-]+\.md\)' docs/adr/README.md | tr -d '](){}' | sort -u)
adr_missing=$(comm -23 <(echo "$adr_files") <(echo "$adr_indexed") | tr '\n' ' ')
adr_extra=$(comm -13 <(echo "$adr_files") <(echo "$adr_indexed") | tr '\n' ' ')
if [ -z "$adr_missing$adr_extra" ]; then
    row "records missing from the index (expect 0)" "0"
else
    row "records missing from the index (expect 0)" "missing: ${adr_missing:-none}  stale rows: ${adr_extra:-none}"
fi

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
#
# **Derived now, for the third time of asking.** The list above named five crates and
# `fepdf-render` was not one of them, so when the renderer learnt to report a glyph whose
# outline would not build and a font that never reached its cache, the row read 82 where
# the truth was 84 — the miss its own comment predicted, in the one crate nobody thought
# to add. It reuses `$engine_dirs`, so this row and the log row above are now complements
# of the same partition and a new crate lands in both by construction.
decisions=$(grep -rn "Decision::ambiguity\|Decision::repaired\|Decision::violation" \
    $engine_dirs --include="*.rs" 2>/dev/null \
    | grep -vc "interpretation.rs" | tr -d ' ')
row "Decision sites in the engine" "$decisions"

# Rule A, the version Cargo can enforce: a frontend declares `fepdf` and no other crate
# of this workspace. ARCHITECTURE.md 7 claimed only that no frontend declares
# `fepdf-model` — true, and narrower than the topology in 2, which puts every frontend
# above the facade and nothing else. `fepdf-gui` declared `fepdf-render` directly and did
# not enable the facade's `render` feature, so it reached the GPU crate around the opt-in
# that ADR-0004 exists to provide. It needed two names, `VelloBackend` and
# `FallbackFontType`, and the facade re-exports both.
#
# The row counts internal dependencies that are not `fepdf`, over the four frontends.
frontend_deps=0
for crate_name in $FRONTEND_CRATES; do
    extra=$(grep -oE '^fepdf[a-z-]*' "crates/$crate_name/Cargo.toml" 2>/dev/null \
        | sort -u | grep -vx "fepdf" | grep -vx "$crate_name" | wc -l | tr -d ' ')
    frontend_deps=$((frontend_deps + extra))
done
row "Rule A: frontend deps that are not the facade (expect 0)" "$frontend_deps"

# ARCHITECTURE.md 4 justified Rule A with "9 references and 2". Cargo has since made
# both impossible (5.7), so anything but 0 means a frontend gained a direct dependency.
#
# `$frontend_dirs` and not the four names again: written twice, the two lists are free to
# disagree, and the one that is wrong is the one nobody re-reads.
leak=$(grep -rn "PdfArena\|Handle<" $frontend_dirs \
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

    # The figure clause 9's row quotes. **Neither number could be re-derived from
    # anything the engine printed.** `TextExtractionBackend` records its 9.10.2 violation
    # only on pages that lost something, so summing those messages counts the glyphs on
    # lossy pages and not the ones on the rest — the denominator lived nowhere, and a
    # 16,321,270 quoted in three documents rested on a probe nobody had committed.
    loss=$(cargo run -q --release -p fepdf --example glyph_loss -- samples/*.pdf 2>/dev/null \
        | awk '/^TOTAL/ {print $3 " of " $5}')
    row "extraction loss (glyphs, samples/)" "${loss:-could not be measured}"

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
