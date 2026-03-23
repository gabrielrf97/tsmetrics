#!/usr/bin/env node
"use strict";

const { spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

const BINARY_NAME = process.platform === "win32" ? "tsmetrics.exe" : "tsmetrics";
const bin = path.join(__dirname, BINARY_NAME);

if (!fs.existsSync(bin)) {
  console.error(
    `tsmetrics: binary not found.\n` +
      `Expected: ${bin}\n\n` +
      `The postinstall script may have failed. Try reinstalling:\n` +
      `  npm install -g tsmetrics\n\n` +
      `Or build from source:\n` +
      `  cargo install --git https://github.com/gabrielrf97/tsmetrics`
  );
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (result.signal) {
  process.kill(process.pid, result.signal);
} else {
  process.exit(result.status ?? 1);
}
