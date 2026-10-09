#!/usr/bin/env bash
# Runs each fuzz target for a while, and fails if any of them found something
# (ROADMAP Z-1). The nightly workflow runs this; so can anyone, from the repository root:
#
#   ./scripts/test/fuzz.sh [SECONDS-PER-TARGET]      (default 900)
#
# Needs a nightly toolchain and `cargo install cargo-fuzz`. `fuzz/README.md` says what
# the targets do and why the flags are what they are; this is where the flags live.
#
# A finding is a file libFuzzer writes under `target/fuzz/artifacts/<target>/` — a
# crash, a timeout, or an allocation past the limit. Only files this run wrote count, so
# a finding already fixed and left on disk does not fail every later run.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

SECONDS_PER_TARGET="${1:-900}"
# `FUZZ_TARGETS="type1_program"` runs only the ones named.
read -r -a TARGETS <<< "${FUZZ_TARGETS:-open_draw_save type1_program}"
status=0

# The open_draw_save corpus starts from every small PDF there is to hand: the external
# corpus (`fetch_external_corpus.sh`) and the malformed one (`make_malformed.py`), where
# present. A corpus already there is kept, so what earlier runs learned carries over.
seed_open_draw_save() {
    local corpus="$1"
    [ -n "$(ls -A "$corpus" 2>/dev/null)" ] && return 0
    for dir in target/external target/malformed; do
        [ -d "$dir" ] && find "$dir" -name '*.pdf' -size -65k -exec cp {} "$corpus/" \;
    done
    echo "  seeded $(ls "$corpus" | wc -l | tr -d ' ') files"
}

for target in "${TARGETS[@]}"; do
    corpus="target/fuzz/corpus/$target"
    artifacts="target/fuzz/artifacts/$target"
    mkdir -p "$corpus" "$artifacts"
    [ "$target" = open_draw_save ] && seed_open_draw_save "$corpus"
    stamp="target/fuzz/.started-$target"
    touch "$stamp"

    echo "=== $target, ${SECONDS_PER_TARGET}s"
    # -a keeps debug assertions on; -timeout is 60 s because the build is instrumented
    # (fuzz/README.md); -max_len matches the 64 KiB the seeds are cut at.
    cargo +nightly fuzz run --fuzz-dir fuzz -a -O "$target" "$corpus" -- \
        -max_total_time="$SECONDS_PER_TARGET" -timeout=60 -rss_limit_mb=2048 \
        -max_len=65536 -artifact_prefix="$artifacts/" 2>&1 | tail -n 20
    run=${PIPESTATUS[0]}

    found=$(find "$artifacts" -type f -newer "$stamp" | wc -l | tr -d ' ')
    rm -f "$stamp"
    if [ "$run" -ne 0 ] || [ "$found" -ne 0 ]; then
        echo "  FINDING: $target exited $run, $found new file(s) in $artifacts"
        status=1
    else
        echo "  clean"
    fi
done

exit "$status"
