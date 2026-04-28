# Contributing to Cato

## Quick start

```bash
git clone https://github.com/Harikrishnareddyl/cato.git
cd cato
cargo test
cargo build --release
```

## Making changes

1. Fork the repo
2. Create a branch (`git checkout -b my-fix`)
3. Make your changes
4. Run tests: `cargo test`
5. Commit with a clear message
6. Open a PR against `main`

## Code style

- Follow existing patterns — the codebase is small, read it first
- No `unsafe` code
- Keep dependencies minimal — only add one if truly needed

## What to contribute

- **Sandbox features** — network filtering improvements, SOCKS5 proxy, resource limits
- **Platform support** — Linux (bubblewrap/Landlock), other OS sandboxing
- **Seatbelt profiles** — better deny patterns, edge case handling
- **Documentation** — examples, guides, clarifications
- **Bug fixes** — especially around sandbox edge cases on different macOS versions

## Testing

```bash
cargo test                                    # unit tests
cato init && cato run -- echo "works"         # manual sandbox test
CATO_DEBUG=1 cato run -- echo "debug"         # inspect Seatbelt profile
```

## Release process

Releases are automated via GitHub Actions on tags:

```bash
git tag v0.X.Y
git push origin v0.X.Y
# → builds all platforms, publishes to npm/pip/crates.io/homebrew
```

## Code of Conduct

Be respectful. Focus on the work. No harassment.
