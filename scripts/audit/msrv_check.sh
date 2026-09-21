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

# 4. Check that the pin is a pin
#
# **This script compared three documents to each other and never asked what was
# building.** `.rust-toolchain.toml` says `channel = "1.94"` and rustup has never read it:
# the file rustup looks for is `rust-toolchain.toml`, without the leading dot, so the pin
# has been inert since it was added on 2026-08-29 while the three documents agreed with
# one another and this check passed.
#
# Measured 2026-09-21: `rustc --version` inside the repository and outside it both
# answered 1.97.1, against a stated minimum of 1.94.
#
# The name is not corrected here, because making the pin live would build this workspace
# with a compiler nothing has been built with (ROADMAP W-T3). What is checked is that the
# compiler doing the work is *at least* what the documents promise — a promise of 1.94
# kept by a 1.97 build is kept; one kept by a 1.93 build is not.
ACTIVE=$(rustc --version | grep -oE "[0-9]+\.[0-9]+\.[0-9]+" | head -n 1)
if [ -z "$ACTIVE" ]; then
    echo "Error: could not read the active rustc version"
    exit 1
fi
if [ -f "rust-toolchain.toml" ]; then
    echo "  note: rust-toolchain.toml is present, so the pin is live"
else
    echo "  note: no rust-toolchain.toml — the pin in $TOOLCHAIN is not one rustup reads"
fi
lowest=$(printf '%s\n%s\n' "$EXPECTED_VERSION" "$ACTIVE" | sort -V | head -n 1)
if [ "$lowest" != "$EXPECTED_VERSION" ]; then
    echo "Error: building with rustc $ACTIVE, below the stated minimum of $EXPECTED_VERSION"
    exit 1
fi
echo "  building with rustc $ACTIVE, at or above the stated minimum of $EXPECTED_VERSION"

echo "MSRV consistency check passed!"
exit 0
