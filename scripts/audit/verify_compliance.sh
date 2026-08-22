#!/bin/bash
# Ferruginous RR-15 & ISO 32000-2 Compliance Auditor v6.0
# Enforces rules defined in CODING.md and AUDITING.md
set -e

ERROR=0
TARGET_DIRS="crates/fepdf-syntax crates/fepdf-font crates/fepdf-model crates/fepdf-content crates/fepdf-doc crates/fepdf-render crates/fepdf crates/fepdf-mcp crates/fepdf-wasm crates/fepdf-gui crates/fepdf-cli"

# Ensure cargo is available
if ! command -v cargo &> /dev/null; then
    if [ -x "$HOME/.cargo/bin/cargo" ]; then
        PATH="$HOME/.cargo/bin:$PATH"
    else
        echo "Error: cargo command not found"
        exit 1
    fi
fi

echo "=== fepdf Compliance Audit Starting ==="
echo "Rules: CODING.md (RR-15) & AUDITING.md (cargo-deny / betterleaks)"

# Emits "<start> <end>" line ranges for every #[cfg(test)] module in $1.
#
# Rules 2, 10 and 11 exempt test code. They used to do that by checking whether
# "mod tests" appeared anywhere above the hit, which exempts the entire rest of
# the file once such a module exists -- correct only because every inline test
# module currently sits at the end of its file. Brace tracking removes that
# dependency, so a test module placed mid-file no longer blinds the audit.
cfg_test_ranges() {
    awk '
    /#\[cfg\(test\)\]/ { pending = 1 }
    pending && /^[[:space:]]*(pub )?mod [A-Za-z_][A-Za-z0-9_]*[[:space:]]*\{/ {
        start = FNR
        depth = gsub(/\{/, "{") - gsub(/\}/, "}")
        inmod = 1; pending = 0
        if (depth <= 0) { print start, FNR; inmod = 0 }
        next
    }
    inmod {
        depth += gsub(/\{/, "{") - gsub(/\}/, "}")
        if (depth <= 0) { print start, FNR; inmod = 0 }
    }
    END { if (inmod) print start, FNR }
    ' "$1"
}

# True when line $2 of file $1 sits inside a #[cfg(test)] module.
# $3 optionally carries pre-computed ranges to avoid re-scanning per hit.
is_test_line() {
    local line=$2 ranges=${3-}
    [ -n "$ranges" ] || ranges=$(cfg_test_ranges "$1")
    local start end
    while read -r start end; do
        [ -n "$start" ] || continue
        if [ "$line" -ge "$start" ] && [ "$line" -le "$end" ]; then return 0; fi
    done <<< "$ranges"
    return 1
}

# Rule 1: Function Line Limit (50 lines, exceptions for verified Dispatchers/GUI up to 200/500)
echo "[Rule 1] Checking function length..."
while read -r file; do
    if [[ $file == *"test"* ]]; then continue; fi
    awk '
    /^[[:space:]]*(pub )?(async )?fn / { 
        if ($0 ~ /mod tests/) { in_test=1; }
        if (!in_test) { 
            in_fn=1; 
            fn_name=$0; 
            fn_start=FNR; 
            effective_lines=0;
            match($0, /^[[:space:]]*/);
            fn_indent = RLENGTH;
            
            limit = 50;
        }
    }
    in_fn {
        # The RR-15 Limit marker annotates the function, not a specific line. rustfmt
        # relocates a trailing comment off the "fn" line onto the next one, so scan the
        # signature region rather than requiring the marker to sit on the "fn" line.
        if (FNR - fn_start < 15) {
            if ($0 ~ /\/\/ RR-15 Limit: Dispatcher/) {
                limit = 500;
            } else if ($0 ~ /\/\/ RR-15 Limit: GUI/ && limit < 200) {
                limit = 200;
            }
        }
        if ($0 !~ /^[[:space:]]*$/ && $0 !~ /^[[:space:]]*\/\// && $0 !~ /^[[:space:]]*\/\*/) {
            effective_lines++;
        }
    }
    in_fn && /^[[:space:]]*\}/ {
        match($0, /^[[:space:]]*/);
        if (RLENGTH == fn_indent) {
            if (effective_lines > limit) {
                print "  FAIL: " FILENAME ":" fn_start " (" effective_lines " effective lines, limit was " limit ") " fn_name;
                # Record and keep going: exiting here reported only the first
                # over-long function per file, so fixing one revealed the next.
                failed=1;
            }
            in_fn=0;
        }
    }
    /^mod tests/ { in_test=1; }
    END { exit failed }
    ' "$file" || ERROR=1
