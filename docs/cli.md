# CLI Reference

## `cato init`

Create `.cato.toml` in the current directory.

```bash
cato init              # standard defaults
cato init --minimal    # secrets and keys only
cato init --strict     # strictest protection
cato init --force      # overwrite existing config
```

Auto-detects project type (Node.js, Python, Rust, etc.) and adjusts defaults.

## `cato run`

Enter the sandbox.

```bash
cato run                          # interactive shell
cato run -- <command>             # single command, then exit
cato run --ephemeral              # disposable workspace copy
cato run --ephemeral -- npm test  # disposable + single command
```

Environment variable `CATO_SANDBOX=1` is set inside the sandbox.

Control output verbosity:

```bash
cato run                        # normal (default) — start/stop summary
CATO_LOG=quiet cato run         # silent — no cato output
CATO_LOG=verbose cato run       # troubleshooting — blocked domains, warnings
CATO_LOG=debug cato run         # full diagnostic — sandbox profile, proxy details
```

Or set in `.cato.toml`:
```toml
[sandbox.options]
log_level = "verbose"
```

## `cato tool`

Manage tool binaries available inside the sandbox.

```bash
cato tool add node               # find via which, save path
cato tool add python3 --path /usr/local/bin/python3  # explicit path
cato tool list                   # show registered tools
cato tool remove node            # unregister
```

Tools are stored in `~/.cato/store.toml`. When run from a project directory, `cato tool add` also adds the tool to `.cato.toml`.

## `cato secret`

Manage secrets injected into the sandbox as environment variables.

```bash
cato secret put API_KEY          # prompts for value
cato secret put API_KEY=sk-...   # inline value
cato secret put DB_URL --project # project-scoped override
cato secret list                 # show names (values masked)
cato secret remove API_KEY       # delete
cato secret remove DB_URL --project  # delete project override
```

Global secrets apply to all projects. Project-scoped secrets (`--project`) override globals for the current project only.

## `cato status`

Show sandbox readiness for the current project.

```bash
cato status
```

Reports: config status, tool availability, secret resolution, SSH agent, audit stats.

## `cato audit`

View the sandbox event log (`~/.cato/audit.jsonl`).

```bash
cato audit                # last 20 entries
cato audit -n 50          # last 50 entries
cato audit -f             # follow in real-time
cato audit -q myproject   # filter by keyword
```
