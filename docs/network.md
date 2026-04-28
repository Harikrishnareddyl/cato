# Network Filtering

## How It Works

When your `.cato.toml` has a non-empty `network` list, Cato enforces domain-based network restrictions using a two-layer approach:

### Layer 1: Kernel (Seatbelt)

The macOS kernel blocks all outbound network connections except to localhost. This is enforced by the Seatbelt profile — no process inside the sandbox can bypass it, regardless of language, permissions, or technique.

### Layer 2: Proxy

A local HTTP CONNECT proxy runs on localhost. Tools inside the sandbox see `http_proxy` and `https_proxy` environment variables pointing to this proxy. The proxy only tunnels connections to domains in your allow list. Requests to any other domain get a `403 Forbidden`.

```
Process inside sandbox
  → curl https://github.com
  → Routes through proxy (https_proxy env var)
  → Proxy checks: "github.com" in allow list? Yes
  → Proxy establishes tunnel to github.com:443
  → Connection succeeds

Process inside sandbox
  → curl https://evil.com
  → Routes through proxy
  → Proxy checks: "evil.com" in allow list? No
  → Proxy returns 403 Forbidden
  → Connection fails
```

Even if a process ignores the proxy environment variables and tries to connect directly, the kernel blocks all non-localhost outbound traffic. There's no way out except through the proxy.

## Configuration

```toml
[sandbox]
network = [
    "github.com",
    "api.anthropic.com",
    "registry.npmjs.org",
    "*.github.com",        # wildcard: matches any subdomain
]
```

### Empty list = unrestricted

```toml
network = []
```

No proxy started, all outbound traffic allowed.

### Wildcards

`*.example.com` matches `api.example.com`, `sub.example.com`, and `example.com` itself.

### Localhost

Connections to `localhost`, `127.0.0.1`, and `::1` always bypass the proxy (via `no_proxy` env var). This means local dev servers, databases, and other localhost services work without being listed in the network allow list.

## What's Covered

| Traffic Type | Handled |
|-------------|---------|
| HTTPS (curl, npm, pip, etc.) | Yes — via HTTP CONNECT proxy |
| HTTP | Yes — via HTTP proxy |
| Git over HTTPS | Yes |
| Git over SSH | Partially — SSH agent forwarding works, but raw SSH is not proxied |
| Raw TCP | Not yet — SOCKS5 proxy planned for future |
| DNS | Handled by proxy (sandbox DNS is blocked) |
