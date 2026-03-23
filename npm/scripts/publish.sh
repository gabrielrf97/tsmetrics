#!/usr/bin/env bash
# Manual npm publish script for tsmetrics packages.
#
# Usage:
#   ./npm/scripts/publish.sh <version>
#
# Prerequisites:
#   - npm login (or NPM_TOKEN env var set)
#   - Compiled binaries already placed in npm/<platform>/bin/
#
# Example:
#   ./npm/scripts/publish.sh 0.2.0

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

VERSION="${1:-}"

if [[ -z "${VERSION}" ]]; then
  echo "Usage: $0 <version>"
  echo "  e.g. $0 0.2.0"
  exit 1
fi

# Strip leading 'v' if present
VERSION="${VERSION#v}"

echo "Publishing tsmetrics v${VERSION} to npm..."
echo ""

PLATFORM_DIRS=(darwin-arm64 darwin-x64 linux-x64 linux-arm64 win32-x64)

# ── Stamp versions ─────────────────────────────────────────────────────────────
echo "Stamping version ${VERSION} into package.json files..."

for dir in "${PLATFORM_DIRS[@]}"; do
  PKG="${REPO_ROOT}/npm/${dir}/package.json"
  TSM_PKG_PATH="${PKG}" TSM_VERSION="${VERSION}" node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync(process.env.TSM_PKG_PATH, 'utf8'));
    pkg.version = process.env.TSM_VERSION;
    fs.writeFileSync(process.env.TSM_PKG_PATH, JSON.stringify(pkg, null, 2) + '\n');
  "
  echo "  ✓ npm/${dir}/package.json"
done

UMBRELLA_PKG="${REPO_ROOT}/npm/tsmetrics/package.json"
TSM_PKG_PATH="${UMBRELLA_PKG}" TSM_VERSION="${VERSION}" node -e "
  const fs = require('fs');
  const pkg = JSON.parse(fs.readFileSync(process.env.TSM_PKG_PATH, 'utf8'));
  pkg.version = process.env.TSM_VERSION;
  for (const dep of Object.keys(pkg.optionalDependencies)) {
    pkg.optionalDependencies[dep] = process.env.TSM_VERSION;
  }
  fs.writeFileSync(process.env.TSM_PKG_PATH, JSON.stringify(pkg, null, 2) + '\n');
"
echo "  ✓ npm/tsmetrics/package.json"
echo ""

# ── Verify binaries exist ──────────────────────────────────────────────────────
echo "Checking binaries..."
MISSING=0
for dir in "${PLATFORM_DIRS[@]}"; do
  if [[ "${dir}" == "win32-x64" ]]; then
    BIN="${REPO_ROOT}/npm/${dir}/bin/tsmetrics.exe"
  else
    BIN="${REPO_ROOT}/npm/${dir}/bin/tsmetrics"
  fi
  if [[ ! -f "${BIN}" ]]; then
    echo "  ✗ MISSING: npm/${dir}/bin/$(basename "${BIN}")"
    MISSING=1
  else
    echo "  ✓ npm/${dir}/bin/$(basename "${BIN}")"
  fi
done

if [[ "${MISSING}" -eq 1 ]]; then
  echo ""
  echo "Error: one or more binaries are missing."
  echo "Build them with: cargo build --release (or use cross for cross-compilation)"
  exit 1
fi
echo ""

# ── Ensure Unix binaries are executable ───────────────────────────────────────
echo "Setting executable bit on Unix binaries..."
for dir in "${PLATFORM_DIRS[@]}"; do
  if [[ "${dir}" != "win32-x64" ]]; then
    BIN="${REPO_ROOT}/npm/${dir}/bin/tsmetrics"
    if [[ -f "${BIN}" ]]; then
      chmod +x "${BIN}"
      echo "  ✓ chmod +x npm/${dir}/bin/tsmetrics"
    fi
  fi
done
echo ""

# ── Publish platform packages first ───────────────────────────────────────────
echo "Publishing platform packages..."
for dir in "${PLATFORM_DIRS[@]}"; do
  echo "  → @tsmetrics/${dir}"
  (cd "${REPO_ROOT}/npm/${dir}" && npm publish --access public)
done
echo ""

# ── Publish umbrella package ───────────────────────────────────────────────────
echo "Publishing umbrella package tsmetrics..."
(cd "${REPO_ROOT}/npm/tsmetrics" && npm publish --access public)
echo ""

echo "Done! tsmetrics@${VERSION} is now on npm."
echo ""
echo "Users can install it with:"
echo "  npm install -g tsmetrics"
echo "  npx tsmetrics analyze ./src"
