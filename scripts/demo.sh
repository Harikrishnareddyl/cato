#!/usr/bin/env bash
# Demo script for recording Cato in action.
# Usage: ./scripts/demo.sh
# Record with: asciinema rec demo.cast -c ./scripts/demo.sh

set -euo pipefail

CATO="${CATO:-cato}"
DEMO_DIR=$(mktemp -d)
trap "rm -rf $DEMO_DIR" EXIT

# Colors
CYAN=$'\033[1;36m'
GREEN=$'\033[1;32m'
RESET=$'\033[0m'

# Simulated typing effect
type_cmd() {
    local cmd="$1"
    printf "${GREEN}>${RESET} "
    for ((i=0; i<${#cmd}; i++)); do
        printf "%s" "${cmd:$i:1}"
        sleep 0.04
    done
    echo ""
    sleep 0.3
}

pause() { sleep "${1:-1.5}"; }

clear
echo ""
echo "  Cato - Agent-agnostic sandbox"
echo "  One config file, any process, anywhere"
echo ""
pause 2

# Step 1: Init
printf "  ${CYAN}# Step 1: Initialize a project${RESET}\n"
echo ""
cd "$DEMO_DIR"
type_cmd "cd my-project && cato init"
mkdir -p "$DEMO_DIR/my-project" && cd "$DEMO_DIR/my-project"
$CATO init 2>&1 | head -5
pause 2

# Create test files
echo "SECRET_API_KEY=sk-live-abc123" > .env
echo "PRIVATE_KEY_DATA" > secret.key
printf "console.log('hello world');\n" > app.js

# Step 2: Enter sandbox
echo ""
printf "  ${CYAN}# Step 2: Enter the sandbox${RESET}\n"
echo ""
type_cmd "cato run -- echo 'Inside sandbox'"
$CATO run -- /bin/sh -c 'echo "Inside sandbox"' 2>&1
pause 2

# Step 3: Show protection
echo ""
printf "  ${CYAN}# Step 3: Try to read secrets${RESET}\n"
echo ""

type_cmd "cato run -- cat .env"
$CATO run -- cat .env 2>&1 || true
pause 1.5

type_cmd "cato run -- cat secret.key"
$CATO run -- cat secret.key 2>&1 || true
pause 1.5

echo ""
printf "  ${CYAN}# Normal files work fine:${RESET}\n"
echo ""
type_cmd "cato run -- cat app.js"
$CATO run -- cat app.js 2>&1
pause 2

# Step 4: Home directory
echo ""
printf "  ${CYAN}# Step 4: Home directory is invisible${RESET}\n"
echo ""
type_cmd "cato run -- ls ~/Documents"
$CATO run -- ls ~/Documents 2>&1 || true
pause 2

# Step 5: Python bypass attempt
echo ""
printf "  ${CYAN}# Step 5: Even Python can't bypass it${RESET}\n"
echo ""
type_cmd "cato run -- python3 -c \"open('.env').read()\""
$CATO run -- python3 -c "open('.env').read()" 2>&1 || true
pause 2

# Step 6: Write protection
echo ""
printf "  ${CYAN}# Step 6: Config is tamper-proof${RESET}\n"
echo ""
type_cmd "cato run -- sh -c 'echo hacked >> .cato.toml'"
$CATO run -- /bin/sh -c 'echo hacked >> .cato.toml' 2>&1 || true
pause 2

# Done
echo ""
echo ""
printf "  ${GREEN}Done. Same rules for any agent, any tool, any person.${RESET}\n"
printf "  ${GREEN}github.com/Harikrishnareddyl/cato${RESET}\n"
echo ""
pause 3
