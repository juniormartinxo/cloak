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
cloak doctor
```

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
