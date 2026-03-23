#!/usr/bin/env node
"use strict";

const https = require("https");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const os = require("os");

const SUPPORTED_PLATFORMS = {
  "darwin-arm64": "tsmetrics",
  "darwin-x64": "tsmetrics",
  "linux-x64": "tsmetrics",
  "linux-arm64": "tsmetrics",
  "win32-x64": "tsmetrics.exe",
};

const platformKey = `${process.platform}-${process.arch}`;
const binaryName = SUPPORTED_PLATFORMS[platformKey];

if (!binaryName) {
  console.warn(
    `tsmetrics: unsupported platform "${platformKey}" — skipping binary download.\n` +
      `Supported platforms: ${Object.keys(SUPPORTED_PLATFORMS).join(", ")}\n` +
      `You can build from source: cargo install --git https://github.com/gabrielrf97/tsmetrics`
  );
  process.exit(0); // non-fatal: don't break npm install
}

const pkg = JSON.parse(
  fs.readFileSync(path.join(__dirname, "package.json"), "utf8")
);
const version = pkg.version;
const tag = `v${version}`;
const isWindows = platformKey === "win32-x64";
const ext = isWindows ? ".zip" : ".tar.gz";
const archiveFilename = `tsmetrics-${tag}-${platformKey}${ext}`;
const url = `https://github.com/gabrielrf97/tsmetrics/releases/download/${tag}/${archiveFilename}`;

const binDir = path.join(__dirname, "bin");
const binaryPath = path.join(binDir, binaryName);

// Already installed (e.g. re-running postinstall)
if (fs.existsSync(binaryPath)) {
  process.exit(0);
}

fs.mkdirSync(binDir, { recursive: true });

const archivePath = path.join(os.tmpdir(), archiveFilename);

console.log(`tsmetrics: downloading binary for ${platformKey}...`);
console.log(`  → ${url}`);

function download(url, dest, redirects = 0) {
  if (redirects > 5) return Promise.reject(new Error("Too many redirects"));
  return new Promise((resolve, reject) => {
    https
      .get(url, (res) => {
        if (res.statusCode === 301 || res.statusCode === 302) {
          return download(res.headers.location, dest, redirects + 1)
            .then(resolve)
            .catch(reject);
        }
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode} from ${url}`));
          return;
        }
        const file = fs.createWriteStream(dest);
        res.pipe(file);
        file.on("finish", () => file.close(resolve));
        file.on("error", (err) => {
          fs.unlink(dest, () => {});
          reject(err);
        });
      })
      .on("error", reject);
  });
}

async function install() {
  try {
    await download(url, archivePath);
  } catch (err) {
    console.error(`\ntsmetrics: download failed — ${err.message}`);
    console.error(`\nTo install manually:`);
    console.error(`  1. Download ${url}`);
    console.error(`  2. Extract the binary to ${binDir}`);
    console.error(`\nOr build from source: cargo install --git https://github.com/gabrielrf97/tsmetrics`);
    process.exit(1);
  }

  try {
    if (isWindows) {
      execSync(
        `powershell -Command "Expand-Archive -Path '${archivePath}' -DestinationPath '${binDir}' -Force"`,
        { stdio: "pipe" }
      );
    } else {
      execSync(`tar -xzf "${archivePath}" -C "${binDir}"`, { stdio: "pipe" });
      fs.chmodSync(binaryPath, 0o755);
    }
  } catch (err) {
    console.error(`\ntsmetrics: extraction failed — ${err.message}`);
    process.exit(1);
  } finally {
    try {
      fs.unlinkSync(archivePath);
    } catch {}
  }

  if (!fs.existsSync(binaryPath)) {
    console.error(`\ntsmetrics: binary not found after extraction at ${binaryPath}`);
    process.exit(1);
  }

  console.log(`tsmetrics: ready.`);
}

install().catch((err) => {
  console.error(`tsmetrics: unexpected error — ${err.message}`);
  process.exit(1);
});
