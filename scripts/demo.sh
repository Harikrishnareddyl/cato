#!/usr/bin/env bash
# Demo script for recording Cato in action.
# Usage: ./scripts/demo.sh
# Record with: asciinema rec demo.cast  (then ./scripts/demo.sh)
#          or: just screen-record your terminal

set -euo pipefail

CATO="${CATO:-cato}"
DEMO_DIR=$(mktemp -d)
trap "rm -rf $DEMO_DIR" EXIT

# Simulated typing effect
type_cmd() {
    local cmd="$1"
    printf "\033[1;32m❯\033[0m "
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
echo "  ┌─────────────────────────────────────────┐"
echo "  │  Cato — Agent-agnostic sandbox           │"
echo "  │  One config file, any process, anywhere  │"
echo "  └─────────────────────────────────────────┘"
echo ""
pause 2

# Step 1: Init
echo "  \033[1;36m# Step 1: Initialize a project\033[0m"
echo ""
cd "$DEMO_DIR"
type_cmd "cd my-project && cato init"
mkdir -p "$DEMO_DIR/my-project" && cd "$DEMO_DIR/my-project"
$CATO init 2>&1 | head -5
pause 2

# Show the config
echo ""
echo "  \033[1;36m# The config file:\033[0m"
echo ""
type_cmd "cat .cato.toml | head -20"
head -20 .cato.toml
pause 2

# Create test files
echo "SECRET_API_KEY=sk-live-abc123" > .env
echo "PRIVATE_KEY_DATA" > secret.key
echo "console.log('hello world');" > app.js

# Step 2: Enter sandbox
echo ""
echo "  \033[1;36m# Step 2: Enter the sandbox\033[0m"
echo ""
type_cmd "cato run -- /bin/sh -c 'echo \"Inside sandbox\"'"
$CATO run -- /bin/sh -c 'echo "Inside sandbox"' 2>&1
pause 2

# Step 3: Show protection
echo ""
echo "  \033[1;36m# Step 3: Try to read secrets\033[0m"
echo ""

type_cmd "cato run -- cat .env"
$CATO run -- cat .env 2>&1 || true
pause 1.5

type_cmd "cato run -- cat secret.key"
$CATO run -- cat secret.key 2>&1 || true
pause 1.5

echo ""
echo "  \033[1;36m# Normal files work fine:\033[0m"
echo ""
type_cmd "cato run -- cat app.js"
$CATO run -- cat app.js 2>&1
pause 2

# Step 4: Home directory
echo ""
echo "  \033[1;36m# Step 4: Home directory is invisible\033[0m"
echo ""
type_cmd "cato run -- ls ~/Documents"
$CATO run -- ls ~/Documents 2>&1 || true
pause 2

# Step 5: Python bypass attempt
echo ""
echo "  \033[1;36m# Step 5: Even Python can't read secrets\033[0m"
echo ""
type_cmd "cato run -- python3 -c \"open('.env').read()\""
$CATO run -- python3 -c "open('.env').read()" 2>&1 || true
pause 2

# Step 6: Write protection
echo ""
echo "  \033[1;36m# Step 6: Config is tamper-proof\033[0m"
echo ""
type_cmd "cato run -- /bin/sh -c 'echo hacked >> .cato.toml'"
$CATO run -- /bin/sh -c 'echo hacked >> .cato.toml' 2>&1 || true
pause 2

# Done
echo ""
echo ""
echo "  \033[1;32m✓ Kernel-enforced on macOS. OS-enforced on Linux.\033[0m"
echo "  \033[1;32m✓ One .cato.toml — works with any agent, any tool.\033[0m"
echo "  \033[1;32m✓ github.com/Harikrishnareddyl/cato\033[0m"
echo ""
pause 3
