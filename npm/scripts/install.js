#!/usr/bin/env node

const { execSync } = require("child_process");
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
    "darwin-arm64": { asset: "cato-macos-arm64", ext: "tar.gz" },
    "darwin-x64": { asset: "cato-macos-x64", ext: "tar.gz" },
    "linux-x64": { asset: "cato-linux-x64", ext: "tar.gz" },
    "win32-x64": { asset: "cato-windows-x64", ext: "zip" },
  };

  const key = `${os}-${arch}`;
  if (!map[key]) {
    console.error(`Unsupported platform: ${key}`);
    console.error("Supported: darwin-arm64, darwin-x64, linux-x64, win32-x64");
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
          reject(new Error(`Download failed: HTTP ${res.statusCode} for ${url}`));
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

async function main() {
  const { asset, ext } = getPlatform();
  const tag = `v${VERSION}`;
  const filename = `${asset}-${tag}.${ext}`;
  const url = `https://github.com/${REPO}/releases/download/${tag}/${filename}`;

  console.log(`[cato] Downloading ${filename}...`);

  mkdirSync(BIN_DIR, { recursive: true });
  const tmpFile = path.join(BIN_DIR, filename);

  try {
    await download(url, tmpFile);
  } catch (e) {
    console.error(`[cato] Download failed: ${e.message}`);
    console.error(`[cato] URL: ${url}`);
    console.error(`[cato] You can download manually from https://github.com/${REPO}/releases`);
    process.exit(1);
  }

  console.log("[cato] Extracting...");

  const binName = process.platform === "win32" ? "cato.exe" : "cato";

  if (ext === "tar.gz") {
    execSync(`tar xzf "${tmpFile}" -C "${BIN_DIR}" --strip-components=1`, { stdio: "pipe" });
  } else {
    // Windows zip
    execSync(`powershell -command "Expand-Archive -Path '${tmpFile}' -DestinationPath '${BIN_DIR}' -Force"`, { stdio: "pipe" });
    // Move from subdirectory
    const subdir = fs.readdirSync(BIN_DIR).find(f => f.startsWith("cato-"));
    if (subdir) {
      const src = path.join(BIN_DIR, subdir, binName);
      const dst = path.join(BIN_DIR, binName);
      if (fs.existsSync(src)) fs.renameSync(src, dst);
      fs.rmSync(path.join(BIN_DIR, subdir), { recursive: true, force: true });
    }
  }

  // Clean up archive
  fs.unlinkSync(tmpFile);

  // Set executable permission (Unix)
  if (process.platform !== "win32") {
    fs.chmodSync(path.join(BIN_DIR, "cato"), 0o755);
  }

  console.log("[cato] Installed successfully!");
  console.log("[cato] Run: cato init");
}

main().catch((e) => {
  console.error(`[cato] Installation failed: ${e.message}`);
  process.exit(1);
});
