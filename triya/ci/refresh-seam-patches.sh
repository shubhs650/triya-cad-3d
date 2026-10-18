#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Triya CAD 3D — regenerate triya/patches/0001-seams.patch (every whitelisted upstream file but
# Cargo.lock) and 0002-cargo-lock.patch from `git diff <baseline>`, or with --check fail when the
# committed patches differ from what the tree would produce.
#
# Usage: triya/ci/refresh-seam-patches.sh [--check] [<baseline-ref>]     default baseline v0.14.0
set -euo pipefail

CHECK=0
if [ "${1:-}" = "--check" ]; then CHECK=1; shift; fi
BASE="${1:-v0.14.0}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# Same row format as check-patch-whitelist.sh: the first backticked cell of each numbered row.
mapfile -t FILES < <(grep -E '^\| *[0-9]+ *\|' triya/patches/PATCHES.md | sed -nE 's/^\| *[0-9]+ *\| *`([^`]+)`.*/\1/p' | grep -vx 'Cargo.lock' | sort -u)

# --full-index: abbreviated blob ids grow with the object count, so they differ between clones.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
git -c core.quotePath=false -c diff.noprefix=false diff --no-color --no-ext-diff --full-index "$BASE" -- "${FILES[@]}" > "$tmp/0001-seams.patch"
git -c diff.noprefix=false diff --no-color --no-ext-diff --full-index "$BASE" -- Cargo.lock > "$tmp/0002-cargo-lock.patch"

status=0
for p in 0001-seams.patch 0002-cargo-lock.patch; do
  if [ "$CHECK" -eq 1 ]; then
    if ! cmp -s "$tmp/$p" "triya/patches/$p"; then
      echo "::error file=triya/patches/$p::stale; run bash triya/ci/refresh-seam-patches.sh $BASE"
      status=1
    fi
  else
    cp "$tmp/$p" "triya/patches/$p"
  fi
done
[ "$CHECK" -eq 1 ] && [ "$status" -eq 0 ] && echo "Seam patches are current against $BASE (${#FILES[@]} files + Cargo.lock)."
exit "$status"
