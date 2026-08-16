#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v python3 >/dev/null 2>&1; then
  echo "Release packaging requires python3 for validation-manifest verification." >&2
  exit 2
fi

python3 scripts/verify-release-gates.py

# The normal packaging script deliberately does not run tests. At this point
# release evidence has already been verified for the exact current commit.
exec bash scripts/package-macos.sh "$@"
