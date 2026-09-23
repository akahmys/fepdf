#!/bin/bash
# The stated minimum, built with.
#
# **`msrv_check.sh` compares documents; this one compiles.** That script reads
# `Cargo.toml`, `README.md` and `.rust-toolchain.toml` and holds them to each other, and
# three documents agreeing is not a compiler. It also reads the *running* rustc and
# refuses one below the minimum — which catches a build that is too old and says nothing
# about whether the minimum itself is reachable.
#
# It was not. Measured 2026-09-23, `cargo +1.94 check --workspace --all-targets` failed:
# `recursion_bounds_test.rs` passed `&[&String, &str, &str]` to a `&[B: AsRef<[u8]>]`, and
# 1.94 infers `&String` from the first element where 1.97 picks `&str` and coerces. One
# line in one test, and the stated minimum had never compiled this workspace.
#
# 73 s warm, against 33 minutes the first time a toolchain is installed and every
# dependency is built for it. The warm figure is what this costs in the gate.
set -u

CARGO_TOML="Cargo.toml"
MSRV=$(grep "rust-version =" "$CARGO_TOML" | head -n 1 | cut -d '"' -f 2)
if [ -z "$MSRV" ]; then
    echo "  FAIL: $CARGO_TOML states no rust-version"
    exit 1
fi

# **A promise you have not installed is one you cannot check.** Skipping here would make
# this a check that runs on one machine and not another, which is how a rule becomes a
# comment — so a missing toolchain fails, and says what to type.
if ! rustup toolchain list 2>/dev/null | grep -q "^${MSRV}[.-]"; then
    echo "  FAIL: rustc $MSRV is the stated minimum and is not installed"
    echo "        rustup toolchain install $MSRV --component rustfmt --component clippy"
    exit 1
fi

echo "  building the workspace with rustc $MSRV..."
if cargo "+$MSRV" check --workspace --all-targets --quiet; then
    echo "  the stated minimum of $MSRV compiles this workspace"
    exit 0
fi
echo "  FAIL: the workspace does not compile with rustc $MSRV, which it promises"
exit 1
