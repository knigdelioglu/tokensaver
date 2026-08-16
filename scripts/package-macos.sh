#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "TokenSaver macOS packaging must run on macOS." >&2
  exit 2
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required." >&2
  exit 2
fi

if ! cargo tauri --version >/dev/null 2>&1; then
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
cargo tauri icon "$ICON_SOURCE" --output icons

# Self-updater artifacts are intentionally disabled in tauri.conf.json until
# TokenSaver has a real updater signing key and trusted release endpoint. The
# release overlay only adds generated branded icon paths.
cargo tauri build --config "$RELEASE_CONFIG" --bundles app,dmg "$@"
