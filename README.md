<p align="center">
  <h1 align="center">Cato</h1>
  <p align="center">Portable sandbox for secure command execution<br/>Kernel-enforced isolation. One config file. Works for humans and AI agents alike.</p>
  <p align="center">
    <a href="https://github.com/Harikrishnareddyl/cato/actions/workflows/ci.yml"><img src="https://github.com/Harikrishnareddyl/cato/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
    <a href="https://github.com/Harikrishnareddyl/cato/releases/latest"><img src="https://img.shields.io/github/v/release/Harikrishnareddyl/cato?label=release" alt="Release"></a>
    <a href="https://crates.io/crates/cato-cli"><img src="https://img.shields.io/crates/v/cato-cli" alt="crates.io"></a>
    <a href="https://www.npmjs.com/package/cato-cli"><img src="https://img.shields.io/npm/v/cato-cli" alt="npm"></a>
    <a href="https://pypi.org/project/cato-cli-py/"><img src="https://img.shields.io/pypi/v/cato-cli-py" alt="PyPI"></a>
    <a href="https://github.com/Harikrishnareddyl/cato/blob/main/LICENSE"><img src="https://img.shields.io/github/license/Harikrishnareddyl/cato" alt="License"></a>
  </p>
</p>

---

Drop a `.cato.toml` in your project, run `cato run`, and everything inside is locked down at the OS level. No process can read your secrets, escape the workspace, or reach unauthorized networks — enforced by the macOS kernel, not by cooperation.

## Install

```bash
brew tap harikrishnareddyl/cato && brew install cato   # macOS (Homebrew)
npm install -g cato-cli                                 # Node.js
pip install cato-cli-py                                 # Python
cargo install cato-cli                                  # Rust
```

