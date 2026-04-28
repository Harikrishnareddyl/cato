# Security Model

## Enforcement

Cato uses macOS `sandbox-exec` (Seatbelt framework) for kernel-level process sandboxing. Every restriction is enforced by the macOS kernel — no process inside the sandbox can bypass it regardless of language, permissions, or technique.

## Deny-Default Architecture

The Seatbelt profile starts with `(deny default)` — everything is blocked. Then specific capabilities are allowed:

| Allowed | What | Why |
|---------|------|-----|
| Workspace read-write | Project directory | Normal work |
| /tmp read-write | Temporary files | Build tools, scripts |
| System dirs read-only | /usr, /bin, /System, /Library, etc. | Shell, tools, libraries |
| Shell config read-only | ~/.zshrc, ~/.zprofile (dotfiles only) | Shell initialization |
| Mach IPC | System services | Process operation |
| sysctl reads | System info | Process operation |
| Localhost network | 127.0.0.1 | Dev servers, proxy, databases |

Everything else is denied:
- Home directory contents (invisible, not just "permission denied")
- Writing outside workspace
- Reading secret files (.env, .pem, .key)
- Outbound network (when domains configured)
- Modifying .cato.toml

## Filesystem Protection

### deny_read

Patterns in `deny_read` are enforced with Seatbelt `(deny file-read-data ...)` rules scoped to the workspace. Even if a file exists and is owned by the user, the kernel blocks reads.

```bash
$ cat .env
cat: .env: Operation not permitted

$ python3 -c "open('.env').read()"
PermissionError: [Errno 1] Operation not permitted: '.env'
```

### Home directory

The home directory is invisible inside the sandbox — not "permission denied" but actually invisible. `~/.ssh/id_rsa` shows "No such file or directory" because the sandbox can't see it at all.

Only shell config dotfiles in the home root (`.zshrc`, `.zprofile`, etc.) are readable for shell initialization.

### Config protection

`.cato.toml` cannot be modified from inside the sandbox, preventing a process from weakening its own restrictions.

## Network Protection

When `network` domains are configured:

1. **Kernel layer**: All outbound blocked except localhost (Seatbelt)
2. **Proxy layer**: HTTP CONNECT proxy on localhost filters by domain

Even if a process ignores proxy env vars and tries to connect directly, the kernel blocks it. The only path to the internet is through the proxy, which enforces the domain allow list.

## Secret Protection

Secrets exist only as environment variables inside the sandbox. The host-side store (`~/.cato/store.toml`) is invisible — it's in the home directory, which the sandbox can't see.

Secret files (`.env`, `.pem`, `.key`) in the workspace are blocked by `deny_read` patterns at the kernel level.

## What Cato Does NOT Protect Against

- **Exfiltration via allowed channels**: If a process can reach `github.com`, it could push data there. Cato controls which domains are reachable, not what gets sent.
- **Environment variable sniffing**: Processes inside the sandbox can read injected env vars. This is by design — tools need secrets to authenticate.
- **Denial of service**: A process can consume CPU, memory, or disk within the workspace. Cato doesn't enforce resource limits.
- **Kernel exploits**: If the macOS kernel itself is compromised, sandbox enforcement can be bypassed. This is a platform trust boundary.
- **Non-HTTP protocols**: Raw TCP connections are not yet proxied (SOCKS5 planned). Git over SSH works via agent forwarding.

## Comparison

| Approach | Enforcement | Bypass |
|----------|------------|--------|
| Agent hooks (cooperative) | Agent chooses to call the hook | Agent can skip the hook entirely |
| File permissions (chmod) | OS-level for the user | Same user can change permissions |
| Container (Docker) | Kernel namespaces | Secure but heavy setup |
| **Cato sandbox** | **macOS Seatbelt (kernel)** | **Requires kernel exploit to bypass** |
