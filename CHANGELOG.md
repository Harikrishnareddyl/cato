# Changelog

All notable changes to Cato are documented here.

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
