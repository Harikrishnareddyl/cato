# Configuration

Cato is configured per-project via `.cato.toml`. Commit this to git — it defines the sandbox boundaries for everyone on the team.

## Full Example

```toml
[sandbox]
# Write: deny by default. Only these paths are writable.
allow_write = ["{workspace}", "/tmp"]

# Write deny: block writes to these patterns even within allow_write paths.
# Deny overrides allow.
deny_write = ["*.lock", ".github/*", "migrations/*"]

# Read deny: block reads for these patterns.
deny_read = [
    "*.env", "*.env.*",
    "*.pem", "*.key", "*.p12",
    "id_rsa", "id_ed25519",
    "*credentials*", "*.keystore",
    ".git-credentials",
]

# Network: deny by default. Only listed domains are reachable.
# Empty = no outbound at all. ["*"] = unrestricted.
network = [
    "github.com",
    "api.anthropic.com",
    "registry.npmjs.org",
]

tools = ["node", "git", "npm", "claude"]

[sandbox.secrets]
ANTHROPIC_API_KEY = {}
DATABASE_URL = { default = "postgres://localhost/mydb" }
NODE_ENV = { default = "development" }

[sandbox.options]
ssh_agent = true
allow_localhost = true
```

## Access Control Model

### Writes

Writes are **denied everywhere by default**. You explicitly list which paths are writable.

```toml
allow_write = ["{workspace}", "/tmp"]
```

- `{workspace}` expands to the project directory
- You can add any absolute path: `"/var/data"`, `"~/shared-output"`
- If you remove `{workspace}`, the workspace becomes read-only (reviewer mode)

Within allowed paths, you can further restrict with `deny_write`:

```toml
deny_write = ["*.lock", "package-lock.json", ".github/*"]
```

**Precedence: deny overrides allow.** A file matching both `allow_write` and `deny_write` is blocked.

### Reads

Reads are **allowed by default** within the workspace and system directories. You block specific patterns:

```toml
deny_read = ["*.env", "*.pem", "*.key", "*credentials*"]
```

On macOS, these are kernel-enforced. On Linux, existing files are kernel-enforced; new files matching these patterns are blocked at the libc level (see [security model](security.md) for details).

Note: the home directory (`~/`) is always invisible regardless of `deny_read`. Only the workspace and system directories are readable.

### Network

Network is **denied by default**. No outbound connections unless you list domains:

```toml
network = ["github.com", "registry.npmjs.org"]
```

- **Empty or missing** = no outbound network at all
- **Specific domains** = only those reachable (via proxy filtering + kernel block)
- **`["*"]`** = unrestricted network access

Wildcards supported: `"*.github.com"` matches `api.github.com`, `raw.github.com`.

Localhost is always allowed regardless of network config (controlled by `allow_localhost` option).

## Precedence Summary

| Layer | Default | Override |
|-------|---------|----------|
| Write | Deny all | `allow_write` opens paths → `deny_write` blocks within |
| Read | Allow (workspace + system) | `deny_read` blocks patterns |
| Network | Deny all | `network` opens domains → `["*"]` opens all |

## Common Patterns

### Standard project (full access within workspace)
```toml
[sandbox]
allow_write = ["{workspace}", "/tmp"]
deny_read = ["*.env", "*.pem", "*.key"]
network = ["github.com", "registry.npmjs.org"]
```

### Read-only reviewer (agent can read but not modify)
```toml
[sandbox]
allow_write = ["/tmp"]
deny_read = ["*.env"]
network = ["*"]
```

### Strict scope (agent only edits specific files)
```toml
[sandbox]
allow_write = ["src/auth/", "tests/auth/", "/tmp"]
deny_read = ["*.env", "*.key"]
network = ["github.com"]
```

### Fully locked down (no writes, no network)
```toml
[sandbox]
allow_write = ["/tmp"]
deny_read = ["*.env", "*.pem", "*.key"]
network = []
```

## Fields Reference

| Field | Default | Description |
|-------|---------|-------------|
| `allow_write` | `["{workspace}", "/tmp"]` | Paths where writes are allowed |
| `deny_write` | `[]` | Patterns blocked from writing within allowed paths |
| `deny_read` | `[]` | Patterns blocked from reading |
| `network` | `[]` (blocked) | Allowed domains. Empty = no network. `["*"]` = unrestricted |
| `tools` | `[]` | Required tool binaries |
| `secrets` | `{}` | Secrets injected as env vars |
| `ssh_agent` | `false` | Forward SSH agent socket |
| `allow_localhost` | `true` | Allow localhost connections |

## Presets

`cato init` generates a `.cato.toml` based on a preset. It also auto-detects your project type (Node.js, Python, Rust, Go, etc.) and populates `tools` and `network` accordingly.

### `cato init` — Standard (default)

Workspace writable, common secret patterns denied, lockfiles protected, network includes detected package registries.

```toml
[sandbox]
allow_write = ["{workspace}", "/tmp"]

deny_write = [
    "*.lock",
]

deny_read = [
    "*.env",
    "*.env.*",
    "*.pem",
    "*.key",
    "*.p12",
    "id_rsa",
    "id_ed25519",
    "*credentials*",
    "*.keystore",
    ".git-credentials",
]

# Auto-populated based on project type
network = [
    "github.com",
    "registry.npmjs.org",    # if Node.js detected
    "pypi.org",              # if Python detected
    "files.pythonhosted.org", # if Python detected
    "crates.io",             # if Rust detected
]

tools = []  # auto-populated: git, node, npm, python3, cargo, etc.

[sandbox.secrets]
# ANTHROPIC_API_KEY = {}
# DATABASE_URL = { default = "postgres://localhost/mydb" }

[sandbox.options]
ssh_agent = true
allow_localhost = true
```

### `cato init --minimal` — Minimal

Just secret file protection. No deny_write patterns. Fewer deny_read patterns. Network only includes detected sources.

```toml
[sandbox]
allow_write = ["{workspace}", "/tmp"]

deny_write = [
    "*.lock",
]

deny_read = [
    "*.env",
    "*.env.*",
    "*.pem",
    "*.key",
    "*.p12",
    "id_rsa",
    "id_ed25519",
]

network = [
    "github.com",
]

tools = []

[sandbox.secrets]

[sandbox.options]
ssh_agent = true
allow_localhost = true
```

### `cato init --strict` — Strict

Everything from standard plus: CI configs and migrations protected from writes.

```toml
[sandbox]
allow_write = ["{workspace}", "/tmp"]

deny_write = [
    "*.lock",
    ".github/*",
    "migrations/*",
]

deny_read = [
    "*.env",
    "*.env.*",
    "*.pem",
    "*.key",
    "*.p12",
    "id_rsa",
    "id_ed25519",
    "*credentials*",
    "*.keystore",
    ".git-credentials",
]

network = [
    "github.com",
    "registry.npmjs.org",
]

tools = []

[sandbox.secrets]

[sandbox.options]
ssh_agent = true
allow_localhost = true
```

## Tips

- Commit `.cato.toml` to git — rules travel with the project
- Use `cato status` to verify tools, secrets, and network readiness
- Use `CATO_DEBUG=1 cato run` to inspect the generated sandbox profile
- `deny_write = ["*.lock"]` prevents agents from modifying lockfiles
- Remove `{workspace}` from `allow_write` for a read-only sandbox
- The presets are starting points — edit the generated file to fit your project
