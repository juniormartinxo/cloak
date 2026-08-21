# Overview and usage

`cloak` isolates credentials per directory for LLM CLIs (for example `claude` and `codex`).

## How it works

1. Resolve the active profile from the current directory.
2. Set the CLI config-home environment variable to that profile directory.
3. Remove conflicting environment variables (global API keys).
4. Execute the real binary via `exec`.

## Install

Global installation from this repository:

```bash
cd cloak
cargo install --path . --force
```

Validation:

```bash
which cloak
cloak --help
```

## Quick workflow

```bash
# 1) Create profiles
cloak profile create work
cloak profile create personal

# 2) Bind a repository to a profile
cd ~/repos/company-api
cloak use work

# 3) Authenticate in that profile context
cloak login claude work
cloak login codex work
cloak login gemini work

# 4) Inspect current context
cloak profile show
cloak profile account work
cloak limits work
cloak limits rank
cloak doctor
```

## Install MCP servers in a profile

Use `cloak mcp install` when you want the MCP configuration to land inside a specific `cloak`
profile instead of the CLI's global home.

Supported native installers today:

- `codex`: translated to `codex mcp add ...`
- `claude`: translated to `claude mcp add ...`
- unsupported CLIs: fail with a clear error

Examples:

```bash
# Codex stdio MCP in one profile
cloak mcp install codex filesystem --profile work -- npx @modelcontextprotocol/server-filesystem /tmp

# Codex HTTP MCP with bearer-token env var
cloak mcp install codex sentry --profile work --transport http --url https://example.com/mcp --bearer-token-env-var SENTRY_TOKEN

# Claude HTTP MCP with headers
cloak mcp install claude sentry --profile work --transport http --url https://mcp.sentry.dev/mcp -H "Authorization: Bearer token"

# Install the same MCP in every existing profile
cloak mcp install codex filesystem --all-profiles -- npx @modelcontextprotocol/server-filesystem /tmp
```

If you omit both `--profile` and `--all-profiles` in an interactive terminal, `cloak` resolves the
current profile first and then asks whether you want to apply the install to all profiles.

### Built-in MCP catalog

For common servers, prefer the registry-backed command:

```bash
# list the current catalog
cloak mcp add

# inspect the native commands without installing
cloak mcp add gitnexus --show

# install for selected CLIs and one profile
cloak mcp add gitnexus --for codex,claude --profile work --yes

# remove an existing registration before reinstalling
cloak mcp add filesystem --replace --profile work --yes
```

The built-in registry currently covers reference servers and popular integrations such as
`filesystem`, `git`, `memory`, `playwright`, `context7`, `gitnexus`, `github`, `shadcn`, and
`sentry`. Entries can expand environment variables plus `${CWD}` and `${HOME}`. Installation
stops with an explicit error when a required variable is missing.

The registry can also be extended without changing the binary. Add entries to
`~/.config/cloak/mcp_registry.toml`; user entries override built-in entries with the same name.

### Remove and diagnose MCP servers

`mcp remove` delegates to the native CLI and is idempotent: an absent server is reported as
`not installed` instead of failing the whole operation.

```bash
# preview one profile/CLI pair
cloak mcp remove filesystem --profile work --for codex --dry-run

# remove from every existing profile for supported CLIs
cloak mcp remove filesystem --all-profiles --yes
```

`mcp doctor` reads configured stdio MCPs from the selected Claude/Codex profiles and performs a
real JSON-RPC `initialize` handshake. HTTP/SSE entries are reported but are not spawned as stdio
processes.

```bash
cloak mcp doctor --profile work
cloak mcp doctor --all-profiles --name gitnexus --timeout 10 --with-tools
```

`--with-tools` sends `tools/list` after a successful initialization. A failed probe makes the
command exit with an error after all matching entries have been checked.

## Configure agent permissions

Run the guided questionnaire to maintain an `[agents.<name>]` policy in `config.toml`:

```bash
cloak permission ask --agent codex
cloak permission ask --agent claude
```

