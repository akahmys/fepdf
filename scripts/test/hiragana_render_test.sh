#!/bin/bash
set -e

# Hiragana Rendering Regression Test
# This script verifies that Hiragana characters are correctly rendered in samples/bokutokitan.pdf.

INPUT="samples/bokutokitan.pdf"
OUTPUT_DIR="out/artifacts/test_results"
mkdir -p "$OUTPUT_DIR"

echo "Running Hiragana rendering test..."

# 1. Render page 3 (contains lots of Hiragana)
PAGE3_IMG="$OUTPUT_DIR/bokutokitan_p3.png"
cargo run --bin fepdf -- produce render "$INPUT" "$PAGE3_IMG" --page 3

# 2. Verify file size (blank pages are usually < 30KB)
SIZE=$(ls -l "$PAGE3_IMG" | awk '{print $5}')
echo "Rendered image size: $SIZE bytes"

if [ "$SIZE" -lt 50000 ]; then
    echo "ERROR: Rendered image size is too small ($SIZE bytes). Hiragana might be missing."
    exit 1
fi

# 3. Test --force-fallback flag
FALLBACK_IMG="$OUTPUT_DIR/bokutokitan_p3_fallback.png"
cargo run --bin fepdf -- produce render "$INPUT" "$FALLBACK_IMG" --page 3 --force-fallback

SIZE_FB=$(ls -l "$FALLBACK_IMG" | awk '{print $5}')
echo "Fallback image size: $SIZE_FB bytes"

if [ "$SIZE_FB" -lt 50000 ]; then
    echo "ERROR: Fallback image size is too small ($SIZE_FB bytes)."
    exit 1
fi

echo "SUCCESS: Hiragana rendering test passed."
