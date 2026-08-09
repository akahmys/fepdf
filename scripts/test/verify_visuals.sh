#!/bin/bash
# scripts/verify_visuals.sh
# Execution script for visual regression tests

set -e

# Setup PATH (Fix for Cargo not being found in some environments)
export PATH="$HOME/.cargo/bin:$PATH"

echo "=== Visual Regression Audit Starting ==="

# Execute test targets
# Specify --update to update reference baselines
if [ "$1" == "--update" ]; then
    echo "Mode: Update Baselines"
    env UPDATE_BASELINES=1 cargo test --package fepdf-render --test visual_regression -- --nocapture
else
    echo "Mode: Verification"
    cargo test --package fepdf-render --test visual_regression -- --nocapture
fi

echo "=== Visual Regression Audit Completed Successfully ==="
