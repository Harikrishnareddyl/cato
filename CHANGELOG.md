# Changelog

All notable changes to Cato are documented here.

## [0.5.0] — 2026-04-29

### Added
- **Linux support** — bubblewrap (bwrap) for filesystem/process isolation
- Kernel-enforced network blocking on Linux (`--unshare-net`)
- Network domain filtering on Linux via socat Unix socket bridge (same architecture as Anthropic's sandbox-runtime)
- LD_PRELOAD library (`libcato_deny.so`) for deny_read/deny_write pattern enforcement on new files
- `deny_write` field — block writes to specific patterns within `allow_write` paths
- `allow_write` replaces `writable` (old name still works via alias)
- Network deny-by-default: empty `network = []` blocks all outbound, `["*"]` for unrestricted
- Platform-specific CI test suites (macOS Seatbelt tests + Linux bwrap tests)

### Fixed
- deny_read patterns now also block overwrites on macOS (prevents wiping host secrets)
- deny_read patterns scoped to workspace (system files like `/etc/ssl/cert.pem` no longer caught by `*.pem`)

### Requirements
- macOS: 12+ (Monterey)
- Linux: kernel 5.13+, bubblewrap, socat (Ubuntu 22.04+, Debian 12+, Fedora 36+)

## [0.4.0] — 2026-04-28

### Added
- **OS-level sandbox** — kernel-enforced isolation using macOS Seatbelt (`sandbox-exec`)
- `cato run` — enter sandboxed shell with deny-default Seatbelt profile
- `cato run -- <cmd>` — run single command in sandbox, forward exit code
- `cato run --ephemeral` — disposable workspace copy
- `cato tool add/list/remove` — register tool binaries for sandbox
- `cato secret put/list/remove` — manage secrets injected as env vars
- Project-scoped secrets (`--project` flag) for per-project overrides
- **Network filtering** — proxy-based domain allow list, kernel blocks all direct outbound
- **deny_read patterns** — `.env`, `*.pem`, `*.key` etc. blocked at kernel level
- `.cato.toml` write-protected from inside sandbox
- SSH agent forwarding (`ssh_agent = true`)
- Audit logging — sessions, network denials, exit codes, duration
- Project-scoped audit (`cato audit` filters by current directory, `--all` for everything)
- `cato status` — sandbox readiness check with tool/secret/network summary
- Signal handling for graceful cleanup on terminal close
- `CATO_DEBUG=1` for inspecting generated Seatbelt profiles

### Research Preview
This is a research preview. macOS only (Apple Silicon + Intel). Linux support planned.