The questionnaire covers shell access, file writes, network access, explicit command allowlists,
and denylists. `cloak exec` checks the first forwarded command token before launching the agent:
explicit denies win, dangerous commands require an explicit allowlist entry, and a non-empty
allowlist rejects commands not listed. Starting an interactive agent without a forwarded command
has no command token to classify.

For Claude, saving the policy also synchronizes generated `allow` and `deny` rules to
`settings.json` in every existing Claude profile while preserving unrelated permission fields
such as `ask` and `defaultMode`. Other agents receive wrapper-level checks but no native settings
synchronization unless an adapter exists.

## Inspect authenticated accounts in a profile

Use this when you want to confirm which identity was captured inside a profile after logging in:

```bash
cloak profile account work
```

Typical output:

```text
Profile 'work'

Accounts
╭────────┬──────────────────────────────────────────────────────────────────────╮
│ CLI    ┆ Account                                                              │
╞════════╪══════════════════════════════════════════════════════════════════════╡
│ Claude ┆ credentials detected, but account identifier unavailable (plan: max) │
│ Codex  ┆ Jane Doe <jane@example.com>                                          │
│ Gemini ┆ Gem User <gem@example.com>                                           │
╰────────┴──────────────────────────────────────────────────────────────────────╯
```

How `cloak` detects this:

- `claude`: inspects `claude/.credentials.json`
- `codex`: inspects `codex/auth.json`
- `gemini`: inspects `gemini/.gemini/oauth_creds.json`, `gemini/.gemini/.env`, and
  `gemini/.gemini/settings.json`
- other configured CLIs: reports a generic "credentials detected" message when the profile
  directory is non-empty

This command only inspects local files inside `profiles/<name>/<cli>`; it does not contact any
remote API.

## Inspect usage limits

Use this when you want the latest local limit snapshots. If you omit the profile name, it will display the limits for **all** registered profiles:

```bash
# Inspect limits of all profiles
cloak limits

# Inspect limits of a specific profile
cloak limits work
```

By default, reset timestamps are displayed in UTC. Use `--utc` to convert them to a specific UTC
offset:

```bash
# Display resets in UTC-3 (e.g. Brasilia)
cloak limits work --utc -3

# Display resets in UTC+5
cloak limits work --utc 5
```

Typical output:

```text
Profile 'work'

Claude
  Status: usage snapshot available
  Details: plan: team, tier: default_raven
  Observed: 2026-03-28T18:12:44Z
  ╭───────────┬────────┬───────┬───────────┬─────────┬─────────────────────────╮
  │ Limit     ┆ Window ┆  Used ┆ Remaining ┆  Pacing ┆ Resets                  │
  ╞═══════════╪════════╪═══════╪═══════════╪═════════╪═════════════════════════╡
  │ five_hour ┆ 5h     ┆ 12.5% ┆     87.5% ┆ 18.2%/h ┆ 2026-03-28 17:42:39 UTC │
  │ seven_day ┆ 1w     ┆   37% ┆       63% ┆ 12.4%/d ┆ 2026-04-03 13:36:17 UTC │
  ╰───────────┴────────┴───────┴───────────┴─────────┴─────────────────────────╯

Codex
  Status: usage snapshot available
  Details: plan: team
  Observed: 2026-03-28T15:23:12.299Z
  Limit: Codex Team
  ╭───────────┬────────┬──────┬───────────┬─────────┬─────────────────────────╮
  │ Limit     ┆ Window ┆ Used ┆ Remaining ┆  Pacing ┆ Resets                  │
  ╞═══════════╪════════╪══════╪═══════════╪═════════╪═════════════════════════╡
  │ primary   ┆ 5h     ┆   1% ┆       99% ┆ 20.6%/h ┆ 2026-03-28 17:42:39 UTC │
  │ secondary ┆ 1w     ┆  30% ┆       70% ┆ 13.8%/d ┆ 2026-04-03 13:36:17 UTC │
  ╰───────────┴────────┴──────┴───────────┴─────────┴─────────────────────────╯
```

How the snapshots are sourced:

