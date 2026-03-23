#!/usr/bin/env bash
# Manual npm publish script for tsmetrics.
#
# Usage:
#   ./npm/scripts/publish.sh <version>
#
# Prerequisites:
#   - npm login (or NPM_TOKEN env var set)
#   - The GitHub Release for <version> must already exist with binaries attached
#     (postinstall downloads from there)
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

# ── Stamp version ──────────────────────────────────────────────────────────────
echo "Stamping version ${VERSION} into package.json..."

TSM_PKG_PATH="${REPO_ROOT}/npm/tsmetrics/package.json" TSM_VERSION="${VERSION}" node -e "
  const fs = require('fs');
  const pkg = JSON.parse(fs.readFileSync(process.env.TSM_PKG_PATH, 'utf8'));
  pkg.version = process.env.TSM_VERSION;
  fs.writeFileSync(process.env.TSM_PKG_PATH, JSON.stringify(pkg, null, 2) + '\n');
"
echo "  ✓ npm/tsmetrics/package.json"
echo ""

# ── Publish ────────────────────────────────────────────────────────────────────
echo "Publishing tsmetrics@${VERSION}..."
(cd "${REPO_ROOT}/npm/tsmetrics" && npm publish --access public)
echo ""

echo "Done! tsmetrics@${VERSION} is now on npm."
echo ""
echo "Users can install it with:"
echo "  npm install -g tsmetrics"
echo "  npx tsmetrics ./src"
echo ""
echo "NOTE: To deprecate the old @tsmetrics/* platform packages, run:"
for dir in darwin-arm64 darwin-x64 linux-x64 linux-arm64 win32-x64; do
  echo "  npm deprecate \"@tsmetrics/${dir}@*\" \"Moved to single 'tsmetrics' package\""
done
