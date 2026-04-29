# Use Cases

Cato is a generic sandbox — it works with any command out of the box. Simple tools (node, python, git, make) work immediately after `cato tool add`.

Some tools need additional setup to work inside the sandbox — authentication, config directories, network access. These guides walk through the setup.

## Guides

### AI Coding Tools
- [Claude Code](claude-code.md) — Anthropic's coding assistant

### Developer Tools
- [GitHub CLI (gh)](github-cli.md) — GitHub's command-line tool

### Environments
- CI Pipelines (coming soon)
- Docker / containers (coming soon)

## When Do You Need a Guide?

**No guide needed** — tools that run locally without network or auth:
```bash
cato tool add node python3 git make
cato run -- node app.js       # just works
cato run -- python3 test.py   # just works
cato run -- make build        # just works
```

**Guide needed** — tools that require auth configs or external API access:
```bash
cato tool add claude     # auto-detects config dirs
# But: needs auth token + network domains → follow the guide
```

## How to Know What's Missing

If a command hangs or fails inside the sandbox, Cato tells you in real time:

```
[cato] blocked: api.anthropic.com (not in allowed domains)
[cato] ⚠ Potential issues:
[cato]   • 'claude' has config at ~/.claude but it's not in allow_read
[cato]   • network is fully blocked. 'claude' likely needs network access
```

Use these messages to figure out what to add to `.cato.toml`.

## Setting Up Any Tool

The pattern is always the same:

```bash
# 1. Register it (auto-detects config dirs)
cato tool add mytool

# 2. If it needs API keys, store them
cato secret put MY_TOOL_API_KEY

# 3. If it needs network, add domains to .cato.toml
# network = ["api.mytool.com"]

# 4. Test
cato run -- mytool --version
```

## Contributing Guides

PRs welcome. Each guide should include:
- Prerequisites and setup steps
- Required network domains
- Authentication method
- Example `.cato.toml`
- Troubleshooting
