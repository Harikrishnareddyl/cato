#!/usr/bin/env bash
# install.sh — Download and install Cato
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/Harikrishnareddyl/cato/main/scripts/install.sh | sh

set -euo pipefail

REPO="Harikrishnareddyl/cato"
INSTALL_DIR="$HOME/.cato/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
DIM='\033[2m'
BOLD='\033[1m'
RESET='\033[0m'

info()  { echo -e "${GREEN}${BOLD}[cato]${RESET} $1"; }
warn()  { echo -e "${YELLOW}${BOLD}[cato]${RESET} $1"; }
error() { echo -e "${RED}${BOLD}[cato]${RESET} $1"; }

info "Detecting system..."

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)
        case "$ARCH" in
            arm64|aarch64) PLATFORM="macos-arm64" ;;
            x86_64)        PLATFORM="macos-x64" ;;
            *)             error "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    Linux)
        case "$ARCH" in
            x86_64)        PLATFORM="linux-x64" ;;
            aarch64|arm64) PLATFORM="linux-arm64" ;;
            *)             error "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        error "Unsupported OS: $OS. Cato supports macOS and Linux."
        exit 1
        ;;
esac
EXT="tar.gz"

info "Fetching latest release..."

LATEST=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | head -1 | sed 's/.*: *"//;s/".*//')

if [[ -z "$LATEST" ]]; then
    error "Could not determine latest release. Check https://github.com/$REPO/releases"
    exit 1
fi

info "Latest version: $LATEST"

ASSET_NAME="cato-${PLATFORM}-${LATEST}.${EXT}"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/${LATEST}/${ASSET_NAME}"

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

info "Downloading $ASSET_NAME..."
if ! curl -sSL -o "$TMPDIR/$ASSET_NAME" "$DOWNLOAD_URL"; then
    error "Download failed. URL: $DOWNLOAD_URL"
    exit 1
fi

# Verify checksum
CHECKSUM_URL="https://github.com/$REPO/releases/download/${LATEST}/checksums.txt"
if curl -sSL -o "$TMPDIR/checksums.txt" "$CHECKSUM_URL" 2>/dev/null; then
    EXPECTED=$(grep "$ASSET_NAME" "$TMPDIR/checksums.txt" | awk '{print $1}')
    if [[ -n "$EXPECTED" ]]; then
        ACTUAL=$(shasum -a 256 "$TMPDIR/$ASSET_NAME" | awk '{print $1}')
        if [[ "$ACTUAL" == "$EXPECTED" ]]; then
            info "Checksum verified."
        else
            error "Checksum mismatch! Expected $EXPECTED, got $ACTUAL"
            exit 1
        fi
    else
        warn "No matching checksum found, skipping verification."
    fi
else
    warn "Could not download checksums, skipping verification."
fi

info "Extracting..."
tar xzf "$TMPDIR/$ASSET_NAME" -C "$TMPDIR"

# Install binary
mkdir -p "$INSTALL_DIR"
BINARY=$(find "$TMPDIR" -name "cato" -type f | head -1)
if [[ -z "$BINARY" ]]; then
    error "Extraction failed — cato binary not found"
    exit 1
fi
cp "$BINARY" "$INSTALL_DIR/cato"
chmod +x "$INSTALL_DIR/cato"

# Add to PATH if not already there
if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    for RC in "$HOME/.zshrc" "$HOME/.bashrc"; do
        if [[ -f "$RC" ]]; then
            echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$RC"
        fi
    done
fi

echo ""
info "${GREEN}${BOLD}Cato installed successfully!${RESET}"
echo ""
echo -e "  ${DIM}Restart your shell or run:${RESET}  source ~/.bashrc"
echo -e "  ${DIM}Get started:${RESET}                cd your-project && cato init && cato run"
echo -e "  ${DIM}Check status:${RESET}               cato status"
echo -e "  ${DIM}View audit log:${RESET}             cato audit"

if [[ "$OS" == "Linux" ]]; then
    echo ""
    echo -e "  ${DIM}Linux dependencies:${RESET}         sudo apt install bubblewrap socat"
fi
echo ""
