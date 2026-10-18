#!/usr/bin/env bash
# Triya CAD 3D — Claude Code cloud environment setup script.
# Paste into: claude.ai/code → Settings → Cloud environments → triya-cad → Setup script.
# Must finish in < ~5 minutes to be cached as the environment snapshot. Do NOT build the workspace here.
set -euo pipefail
export PATH="$HOME/.cargo/bin:/usr/local/bin:$PATH"

echo "== rust toolchain"
rustup show active-toolchain || true
rustup target add wasm32-unknown-unknown
rustup component add clippy rustfmt

echo "== trunk (prebuilt; cargo install would exceed the cache window)"
TRUNK_VERSION="${TRUNK_VERSION:-v0.21.14}"
curl -sSL "https://github.com/trunk-rs/trunk/releases/download/${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" | tar -xz -C /usr/local/bin
trunk --version

echo "== chromium for cargo xtask web-offline-test (optional; the gate is skipped when absent)"
# Ubuntu 24.04's chromium package is a snap stub; use the Playwright-managed Chromium instead (Node is pre-installed).
if command -v npx >/dev/null; then
  npx --yes playwright@1.47.0 install chromium >/dev/null 2>&1 || echo "playwright chromium install skipped"
  CHROME_BIN="$(ls -d "$HOME"/.cache/ms-playwright/chromium-*/chrome-linux/chrome 2>/dev/null | head -1 || true)"
  if [ -n "$CHROME_BIN" ]; then ln -sf "$CHROME_BIN" /usr/local/bin/chromium; echo "CHROME=$CHROME_BIN"; fi
fi

echo "== software GL for headless snapshots (solidcraft-cli snapshot uses wgpu; lavapipe gives it a Vulkan device)"
apt-get update -qq && apt-get install -y -qq --no-install-recommends mesa-vulkan-drivers libvulkan1 xvfb >/dev/null || echo "apt step skipped"

echo "== done"
