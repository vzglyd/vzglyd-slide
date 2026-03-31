# Contributing to vzglyd-slide

Thanks for your interest in contributing.

## What this crate is

`vzglyd-slide` defines the ABI contract between the vzglyd engine and compiled slides. Changes here affect every slide ever built against this crate — treat it accordingly.

## ABI discipline

Before changing any `extern "C"` function signature, exported type, or `#[repr(C)]` struct, read [ABI_POLICY.md](ABI_POLICY.md). Breaking changes require a major version bump and must be coordinated with the main `vzglyd` repo.

## Development

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt
```

Check the WASM target compiles cleanly:

```bash
cargo check --target wasm32-wasip1
```

## Pull requests

- Keep PRs focused — one concern per PR
- Update CHANGELOG.md under `[Unreleased]`
- ABI-breaking changes must be explicitly called out in the PR description

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
