#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/tokensaver-target}"
if [[ -d "/Library/Developer/CommandLineTools" ]]; then
  export DEVELOPER_DIR="${DEVELOPER_DIR:-/Library/Developer/CommandLineTools}"
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "TokenSaver macOS packaging must run on macOS." >&2
  exit 2
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required." >&2
  exit 2
fi

TAURI_BIN=()
if cargo tauri --version >/dev/null 2>&1; then
  TAURI_BIN=(cargo tauri)
elif command -v tauri >/dev/null 2>&1; then
  TAURI_BIN=(tauri)
elif command -v npx >/dev/null 2>&1; then
  TAURI_BIN=(npx @tauri-apps/cli)
elif command -v bunx >/dev/null 2>&1; then
  TAURI_BIN=(bunx @tauri-apps/cli)
else
  echo "Tauri CLI is required. Install a Tauri 2 CLI compatible with this project before packaging." >&2
  exit 2
fi

ICON_SOURCE="assets/app-icon.svg"
RELEASE_CONFIG="tauri.release.conf.json"
if [[ ! -f "$ICON_SOURCE" ]]; then
  echo "Missing icon source: $ICON_SOURCE" >&2
  exit 2
fi
if [[ ! -f "$RELEASE_CONFIG" ]]; then
  echo "Missing release bundle config: $RELEASE_CONFIG" >&2
  exit 2
fi

# Generated icon files are release artifacts, not source-of-truth assets.
"${TAURI_BIN[@]}" icon "$ICON_SOURCE" --output icons

# Self-updater artifacts are intentionally disabled in tauri.conf.json until
# TokenSaver has a real updater signing key and trusted release endpoint. The
# release overlay only adds generated branded icon paths.
"${TAURI_BIN[@]}" build --config "$RELEASE_CONFIG" --bundles app,dmg "$@"
