# Running Claude Code Inside Cato

## Prerequisites

- Cato installed (`brew install cato` / `npm i -g cato-cli` / `cargo install cato-cli`)
- Claude Code installed (`npm i -g @anthropic-ai/claude-code`)
- Active Claude subscription or API key

## Setup (one time)

### Step 1: Initialize your project

```bash
cd your-project
cato init
```

### Step 2: Register Claude as a tool

```bash
cato tool add claude
```

This auto-detects Claude's config directories and adds them to `.cato.toml`:
```
[cato] ✓ claude → /path/to/claude
[cato] ✓ found config: ~/.claude (will mount read-write)
[cato] ✓ found config: ~/.local/share/claude (will mount read-write)
[cato] ✓ found config: ~/.claude.json (will mount read-write)
```

### Step 3: Set up authentication

Claude Code uses OAuth (subscription) or API keys. Inside the sandbox, the macOS Keychain isn't accessible, so you need to pass auth via environment variable.

**Option A: Subscription (OAuth token)**

Generate a long-lived token on your host:
```bash
claude setup-token
```

Copy the token and store it in Cato:
```bash
cato secret put CLAUDE_CODE_OAUTH_TOKEN
# Paste the token when prompted
```

Then add it to your `.cato.toml`:
```toml
[sandbox.secrets]
CLAUDE_CODE_OAUTH_TOKEN = {}
```

**Option B: API key**

If you have an Anthropic API key (`console.anthropic.com`):
```bash
cato secret put ANTHROPIC_API_KEY
```

Then add it to your `.cato.toml`:
```toml
[sandbox.secrets]
ANTHROPIC_API_KEY = {}
```

### Step 4: Configure network

Add Claude's required domains to `.cato.toml`:

```toml
network = [
    "*.anthropic.com",
]
```

When you first run Claude inside the sandbox, watch for `[cato] blocked: ...` messages. These show exactly which domains Claude is trying to reach. Add any that are needed (e.g., `*.datadoghq.com` for telemetry).

Add your project-specific domains separately (e.g., `github.com`, `registry.npmjs.org`).

## Usage

```bash
# Single command
cato run -- claude -p "review this code and suggest improvements"

# Interactive session
cato run
🔒 $ claude
```

## What Claude Can and Can't Do Inside

| Action | Status |
|--------|--------|
| Read/write workspace files | Allowed |
| Read .env, .key, .pem | Blocked |
| Access home directory | Only mounted config dirs |
| Reach api.anthropic.com | Allowed (in network list) |
| Reach unauthorized domains | Blocked |
| Modify .cato.toml | Blocked |

## Troubleshooting

**"Not logged in · Please run /login"**
- Auth token not configured. Follow Step 3 above.
- If using `CLAUDE_CODE_OAUTH_TOKEN`, make sure it's declared in `[sandbox.secrets]`.

**Command hangs with no output**
- Run with verbose logging to see blocked domains and warnings:
  ```bash
  CATO_LOG=verbose cato run -- claude -p "hi"
  ```
- Check `[cato] blocked:` messages — a required domain might be missing from the network list.
- Make sure `cato tool add claude` was run to mount config directories.

**"Warning: no stdin data received"**
- Normal for non-interactive mode. Claude waits briefly for piped input then proceeds.

## Example .cato.toml

```toml
[sandbox]
allow_read = [
    "~/.claude",
    "~/.local/share/claude",
    "~/.local/state/claude",
    "~/.claude.json",
]
allow_write = ["{workspace}", "/tmp"]
deny_read = ["*.env", "*.key", "*.pem", "*credentials*"]
deny_write = ["*.lock"]
network = [
    "*.anthropic.com",
    # Add domains shown by [cato] blocked: messages
    # Add project-specific domains:
    # "github.com",
]
tools = ["claude"]

[sandbox.secrets]
CLAUDE_CODE_OAUTH_TOKEN = {}

[sandbox.options]
allow_localhost = true
```