done < <(find $TARGET_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")

# Rule 2: Panic Exclusion
echo "[Rule 2] Checking for unwrap/expect in production code..."
rule2_failed=0
while read -r file; do
    ranges=$(cfg_test_ranges "$file")
    while read -r line; do
        lnum=${line%%:*}
        if ! is_test_line "$file" "$lnum" "$ranges"; then
            echo "  FAIL: $file:$line"
            ERROR=1
            rule2_failed=1
        fi
    done < <(grep -nE "\.(unwrap|expect)\(" "$file" | grep -vE "unwrap_(or|err)\(" | grep -v "// RR-15 Safe")
done < <(find $TARGET_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")
[ "$rule2_failed" -eq 0 ] && echo "  PASS"

# Rule 3: No Unsafe
echo "[Rule 3] Checking for unsafe blocks..."
grep -rn "unsafe {" $TARGET_DIRS --include="*.rs" && { echo "  FAIL: Unsafe block found"; ERROR=1; } || echo "  PASS"

# Rule 5: No wildcard match arms over domain enums.
#
# A plain grep cannot enforce this: matching on &str, u8 or usize *requires* a
# wildcard, so a textual search for "_ =>" reports hundreds of unavoidable hits.
# clippy::wildcard_enum_match_arm is type-aware and fires only on enums.
#
# Types whose variant set is fixed by ISO 32000-2 (the object and operator
# taxonomy), or owned by an external crate as #[non_exhaustive]. See CODING.md
# Rule 5 for why these are exempt.
RULE5_EXEMPT_TYPES="Object|Token|Command|IrObject|RefinedObject|Data|Fields"
# Files whose `match self` is over one of the exempt types above. `Self` is not
# exempt anywhere else, so a self-match on a new domain enum still fails.
RULE5_EXEMPT_SELF="crates/fepdf-model/src/object.rs\
|crates/fepdf-model/src/object/sublimation.rs\
|crates/fepdf-model/src/refine/mod.rs"

echo "[Rule 5] Checking wildcard match arms over domain enums..."
rule5_raw=$(cargo clippy --workspace --all-targets --quiet -- \
    -W clippy::wildcard_enum_match_arm \
    -A clippy::all -A clippy::pedantic -A clippy::nursery 2>&1 \
    | grep -E "^[[:space:]]+--> |help: try" \
    | awk '
        /^[[:space:]]*--> / { if (loc != "") print loc "\t"; loc = $0; next }
        /help: try/ { if (loc != "") { print loc "\t" $0; loc = "" } }
        END { if (loc != "") print loc "\t" }
      ' || true)

rule5_failed=0
while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    loc=$(echo "$entry" | sed -E 's/^[[:space:]]*--> ([^[:space:]]+).*/\1/')
    suggestion=$(echo "$entry" | sed -E 's/.*help: try: //')

    # Every enum named in clippy's suggested arm list, minus the exempt ones.
    residue=$(echo "$suggestion" \
        | grep -oE "\b[A-Z][A-Za-z0-9_]*::[A-Z][A-Za-z0-9_]*" \
        | sed -E 's/::.*//' | sort -u \
        | grep -vE "^($RULE5_EXEMPT_TYPES)\$" || true)

    if echo "$residue" | grep -q "^Self\$"; then
        if echo "${loc%%:*}" | grep -qE "^($RULE5_EXEMPT_SELF)\$"; then
            residue=$(echo "$residue" | grep -v "^Self\$" || true)
        fi
    fi

    residue=$(echo "$residue" | grep -v '^$' || true)
    if [ -n "$residue" ]; then
        echo "  FAIL: $loc wildcard over domain enum: $(echo "$residue" | tr '\n' ' ')"
        ERROR=1
        rule5_failed=1
    fi
done <<< "$rule5_raw"
[ "$rule5_failed" -eq 0 ] && echo "  PASS"

# Rule 7: No Global Mutable State
echo "[Rule 7] Checking for static mut..."
grep -rn "static mut" $TARGET_DIRS --include="*.rs" && { echo "  FAIL: Global mutable state found"; ERROR=1; } || echo "  PASS"

# fepdf owns the facade and writer delegation, so iteration order there reaches
# the produced PDF just as directly as it does in model/doc.
RULE10_DIRS="crates/fepdf-syntax crates/fepdf-model crates/fepdf-content crates/fepdf-doc crates/fepdf-render crates/fepdf"
echo "[Rule 10] Checking for non-deterministic collections..."
rule10_failed=0
while read -r file; do
    ranges=$(cfg_test_ranges "$file")
    while read -r line; do
        lnum=${line%%:*}
        if ! is_test_line "$file" "$lnum" "$ranges"; then
            echo "  FAIL: $file:$line"
            ERROR=1
            rule10_failed=1
        fi
    done < <(grep -nE "HashMap|HashSet" "$file")
done < <(find $RULE10_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")
[ "$rule10_failed" -eq 0 ] && echo "  PASS"

# Rule 11: Explicit Error Transparency
echo "[Rule 11] Checking for String/anyhow errors in Result..."
rule11_failed=0
while read -r file; do
    if [[ $file == *"crates/fepdf-mcp"* || $file == *"crates/fepdf-cli"* || $file == *"crates/fepdf-gui/src/main.rs"* ]]; then continue; fi
    ranges=$(cfg_test_ranges "$file")
    while read -r line; do
        lnum=${line%%:*}
        if ! is_test_line "$file" "$lnum" "$ranges"; then
            echo "  FAIL: $file:$line"
            ERROR=1
            rule11_failed=1
        fi
    done < <(grep -nE "\\bResult<[^,<>]+(<[^<>]+(,[^<>]+)*>)*[^,<>]* *, *String *>|anyhow!" "$file")
done < <(find $TARGET_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")
[ "$rule11_failed" -eq 0 ] && echo "  PASS"

# Rule 13: Zero Silent Swallowing
echo "[Rule 13] Checking for filter_map(Result::ok)..."
grep -rn "filter_map(Result::ok)" $TARGET_DIRS --include="*.rs" && { echo "  FAIL: Silent swallowing found"; ERROR=1; } || echo "  PASS"

# Rule 14: Test Code Separation (No dedicated test files inside src/)
echo "[Rule 14] Checking test code separation (no standalone test files in src/)..."
stray_tests=$(find $TARGET_DIRS -path "*/src/*" \
    \( -name "*_test*.rs" -o -name "test_*.rs" -o -name "tests.rs" -o -name "test.rs" \) || true)
if [ -n "$stray_tests" ]; then
    echo "  FAIL: Standalone test files found inside src/: $stray_tests"
    ERROR=1
else
    echo "  PASS"
fi

# Rule 15: Clone Restriction
echo "[Rule 15] Checking for excessive cloning..."
while read -r file; do
    clones=$(grep -o "\.clone()" "$file" | wc -l)
    if [ "$clones" -gt 15 ]; then
        echo "  WARN: High clone density in $file ($clones clones)"
    fi
done < <(find $TARGET_DIRS -name "*.rs")

# MSRV & Workspace Check
echo "[MSRV] Checking Rust workspace compilation..."
cargo check --quiet || ERROR=1

# Rule 17: Clippy Audit
#
# --all-targets so that tests, examples and benches are linted too. Without it
# those targets were never checked, and lint debt accumulated there unseen.
echo "[Rule 17] Running clippy audit..."
cargo clippy --workspace --all-targets -- -D warnings || ERROR=1

# Rule 9: Pure Rust
#
# The line is compiled foreign source, not FFI — `std` links libc and always will, so a
# rule against FFI would be a rule against the language. What no Rust tool audits is a
# vendored C library: clippy, the `unsafe` ban and the rest of RR-15 stop at the language
# boundary. `cc` is how a Rust build compiles C and nothing compiles C without it, so its
# absence from every member's tree is the whole check.
#
# It was written after three dependencies were removed to make it pass, two of which no
# line of code referenced (ADR-0024).
echo "[Rule 9] Checking for dependencies that compile C..."
C_BUILDERS=""
for member in $(cargo metadata --no-deps --format-version 1 2>/dev/null \
        | python3 -c "import sys,json;print(' '.join(p['name'] for p in json.load(sys.stdin)['packages']))"); do
    if cargo tree -p "$member" --prefix none 2>/dev/null | grep -q '^cc v'; then
        C_BUILDERS="$C_BUILDERS $member"
    fi
done
if [ -n "$C_BUILDERS" ]; then
    echo "  FAIL: these crates pull a dependency that compiles C:$C_BUILDERS"
    for member in $C_BUILDERS; do
        cargo tree -p "$member" -i cc 2>/dev/null | head -8 | sed 's/^/    /'
    done
    ERROR=1
else
    echo "  PASS"
fi

# Rule 19: Formatting
#
# Previously only `make fmt` checked this, and nothing forced anyone to run it,
# so diffs accumulated silently while the audit stayed green. Rule 1 reads the
# "// RR-15 Limit:" marker from a function's signature region rather than its
# fn line precisely so that formatting and the audit can both hold at once.
echo "[Rule 19] Checking formatting..."
if cargo fmt --all --check > /dev/null 2>&1; then
    echo "  PASS"
else
    fmt_files=$(cargo fmt --all --check 2>/dev/null \
        | grep -E "^Diff in" | sed -E 's|^Diff in ||; s|:[0-9]+:$||' | sort -u)
    echo "  FAIL: not formatted. Run 'cargo fmt --all'. Files:"
    echo "$fmt_files" | sed 's/^/    /'
    ERROR=1
fi

# Rule 16: License Compliance via cargo-deny
echo "[Rule 16] Checking for license compliance (cargo-deny)..."
cargo deny check licenses || ERROR=1

# Rule 18: Secret & PII Protection via betterleaks
echo "[Rule 18] Checking for secrets and PII (betterleaks)..."
betterleaks dir . || ERROR=1

if [ $ERROR -eq 1 ]; then
    echo "=== AUDIT FAILED ==="
    exit 1
else
    echo "=== AUDIT PASSED ==="
    exit 0
fi
