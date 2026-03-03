# Development guide

## Requirements

- Rust toolchain (`cargo`/`rustc`)

## Build and local execution

```bash
cargo run -- --help
cargo run -- doctor
```

## Tests

```bash
cargo test
cargo test --test exec_integration -- --nocapture
```

## Quality checks

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Update global install during development

```bash
cargo install --path . --force
```

## Current testing strategy

- Unit tests in `src/*.rs` for parsing and profile resolution.
- Integration tests in `tests/exec_integration.rs` validating:
  - profile env wiring in `exec`
  - API key variable removal
  - `default_profile` fallback
  - logical path (`PWD`) resolution
