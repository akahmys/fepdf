#!/usr/bin/env bash
# Tags a release, and only a release whose commit GitHub Actions has passed (TESTING.md §5,
# V6).
#
# The local gate runs with `--include-ignored`, on a machine that holds `samples/`. CI runs
# without either, so a test that reads a sample and is not marked as needing one passes here
# and fails there. Three did, from 319ab25 until 2026-10-14, and nothing local could see it.
# So a tag names a commit only once every workflow a push to `main` runs has succeeded on it.
#
# Usage: ./scripts/dev/release.sh [--dry-run]
#   Reads the version from Cargo.toml, checks, tags `v<version>`, pushes the tag, and waits
#   for the release workflow it starts.
set -euo pipefail
cd "$(dirname "$0")/../.."

dry_run=0
[ "${1:-}" = "--dry-run" ] && dry_run=1

fail() { echo "release: $*" >&2; exit 1; }

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
[ -n "$version" ] || fail "no version in [workspace.package]"
tag="v$version"

# The tree is what was tested, and it is what origin holds.
[ -z "$(git status --porcelain --untracked-files=no)" ] || fail "the working tree has changes"
git fetch -q origin main
sha=$(git rev-parse HEAD)
[ "$sha" = "$(git rev-parse origin/main)" ] || fail "HEAD is not origin/main; push it first"

# V3 and V4: the release table names this version, and no tag does yet.
drift=$(./scripts/dev/status.sh 2>/dev/null | sed $'s/\x1b\\[[0-9;]*m//g' \
    | awk '/release table behind Cargo.toml/{print $NF}')
[ "$drift" = "0" ] || fail "ROADMAP.md's release table does not end at $version"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null \
    || git ls-remote --exit-code --tags origin "$tag" >/dev/null; then
    fail "$tag exists"
fi

# V6: every workflow a push to main runs has completed, successfully, on this commit.
# Derived from the workflows rather than listed, so a new one is held too.
required=$(for f in .github/workflows/*.yml; do
    if sed -n '/^on:/,/^[a-z]/p' "$f" | grep -q 'push:' \
        && sed -n '/^on:/,/^[a-z]/p' "$f" | grep -qE 'branches: \[ *"?main"? *\]'; then
        sed -n 's/^name: *//p' "$f" | head -1
    fi
done)
[ -n "$required" ] || fail "found no workflow that runs on a push to main"

runs=$(gh run list --commit "$sha" --branch main --event push --limit 50 \
    --json workflowName,status,conclusion \
    --jq '.[] | "\(.workflowName)\t\(.status)\t\(.conclusion)"')
missing=0
while IFS= read -r name; do
    line=$(printf '%s\n' "$runs" | awk -F'\t' -v n="$name" '$1 == n' | head -1)
    if [ -z "$line" ]; then
        echo "  $name: no run on $sha"; missing=1
    elif [ "$(printf '%s' "$line" | cut -f2,3)" != "$(printf 'completed\tsuccess')" ]; then
        echo "  $name: $(printf '%s' "$line" | cut -f2,3 | tr '\t' ' ')"; missing=1
    else
        echo "  $name: success"
    fi
done <<<"$required"
[ "$missing" = 0 ] || fail "GitHub Actions has not passed on $sha"

if [ "$dry_run" = 1 ]; then
    echo "release: $tag would be tagged on $sha"
    exit 0
fi

git tag "$tag" "$sha"
git push -q origin "$tag"
echo "release: pushed $tag; waiting for the release workflow"

run_id=""
for _ in $(seq 1 30); do
    run_id=$(gh run list --branch "$tag" --event push --limit 1 --json databaseId \
        --jq '.[0].databaseId // empty')
    [ -n "$run_id" ] && break
    sleep 10
done
[ -n "$run_id" ] || fail "no workflow run started for $tag"
gh run watch "$run_id" --exit-status >/dev/null || fail "the release workflow failed: run $run_id"
gh release view "$tag" --json tagName,assets --jq '"\(.tagName): \(.assets | length) assets"'
