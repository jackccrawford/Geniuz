# Contributing to Clawmark

Thank you for your interest in contributing. Clawmark is a small project with a clear scope — persistent memory for AI agents. Contributions that improve reliability, platform coverage, and install experience are especially welcome.

## How to contribute

1. **Fork the repo** and create a branch from `main`
2. **Make your changes** — keep commits focused and well-described
3. **Test on your platform** — if you're fixing a platform issue, confirm it works on that platform
4. **Submit a PR** — describe what you changed and why

## What we look for

- **Bug fixes** — especially platform compatibility (glibc, ORT linking, install paths)
- **Install experience** — the first 60 seconds matter
- **Documentation** — clear, concise, no hype
- **Performance** — measured, not assumed

## What we avoid

- Adding runtime dependencies (clawmark is a single binary + SQLite)
- Cloud features or API keys — clawmark runs fully offline
- Breaking the CLI interface — `signal`, `tune`, `capture` are stable
- Over-engineering — if the fix is one line, the PR should be one line

## Code style

- Rust, edition 2021
- No `unsafe` unless absolutely necessary
- Error handling: `Result<T, String>` for simplicity (we know, we know)
- Comments explain *why*, not *what*

## Build

```bash
cargo build --release
cargo test
```

On Linux with system ORT:
```bash
ORT_LIB_LOCATION=/usr/local/lib ORT_PREFER_DYNAMIC_LINK=1 cargo build --release
```

## License

By contributing, you agree that your contributions will be licensed under MIT.
