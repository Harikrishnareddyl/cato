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

PASS=0
FAIL=0

sandbox_check() {
    local desc="$1"
    local cmd="$2"
    local expect_fail="$3"

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

# ─────────────────────────────────────
# Test 1: Standard sandbox config
# ─────────────────────────────────────
echo ""
echo "=== Setup: standard config ==="
TESTDIR="$TMPDIR/standard"
mkdir -p "$TESTDIR" && cd "$TESTDIR"
cat > .cato.toml << 'EOF'
[sandbox]
allow_write = ["{workspace}", "/tmp"]
deny_write = ["*.lock"]
deny_read = ["*.env", "*.pem", "*.key"]
network = ["*"]
[sandbox.options]
allow_localhost = true
EOF
echo "SECRET=leaked" > .env
echo "PRIVATE_KEY" > secret.key
echo "normal content" > test.txt

echo ""
echo "=== deny_read ==="
sandbox_check ".env blocked" "cat .env" true
sandbox_check ".key blocked" "cat secret.key" true
sandbox_check "normal file allowed" "cat test.txt" false

echo ""
echo "=== allow_write + deny_write ==="
sandbox_check "workspace write allowed" "echo x > newfile.txt" false
sandbox_check "deny_write *.lock blocked" '/bin/sh -c "echo x > test.lock"' true

echo ""
echo "=== Home directory isolation ==="
sandbox_check "home dir blocked" "ls ~/Documents" true
sandbox_check "home write blocked" "touch ~/blocked.txt" true

echo ""
echo "=== Config protection ==="
sandbox_check ".cato.toml write blocked" '/bin/sh -c "echo hacked >> .cato.toml"' true

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

# ─────────────────────────────────────
# Test 2: Network deny-by-default (empty = blocked)
# ─────────────────────────────────────
echo ""
echo "=== Setup: network blocked (empty list) ==="
NETDIR="$TMPDIR/net-blocked"
mkdir -p "$NETDIR" && cd "$NETDIR"
cat > .cato.toml << 'EOF'
[sandbox]
allow_write = ["{workspace}", "/tmp"]
network = []
[sandbox.options]
allow_localhost = true
EOF

sandbox_check "network blocked (empty list)" \
    'curl -s -o /dev/null -w "%{http_code}" --max-time 5 https://example.com | grep -q 000' false

# ─────────────────────────────────────
# Test 3: Ephemeral mode
# ─────────────────────────────────────
echo ""
echo "=== Setup: ephemeral mode ==="
EPHDIR="$TMPDIR/ephemeral"
mkdir -p "$EPHDIR" && cd "$EPHDIR"
cat > .cato.toml << 'EOF'
[sandbox]
allow_write = ["{workspace}", "/tmp"]
network = ["*"]
[sandbox.options]
allow_localhost = true
EOF
echo "original" > eph-file.txt

"$CATO" run --ephemeral -- /bin/sh -c 'echo "modified" > eph-file.txt; echo "new" > eph-new.txt' > /dev/null 2>&1

CONTENT=$(cat "$EPHDIR/eph-file.txt")
if [ "$CONTENT" = "original" ]; then
    echo "  PASS: ephemeral — original file unchanged"
    PASS=$((PASS + 1))
else
    echo "  FAIL: ephemeral — file was modified (got: $CONTENT)"
    FAIL=$((FAIL + 1))
fi

if [ ! -f "$EPHDIR/eph-new.txt" ]; then
    echo "  PASS: ephemeral — new file not persisted"
    PASS=$((PASS + 1))
else
    echo "  FAIL: ephemeral — new file leaked to original"
    FAIL=$((FAIL + 1))
fi

# ─────────────────────────────────────
# Results
# ─────────────────────────────────────
echo ""
echo "=== Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"

if [ "$FAIL" -gt 0 ]; then
    echo "SOME TESTS FAILED"
    exit 1
fi
echo "ALL TESTS PASSED"
