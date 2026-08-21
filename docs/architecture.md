# Internal architecture

## Modules

- `src/main.rs`: entrypoint, command dispatch, and main flows.
- `src/cli.rs`: argument and subcommand definitions (`clap`).
- `src/account.rs`: local credential-file inspection for `profile account`.
- `src/backup.rs`: allowlisted collection, manifest, GPG archive orchestration, and safe restore.
- `src/config.rs`: `config.toml` loading/bootstrap and validation.
- `src/profile.rs`: `.cloak` resolution and local profile-file handling.
- `src/paths.rs`: XDG path helpers, permission helpers, and name validation.
- `src/exec.rs`: environment preparation and target CLI execution.
- `src/mcp.rs`: per-CLI native MCP install/remove adapters.
- `src/mcp_registry.rs`: built-in/user catalog loading and entry resolution.
- `src/mcp_doctor.rs`: MCP config parsing and stdio JSON-RPC probes.
- `src/doctor.rs`: health checks (binaries, profiles, credential hints, backup tools).

## `exec` flow

1. Load global config.
2. Resolve profile via `.cloak` (or default fallback).
3. Look up CLI in `config.cli`.
4. Ensure `profiles/<profile>/<cli>` exists.
5. Set CLI home env var (`config_dir_env`) to that path.
6. Remove env vars listed in `remove_env_vars`.
7. Enforce `[agents.<cli>]` against the first forwarded command token, when one exists.
8. Execute the real binary (`exec` on Unix).

## Current directory resolution

`main.rs` prefers logical `PWD` when it resolves to the same real path as `current_dir()`.
This preserves expected behavior with symlinks and worktrees.

## `profile account` flow

1. Validate the requested profile name.
2. Ensure `profiles/<profile>` exists.
3. Iterate configured CLI names from `config.cli`.
4. Inspect each CLI-specific home directory.
5. Print either an identified account, a credential-presence hint, or `not authenticated`.

Current CLI-specific detectors:

- `claude`: `.credentials.json`
- `codex`: `auth.json` (including decoded JWT claims from `id_token`)
- `gemini`: `gemini/.gemini/oauth_creds.json`, `gemini/.gemini/.env`,
  `gemini/.gemini/settings.json`
- other CLIs: generic non-empty-directory detection

## MCP lifecycle

### `mcp add`

1. Load the registry compiled from `resources/mcp_registry.toml`.
2. Merge `~/.config/cloak/mcp_registry.toml`, with user entries winning by name.
3. Resolve target CLIs, profile scope, transport, environment placeholders, and commands.
4. Optionally preview with `--show` or remove the existing entry with `--replace`.
5. Delegate installation to the native adapter in `mcp.rs` for each CLI/profile pair.

### `mcp install`

1. Resolve the requested profile, or the current-directory profile if `--profile` was omitted.
2. In interactive terminals, ask whether the install should target all profiles when
   `--all-profiles` was not passed.
3. Validate the MCP request shape against the selected transport.
4. Translate the request to the target CLI's native MCP syntax.
5. Run the target CLI inside each selected profile home so the MCP config is written per profile.

### `mcp remove` and `mcp doctor`

- Removal reads the per-profile native config first. Missing registrations are skipped, making the
  operation idempotent; present registrations are removed through the native CLI.
- Doctor parses Codex `config.toml` and Claude `.claude.json`. Stdio entries are spawned and receive
  JSON-RPC `initialize`; remote transports are reported as skipped. `--with-tools` adds a
  `tools/list` request after initialization.

## Permission policy flow

1. `permission ask` loads the current `[agents.<name>]` policy.
2. The questionnaire updates shell, file-write, network, allowlist, and denylist fields.
3. `config.rs` validates and writes `config.toml` with `0600` permissions.
4. `exec.rs` classifies the first forwarded command and enforces explicit denies, capability
   categories, dangerous-command opt-in, and non-empty allowlists before launching the CLI.
5. For Claude, the generated `allow`/`deny` rules are synchronized to every existing profile's
   `settings.json`; unrelated fields such as `ask` and `defaultMode` are preserved.

An interactive launch without a forwarded command has no command token to classify. Native
settings synchronization is currently Claude-specific; wrapper enforcement applies to every
enabled CLI.

## Backup and restore flow

### Backup

1. Select one profile or all existing profiles.
2. Collect only built-in/user-allowlisted files and build an aggregated uncovered report.
3. Add global `config.toml` and a versioned manifest with origin, profile, OAuth-hint, MCP, and
   uncovered metadata.
4. Create `tar.gz` in a private `0700` staging directory.
5. Encrypt with GPG/AES-256 into a `.partial` file, set `0600`, and atomically rename it to the
   final artifact name.
6. Run the optional quoted `upload_command` after the local artifact is complete.

### Restore

1. Decrypt and extract into a private staging directory.
2. Parse the manifest and reject unsupported future format versions.
3. Check destination uid, OAuth identity hints, requested profiles, and overwrite conditions.
4. Optionally rewrite source home/profile roots in supported text files.
5. Merge files into the destination with `0700` directories, `0700` for files that were
   executable at the source and `0600` for the rest; never delete destination-only files, and
   report them as preserved.
6. Write the archived global `config.toml` to `config.toml.from-backup` as a reference, with path
   rewriting already applied. The `config.toml` in use is never overwritten or merged into.

## Security model

- Profile directories and subdirs: `0700` on Unix.
- Sensitive files created by `cloak`: `0600` on Unix.
- Decrypted backup staging is private and temporary; final backup artifacts are always encrypted.
- OAuth credentials remain owned by the target CLIs. They are excluded from backups unless the
  user explicitly passes `--include-credentials`.
