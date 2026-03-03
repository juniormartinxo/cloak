# Internal architecture

## Modules

- `src/main.rs`: entrypoint, command dispatch, and main flows.
- `src/cli.rs`: argument and subcommand definitions (`clap`).
- `src/config.rs`: `config.toml` loading/bootstrap and validation.
- `src/profile.rs`: `.cloak` resolution and local profile-file handling.
- `src/paths.rs`: XDG path helpers, permission helpers, and name validation.
- `src/exec.rs`: environment preparation and target CLI execution.
- `src/doctor.rs`: health checks (binaries, profiles, credential hints).

## `exec` flow

1. Load global config.
2. Resolve profile via `.cloak` (or default fallback).
3. Look up CLI in `config.cli`.
4. Ensure `profiles/<profile>/<cli>` exists.
5. Set CLI home env var (`config_dir_env`) to that path.
6. Remove env vars listed in `remove_env_vars`.
7. Execute the real binary (`exec` on Unix).

## Current directory resolution

`main.rs` prefers logical `PWD` when it resolves to the same real path as `current_dir()`.
This preserves expected behavior with symlinks and worktrees.

## Security model

- Profile directories and subdirs: `0700` on Unix.
- Sensitive files created by `cloak`: `0600` on Unix.
- `cloak` does not implement OAuth storage itself; it only isolates CLI homes.
