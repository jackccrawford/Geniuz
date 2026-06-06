# Contributing to Geniuz

Thanks for your interest in contributing to Geniuz, the memory and configuration substrate for agent-native work.

## How to contribute

### Reporting bugs

Open an issue at https://github.com/jackccrawford/Geniuz/issues. Include:
- What you were trying to do
- What happened instead
- The Geniuz version (`geniuz --version`)
- Your platform (macOS, Linux distro, Windows)
- Steps to reproduce

### Suggesting changes

Open an issue describing the change before opening a PR. Geniuz is a small substrate by design. Feature additions are evaluated against whether they keep the substrate small or make it bigger. Most ideas are better as external tooling that uses Geniuz than as features inside it.

### Submitting code

1. Fork the repo
2. Create a branch off `main`
3. Make your change
4. Add tests if applicable
5. Ensure `cargo test` passes
6. Open a PR with a clear description of what changed and why

## What we look for in PRs

- **Substrate hygiene.** Geniuz's records table is intentionally minimal. PRs that add columns, change schema, or alter triggers need to clear a high bar.
- **Backward compatibility.** Existing stations and existing CLI behavior should keep working. If a change must break compatibility, it needs a migration path.
- **Tests.** New behavior gets new tests. Changed behavior gets updated tests.
- **Documentation.** User-facing changes update the README and any affected guides.
- **No dependencies for taste.** Adding a dependency is a real decision. The substrate should run on as little as possible.

## Code of conduct

Be the kind of person you'd want to collaborate with. Geniuz is built by people who think about agents as collaborators rather than tools, and we extend that to the humans contributing here.

Hostile, demeaning, or off-topic comments will be moderated. Persistent bad-faith engagement will be blocked.

## License

Contributions are accepted under the MIT license (see LICENSE). By submitting a PR you confirm that you have the right to license your contribution under MIT.

## Geniuz Team

If you're interested in the paid tier (perpetual agents, team chassis, customer-environment deployment), Geniuz Team is built and operated by [mVara](https://github.com/mvara-ai). Contact through the link in the main README for pilot and enterprise inquiries.
