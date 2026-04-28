#!/usr/bin/env bash
# Smoke test for Cato sandbox
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CATO="$PROJECT_DIR/target/release/cato"
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

echo "=== Building ==="
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release 2>&1 | tail -3

echo ""
echo "=== Setup ==="
cd "$TMPDIR"
"$CATO" init
echo "  Created .cato.toml in $TMPDIR"

# Create test files
echo "SECRET=leaked" > .env
echo "PRIVATE_KEY" > secret.key
echo "normal content" > test.txt

PASS=0
FAIL=0

sandbox_check() {
    local desc="$1"
    local cmd="$2"
    local expect_fail="$3"  # true = expect command to fail inside sandbox

    local exit_code=0
    "$CATO" run -- /bin/sh -c "$cmd" > /dev/null 2>&1 || exit_code=$?

    if [[ "$expect_fail" == "true" && $exit_code -ne 0 ]]; then
        echo "  PASS: $desc (blocked, exit $exit_code)"
        PASS=$((PASS + 1))
    elif [[ "$expect_fail" == "false" && $exit_code -eq 0 ]]; then
        echo "  PASS: $desc (allowed)"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $desc (expect_fail=$expect_fail, got exit=$exit_code)"
        FAIL=$((FAIL + 1))
    fi
}

echo ""
echo "=== File protection ==="
sandbox_check ".env blocked" "cat .env" true
sandbox_check ".key blocked" "cat secret.key" true
sandbox_check "normal file allowed" "cat test.txt" false
sandbox_check "workspace write allowed" "echo x > newfile.txt" false

echo ""
echo "=== Home directory isolation ==="
sandbox_check "home dir blocked" "ls ~/Documents" true
sandbox_check "home write blocked" "touch ~/blocked.txt" true

echo ""
echo "=== Config protection ==="
sandbox_check ".cato.toml write blocked" "echo hacked >> .cato.toml" true

echo ""
echo "=== Sandbox indicator ==="
sandbox_check "CATO_SANDBOX set" 'test "$CATO_SANDBOX" = "1"' false

echo ""
echo "=== Python bypass prevention ==="
sandbox_check "python .env read blocked" 'python3 -c "open(\".env\").read()"' true

echo ""
echo "=== Exit code forwarding ==="
local_exit=0
"$CATO" run -- /bin/sh -c "exit 0" > /dev/null 2>&1 || local_exit=$?
if [ $local_exit -eq 0 ]; then
    echo "  PASS: exit 0 forwarded"
    PASS=$((PASS + 1))
else
    echo "  FAIL: exit 0 not forwarded (got $local_exit)"
    FAIL=$((FAIL + 1))
fi

local_exit=0
"$CATO" run -- /bin/sh -c "exit 42" > /dev/null 2>&1 || local_exit=$?
if [ $local_exit -eq 42 ]; then
    echo "  PASS: exit 42 forwarded"
    PASS=$((PASS + 1))
else
    echo "  FAIL: exit 42 not forwarded (got $local_exit)"
    FAIL=$((FAIL + 1))
fi

echo ""
echo "=== Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"

if [ "$FAIL" -gt 0 ]; then
    echo "SOME TESTS FAILED"
    exit 1
fi
echo "ALL TESTS PASSED"
