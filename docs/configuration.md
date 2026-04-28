# Configuration

Cato is configured per-project via `.cato.toml`. Commit this to git — it defines the sandbox rules for everyone on the team.

## Full Example

```toml
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
NODE_ENV = { default = "development" }

[sandbox.options]
ssh_agent = true
allow_localhost = true
```

## Fields

### `writable`

Paths that are writable inside the sandbox. Everything else is read-only or invisible.

- `{workspace}` expands to the project directory
- Default: `["{workspace}", "/tmp"]`

### `deny_read`

Glob patterns for files that cannot be read inside the sandbox, even within the workspace. Enforced at the kernel level — no process can bypass it.

```toml
deny_read = ["*.env", "*.pem", "*.key"]
```

Patterns:
- `*.env` matches `.env`, `prod.env`, `test.env`
- `*.env.*` matches `.env.local`, `.env.production`
- `*credentials*` matches anything with "credentials" in the name
- `id_rsa` matches the exact filename anywhere in the workspace

These patterns are scoped to the workspace — system files like `/etc/ssl/cert.pem` are not affected.

### `network`

Allowed domains. Controls which external hosts are reachable from inside the sandbox.

- **Empty list** (`network = []`): all outbound traffic allowed
- **Non-empty list**: only listed domains are reachable, everything else blocked

```toml
network = ["github.com", "*.npmjs.org", "api.anthropic.com"]
```

Wildcards: `*.github.com` matches `api.github.com`, `raw.github.com`, etc.

See [Network Filtering](network.md) for details.

### `tools`

Tool binaries required inside the sandbox. Cato checks these before entering and will error if any are missing.

```toml
tools = ["node", "git", "npm"]
```

Register tools with `cato tool add <name>`.

### `[sandbox.secrets]`

Secrets to inject as environment variables. Values come from the host — either environment variables or `~/.cato/store.toml`.

```toml
[sandbox.secrets]
ANTHROPIC_API_KEY = {}                              # required, no default
DATABASE_URL = { default = "postgres://localhost" }  # has fallback
```

See [Secrets & Tools](secrets-and-tools.md) for the full resolution order.

### `[sandbox.options]`

| Option | Default | Description |
|--------|---------|-------------|
| `ssh_agent` | `false` | Forward SSH agent socket for git push via SSH |
| `allow_localhost` | `true` | Allow connections to localhost (dev servers, databases) |

## Presets

`cato init` supports presets:

```bash
cato init              # standard defaults
cato init --minimal    # secrets and keys only
cato init --strict     # strictest defaults
```

## Tips

- Commit `.cato.toml` to git — the sandbox config travels with the project
- Use `cato status` to check if all tools and secrets are ready
- Use `CATO_DEBUG=1 cato run` to see the generated Seatbelt profile
