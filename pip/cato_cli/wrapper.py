"""
Thin wrapper that ensures the cato binary is available, then execs it.

On first run, downloads the correct platform binary from GitHub Releases,
verifies its SHA256 checksum, and caches it locally.
"""

import hashlib
import platform
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

__version__ = "0.5.0"
REPO = "Harikrishnareddyl/cato"

PLATFORM_MAP = {
    ("Darwin", "arm64"):   "cato-macos-arm64",
    ("Darwin", "x86_64"):  "cato-macos-x64",
    ("Linux", "x86_64"):   "cato-linux-x64",
    ("Linux", "aarch64"):  "cato-linux-arm64",
}


def _bin_dir() -> Path:
    """Return the directory where we cache the binary."""
    return Path.home() / ".cato" / "bin"


def _bin_path() -> Path:
    return _bin_dir() / "cato"


def _fetch_checksums(tag: str) -> dict[str, str]:
    """Download checksums.txt and parse into {filename: sha256} dict."""
    url = f"https://github.com/{REPO}/releases/download/{tag}/checksums.txt"
    try:
        resp = urllib.request.urlopen(url)
        checksums = {}
        for line in resp.read().decode().strip().splitlines():
            parts = line.split()
            if len(parts) == 2:
                sha, name = parts
                # Strip path prefix (e.g., ./cato-macos-arm64-v0.5.0/...)
                basename = name.split("/")[-1] if "/" in name else name
                checksums[basename] = sha
                # Also store with full relative path
                checksums[name] = sha
        return checksums
    except Exception:
        return {}


def _verify_checksum(filepath: Path, expected_sha: str) -> bool:
    """Verify SHA256 of a file against expected hash."""
    sha = hashlib.sha256()
    with open(filepath, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            sha.update(chunk)
    return sha.hexdigest() == expected_sha


def _safe_extract(tar: tarfile.TarFile, dest: str):
    """Extract tarball safely — reject paths that escape the destination."""
    dest_path = Path(dest).resolve()
    for member in tar.getmembers():
        member_path = (dest_path / member.name).resolve()
        if not str(member_path).startswith(str(dest_path)):
            raise RuntimeError(f"Path traversal detected in archive: {member.name}")
    # Use filter='data' on Python 3.12+ to block device files, setuid, etc.
    if sys.version_info >= (3, 12):
        tar.extractall(dest, filter='data')
    else:
        tar.extractall(dest)


def _download_binary() -> Path:
    system = platform.system()
    machine = platform.machine()
    key = (system, machine)

    if key not in PLATFORM_MAP:
        print(f"[cato] Unsupported platform: {system}-{machine}", file=sys.stderr)
        sys.exit(1)

    asset = PLATFORM_MAP[key]
    tag = f"v{__version__}"
    filename = f"{asset}-{tag}.tar.gz"
    url = f"https://github.com/{REPO}/releases/download/{tag}/{filename}"

    bin_dir = _bin_dir()
    bin_dir.mkdir(parents=True, exist_ok=True, mode=0o700)

    # Download to secure temp file
    tmp_dir = tempfile.mkdtemp(prefix="cato-dl-")
    tmp_file = Path(tmp_dir) / filename

    try:
        print(f"[cato] Downloading {url}...", file=sys.stderr)
        urllib.request.urlretrieve(url, tmp_file)

        # Verify checksum
        checksums = _fetch_checksums(tag)
        if checksums:
            matched = False
            for name, sha in checksums.items():
                if filename in name:
                    if _verify_checksum(tmp_file, sha):
                        print("[cato] Checksum verified.", file=sys.stderr)
                        matched = True
                        break
                    else:
                        print("[cato] ERROR: Checksum mismatch! Download may be corrupted.",
                              file=sys.stderr)
                        sys.exit(1)
            if not matched:
                print("[cato] Warning: no matching checksum found, skipping verification.",
                      file=sys.stderr)
        else:
            print("[cato] Warning: checksums.txt not available, skipping verification.",
                  file=sys.stderr)

        # Extract to secure temp directory
        extract_dir = tempfile.mkdtemp(prefix="cato-extract-")
        with tarfile.open(tmp_file, "r:gz") as tar:
            _safe_extract(tar, extract_dir)

        extracted = list(Path(extract_dir).rglob("cato"))
        extracted = [p for p in extracted if p.is_file() and p.name == "cato"]
        if not extracted:
            print("[cato] Binary not found in archive", file=sys.stderr)
            sys.exit(1)

        dest = bin_dir / "cato"
        shutil.copy2(extracted[0], dest)
        dest.chmod(stat.S_IRWXU)  # 0700 — owner only

        shutil.rmtree(extract_dir, ignore_errors=True)
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)

    path = _bin_path()
    print(f"[cato] Installed to {path}", file=sys.stderr)
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
