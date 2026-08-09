#!/bin/bash
# Ferruginous RR-15 & ISO 32000-2 Compliance Auditor v6.0
# Enforces rules defined in CODING.md and AUDITING.md
set -e

ERROR=0
TARGET_DIRS="crates/ferruginous-core crates/ferruginous-render crates/ferruginous-sdk crates/ferruginous-mcp crates/ferruginous-wasm crates/ferruginous crates/fepdf"

# Ensure cargo is available
if ! command -v cargo &> /dev/null; then
    if [ -x "$HOME/.cargo/bin/cargo" ]; then
        PATH="$HOME/.cargo/bin:$PATH"
    else
        echo "Error: cargo command not found"
        exit 1
    fi
fi

echo "=== Ferruginous Compliance Audit Starting ==="
echo "Rules: CODING.md (RR-15) & AUDITING.md (cargo-deny / betterleaks)"

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
            
            if ($0 ~ /\/\/ RR-15 Limit: Dispatcher/) {
                limit = 500;
            } else if ($0 ~ /\/\/ RR-15 Limit: GUI/) {
                limit = 200;
            } else {
                limit = 50;
            }
        }
    } 
    in_fn {
        if ($0 !~ /^[[:space:]]*$/ && $0 !~ /^[[:space:]]*\/\// && $0 !~ /^[[:space:]]*\/\*/) {
            effective_lines++;
        }
    }
    in_fn && /^[[:space:]]*\}/ { 
        match($0, /^[[:space:]]*/);
        if (RLENGTH == fn_indent) {
            if (effective_lines > limit) { 
                print "  FAIL: " FILENAME ":" fn_start " (" effective_lines " effective lines, limit was " limit ") " fn_name;
                exit 1;
            } 
            in_fn=0;
        }
    } 
    /^mod tests/ { in_test=1; }
    ' "$file" || ERROR=1
done < <(find $TARGET_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")

# Rule 2: Panic Exclusion
echo "[Rule 2] Checking for unwrap/expect in production code..."
while read -r file; do
    while read -r line; do
        lnum=$(echo "$line" | cut -d: -f1)
        is_test=$(sed -n "1,${lnum}p" "$file" | grep -c "mod tests" || true)
        if [ "$is_test" -eq 0 ]; then
            echo "  FAIL: $file:$line"
            ERROR=1
        fi
    done < <(grep -nE "\.(unwrap|expect)\(" "$file" | grep -vE "unwrap_(or|err)\(" | grep -v "// RR-15 Safe")
done < <(find $TARGET_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")

# Rule 3: No Unsafe
echo "[Rule 3] Checking for unsafe blocks..."
grep -rn "unsafe {" $TARGET_DIRS --include="*.rs" && { echo "  FAIL: Unsafe block found"; ERROR=1; } || echo "  PASS"

# Rule 5: No Wildcard in Match
echo "[Rule 5] Checking for wildcards in match..."
grep -rnE "match .* \{" $TARGET_DIRS --include="*.rs" -A 10 | grep "=> _" && { echo "  FAIL: Wildcard pattern found"; ERROR=1; } || echo "  PASS"

# Rule 7: No Global Mutable State
echo "[Rule 7] Checking for static mut..."
grep -rn "static mut" $TARGET_DIRS --include="*.rs" && { echo "  FAIL: Global mutable state found"; ERROR=1; } || echo "  PASS"

# Rule 10: Determinism (No HashMap/HashSet in Core/Render)
echo "[Rule 10] Checking for non-deterministic collections..."
while read -r file; do
    while read -r line; do
        lnum=$(echo "$line" | cut -d: -f1)
        is_test=$(sed -n "1,${lnum}p" "$file" | grep -c "mod tests" || true)
        if [ "$is_test" -eq 0 ]; then
            echo "  FAIL: $file:$line"
            ERROR=1
        fi
    done < <(grep -nE "HashMap|HashSet" "$file")
done < <(find crates/ferruginous-core crates/ferruginous-render -name "*.rs" | grep -vE "(tests|examples|src/bin)")

# Rule 11: Explicit Error Transparency
echo "[Rule 11] Checking for String/anyhow errors in Result..."
while read -r file; do
    if [[ $file == *"crates/ferruginous-mcp"* || $file == *"crates/fepdf"* || $file == *"crates/ferruginous/src/main.rs"* ]]; then continue; fi
    while read -r line; do
        lnum=$(echo "$line" | cut -d: -f1)
        is_test=$(sed -n "1,${lnum}p" "$file" | grep -c "mod tests" || true)
        if [ "$is_test" -eq 0 ]; then
            echo "  FAIL: $file:$line"
            ERROR=1
        fi
    done < <(grep -nE "\\bResult<[^,<>]+(<[^<>]+(,[^<>]+)*>)*[^,<>]* *, *String *>|anyhow!" "$file")
done < <(find $TARGET_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")

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
echo "[Rule 17] Running clippy audit..."
cargo clippy --workspace -- -D warnings || ERROR=1

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
