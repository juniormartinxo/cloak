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

## `doctor` shows `no credential file detected`

Usually this means you have not authenticated in that profile yet.

Authenticate in the profile context:

```bash
cloak login claude <profile>
cloak login codex <profile>
cloak login gemini <profile>
```

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
