# Troubleshooting

## `cloak: command not found`

Install globally:

```bash
cd cloak
cargo install --path . --force
```

Ensure PATH:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

## `CLI '<name>' not configured in config.toml`

Add a `[cli.<name>]` block to `~/.config/cloak/config.toml`.

## `"<binary>" not found in PATH`

- install the target CLI binary, or
- set `binary = "/absolute/path/to/binary"` in `config.toml`.

## Wrong profile is being resolved

```bash
cloak profile show
```

Check whether a parent directory contains a `.cloak` that is taking priority.

## `cloak exec cursor` (or another custom CLI) is temporarily disabled

Adding a `[cli.cursor]`, `[cli.vscode]`, or another custom block does not enable profile execution.
The current compiled allowlist contains only `claude`, `codex`, and `gemini`, so the expected error
is:

```text
profile management for CLI 'cursor' is temporarily disabled; enabled CLIs: claude, codex, gemini
```

This is a product boundary, not a malformed `config.toml`. The config schema and execution layer
retain editor-oriented `launch_args`, `extra_env`, detached launch, and WSL helpers, but those paths
cannot be reached until the CLI is enabled in the implementation.

## `doctor` shows `no credential file detected`

Usually this means you have not authenticated in that profile yet.

Authenticate in the profile context:

```bash
cloak login claude <profile>
cloak login codex <profile>
cloak login gemini <profile>
```

## `cloak profile account <profile>` shows `not authenticated`

That means `cloak` did not find any supported local credential files in that profile's CLI
directory.

Check:

- whether you logged in through `cloak login <cli> <profile>` or `cloak exec <cli> --profile <profile>`
- whether the CLI actually writes credentials inside the configured home directory
- whether the CLI name exists under `[cli.<name>]` in `config.toml`

Then re-run:

```bash
cloak profile account <profile>
```

## `cloak profile account <profile>` says the CLI is not yet supported

This is the fallback for configured CLIs that have files in their profile directory but do not yet
have parser logic in `src/account.rs`.

The profile isolation still works for `cloak exec`; only the account-identification message is
generic.

## `cloak login gemini <profile>` fails with `illegal access` (Snap)

Common symptoms:

- `starting express`
- `SNAP env is defined, updater is disabled`
- `illegal access`
- `snap-confine ... cap_dac_override not found`

This usually happens when Gemini is installed via Snap and runs with confinement restrictions that conflict with `GEMINI_CLI_HOME` profile isolation.

Recommended fix:

```bash
# 1) Remove snap package
sudo snap remove gemini

# 2) Install Gemini CLI outside snap (example: npm)
npm install -g @google/gemini-cli

# 3) Validate binary
which gemini
gemini --version
```

Then set an explicit binary path in `~/.config/cloak/config.toml`:

```toml
[cli.gemini]
binary = "/absolute/path/to/gemini"
config_dir_env = "GEMINI_CLI_HOME"
remove_env_vars = ["GEMINI_API_KEY", "GOOGLE_API_KEY"]
```

Finally retry:

```bash
cloak login gemini <profile>
```

## Profile existed before statusline feature

Re-apply safely:

```bash
cloak profile create <profile>
```

It will not overwrite an existing `statusLine` entry.

## Config created before Gemini support

Run:

```bash
cloak doctor
```

If `gemini` (or another recommended CLI block) is missing, `doctor` offers an optional migration prompt to append the default block.

## `mcp doctor` reports a failed handshake

Start with the exact profile and server so the output stays focused:

```bash
cloak mcp doctor --profile <profile> --name <server> --timeout 10 --with-tools
```

For stdio servers, check the captured stderr, confirm the configured command is on `PATH`, and
verify every required environment variable in the profile. Remote HTTP/SSE entries are currently
reported as skipped because `mcp doctor` only performs active probes for stdio transports.

If the registration itself is stale, preview an idempotent re-install:

```bash
cloak mcp add <server> --profile <profile> --show
cloak mcp add <server> --profile <profile> --replace --yes
```

## Backup or restore cannot find `tar`, `gzip`, or `gpg`

Run:

```bash
cloak doctor
```

The `Backup Tools` section checks all three binaries and performs a real test encryption with GPG.
Install or repair the missing tool before retrying; the presence of the `gpg` executable alone is
not considered sufficient.

## GPG rejects the backup passphrase

Interactive runs use GPG `pinentry`. Non-interactive runs must provide the same passphrase through
`CLOAK_BACKUP_PASSPHRASE` for both backup and restore. A failed backup removes its `.partial`
output, so do not treat a missing final artifact as data loss from a previously completed backup.

## Restore requires `--force`

`--force` is required when a destination profile already exists or when uid/OAuth identity cannot
be verified. It permits a merge and bypasses identity checks, but does not delete destination-only
files.

Do not use `--force` to work around this error:

```text
este artefato usa o formato de backup vN e este cloak suporta ate vM
```

A newer `format_version` is never bypassed. Update `cloak` to a version that supports the artifact.
