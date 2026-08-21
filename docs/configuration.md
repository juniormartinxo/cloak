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
├── mcp_registry.toml        # optional user MCP catalog
├── backups/                # default encrypted backup destination
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

## Supported and custom CLI blocks

Profile management is currently enabled only for `claude`, `codex`, and `gemini`. A custom block
is valid configuration syntax:

```toml
[cli.aider]
binary = "aider"
config_dir_env = "AIDER_CONFIG_HOME"
remove_env_vars = ["OPENAI_API_KEY"]
```

but it does not enable execution by itself. Today this command fails with a clear
`profile management ... temporarily disabled` error:

```bash
cloak exec aider
```

Custom blocks can still appear in config/account inspection paths, but `exec`, `login`, raw MCP
execution, and profile-directory creation are restricted by the compiled allowlist. Extending
that allowlist is an implementation change, not a configuration-only operation.

## Agent permission policy

The default config includes a policy for Codex:

```toml
[agents.codex]
allow_shell = true
allow_file_write = true
allow_network = true
allowed_commands = []
deny_commands = []
```

Use `cloak permission ask --agent <name>` instead of editing this block manually. The command
validates command names and prevents the same command from ending up in both lists. A Claude policy
is also synchronized to the generated `permissions.allow` and `permissions.deny` arrays in every
existing Claude profile.

The `cloak exec` wrapper enforces the policy against the first forwarded command token: explicit
denies win; disabled shell/file/network categories block known commands; dangerous commands need
an explicit allowlist entry; and a non-empty `allowed_commands` list rejects commands not listed.
Launching an interactive agent without a forwarded command has no command token to classify.

Claude additionally receives synchronized native `settings.json` rules. Other agents still get
the wrapper-level enforcement, but no target-CLI settings synchronization unless an adapter exists.

## Backup configuration

The optional block below controls the default destination, a post-backup upload command, and
allowlist extensions:

```toml
[backup]
output_dir = "/path/to/cloak-backups"
upload_command = "rclone copy {archive} remote:cloak/"
include = ["extra/*.json", "custom-knowledge/"]
```

`--output` takes priority over `output_dir`; when neither is set, artifacts go to
`~/.config/cloak/backups`. `{archive}` is replaced with the safely quoted final artifact path.
Additional `include` patterns add files to the built-in allowlist and never remove defaults.

## User MCP registry

`resources/mcp_registry.toml` is compiled into the binary. Create
`~/.config/cloak/mcp_registry.toml` to add or override entries for `cloak mcp add`. Registry values
support environment-variable expansion plus `${CWD}` and `${HOME}`; missing variables are errors.

## Advanced launch config

`config_dir_env` is optional. You can also define:

- `launch_args`: arguments prepended before forwarded CLI args
- `[cli.<name>.extra_env]`: extra environment variables to inject

Supported placeholders:

- `{profile_dir}`
- `{profile_name}`
- `{cli_name}`

Example for VS Code/Cursor-style editors:

```toml
[cli.cursor]
binary = "cursor"
launch_args = ["--user-data-dir", "{profile_dir}", "--extensions-dir", "{profile_dir}/extensions", "--new-window"]

[cli.cursor.extra_env]
CURSOR_USER_DATA_DIR = "{profile_dir}"
CURSOR_EXTENSIONS_DIR = "{profile_dir}/extensions"

[cli.vscode]
binary = "code"
launch_args = ["--user-data-dir", "{profile_dir}", "--extensions-dir", "{profile_dir}/extensions", "--new-window"]
```

This shape avoids reusing a GUI instance that is already logged into a different account, but
`cursor` and `vscode` are not enabled by the current profile-management allowlist. The example
documents the supported schema and dormant editor launch path, not a currently callable CLI.

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
