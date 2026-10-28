#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Copyright (c) 2026 Triya CAD 3D contributors
#
# T7.3 / ADR-0006: a design Triya saves opens in UNMODIFIED upstream SolidCraft with its geometry.
# Builds Triya's CLI from this tree and upstream's `solidcraft-cli` from a clean worktree of the
# upstream base tag, regenerates a vessel with Triya, saves it as `.solidcraft`, and compares the body
# volumes both CLIs measure after opening it (upstream rebuilds every feature from the file).
#
#   bash triya/ci/upstream-roundtrip.sh [script.json] [tag]
#   (default: triya/crates/triya-vessel/examples/vessel-a-shell.json, v0.25.1)
#
# Needs the tag locally (`git tag v0.25.1 37a586d` in a shallow clone). Upstream builds in its own
# target folder (`target-upstream/`, gitignored as `target*/`): the first run compiles it all.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SCRIPT="${1:-triya/crates/triya-vessel/examples/vessel-a-shell.json}"
TAG="${2:-v0.25.1}"
cd "$ROOT"
WORK="$(mktemp -d)"
trap 'git worktree remove --force "$WORK/upstream" >/dev/null 2>&1 || true; rm -rf "$WORK"' EXIT

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cargo build -q -p triya-cli
TRIYA="$CARGO_TARGET_DIR/debug/triya-cli"

git worktree add --quiet --detach "$WORK/upstream" "$TAG" >/dev/null
if ! git -C "$WORK/upstream" diff --quiet "$TAG"; then
  echo "upstream worktree is not clean" >&2
  exit 1
fi
CARGO_TARGET_DIR="$ROOT/target-upstream" cargo build -q --manifest-path "$WORK/upstream/Cargo.toml" -p solidcraft-cli
UPSTREAM="$ROOT/target-upstream/debug/solidcraft-cli"

DESIGN="$WORK/vessel.solidcraft"
"$TRIYA" run "$SCRIPT" --save "$DESIGN" --quiet
"$TRIYA" eval "$DESIGN" --json-out "$WORK/triya.json"
"$UPSTREAM" eval "$DESIGN" --json-out "$WORK/upstream.json"

# Body names and volumes, sorted; equal within 1e-6 relative (both measure the same tessellation).
jq -n --slurpfile t "$WORK/triya.json" --slurpfile u "$WORK/upstream.json" --arg tag "$TAG" '
  def vols(d): [d.bodies[] | {name, v: .volume_mm3}] | sort_by(.name);
  (vols($t[0])) as $a | (vols($u[0])) as $b |
  ([range(0; [$a, $b] | map(length) | min)] | map(. as $i | $a[$i].name == $b[$i].name
     and (($a[$i].v - $b[$i].v) | fabs) <= 1e-6 * ([($a[$i].v | fabs), 1] | max)) | all) as $same |
  "\($a | length) bodies in Triya, \($b | length) in unmodified upstream \($tag); total volume \($t[0].total.volume_mm3) vs \($u[0].total.volume_mm3) mm3",
  ($a[] | "  \(.name): \(.v)"),
  if ($a | length) > 0 and ($a | length) == ($b | length) and $same then "same geometry" else error("the geometry differs") end
' -r