Pre-built binaries on [Releases](https://github.com/Harikrishnareddyl/cato/releases/latest).

## Quick start

```bash
cd my-project
cato init                        # creates .cato.toml
cato tool add node git python3   # register tools (once per machine)
cato secret put ANTHROPIC_API_KEY  # store secrets (once per machine)
cato run                         # enter sandbox
```

Inside the sandbox:

```
🔒 my-project $ cat .env
cat: .env: Operation not permitted          # kernel blocks it

🔒 my-project $ ls ~/Documents
ls: Operation not permitted                 # home dir invisible

🔒 my-project $ curl https://evil.com
000                                         # network blocked

🔒 my-project $ echo $ANTHROPIC_API_KEY
sk-ant-...                                  # secrets injected as env vars

🔒 my-project $ node app.js
listening on :3000                           # normal work, just safe

🔒 my-project $ exit
```

## How it works

```
You run `cato run` in your project directory
  → Cato reads .cato.toml (committed to git, shared with your team)
  → Generates a macOS Seatbelt profile: deny everything, allow only what's needed
  → Starts a network proxy for domain filtering
  → Enters sandbox-exec with your shell
  → Everything inside is kernel-enforced — no process can bypass it
```

The sandbox is the same whether a human or an AI agent is inside. `cato run -- claude "review this code"` gives Claude the same restrictions as a developer typing commands.

## What's protected

| Layer | How | What |
|-------|-----|------|
| **Filesystem** | Deny-default Seatbelt profile | Only workspace + /tmp writable. Home dir invisible. System dirs read-only. |
| **Secrets** | `deny_read` patterns | `.env`, `*.pem`, `*.key`, credentials — blocked at kernel level, even from Python/Node |
| **Network** | Localhost-only + proxy filter | Only domains in your allow list are reachable. Everything else refused. |
| **Config** | Write-protected | `.cato.toml` can't be modified from inside the sandbox |
| **Tools** | Registered binaries | Only tools you explicitly registered are available |

## Configuration

```toml
# .cato.toml — commit to git, portable rules for the team

[sandbox]
writable = ["{workspace}", "/tmp"]

deny_read = [
    "*.env", "*.env.*",
    "*.pem", "*.key", "*.p12",
    "id_rsa", "id_ed25519",
    "*credentials*", "*.keystore",
    ".git-credentials",
]

network = [
    "github.com",
    "api.anthropic.com",
    "registry.npmjs.org",
]

tools = ["node", "git", "npm", "claude"]

[sandbox.secrets]
ANTHROPIC_API_KEY = {}
DATABASE_URL = { default = "postgres://localhost/mydb" }

[sandbox.options]
ssh_agent = true
allow_localhost = true
```

### Configuration reference

| Field | Description |
|-------|-------------|
| `writable` | Paths writable inside sandbox. `{workspace}` expands to project dir. |
| `deny_read` | Glob patterns for files blocked from reading (kernel-enforced). |
| `network` | Allowed domains. Empty = unrestricted. Non-empty = deny all others. |
| `tools` | Tool binaries required inside the sandbox. |
| `secrets` | Secrets injected as env vars. Values from host env or `~/.cato/store.toml`. |
| `ssh_agent` | Forward SSH agent socket (for git push via SSH). |
| `allow_localhost` | Allow localhost connections (dev servers, databases). |

### Network filtering

When `network` has domains listed, Cato enforces a **deny-all-except** model:

1. The macOS kernel blocks all outbound connections except to localhost
2. A local proxy on localhost only forwards to allowed domains
3. Tools see `http_proxy`/`https_proxy` env vars and route through the proxy

Even if a process ignores the proxy env vars, it can't reach the internet — blocked at the kernel level.

Wildcards supported: `*.github.com` matches `api.github.com`, `raw.github.com`.

### Secrets

Secrets never exist as files inside the sandbox. They're injected as environment variables.

```bash
# Store globally (available to all projects)
cato secret put ANTHROPIC_API_KEY

# Store per-project (overrides global for this project)
cato secret put DATABASE_URL=postgres://localhost/myapp --project
```

Resolution order: host env var > project-scoped store > global store > default value.

In CI, secrets come from environment variables automatically — no `cato secret put` needed.

## CLI reference

| Command | Description |
|---------|-------------|
| `cato init` | Create `.cato.toml` with sensible defaults |
| `cato init --strict` | Stricter defaults (more deny patterns) |
| `cato run` | Enter sandboxed shell |
| `cato run -- <cmd>` | Run single command in sandbox |
| `cato run --ephemeral` | Sandbox with disposable workspace copy |
| `cato tool add <name>` | Register a tool binary |
| `cato tool list` | List registered tools |
| `cato tool remove <name>` | Remove a tool |
| `cato secret put <NAME>` | Store a secret (prompts for value) |
| `cato secret put <N>=<V>` | Store a secret with value |
| `cato secret put <N> --project` | Store project-scoped secret |
| `cato secret list` | List secrets (values masked) |
| `cato secret remove <NAME>` | Remove a secret |
| `cato status` | Show sandbox readiness |
| `cato audit` | View sandbox event log |
| `cato audit -f` | Follow new entries in real-time |

## Use with AI agents

Run any AI agent inside the sandbox. The agent gets the same restrictions as everything else:

```bash
# Claude Code
cato run -- claude "add authentication to this app"

# Interactive — start sandbox, then run your agent inside
cato run
🔒 $ claude
🔒 $ cursor
🔒 $ node my-agent.js
```

The agent can read and write workspace files, use registered tools, access allowed network domains, and use injected secrets — nothing more.

## Architecture

```
Host (unrestricted)
  ├── ~/.cato/store.toml     ← tool paths + secrets (never visible inside)
  ├── cato binary
  │     ├── Reads .cato.toml
  │     ├── Starts network proxy (if domains configured)
  │     ├── Generates Seatbelt profile (deny-default)
  │     ├── Injects secrets as env vars
  │     └── Enters: sandbox-exec -f profile.sb /bin/zsh
  │
  └── Sandbox (kernel-enforced)
        ├── Workspace: read-write
        ├── /tmp: read-write
        ├── System dirs: read-only
        ├── Home dir: invisible
        ├── Network: allowed domains only (via proxy)
        ├── .env, *.pem, etc: Operation not permitted
        └── All child processes inherit restrictions
```

## Building from source

```bash
git clone https://github.com/Harikrishnareddyl/cato.git
cd cato
cargo build --release
./target/release/cato --help
```

## Platform support

| Platform | Status |
|----------|--------|
| macOS (Apple Silicon) | Supported (Seatbelt/sandbox-exec) |
| macOS (Intel) | Supported |
| Linux | Planned (bubblewrap/Landlock) |
| Windows | Not planned |

## Contributing

PRs welcome. Run `cargo test` before submitting.

## Security

Cato uses macOS `sandbox-exec` (Seatbelt framework) for kernel-level process sandboxing. The deny-default profile blocks everything not explicitly allowed. Network filtering uses a local proxy with domain allow lists, backed by kernel-level outbound blocking.

Report vulnerabilities via [GitHub Issues](https://github.com/Harikrishnareddyl/cato/issues).

## License

[MIT](LICENSE)
