# Secrets & Tools

## Tools

Tools are binaries that should be available inside the sandbox. Register them once per machine.

### Register

```bash
cato tool add node           # auto-detects path via `which`
cato tool add python3 --path /usr/local/bin/python3  # explicit path
```

When run from a project directory with `.cato.toml`, the tool is also added to the project's `tools` list.

### How it works

Tool paths are stored in `~/.cato/store.toml`:

```toml
[tools]
node = "/opt/homebrew/bin/node"
git = "/usr/bin/git"
python3 = "/opt/homebrew/bin/python3"
```

When entering the sandbox, Cato checks that all tools listed in `.cato.toml` are available. Missing tools block sandbox entry with actionable error messages.

Inside the sandbox, system paths like `/usr`, `/bin`, and `/opt/homebrew` are read-only accessible, so registered tools work normally.

## Secrets

Secrets are injected into the sandbox as environment variables. They never exist as files inside the sandbox.

### Store

```bash
# Global (available to all projects)
cato secret put ANTHROPIC_API_KEY
# Enter value: sk-ant-...

# Inline
cato secret put DATABASE_URL=postgres://localhost/mydb

# Project-scoped (overrides global for this project only)
cato secret put DATABASE_URL=postgres://localhost/myapp --project
```

### Resolution Order

When `cato run` resolves a secret declared in `.cato.toml`:

```
1. Host environment variable (export ANTHROPIC_API_KEY=...)
2. Project-scoped store (~/.cato/store.toml under [secrets.projects."/path"])
3. Global store (~/.cato/store.toml under [secrets])
4. Default value (from .cato.toml: { default = "value" })
5. Warning (secret not found, not injected)
```

### Why project scope?

Different projects need different values for the same secret name:

```bash
cd ~/projects/app-a
cato secret put DATABASE_URL=postgres://localhost/app_a --project

cd ~/projects/app-b
cato secret put DATABASE_URL=postgres://localhost/app_b --project
```

Both share the same global `ANTHROPIC_API_KEY`, but each has its own database.

### CI/CD

In CI, secrets come from environment variables automatically. No `cato secret put` needed:

```yaml
# GitHub Actions
env:
  ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
run: |
  cato init --minimal
  cato run -- claude "review this PR"
```

### Store format

```toml
# ~/.cato/store.toml (auto-managed, never committed to git)

[tools]
node = "/opt/homebrew/bin/node"

[secrets]
ANTHROPIC_API_KEY = "sk-ant-..."
DATABASE_URL = "postgres://localhost/mydb"

[secrets.projects."/Users/hari/projects/app-a"]
DATABASE_URL = "postgres://localhost/app_a"
```

This file lives on the host and is never visible inside the sandbox.
