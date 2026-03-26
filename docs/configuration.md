# Configuration and profile layout

## Global config file

Path:

- `~/.config/cloak/config.toml`

It is generated automatically on first use.

Default example:

```toml
[general]
default_profile = "personal"

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"
remove_env_vars = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]

[cli.codex]
binary = "codex"
config_dir_env = "CODEX_HOME"
remove_env_vars = ["OPENAI_API_KEY"]

[cli.gemini]
binary = "gemini"
config_dir_env = "GEMINI_CLI_HOME"
remove_env_vars = ["GEMINI_API_KEY", "GOOGLE_API_KEY"]
```

## Per-directory association (.cloak)

Repository root file:

```toml
profile = "work"
```

`cloak` walks up from the current directory to `/` and uses the closest `.cloak`.
If none is found, it uses `general.default_profile`.

## Profile directory layout

```text
~/.config/cloak/
├── config.toml
└── profiles/
    ├── work/
    │   ├── claude/
    │   ├── codex/
    │   └── gemini/
    └── personal/
        ├── claude/
        ├── codex/
        └── gemini/
```

## Add another CLI

Add to `config.toml`:

```toml
[cli.aider]
binary = "aider"
config_dir_env = "AIDER_CONFIG_HOME"
remove_env_vars = ["OPENAI_API_KEY"]
```

Then use it normally:

```bash
cloak exec aider
```

It will also appear in `cloak profile account <name>`. For CLIs without dedicated inspection logic,
the command falls back to a generic "credentials detected" message when the profile directory is
non-empty.

## Optional migration for existing configs

If your `config.toml` existed before a new recommended CLI block (for example `gemini`), run:

```bash
cloak doctor
```

`doctor` will detect missing recommended CLI blocks and, in an interactive terminal, ask whether it should append defaults automatically.

## Profile naming rules

Allowed:

- letters and numbers
- `-`, `_`, `.`

Rejected:

- empty name
- `.` or `..`
- containing `/` or `\\`
- starting with `-`
