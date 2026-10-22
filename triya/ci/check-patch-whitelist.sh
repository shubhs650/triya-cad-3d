#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Copyright (c) 2026 Triya CAD 3D contributors
# Triya CAD 3D — fail if any upstream file differs from the baseline unless it is listed in
# triya/patches/PATCHES.md (the seam whitelist) or is one of the root files the fork owns
# outright (.gitattributes merge=ours set).
#
# Usage: triya/ci/check-patch-whitelist.sh <baseline-ref>     e.g. v0.14.0 or upstream/main
set -euo pipefail

BASE="${1:-v0.14.0}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# Paths the fork owns: never compared against upstream.
OWNED_EXCLUDES=(
  ':!triya/**' ':!triya/*'
  ':!.github/workflows/triya-*.yml' ':!.github/workflows/upstream-sync.yml'
  ':!.gitattributes' ':!NOTICE' ':!README.md' ':!REPO-SETUP.md' ':!CLOUD-SETUP.md'
)

# Row format in PATCHES.md: | # | `path` | anchor | change | reason |   (path = first backticked cell)
WHITELIST="$(grep -E '^\| *[0-9]+ *\|' triya/patches/PATCHES.md | sed -nE 's/^\| *[0-9]+ *\| *`([^`]+)`.*/\1/p' | sort -u || true)"

CHANGED="$(git diff --name-only "$BASE" -- . "${OWNED_EXCLUDES[@]}" | sort -u)"

fail=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  [ "$f" = "CLAUDE.md" ] && continue   # checked separately below
  if ! grep -qxF "$f" <<<"$WHITELIST"; then
    echo "::error file=$f::upstream file modified but not whitelisted in triya/patches/PATCHES.md"
    fail=1
    continue
  fi
  # Whitelisted files must carry the marker (Apache-2.0 §4(b) + greppable), except files whose
  # syntax has no comments (JSON catalogs) and generated files (Cargo.lock) — PATCHES.md records those.
  case "$f" in *.json|Cargo.lock) continue;; esac
  if [ -f "$f" ] && ! grep -q "Modified by Triya CAD 3D" "$f"; then
    echo "::error file=$f::whitelisted seam lacks the 'Modified by Triya CAD 3D' marker"
    fail=1
  fi
done <<<"$CHANGED"

# Root CLAUDE.md is a seam too (one import line) — make sure it is exactly that.
if ! git diff --quiet "$BASE" -- CLAUDE.md; then
  added="$(git diff "$BASE" -- CLAUDE.md | grep -E '^\+[^+]' | sed 's/^+//' | grep -v '^\s*$' || true)"
  if [ "$added" != "@triya/CLAUDE.md" ]; then
    echo "::error file=CLAUDE.md::only the single import line '@triya/CLAUDE.md' may be added to upstream CLAUDE.md"
    fail=1
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo
  echo "Seam whitelist check FAILED against $BASE. Either move the change into triya/ or add a"
  echo "row to triya/patches/PATCHES.md and a 'Modified by Triya CAD 3D: <why>' marker."
  exit 1
fi
echo "Seam whitelist check passed against $BASE ($(grep -c . <<<"$WHITELIST") whitelisted files)."
