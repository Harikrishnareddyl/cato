# Known Issues and Limitations

## Platform-Specific

### macOS: Mach IPC fully open

The Seatbelt profile uses `(allow mach-lookup)` with no service allowlist. This means sandboxed processes can talk to any system service (Keychain, pasteboard, accessibility APIs, etc.).

Anthropic's sandbox-runtime allowlists specific services, but their list was built for older macOS versions and fails on macOS 26 (Tahoe). We opened it up for compatibility. A tested per-version allowlist would be more restrictive but is fragile to maintain.

**Impact:** Low. Mach services don't provide filesystem or network access beyond what the Seatbelt profile already controls.

### Linux: deny patterns only cover existing files at kernel level

`deny_read` and `deny_write` use bwrap bind mounts, which are set at sandbox creation time. Files created DURING the session are only caught by LD_PRELOAD (libc-level, bypassable by Go, static binaries, raw syscalls).

**Impact:** Host secrets are protected (existing files hidden). The gap only affects new files created by the agent inside the sandbox — which contain the agent's own data, not host secrets.

### Linux: three separate glob implementations

`seatbelt.rs`, `bwrap.rs`, and `deny_open.c` each implement their own pattern matching. Edge cases may produce different results for the same `.cato.toml` pattern across macOS and Linux.

**Impact:** Uncommon patterns might match differently. Common patterns (`*.env`, `*.key`, `id_rsa`, `*credentials*`) are tested on both platforms.

**Tracked for fix:** Consolidate to one canonical Rust implementation that generates output for each platform.

## Configuration

### ssh_agent = true forwards your SSH keys

When enabled, the sandboxed process can use your SSH keys to authenticate to any server — push to any git repo, SSH into any machine. This is a deliberate escape hatch for git workflows but significantly weakens sandbox isolation.

`cato init` generates `ssh_agent` commented out (disabled by default). If you enable it, cato prints a warning at sandbox start.

### Proxy has no authentication

The network proxy listens on `127.0.0.1:random` with no auth. Any local process running as your user can connect. This is by design — the proxy restricts access (filters to allowed domains), it doesn't grant access to anything the host can't already reach.

## Operational

### Audit log has no rotation

`~/.cato/audit.jsonl` grows indefinitely. Frequent sandbox use accumulates entries. No built-in rotation or cleanup. Manually truncate with `> ~/.cato/audit.jsonl` if needed.

### Audit log has no integrity protection

Entries are append-only JSON lines but not signed or tamper-proofed. Any process with file access (outside the sandbox) can edit or delete entries.
