# Contributing to Cannon

Thanks for your interest in improving Cannon! Bug reports, ideas and pull
requests are all welcome.

## Getting started

```bash
git clone https://github.com/FelipeFelipeRenan/cannon-project.git
cd cannon-project
cargo build
cargo test
```

You only need a recent stable Rust toolchain.

## Before opening a pull request

CI runs the following on Linux, macOS and Windows; please run them locally:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --locked --doc
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Guidelines:

- Add or update tests with your change. `tests/e2e.rs` runs the real binary
  against local HTTP/TCP servers and is the best place for user-visible
  behaviour; use unit tests for internal logic.
- Public items need doc comments (`#![warn(missing_docs)]` is enabled).
- Update `README.md` and the `[Unreleased]` section of `CHANGELOG.md` for
  user-visible changes.
- Keep pull requests focused: one logical change each.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
  (`fix:`, `feat:`, `docs:`, `test:`, `build:`, ...).

## Performance changes

Cannon is a performance tool, so claims need evidence. For changes that touch
the hot path, include before/after numbers and the command, machine and
workload used. See [`docs/benchmarks/tcp-baseline.md`](docs/benchmarks/tcp-baseline.md)
for the reference methodology.

## Reporting bugs and security issues

Use the issue templates for bugs and feature requests. For security
vulnerabilities, follow [`SECURITY.md`](SECURITY.md) instead of opening a public
issue.
