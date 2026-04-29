# Getting Started

## Installation

```bash
brew tap harikrishnareddyl/cato && brew install cato   # macOS (Homebrew)
npm install -g cato-cli                                 # Node.js
pip install cato-cli-py                                 # Python
cargo install cato-cli                                  # Rust
```

Or build from source:

```bash
git clone https://github.com/Harikrishnareddyl/cato.git
cd cato && cargo build --release
```

## Quick Start

### 1. Initialize your project

```bash
cd my-project
cato init
```

This creates `.cato.toml` with sensible defaults — deny reading secrets, restrict network, protect config files. Commit it to git so everyone on the team gets the same rules.

### 2. Register tools (once per machine)

```bash
cato tool add node git python3 npm
```

Cato finds each binary via `which` and saves the path. These tools become available inside the sandbox.

### 3. Store secrets (once per machine)

```bash
cato secret put ANTHROPIC_API_KEY
# Enter value: sk-ant-...

cato secret put DATABASE_URL=postgres://localhost/mydb
```

Secrets are stored on the host in `~/.cato/store.toml`. Inside the sandbox they appear as environment variables — no secret files exist inside.

### 4. Enter the sandbox

```bash
cato run
```

You're now in an OS-enforced sandbox. Every process inside — your shell, any tool, any AI agent — is restricted.

### 5. Work normally

```bash
🔒 my-project $ npm install        # works
🔒 my-project $ node app.js        # works
🔒 my-project $ cat .env           # Operation not permitted
🔒 my-project $ curl evil.com      # blocked
🔒 my-project $ exit               # back to host
```

## How It Works

Cato uses OS-level sandboxing — macOS Seatbelt (`sandbox-exec`) or Linux bubblewrap — to create an isolated environment around your shell session.

```
Host
  │
  ├── cato reads .cato.toml
  ├── Starts network proxy (if domains configured)
  ├── Generates Seatbelt profile: deny everything, allow only what's needed
  ├── Injects secrets as env vars
  └── Runs: sandbox-exec -f profile.sb /bin/zsh
        │
        └── Sandbox (OS-enforced)
              ├── Workspace: read-write
              ├── System dirs: read-only
              ├── Home dir: invisible
              ├── .env / .pem / .key: blocked at kernel level
              ├── Network: only allowed domains (via proxy)
              └── All child processes inherit restrictions
```

The key insight: the sandbox doesn't care what's running inside. Human, AI agent, script — all get the same restrictions. No cooperation needed. No hooks to configure. Just `cato run`.

## Running AI Agents

```bash
# Run Claude inside the sandbox
cato run -- claude "review this code and fix the bugs"

# Or start an interactive sandbox and run agents inside
cato run
🔒 $ claude
🔒 $ cursor
```

The agent can read/write workspace files, use registered tools, access allowed domains, and read injected secrets. It can't read `.env` files, access your home directory, or reach unauthorized networks.

## Single Command Mode

Run a command and exit:

```bash
cato run -- npm test
cato run -- python3 script.py
cato run -- claude "add error handling"
```

The exit code is forwarded.

## Ephemeral Mode

Run in a disposable copy of the workspace:

```bash
cato run --ephemeral
# Make changes, experiment freely
# On exit, everything is thrown away — original untouched
```