- `claude`: reads `profiles/<name>/claude/usage-limits.json`, which is written by the default
  Claude statusline script after Claude receives at least one response in that profile.
- `codex`: reads the newest `token_count` event under `profiles/<name>/codex/sessions` and uses
  the `rate_limits` payload persisted by the Codex CLI.

Refresh guidance:

- `claude`: if no snapshot exists yet, or a window shows `expired *`, open or continue Claude in
  that profile and wait for a response. The statusline writes the next `usage-limits.json`
  snapshot automatically; no separate `/usage` step is required.
- `codex`: if no snapshot exists yet, or a window shows `expired *`, open or continue Codex in
  that profile. `cloak limits` will pick up the next `token_count` snapshot written under
  `codex/sessions`; no separate `/status` step is required.

## Rank usage limits across profiles

To see which profile has the highest percentage of weekly limit left for a given AI, use:

```bash
cloak limits rank
```

This command queries all your local snapshots and presents a descending list of available weekly limits (the 7-day window) grouped by AI, helping you decide which profile to balance usage towards.

Ranking behavior:

- rows now include a `Snapshot` column
- `fresh` means the weekly snapshot is still valid
- `expired` means the weekly snapshot has already rolled over; the row is kept for visibility, but
  it is sorted after fresh snapshots
- expired rows still show `expired *` in the `Resets` column, plus a CLI-specific hint below the
  table explaining how to capture a fresh snapshot

## Change profile for a repository

Inside the repository:

```bash
cloak use personal
```

Note: `cloak init <profile>` is still available as a compatibility alias.

## Optional shell aliases

Without aliases, call `cloak exec` explicitly:

```bash
cloak exec claude
cloak exec codex
cloak exec codex --profile work
```

With aliases:

```bash
alias claude='cloak exec claude'
alias codex='cloak exec codex'
alias gemini='cloak exec gemini'
```

With these aliases, `claude`, `codex`, and `gemini` automatically run through `cloak`.

When needed, `cloak exec` also accepts an explicit profile:

```bash
cloak exec codex --profile work
cloak exec codex --profile work -- --model gpt-5.4
```

Pass `--profile <name>` before forwarded CLI args. Use `--` to forward a flag like `--profile`
to the target CLI itself.

If the explicit profile does not exist, `cloak` shows the existing profiles and asks whether it
should create the requested one. If you answer `no`, it exits cleanly without running the target
CLI.

Visual example of execution with an explicit profile:

![Demonstration of cloak running Claude with isolated profiles](../sources/images/cloak_claude.jpg)

## Backup and restore

Use `cloak backup` to generate an encrypted artifact with the configuration and knowledge of one
or more profiles, and `cloak restore` to bring that artifact back on another machine (or the same
one, after a reinstall).

```text
cloak backup  [--profile <name>] [--output <dir>] [--include-credentials] [--dry-run]
cloak restore <archive> [--profile <name>] [--force] [--dry-run] [--no-rewrite-paths]
```

Without `--profile`, `cloak backup` includes **all** profiles in a single artifact, and
`cloak restore` restores every profile present in the artifact.

> **Warning: keep the passphrase safe.** The artifact is always encrypted with
> `gpg --symmetric` (AES-256). If you lose the passphrase, **the backup becomes unrecoverable** —
> there is no recovery path. Store the passphrase in a password manager.

### Example workflow

```bash
# see what would go into the backup, without writing any file
cloak backup --dry-run

# back up all profiles
cloak backup --output /path/to/destination

# restore on the new machine
cloak restore /path/to/cloak-backup-20260725-122130.tar.gz.gpg
```

### What goes into the backup

Selection uses an allowlist, not a full copy of the profile. It includes:

- `settings.json`, `keybindings.json`, top-level `*.md` files, and the full `skills/` and
  `.agents/` directories;
- a profile-root `.cloak` file when present;
- for `claude`: `statusline-command.sh`, `plans/`, project memories under
  `projects/*/memory/`, and the plugin manifests (`plugins/installed_plugins.json`,
  `plugins/known_marketplaces.json`, `plugins/blocklist.json`);
