#!/usr/bin/env node
"use strict";

const { execFileSync } = require("child_process");
const path = require("path");
const fs = require("fs");

// Map from Node's process.platform + process.arch to npm package names
const PLATFORM_MAP = {
  "darwin-arm64": "@tsm-cli/darwin-arm64",
  "darwin-x64": "@tsm-cli/darwin-x64",
  "linux-x64": "@tsm-cli/linux-x64",
  "linux-arm64": "@tsm-cli/linux-arm64",
  "win32-x64": "@tsm-cli/win32-x64",
};

const BINARY_NAME = process.platform === "win32" ? "tsm.exe" : "tsm";

function getBinaryPath() {
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORM_MAP[key];

  if (!pkg) {
    return null;
  }

  // When installed via npm, the platform package lives in node_modules
  // alongside this umbrella package.
  try {
    // resolve the platform package relative to this file
    const pkgDir = path.dirname(require.resolve(`${pkg}/package.json`));
    const bin = path.join(pkgDir, "bin", BINARY_NAME);
    if (fs.existsSync(bin)) {
      return bin;
    }
  } catch {
    // package not installed (optional dep skipped)
  }

  return null;
}

function run() {
  const bin = getBinaryPath();

  if (!bin) {
    const key = `${process.platform}-${process.arch}`;
    const supported = Object.keys(PLATFORM_MAP).join(", ");
    console.error(
      `tsm: unsupported platform "${key}".\n` +
        `Supported platforms: ${supported}\n\n` +
        `If you are on a supported platform, try reinstalling:\n` +
        `  npm install -g tsm-cli\n\n` +
        `Or build from source:\n` +
        `  cargo install --git https://github.com/gabrielrf97/tsm`
    );
    process.exit(1);
  }

  try {
    execFileSync(bin, process.argv.slice(2), { stdio: "inherit" });
  } catch (err) {
    // execFileSync throws with status when the child exits non-zero
    process.exit(err.status ?? 1);
  }
}

run();
