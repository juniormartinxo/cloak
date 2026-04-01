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
cd /home/junior/apps/jm/cloak
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

## Rank usage limits across profiles

To see which profile has the highest percentage of weekly limit left for a given AI, use:

```bash
cloak limits rank
```

This command queries all your local snapshots and presents a descending list of available weekly limits (the 7-day window) grouped by AI, helping you decide which profile to balance usage towards.

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

Visual example of execution with an explicit profile:

![Demonstration of cloak running Claude with isolated profiles](../sources/images/cloak_claude.jpg)
