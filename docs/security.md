# Security Model

## Enforcement by Platform

Cato's enforcement varies by platform. Not everything is kernel-enforced on all platforms.

### macOS

Uses `sandbox-exec` (Seatbelt framework). All restrictions — filesystem, network, deny patterns — are enforced by the macOS kernel. No process can bypass it regardless of language, permissions, or technique.

### Linux

Uses bubblewrap (bwrap) for namespace isolation + socat bridge for network. Enforcement depends on the feature:

| Feature | Enforcement | Bypassable? |
|---------|------------|-------------|
| Filesystem isolation (workspace only writable) | Kernel (mount namespace) | No |
| Home directory invisible | Kernel (tmpfs overlay) | No |
| deny_read on existing files | Kernel (mount /dev/null) | No |
| deny_read on NEW files created during session | **LD_PRELOAD (libc-level)** | **Yes** — by Go binaries, statically-linked tools, raw syscalls, io_uring |
| deny_write on existing files | Kernel (ro-bind mount) | No |
| Network full block (empty list) | Kernel (--unshare-net) | No |
| Network domain filtering | Kernel (--unshare-net) + socat bridge + proxy | No |
| .cato.toml protection | Kernel (ro-bind mount) | No |

**The LD_PRELOAD gap on Linux:** When an agent creates a NEW file matching a deny_read pattern during a session (e.g., creates `new.env`), only the LD_PRELOAD interceptor blocks reading it. This catches Python, Node, Ruby, shell, and most tools that use libc — but NOT Go binaries (statically linked by default), raw syscalls, or io_uring. The README and this document only call something "kernel-enforced" when it actually is.

For existing files, there is no gap — bwrap mount namespaces provide real kernel enforcement on both platforms.

## Deny-Default Architecture

### macOS

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

### Linux

Bubblewrap creates isolated namespaces. Only explicitly bind-mounted paths are visible:

| Allowed | What | Why |
|---------|------|-----|
| Workspace (bind mount) | Project directory | Normal work |
| /tmp (tmpfs or bind) | Temporary files | Build tools, scripts |
| System dirs (ro-bind) | /usr, /lib, /bin, /etc | Shell, tools, libraries |
| Shell config (ro-bind) | ~/.bashrc, ~/.profile | Shell initialization |
| /proc, /dev | Process info, devices | Required for operation |

Everything else doesn't exist — the mount namespace simply doesn't contain it.

## Filesystem Protection

### deny_read

**macOS:** Patterns in `deny_read` are enforced with Seatbelt `(deny file-read-data ...)` rules. The kernel blocks reads to ANY file matching the pattern, whether it existed before or was created during the session.

**Linux:** Existing files matching `deny_read` are hidden by mounting `/dev/null` over them (kernel enforcement). New files created during the session are intercepted by LD_PRELOAD at the libc level (safety net, not security boundary).

```bash
# Both platforms:
$ cat .env
cat: .env: Operation not permitted

$ python3 -c "open('.env').read()"
PermissionError: [Errno 1] Operation not permitted: '.env'
```

### deny_read implies write protection

Files matching `deny_read` patterns are also protected from overwrites. On macOS via `(deny file-write-data ...)`. On Linux via `--ro-bind` for existing files. This prevents an agent from wiping host secrets even if it can't read them.

### Home directory

Invisible on both platforms. macOS: Seatbelt deny-default. Linux: tmpfs overlay.

### Config protection

`.cato.toml` is read-only inside the sandbox on both platforms. macOS: Seatbelt deny. Linux: `--ro-bind`.

## Network Protection

When `network` domains are configured:

**macOS:**
1. Kernel blocks all outbound except localhost (Seatbelt)
2. Local proxy filters by domain
3. SNI validated against CONNECT host (prevents domain fronting)

**Linux:**
1. Kernel blocks all outbound (`--unshare-net` removes network interfaces)
2. Socat bridge provides only path out via Unix socket
3. Local proxy filters by domain
4. SNI validated against CONNECT host

Both platforms: even if a process ignores proxy env vars, the kernel blocks direct connections. The only path out is through the proxy.

### SNI Validation

The proxy inspects the TLS ClientHello to extract the SNI (Server Name Indication) and validates it matches the CONNECT host. This prevents domain fronting attacks where a process connects via an allowed domain (e.g., `github.com`) but sends traffic to an unauthorized domain on a shared CDN.

## Secret Store Protection

`~/.cato/store.toml` (contains API keys and secrets) is:
- Owner-only permissions: 0600 (read/write only by you)
- Inside `~/.cato/` directory with 0700 permissions
- Invisible from inside the sandbox (home directory not mounted)

The audit log (`~/.cato/audit.jsonl`) is similarly restricted to 0600.

## What Cato Does NOT Protect Against

- **Exfiltration via allowed channels**: If a process can reach `github.com`, it could push data there. Cato controls which domains are reachable, not what gets sent.
- **Environment variable sniffing**: Processes inside the sandbox can read injected env vars. This is by design — tools need secrets to authenticate.
- **Denial of service**: A process can consume CPU, memory, or disk within the workspace. Cato doesn't enforce resource limits.
- **Kernel exploits**: If the OS kernel is compromised, sandbox enforcement can be bypassed.
- **New files matching deny_read on Linux**: Only libc-level enforcement (LD_PRELOAD). Go, static binaries, and raw syscalls bypass this. Existing files are kernel-protected.
- **LD_PRELOAD bypasses on Linux**: `openat2`, `creat`, `io_uring`, `renameat`, inline `syscall(SYS_*)` are not intercepted.

## Comparison

| Approach | Enforcement | Bypass |
|----------|------------|--------|
| Agent hooks (cooperative) | Agent chooses to call the hook | Agent can skip the hook entirely |
| File permissions (chmod) | OS-level for the user | Same user can change permissions |
| Container (Docker) | Kernel namespaces | Secure but heavy setup |
| **Cato (macOS)** | **Seatbelt (kernel)** | **Requires kernel exploit** |
| **Cato (Linux)** | **bwrap (kernel) + LD_PRELOAD (libc)** | **Kernel for existing files/network. Libc-only for new file deny patterns.** |
