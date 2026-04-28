"""
Thin wrapper that ensures the cato binary is available, then execs it.

On first run, downloads the correct platform binary from GitHub Releases.
Subsequent runs just exec the cached binary.
"""

import os
import platform
import shutil
import stat
import subprocess
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

__version__ = "0.2.1"
REPO = "Harikrishnareddyl/cato"

PLATFORM_MAP = {
    ("Darwin", "arm64"):   ("cato-macos-arm64",  "tar.gz"),
    ("Darwin", "x86_64"):  ("cato-macos-x64",    "tar.gz"),
    ("Linux", "x86_64"):   ("cato-linux-x64",    "tar.gz"),
    ("Linux", "aarch64"):  ("cato-linux-arm64",   "tar.gz"),
    ("Windows", "AMD64"):  ("cato-windows-x64",  "zip"),
}


def _bin_dir() -> Path:
    """Return the directory where we cache the binary."""
    return Path.home() / ".cato" / "bin"


def _bin_path() -> Path:
    name = "cato.exe" if platform.system() == "Windows" else "cato"
    return _bin_dir() / name


def _download_binary() -> Path:
    system = platform.system()
    machine = platform.machine()
    key = (system, machine)

    if key not in PLATFORM_MAP:
        print(f"[cato] Unsupported platform: {system}-{machine}", file=sys.stderr)
        sys.exit(1)

    asset, ext = PLATFORM_MAP[key]
    tag = f"v{__version__}"
    filename = f"{asset}-{tag}.{ext}"
    url = f"https://github.com/{REPO}/releases/download/{tag}/{filename}"

    bin_dir = _bin_dir()
    bin_dir.mkdir(parents=True, exist_ok=True)

    tmp = Path(f"/tmp/cato-download-{filename}")
    print(f"[cato] Downloading {url}...", file=sys.stderr)
    urllib.request.urlretrieve(url, tmp)

    if ext == "tar.gz":
        with tarfile.open(tmp, "r:gz") as tar:
            tar.extractall("/tmp/cato-extract")
        extracted = list(Path("/tmp/cato-extract").rglob("cato"))
        if not extracted:
            print("[cato] Binary not found in archive", file=sys.stderr)
            sys.exit(1)
        dest = bin_dir / "cato"
        shutil.copy2(extracted[0], dest)
        dest.chmod(dest.stat().st_mode | stat.S_IEXEC)
    elif ext == "zip":
        with zipfile.ZipFile(tmp, "r") as z:
            z.extractall("/tmp/cato-extract")
        extracted = list(Path("/tmp/cato-extract").rglob("cato.exe"))
        if not extracted:
            print("[cato] Binary not found in archive", file=sys.stderr)
            sys.exit(1)
        shutil.copy2(extracted[0], bin_dir / "cato.exe")

    tmp.unlink(missing_ok=True)
    shutil.rmtree("/tmp/cato-extract", ignore_errors=True)

    path = _bin_path()
    print(f"[cato] Installed binary to {path}", file=sys.stderr)
    return path


def main():
    binary = _bin_path()
    if not binary.exists():
        binary = _download_binary()

    try:
        result = subprocess.run([str(binary)] + sys.argv[1:])
        sys.exit(result.returncode)
    except KeyboardInterrupt:
        sys.exit(130)


if __name__ == "__main__":
    main()
