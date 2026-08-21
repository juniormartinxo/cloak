# Development guide

## Requirements

- Rust toolchain (`cargo`/`rustc`); the current checkout is verified with Rust 1.93.1

## Build and local execution

```bash
cargo run -- --help
cargo run -- doctor
```

## Tests

```bash
cargo test
cargo test --test exec_integration -- --nocapture
cargo test --test backup_integration -- --nocapture
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

- Unit tests in `src/*.rs` for parsing, profile resolution, MCP registry/probes, permissions,
  backup selection, manifests, and path rewriting.
- Integration tests in `tests/exec_integration.rs` validating:
  - profile env wiring in `exec`
  - API key variable removal
  - `default_profile` fallback
  - logical path (`PWD`) resolution
  - native MCP install and catalog-based MCP add flows
- Integration tests in `tests/backup_integration.rs` validating:
  - real GPG-encrypted backup and restore when the system tool is available
  - identity/overwrite guards, merge behavior, and path rewriting
  - secure permissions and cleanup of partial artifacts on failure
