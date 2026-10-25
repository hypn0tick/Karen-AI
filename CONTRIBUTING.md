# Contributing to Karen-AI

**The Karen who insists on standards before she lets you near the code.**

Thank you for considering a contribution. Karen-AI is a guardrails engine — she enforces rules. That means we hold ourselves to the same standard. Yes, all of them. She checked twice.

---

## Quick Start

```bash
# Clone and build
git clone https://github.com/hypn0tick/Karen-AI
cd Karen-AI
cargo build --release

# Run the full quality suite (what CI runs)
cargo fmt --check
cargo clippy -- -D warnings
cargo test

# Run the binary manually
./target/release/karen validate
./target/release/karen check
```

---

## Development Workflow

1. **Fork** the repository (she will be watching yours too)
2. **Create a branch**: `git checkout -b feature/your-feature` or `fix/your-fix`
3. **Make changes** — follow the coding standards below
4. **Run quality checks** locally (see below)
5. **Commit** — karen will enforce commit message standards
6. **Push** and open a PR

---

## Coding Standards

### Rust

- **Edition 2024**, minimum Rust 1.85
- `cargo fmt` — formatting is non-negotiable
- `cargo clippy -- -D warnings` — zero warnings
- All public items must have doc comments
- Follow the existing Karen voice in doc comments: "The Karen who..."

### Commit Messages

karen enforces these via the `commit-attribution` gate:

- **No AI attribution** — no `Co-Authored-By: Claude`, no `🤖`, no `noreply@anthropic.com`
- **Conventional Commits** preferred: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
- **Imperative mood**: "Add feature" not "Added feature"
- **One logical change per commit**

### Code Style

- **Comments use backticks** for commands, types, keywords: `cargo`, `karen`, `fn`, `async`
- **No `unwrap()` in production code** — use `expect()` with context or proper error handling
- **No `todo!()` or `unimplemented!()`** in merged code
- **Error types use `thiserror`** — no `Box<dyn Error>` in public APIs

---

## Quality Gates (run locally before pushing)

```bash
# The full suite — mirrors CI exactly
cargo fmt --check
cargo clippy -- -D warnings
cargo test

# Quick validation
cargo build --release
./target/release/karen validate
./target/release/karen check
```

karen's `cargo-quality-gate`, `backticks-in-comments`, and `head-to-select-object` gates will deny pushes that don't pass. Yes, she gates her own repo. She told you she holds herself to the same standard.

---

## Pull Request Checklist

The Karen who checks your boxes, then adds three more.

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes (zero warnings)
- [ ] `cargo build --release` ran (e2e and integration tests exec this binary)
- [ ] `cargo test` passes (all 128 tests: 9 unit + 17 integration + 102 e2e, zero ignored)
- [ ] New code has doc comments with Karen voice
- [ ] Commit messages follow conventions (no AI attribution)
- [ ] Comments use backticks for code identifiers
- [ ] No new warnings from clippy or rustc
- [ ] PR description explains the change and why

---

## Architecture Overview

```
src/
├── main.rs              # CLI entry point, commands
├── lib.rs               # Library exports
├── adapters/            # Runtime protocol translation
├── engine/              # Dispatcher, injector, maintenance
├── policy/              # Loader, gate compiler, config
├── state/               # FileStore, MemoryStore, ReadOnlyStore
└── types.rs             # Shared types (Event, Action, Injection, etc.)

policies/
├── config.toml          # Engine config
├── rules/               # Always-on markdown rules (7 files)
└── gates/               # JSON/JSONC/JSON5 gates (7 files)
```

---

## Testing

```bash
# Full suite (what CI runs)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release        # e2e and integration tests exec this binary
cargo test                   # unit + integration + e2e

# E2E lane only
cargo test --test test_e2e

# Integration tests only
cargo test --test integration

# Specific test
cargo test karen_verifies_deny_protocol_shape
```

All 128 tests must pass (9 unit + 17 integration + 102 e2e — zero ignored). The Karen who counts, then counts again.

---

## Releases

Releases are manual (the Karen who trusts nobody else with the big red button):

```bash
# Update version in Cargo.toml
git tag v0.x.y
git push origin v0.x.y
```

GitHub Actions will build and publish binaries.

---

## Getting Help

- Check existing issues and PRs
- Read the README — it's thorough, and she made it that way on purpose
- Ask in discussions if unsure

---

## Code of Conduct

Be respectful. karen doesn't negotiate with agents; she expects the same from contributors.

---

*karen does not negotiate with agents. She otherwise gets along with them splendidly.*