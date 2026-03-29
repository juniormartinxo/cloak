# Claude statusline per profile

When running `cloak profile create <name>`, `cloak` provisions these files in the Claude profile:

- `~/.config/cloak/profiles/<name>/claude/statusline-command.sh`
- `~/.config/cloak/profiles/<name>/claude/settings.json` (with `statusLine`)
- `~/.config/cloak/profiles/<name>/claude/usage-limits.json` (written later by the script, after Claude responses)

## Behavior

- If the script does not exist: create it.
- If the script matches the older generated template: update it to the current default.
- If `settings.json` does not exist: create it.
- If `settings.json` already contains `statusLine`: do not override it.

## What the default script does

- Reads JSON from stdin (sent by Claude Code).
- If `jq` is available, it tries to show model, context usage, and cost.
- When Claude provides `rate_limits`, it also persists the latest 5-hour / 7-day usage snapshot to
  `usage-limits.json` so `cloak profile limits <name>` can read it later.
- If `jq` is unavailable, it prints a simple `Claude` fallback line.

## Customization

You can customize per profile, for example:

- `~/.config/cloak/profiles/work/claude/statusline-command.sh`

To re-apply the template for an existing profile:

```bash
cloak profile create work
```

The same provisioning refresh also happens when you run `cloak exec claude`, `cloak login claude`,
or `cloak profile limits <name>`.

To re-apply for all profiles:

```bash
for p in $(cloak profile list); do
  cloak profile create "$p"
done
```
