#!/bin/bash
# MSRV Consistency Check Script

# Sources of truth
ROOT_CARGO="Cargo.toml"
TOOLCHAIN=".rust-toolchain.toml"

# Extract expected version from root Cargo.toml
EXPECTED_VERSION=$(grep "rust-version =" "$ROOT_CARGO" | head -n 1 | cut -d '"' -f 2)

if [ -z "$EXPECTED_VERSION" ]; then
    echo "Error: Could not determine expected rust-version from $ROOT_CARGO"
    exit 1
fi

echo "Checking for MSRV consistency with version: $EXPECTED_VERSION"

# 1. Check .rust-toolchain.toml
TOOLCHAIN_VERSION=$(grep "channel =" "$TOOLCHAIN" | cut -d '"' -f 2)
if [[ "$TOOLCHAIN_VERSION" != "$EXPECTED_VERSION" ]]; then
    echo "Error: $TOOLCHAIN version ($TOOLCHAIN_VERSION) does not match $ROOT_CARGO ($EXPECTED_VERSION)"
    exit 1
fi

# 2. Check README's stated minimum
#
# This hunted a hardcoded "1.85.0" until 2026-08-29 — a version retired before the check
# was written, so it passed by finding nothing and would have gone on passing after the
# next bump too. README.md is where the claim a reader acts on actually lives, and it was
# the one source this script did not read.
README_VERSION=$(grep -oE "Rust [0-9]+\.[0-9]+(\.[0-9]+)?" README.md | head -n 1 | cut -d ' ' -f 2)
if [ -z "$README_VERSION" ]; then
    echo "Error: README.md states no Rust version"
    exit 1
fi
if [[ "$README_VERSION" != "$EXPECTED_VERSION" ]]; then
    echo "Error: README.md says Rust $README_VERSION; $ROOT_CARGO says $EXPECTED_VERSION"
    exit 1
fi

# 3. Check for any version mismatch in other Cargo.toml files
CARGO_TOMLS=$(find . -name "Cargo.toml" -not -path "./target/*")
for file in $CARGO_TOMLS; do
    # Skip if it is the root Cargo.toml (already checked)
    if [[ "$file" == "./$ROOT_CARGO" ]] || [[ "$file" == "$ROOT_CARGO" ]]; then
        continue
    fi
    # Skip if it uses workspace inheritance
    if grep -q "rust-version = { workspace = true }" "$file"; then
        continue
    fi
    
    version=$(grep "rust-version =" "$file" | cut -d '"' -f 2)
    if [ -n "$version" ] && [[ "$version" != "$EXPECTED_VERSION" ]]; then
        echo "Error: $file rust-version ($version) does not match $ROOT_CARGO ($EXPECTED_VERSION)"
        exit 1
    fi
done

echo "MSRV consistency check passed!"
exit 0
