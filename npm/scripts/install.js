#!/usr/bin/env node

const { execSync } = require("child_process");
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");
const https = require("https");
const { createWriteStream, mkdirSync } = require("fs");

const VERSION = require("../package.json").version;
const REPO = "Harikrishnareddyl/cato";
const BIN_DIR = path.join(__dirname, "..", "bin");

function getPlatform() {
  const os = process.platform;
  const arch = process.arch;

  const map = {
    "darwin-arm64": "cato-macos-arm64",
    "darwin-x64": "cato-macos-x64",
    "linux-x64": "cato-linux-x64",
    "linux-arm64": "cato-linux-arm64",
  };

  const key = `${os}-${arch}`;
  if (!map[key]) {
    console.error(`[cato] Unsupported platform: ${key}`);
    console.error("[cato] Supported: darwin-arm64, darwin-x64, linux-x64, linux-arm64");
    process.exit(1);
  }
  return map[key];
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const follow = (url) => {
      https.get(url, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          follow(res.headers.location);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode} for ${url}`));
          return;
        }
        const file = createWriteStream(dest);
        res.pipe(file);
        file.on("finish", () => { file.close(); resolve(); });
      }).on("error", reject);
    };
    follow(url);
  });
}

function downloadText(url) {
  return new Promise((resolve, reject) => {
    const follow = (url) => {
      https.get(url, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          follow(res.headers.location);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode}`));
          return;
        }
        let data = "";
        res.on("data", (chunk) => { data += chunk; });
        res.on("end", () => resolve(data));
      }).on("error", reject);
    };
    follow(url);
  });
}

function sha256File(filepath) {
  const hash = crypto.createHash("sha256");
  const data = fs.readFileSync(filepath);
  hash.update(data);
  return hash.digest("hex");
}

async function main() {
  const asset = getPlatform();
  const tag = `v${VERSION}`;
  const filename = `${asset}-${tag}.tar.gz`;
  const url = `https://github.com/${REPO}/releases/download/${tag}/${filename}`;

  console.log(`[cato] Downloading ${filename}...`);

  mkdirSync(BIN_DIR, { recursive: true });
  const tmpFile = path.join(BIN_DIR, filename);

  try {
    await download(url, tmpFile);
  } catch (e) {
    console.error(`[cato] Download failed: ${e.message}`);
    console.error(`[cato] URL: ${url}`);
    console.error(`[cato] Download manually: https://github.com/${REPO}/releases`);
    process.exit(1);
  }

  // Verify checksum
  const checksumUrl = `https://github.com/${REPO}/releases/download/${tag}/checksums.txt`;
  try {
    const checksumText = await downloadText(checksumUrl);
    const lines = checksumText.trim().split("\n");
    const match = lines.find((l) => l.includes(filename));
    if (match) {
      const expectedSha = match.split(/\s+/)[0];
      const actualSha = sha256File(tmpFile);
      if (actualSha === expectedSha) {
        console.log("[cato] Checksum verified.");
      } else {
        console.error(`[cato] ERROR: Checksum mismatch!`);
        console.error(`[cato]   Expected: ${expectedSha}`);
        console.error(`[cato]   Got:      ${actualSha}`);
        fs.unlinkSync(tmpFile);
        process.exit(1);
      }
    } else {
      console.log("[cato] Warning: no matching checksum found, skipping verification.");
    }
  } catch (e) {
    console.log("[cato] Warning: could not verify checksum:", e.message);
  }

  console.log("[cato] Extracting...");
  execSync(`tar xzf "${tmpFile}" -C "${BIN_DIR}" --strip-components=1`, { stdio: "pipe" });

  // Clean up archive
  fs.unlinkSync(tmpFile);

  // Set executable permission
  fs.chmodSync(path.join(BIN_DIR, "cato"), 0o755);

  console.log("[cato] Installed successfully!");
  console.log("[cato] Run: cato init");
}

main().catch((e) => {
  console.error(`[cato] Installation failed: ${e.message}`);
  process.exit(1);
});