- for `codex`: `config.toml`, `hooks.json`, and the `memories/` directory.
- the global Cloak `config.toml` and a versioned `manifest.json` at the artifact root.

Sessions, logs, caches, downloaded plugins, and project history are left out. On real profiles
this typically shrinks several GB down to a few MB.

On every backup, `cloak` lists what it found in the profile that did **not** make it into the
artifact — this exists because an allowlist, by nature, omits the unknown, and the report ensures
an omission surfaces before it turns into data loss. A directory that is entirely outside the
allowlist shows up as a single line with the aggregate size; a partially-covered directory
reports each file that was left out.

### Credentials

`claude/.credentials.json` and `codex/auth.json` are **excluded from the backup by default**.
They are OAuth tokens that expire and can be regenerated in minutes with `cloak login` on the
destination machine. Use `--include-credentials` to include them explicitly — in that case, a
leak of the artifact together with the passphrase grants direct access to the accounts, so weigh
the risk before using this flag.

### Configuration in `config.toml`

The optional `[backup]` block in `~/.config/cloak/config.toml` controls destination and upload:

```toml
[backup]
output_dir = "/mnt/c/Users/junior/OneDrive/cloak-backups"
upload_command = "rclone copy {archive} gdrive:cloak/"
include = []
```

- `output_dir`: default output directory. The final destination is resolved in this priority
  order: `--output` on the command line, then `output_dir` from `config.toml`, then the built-in
  default `~/.config/cloak/backups`.
- `upload_command`: command run after generating the artifact, with `{archive}` substituted for
  the generated file's path (with safe quoting, so paths containing spaces work without manual
  escaping).
- `include`: additional file/directory patterns that **add** to the built-in allowlist — they do
  not remove any default entry.

The artifact name follows the pattern `cloak-backup-<YYYYMMDD-HHMMSS>.tar.gz.gpg` and is created
with `0600` permissions. Encryption is written to a `.partial` path first and renamed only after
GPG succeeds, so an interrupted run does not leave a truncated artifact under the final name.

### Non-interactive use (cron/CI)

By default, the encryption passphrase is requested via `pinentry`. To run `cloak backup` without
interaction (for example, in a cron job or a CI pipeline), set the `CLOAK_BACKUP_PASSPHRASE`
environment variable — with it set, `gpg` runs in non-interactive mode.

### Restoring a backup

`cloak restore <archive>` decrypts the artifact, validates the manifest, and verifies identity
(uid and OAuth account) **before** writing anything to the destination. Key points:

- It refuses to overwrite an existing profile; pass `--force` to allow it.
- If the identity recorded in the manifest cannot be verified, restore also requires `--force` —
  the failure mode is safe by default, never silent.
- A backup with a newer `format_version` is rejected even with `--force`; update `cloak` before
  restoring it.
- **It is a merge, not a replacement**: nothing in the destination profile is deleted. Files that
  already exist at the destination and are not present in the artifact are preserved, and restore
  explicitly lists those preserved files at the end.
- It rewrites absolute paths from the origin machine (the source `$HOME` and the source profiles
  root) inside `.json`, `.toml`, `.md`, and `.sh` files. Use `--no-rewrite-paths` to disable this
  rewriting.
- At the end, it reports what was not part of the backup and will be rebuilt automatically by the
  CLIs on first run (plugins and marketplaces). The manifest also records the Claude MCP names
  detected in `.claude.json` so they can be reconciled manually.
- The global Cloak `config.toml` is included in the archive as reference, but the current restore
  command only merges `profiles/`; it does not replace the destination's global config.
- Use `--dry-run` to see the restore plan without touching the destination.

### System dependencies

`cloak backup` and `cloak restore` depend on the `tar`, `gzip`, and `gpg` binaries being on
`PATH`. Run `cloak doctor` to check: it has a "Backup Tools" section that reports the presence of
all three and, for `gpg`, confirms it can actually encrypt a test file (not just that the binary
exists).
