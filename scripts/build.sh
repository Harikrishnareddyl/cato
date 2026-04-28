#!/usr/bin/env bash
# Build Cato and stage to dist/
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist"
PROFILE="release"
CARGO_FLAG="--release"
[[ "${1:-}" == "--debug" ]] && PROFILE="debug" && CARGO_FLAG=""

echo "=== Cato Build ==="
source "$HOME/.cargo/env" 2>/dev/null || true
(cd "$PROJECT_DIR" && cargo build $CARGO_FLAG 2>&1)

rm -rf "$DIST_DIR" && mkdir -p "$DIST_DIR"
cp "$PROJECT_DIR/target/$PROFILE/cato" "$DIST_DIR/cato"
chmod 755 "$DIST_DIR/cato"
codesign --force --sign - "$DIST_DIR/cato" 2>/dev/null || true

echo ""
ls -lh "$DIST_DIR/cato"
echo ""
echo "=== Done ==="
echo "  cato init              # create .cato.toml"
echo "  cato run               # enter sandbox"
echo "  cato run -- echo hi    # run command in sandbox"
