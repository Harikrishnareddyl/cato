# Running GitHub CLI (gh) Inside Cato

## Setup

```bash
cato tool add gh
```

This detects `~/.config/gh/` and adds it to `allow_read`.

### Network

Add to `.cato.toml`:
```toml
network = [
    "github.com",
    "api.github.com",
]
```

### Auth

`gh` stores auth in `~/.config/gh/hosts.yml`. Since `cato tool add gh` mounts this directory, auth works automatically. No secrets needed.

If you prefer, you can also use a token:
```bash
cato secret put GH_TOKEN
```

```toml
[sandbox.secrets]
GH_TOKEN = {}
```

## Usage

```bash
cato run -- gh pr list
cato run -- gh issue create --title "bug" --body "details"
```

## Example .cato.toml

```toml
[sandbox]
allow_read = ["~/.config/gh"]
allow_write = ["{workspace}", "/tmp"]
deny_read = ["*.env", "*.key"]
network = ["github.com", "api.github.com"]
tools = ["gh", "git"]

[sandbox.options]
allow_localhost = true
```
