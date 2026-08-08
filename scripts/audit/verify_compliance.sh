#!/bin/bash
# Ferruginous RR-15 Auditor v5.0
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

echo "=== RR-15 Compliance Audit Starting ==="

# Rule 1: Function Line Limit (50 lines, exceptions for verified Dispatchers/GUI up to 150/200)
echo "[Rule 1] Checking function length..."
while read -r file; do
    # Skip tests
    if [[ $file == *"test"* ]]; then continue; fi
    awk '
    # Handle both top-level and indented functions
    /^[[:space:]]*(pub )?(async )?fn / { 
        if ($0 ~ /mod tests/) { in_test=1; }
        if (!in_test) { 
            in_fn=1; 
            fn_name=$0; 
            fn_start=FNR; 
            effective_lines=0;
            # Capture indentation level
            match($0, /^[[:space:]]*/);
            fn_indent = RLENGTH;
            
            # Determine limit based on explicit safeguards
            if ($0 ~ /\/\/ RR-15 Limit: Dispatcher/) {
                limit = 200;
            } else if ($0 ~ /\/\/ RR-15 Limit: GUI/) {
                limit = 150;
            } else {
                limit = 50;
            }
        }
    } 
    in_fn {
        # Skip blank lines and comments
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
echo "[Rule 2] Checking for unwrap/expect..."
# Only check production code, exclude lines within mod tests
while read -r file; do
    while read -r line; do
        lnum=$(echo "$line" | cut -d: -f1)
        # Check if line is inside a mod tests block (simple check)
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

# Rule 10: Determinism (No HashMap/HashSet)
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
done < <(find $TARGET_DIRS -name "*.rs" | grep -vE "(tests|examples|src/bin)")

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

# Rule 15: Clone Restriction (Heuristic: >3 clones in 50 lines)
echo "[Rule 15] Checking for excessive cloning..."
while read -r file; do
    clones=$(grep -o "\.clone()" "$file" | wc -l)
    if [ "$clones" -gt 10 ]; then
        echo "  WARN: High clone density in $file ($clones clones)"
    fi
done < <(find $TARGET_DIRS -name "*.rs")

# MSRV 1.94
echo "[MSRV] Checking 1.94 compatibility..."
cargo check --quiet || ERROR=1
# Rule 17: Idiomatic Quality
echo "[Rule 17] Running clippy audit..."
cargo clippy --workspace -- -D warnings || ERROR=1

# Rule 16: License Compliance
echo "[Rule 16] Checking for license conflicts..."
cargo deny check licenses || ERROR=1

# Rule 18: Secret & PII Protection
echo "[Rule 18] Checking for secrets and PII..."
betterleaks dir . || ERROR=1

if [ $ERROR -eq 1 ]; then
    echo "=== AUDIT FAILED ==="
    exit 1
else
    echo "=== AUDIT PASSED ==="
    exit 0
fi
