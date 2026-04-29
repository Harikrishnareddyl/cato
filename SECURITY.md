# Security Policy

## Reporting a Vulnerability

**Do NOT file a public GitHub issue for security vulnerabilities.**

Report via [GitHub Security Advisories](https://github.com/Harikrishnareddyl/cato/security/advisories/new).

Include:
- Description of the vulnerability
- Steps to reproduce
- Impact assessment
- Suggested fix (if any)

I aim to respond within 48 hours and release a fix within 7 days for critical issues.

## Scope

- **In scope:** Bypasses that allow processes to escape the sandbox (read protected files, write outside workspace, reach blocked networks)
- **Out of scope:** Attacks that require replacing the cato binary or exploiting the macOS kernel

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.4.x | ✅ Current (research preview) |

## Security Design

- **OS-level enforcement** — macOS: Seatbelt (`sandbox-exec`) with deny-default profile. Linux: bubblewrap (mount/network namespaces) + LD_PRELOAD for new file patterns. See [security model](docs/security.md) for per-feature enforcement details.
- **Zero unsafe code** — The entire codebase uses safe Rust
- **Minimal dependencies** — Direct dependencies: serde, clap, toml, chrono, dirs, ctrlc, serde_json
- **Deny-default model** — Everything blocked unless explicitly allowed. Home directory invisible, network blocked unless domains listed.
- **Append-only audit log** — JSON lines at `~/.cato/audit.jsonl`, stored on host (outside sandbox)
- **Config tamper-proof** — `.cato.toml` write-protected from inside sandbox
- **Secrets as env vars only** — No secret files inside sandbox. Host store (`~/.cato/store.toml`) invisible from inside.
