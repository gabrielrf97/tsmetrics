#!/usr/bin/env bash
# Manual npm publish script for tsm-cli packages.
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

echo "Publishing tsm-cli v${VERSION} to npm..."
echo ""

PLATFORM_DIRS=(darwin-arm64 darwin-x64 linux-x64 linux-arm64 win32-x64)

# ── Stamp versions ─────────────────────────────────────────────────────────────
echo "Stamping version ${VERSION} into package.json files..."

for dir in "${PLATFORM_DIRS[@]}"; do
  PKG="${REPO_ROOT}/npm/${dir}/package.json"
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('${PKG}', 'utf8'));
    pkg.version = '${VERSION}';
    fs.writeFileSync('${PKG}', JSON.stringify(pkg, null, 2) + '\n');
  "
  echo "  ✓ npm/${dir}/package.json"
done

UMBRELLA_PKG="${REPO_ROOT}/npm/tsm-cli/package.json"
node -e "
  const fs = require('fs');
  const pkg = JSON.parse(fs.readFileSync('${UMBRELLA_PKG}', 'utf8'));
  pkg.version = '${VERSION}';
  for (const dep of Object.keys(pkg.optionalDependencies)) {
    pkg.optionalDependencies[dep] = '${VERSION}';
  }
  fs.writeFileSync('${UMBRELLA_PKG}', JSON.stringify(pkg, null, 2) + '\n');
"
echo "  ✓ npm/tsm-cli/package.json"
echo ""

# ── Verify binaries exist ──────────────────────────────────────────────────────
echo "Checking binaries..."
MISSING=0
for dir in "${PLATFORM_DIRS[@]}"; do
  if [[ "${dir}" == "win32-x64" ]]; then
    BIN="${REPO_ROOT}/npm/${dir}/bin/tsm.exe"
  else
    BIN="${REPO_ROOT}/npm/${dir}/bin/tsm"
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

# ── Publish platform packages first ───────────────────────────────────────────
echo "Publishing platform packages..."
for dir in "${PLATFORM_DIRS[@]}"; do
  echo "  → @tsm-cli/${dir}"
  (cd "${REPO_ROOT}/npm/${dir}" && npm publish --access public)
done
echo ""

# ── Publish umbrella package ───────────────────────────────────────────────────
echo "Publishing umbrella package tsm-cli..."
(cd "${REPO_ROOT}/npm/tsm-cli" && npm publish --access public)
echo ""

echo "Done! tsm-cli@${VERSION} is now on npm."
echo ""
echo "Users can install it with:"
echo "  npm install -g tsm-cli"
echo "  npx tsm-cli analyze ./src"
