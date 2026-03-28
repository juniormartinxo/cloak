# Troubleshooting

## `cloak: command not found`

Install globally:

```bash
cd /home/junior/apps/jm/cloak
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

## Editor opens with the wrong account intermittently

This usually happens with GUI apps like VS Code or Cursor when their CLI reuses an existing app
instance that was already logged into another account.

When `cloak exec cursor ...` is run from an interactive terminal, `cloak` now launches Cursor as a
detached child process instead of replacing the shell process with `exec(2)`. That prevents the
terminal from staying blocked until the editor window closes.

Configure the editor with per-profile launch isolation:

```toml
[cli.cursor]
binary = "cursor"
launch_args = ["--user-data-dir", "{profile_dir}", "--extensions-dir", "{profile_dir}/extensions", "--new-window"]

[cli.cursor.extra_env]
CURSOR_USER_DATA_DIR = "{profile_dir}"
CURSOR_EXTENSIONS_DIR = "{profile_dir}/extensions"
```

For VS Code, use the same `launch_args` pattern with `binary = "code"`.

On WSL, if `cursor` resolves to the Windows wrapper path
(`/mnt/c/.../cursor/resources/app/bin/cursor`), `cloak` now tries to launch `Cursor.exe` directly
instead. In that mode it keeps `--user-data-dir` for profile isolation and drops
`--extensions-dir`, so installed extensions remain available while `User/globalStorage` moves into
the profile-specific directory.

If `Cursor.exe` cannot be launched directly for some reason, and you still see:

```text
Ignoring option 'user-data-dir': not supported for cursor.
Ignoring option 'extensions-dir': not supported for cursor.
```

then the Cursor GUI state is still falling back to the global Windows profile.

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
