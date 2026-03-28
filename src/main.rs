mod account;
mod cli;
mod config;
mod doctor;
mod exec;
mod paths;
mod profile;

use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use clap_complete::generate;
use color_eyre::eyre::{eyre, Context, Result};
use serde_json::{json, Value};

use crate::{
    account::{inspect_profile_accounts, AccountStatus},
    cli::{Cli, Commands, ProfileCommands},
    profile::{ProfileSource, ResolvedProfile},
};

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    if let Commands::Completions { shell } = &cli.command {
        let mut cmd = crate::cli::command_for_completions();
        generate(shell.clone(), &mut cmd, "cloak", &mut io::stdout());
        return Ok(());
    }

    let loaded = config::load_or_create_config()?;

    match cli.command {
        Commands::Exec { cli, profile, args } => {
            let selected_profile = match profile {
                Some(name) => {
                    paths::validate_profile_name(&name)?;
                    name
                }
                None => {
                    let cwd = current_dir()?;
                    profile::resolve_profile(&cwd, &loaded.config.general.default_profile)?.name
                }
            };

            exec::exec_cli(&cli, &selected_profile, &args, &loaded.config)?;
        }
        Commands::Use {
            profile: profile_name,
        } => {
            paths::validate_profile_name(&profile_name)?;

            let cwd = current_dir()?;
            let cloak_path = cwd.join(".cloak");

            if cloak_path.exists()
                && !confirm(&format!(
                    "{} already exists. Overwrite?",
                    cloak_path.display()
                ))?
            {
                println!("Aborted");
                return Ok(());
            }

            if !profile_exists(&profile_name)? {
                if confirm(&format!(
                    "Profile '{}' does not exist yet. Create it now?",
                    profile_name
                ))? {
                    create_profile(&profile_name, &loaded.config)?;
                } else {
                    return Err(eyre!(
                        "profile '{}' does not exist. Run: cloak profile create {}",
                        profile_name,
                        profile_name
                    ));
                }
            }

            let path = profile::write_cloak_file(&cwd, &profile_name)?;
            println!(
                "Created {} with profile \"{}\"",
                display_path(&path),
                profile_name
            );
        }
        Commands::Profile(sub) => match sub {
            ProfileCommands::List => {
                let names = list_profiles()?;
                if names.is_empty() {
                    println!("No profiles found. Run: cloak profile create <name>");
                } else {
                    for name in names {
                        println!("{}", name);
                    }
                }
            }
            ProfileCommands::Account { name } => {
                show_profile_accounts(&name, &loaded.config)?;
            }
            ProfileCommands::Create { name } => {
                create_profile(&name, &loaded.config)?;
            }
            ProfileCommands::Delete { name, yes } => {
                delete_profile(&name, yes, &loaded)?;
            }
            ProfileCommands::Show => {
                let cwd = current_dir()?;
                let resolved =
                    profile::resolve_profile(&cwd, &loaded.config.general.default_profile)?;
                show_profile(&resolved, &loaded.config)?;
            }
        },
        Commands::Login {
            cli,
            profile: explicit_profile,
        } => {
            let selected_profile = match explicit_profile {
                Some(name) => {
                    paths::validate_profile_name(&name)?;
                    name
                }
                None => {
                    let cwd = current_dir()?;
                    profile::resolve_profile(&cwd, &loaded.config.general.default_profile)?.name
                }
            };

            exec::exec_cli(&cli, &selected_profile, &[], &loaded.config)?;
        }
        Commands::Doctor => {
            let mut config_for_doctor = loaded.config.clone();
            let missing = config::missing_recommended_cli_names(&config_for_doctor);

            if !missing.is_empty() {
                println!(
                    "Missing recommended CLI config blocks in {}: {}",
                    display_path(&loaded.path),
                    missing.join(", ")
                );

                if is_interactive_terminal() {
                    if confirm(&format!(
                        "Append defaults for missing CLI entries ({})?",
                        missing.join(", ")
                    ))? {
                        let added = config::append_default_cli_blocks(&loaded.path, &missing)?;
                        if !added.is_empty() {
                            println!("Added default config for: {}", added.join(", "));
                            config_for_doctor = config::load_config_from_path(&loaded.path)?;
                        }
                    } else {
                        println!("Skipped optional config migration.");
                    }
                } else {
                    println!(
                        "Non-interactive terminal detected. Skipping optional migration prompt."
                    );
                }
            }

            doctor::run_doctor(&config_for_doctor, &loaded.path, loaded.created)?;
        }
        Commands::Completions { .. } => unreachable!("handled before config load"),
    }

    Ok(())
}

fn create_profile(name: &str, cfg: &config::Config) -> Result<()> {
    paths::validate_profile_name(name)?;

    let profile_dir = paths::profile_dir(name)?;
    let existed = profile_dir.exists();

    paths::ensure_secure_dir(&profile_dir)?;

    let mut cli_names: Vec<&String> = cfg.cli.keys().collect();
    cli_names.sort();

    for cli_name in cli_names {
        let cli_dir = profile_dir.join(cli_name);
        paths::ensure_secure_dir(&cli_dir)?;
    }

    let statusline_result = provision_default_claude_statusline(&profile_dir, cfg)?;

    if existed {
        println!(
            "Profile '{}' already exists at {}",
            name,
            display_path(&profile_dir)
        );
    } else {
        println!(
            "Profile '{}' created at {}",
            name,
            display_path(&profile_dir)
        );
        println!("Run `cloak login <cli> {}` to authenticate.", name);
    }

    if statusline_result.script_created {
        println!(
            "Claude statusline script created at {}",
            display_path(&statusline_result.script_path)
        );
    }

    if statusline_result.settings_updated {
        println!(
            "Claude settings updated at {}",
            display_path(&statusline_result.settings_path)
        );
    }

    Ok(())
}

fn delete_profile(name: &str, yes: bool, loaded: &config::LoadedConfig) -> Result<()> {
    paths::validate_profile_name(name)?;

    let profile_dir = paths::profile_dir(name)?;
    if !profile_dir.exists() {
        return Err(eyre!("profile '{}' does not exist", name));
    }

    if !yes
        && !confirm(&format!(
            "Delete profile '{}' at {}?",
            name,
            profile_dir.display()
        ))?
    {
        println!("Aborted");
        return Ok(());
    }

    let is_default = loaded.config.general.default_profile == name;

    if is_default {
        let all = list_profiles()?;
        let remaining: Vec<String> = all.into_iter().filter(|n| n != name).collect();

        if !remaining.is_empty() {
            let new_default = if yes || !is_interactive_terminal() {
                remaining[0].clone()
            } else {
                pick_new_default_profile(&remaining)?
            };

            config::update_default_profile(&loaded.path, &new_default)?;
            println!(
                "Default profile updated: '{}' -> '{}'",
                name, new_default
            );
        } else {
            println!(
                "Warning: '{}' is your default profile and no other profiles exist.",
                name
            );
            println!(
                "  After deletion, create a new profile and update default_profile in {}",
                display_path(&loaded.path)
            );
        }
    }

    fs::remove_dir_all(&profile_dir)
        .wrap_err_with(|| format!("failed deleting {}", profile_dir.display()))?;

    println!("Profile '{}' deleted", name);
    println!(
        "Note: .cloak files in project directories may still reference '{}'.",
        name
    );
    println!("  Run `cloak use <profile>` in those directories to update them.");
    Ok(())
}

fn pick_new_default_profile(remaining: &[String]) -> Result<String> {
    println!("Choose a new default profile:");
    for (i, name) in remaining.iter().enumerate() {
        println!("  [{}] {}", i + 1, name);
    }
    print!("Enter number (1-{}): ", remaining.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice: usize = input
        .trim()
        .parse()
        .map_err(|_| eyre!("invalid selection"))?;

    if choice < 1 || choice > remaining.len() {
        return Err(eyre!("selection out of range"));
    }

    Ok(remaining[choice - 1].clone())
}

fn show_profile(resolved: &ResolvedProfile, cfg: &config::Config) -> Result<()> {
    match &resolved.source {
        ProfileSource::CloakFile(path) => {
            println!("Profile: {} (from {})", resolved.name, display_path(path));
        }
        ProfileSource::DefaultProfile => {
            println!("Profile: {} (fallback to default)", resolved.name);
        }
    }

    let mut cli_names: Vec<&String> = cfg.cli.keys().collect();
    cli_names.sort();

    for cli_name in cli_names {
        let cli_cfg = &cfg.cli[cli_name];
        let cli_dir = paths::profile_cli_dir(&resolved.name, cli_name)?;
        println!("{} -> profile_dir={}", cli_name, display_path(&cli_dir));

        if let Some(config_dir_env) = &cli_cfg.config_dir_env {
            println!(
                "{} -> {}={}",
                cli_name,
                config_dir_env,
                display_path(&cli_dir)
            );
        }

        let mut extra_env: Vec<_> = cli_cfg.extra_env.iter().collect();
        extra_env.sort_by(|a, b| a.0.cmp(b.0));
        for (name, value) in extra_env {
            println!(
                "{} -> {}={}",
                cli_name,
                name,
                exec::render_template(
                    value,
                    &exec::TemplateContext {
                        cli_name,
                        profile: &resolved.name,
                        profile_dir: &cli_dir,
                    },
                )
            );
        }

        let resolved_binary =
            which::which(&cli_cfg.binary).unwrap_or_else(|_| std::path::PathBuf::from(&cli_cfg.binary));
        if let Some(agent_folder) =
            exec::resolve_remote_agent_folder(cli_name, &resolved_binary, &cli_dir)
        {
            println!(
                "{} -> VSCODE_AGENT_FOLDER={}",
                cli_name,
                display_path(&agent_folder)
            );
        }

        if !cli_cfg.launch_args.is_empty() {
            let rendered_args = cli_cfg
                .launch_args
                .iter()
                .map(|arg| {
                    exec::render_template(
                        arg,
                        &exec::TemplateContext {
                            cli_name,
                            profile: &resolved.name,
                            profile_dir: &cli_dir,
                        },
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            println!("{} -> launch_args={}", cli_name, rendered_args);
        }
    }

    Ok(())
}

fn show_profile_accounts(profile: &str, cfg: &config::Config) -> Result<()> {
    paths::validate_profile_name(profile)?;

    if !profile_exists(profile)? {
        return Err(eyre!("profile '{}' does not exist", profile));
    }

    println!("Profile '{}'", profile);

    for account in inspect_profile_accounts(profile, cfg)? {
        match account.status {
            AccountStatus::Identified { display } => {
                println!("{} -> {}", account.cli_name, display);
            }
            AccountStatus::CredentialsPresent { detail } => {
                println!("{} -> {}", account.cli_name, detail);
            }
            AccountStatus::NoCredentials => {
                println!("{} -> not authenticated", account.cli_name);
            }
        }
    }

    Ok(())
}

fn list_profiles() -> Result<Vec<String>> {
    let root = paths::profiles_dir()?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut profiles = Vec::new();
    for entry in
        fs::read_dir(&root).wrap_err_with(|| format!("failed reading {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                profiles.push(name.to_string());
            }
        }
    }

    profiles.sort();
    Ok(profiles)
}

fn profile_exists(name: &str) -> Result<bool> {
    Ok(paths::profile_dir(name)?.is_dir())
}

fn current_dir() -> Result<PathBuf> {
    let physical = std::env::current_dir().wrap_err("failed to read current directory")?;

    if let Some(pwd_os) = std::env::var_os("PWD") {
        let pwd = PathBuf::from(pwd_os);
        if pwd.is_absolute() {
            let pwd_real = fs::canonicalize(&pwd).ok();
            let physical_real = fs::canonicalize(&physical).ok();

            if pwd_real.is_some() && pwd_real == physical_real {
                return Ok(pwd);
            }
        }
    }

    Ok(physical)
}

fn confirm(question: &str) -> Result<bool> {
    print!("{} [y/N]: ", question);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let decision = input.trim().to_ascii_lowercase();
    Ok(matches!(decision.as_str(), "y" | "yes"))
}

fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(home) {
            return format!("~/{}", stripped.display());
        }
    }

    path.display().to_string()
}

struct StatuslineProvisionResult {
    script_created: bool,
    settings_updated: bool,
    script_path: PathBuf,
    settings_path: PathBuf,
}

fn provision_default_claude_statusline(
    profile_dir: &Path,
    cfg: &config::Config,
) -> Result<StatuslineProvisionResult> {
    let claude_cfg_present = cfg.cli.contains_key("claude");
    let claude_dir = profile_dir.join("claude");
    let script_path = claude_dir.join("statusline-command.sh");
    let settings_path = claude_dir.join("settings.json");

    #[cfg(not(unix))]
    {
        return Ok(StatuslineProvisionResult {
            script_created: false,
            settings_updated: false,
            script_path,
            settings_path,
        });
    }

    if !claude_cfg_present || !claude_dir.is_dir() {
        return Ok(StatuslineProvisionResult {
            script_created: false,
            settings_updated: false,
            script_path,
            settings_path,
        });
    }

    let mut script_created = false;
    if !script_path.exists() {
        fs::write(&script_path, default_claude_statusline_script()).wrap_err_with(|| {
            format!(
                "failed to write Claude statusline script at {}",
                script_path.display()
            )
        })?;

        set_script_permissions_owner_only(&script_path)?;
        script_created = true;
    }

    let mut settings: Value = if settings_path.exists() {
        let raw = fs::read_to_string(&settings_path)
            .wrap_err_with(|| format!("failed reading {}", settings_path.display()))?;
        serde_json::from_str(&raw)
            .wrap_err_with(|| format!("invalid JSON at {}", settings_path.display()))?
    } else {
        json!({})
    };

    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| eyre!("{} must contain a JSON object", settings_path.display()))?;

    let needs_statusline = settings_obj
        .get("statusLine")
        .map(|value| value.is_null())
        .unwrap_or(true);

    let mut settings_updated = false;
    if needs_statusline {
        settings_obj.insert(
            "statusLine".to_string(),
            json!({
                "type": "command",
                "command": default_claude_statusline_command(&script_path),
            }),
        );

        let serialized = serde_json::to_string_pretty(&settings)
            .wrap_err("failed serializing Claude settings")?;
        fs::write(&settings_path, format!("{serialized}\n"))
            .wrap_err_with(|| format!("failed writing {}", settings_path.display()))?;
        paths::set_owner_only_file(&settings_path)?;
        settings_updated = true;
    }

    Ok(StatuslineProvisionResult {
        script_created,
        settings_updated,
        script_path,
        settings_path,
    })
}

fn default_claude_statusline_script() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail

input="$(cat)"

if command -v jq >/dev/null 2>&1; then
  line="$(printf '%s' "$input" | jq -r '
    def firststr($arr): ($arr | map(select(type=="string" and . != "")) | .[0] // "");
    def firstnum($arr): ($arr | map(select(type=="number")) | .[0] // null);

    {
      model: firststr([.model.display_name?, .model.name?, .model?]),
      context: firstnum([.context_window.percent_used?, .contextWindow.percentUsed?, .context_usage.percent?]),
      cost: firstnum([.cost.session_usd?, .cost.session?, .session.cost_usd?]),
      cwd: firststr([.workspace.current_dir?, .cwd?, .working_directory?])
    } | "\(.model)\t\(.context // "")\t\(.cost // "")\t\(.cwd)"
  ')"

  IFS=$'\t' read -r model context cost cwd <<< "$line"
  if [ -z "${model:-}" ]; then
    model="Claude"
  fi

  out="$model"
  if [ -n "${context:-}" ]; then
    out="$out | ctx ${context}%"
  fi
  if [ -n "${cost:-}" ]; then
    out="$out | \$$cost"
  fi
  if [ -n "${cwd:-}" ]; then
    out="$out | $(basename "$cwd")"
  fi

  printf '%s\n' "$out"
  exit 0
fi

printf 'Claude\n'
"#
}

fn default_claude_statusline_command(script_path: &Path) -> String {
    #[cfg(unix)]
    {
        return format!("bash {}", shell_single_quote(script_path));
    }

    #[cfg(not(unix))]
    {
        let _ = script_path;
        String::new()
    }
}

fn shell_single_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\"'\"'"))
}

fn set_script_permissions_owner_only(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms)
            .wrap_err_with(|| format!("failed setting script permissions on {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::config::{CliConfig, Config, GeneralConfig};

    use super::provision_default_claude_statusline;

    #[test]
    fn test_provision_statusline_assets_for_claude_profile() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("work");
        fs::create_dir_all(profile_dir.join("claude")).expect("create claude dir");

        let mut cli_map = std::collections::HashMap::new();
        cli_map.insert(
            "claude".to_string(),
            CliConfig {
                binary: "claude".to_string(),
                config_dir_env: Some("CLAUDE_CONFIG_DIR".to_string()),
                remove_env_vars: vec![],
                extra_env: Default::default(),
                launch_args: vec![],
            },
        );

        let cfg = Config {
            general: GeneralConfig {
                default_profile: "personal".to_string(),
            },
            cli: cli_map,
        };

        let result = provision_default_claude_statusline(&profile_dir, &cfg).expect("provision");
        assert!(result.script_created);
        assert!(result.settings_updated);

        let script = fs::read_to_string(profile_dir.join("claude/statusline-command.sh"))
            .expect("read script");
        assert!(script.contains("jq -r"));

        let settings =
            fs::read_to_string(profile_dir.join("claude/settings.json")).expect("read settings");
        assert!(settings.contains("\"statusLine\""));
        assert!(settings.contains("statusline-command.sh"));
    }

    #[test]
    fn test_provision_does_not_override_existing_statusline() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("work");
        let claude_dir = profile_dir.join("claude");
        fs::create_dir_all(&claude_dir).expect("create claude dir");
        fs::write(
            claude_dir.join("settings.json"),
            r#"{"statusLine":{"type":"command","command":"bash /tmp/custom.sh"}}"#,
        )
        .expect("write settings");

        let mut cli_map = std::collections::HashMap::new();
        cli_map.insert(
            "claude".to_string(),
            CliConfig {
                binary: "claude".to_string(),
                config_dir_env: Some("CLAUDE_CONFIG_DIR".to_string()),
                remove_env_vars: vec![],
                extra_env: Default::default(),
                launch_args: vec![],
            },
        );

        let cfg = Config {
            general: GeneralConfig {
                default_profile: "personal".to_string(),
            },
            cli: cli_map,
        };

        let result = provision_default_claude_statusline(&profile_dir, &cfg).expect("provision");
        assert!(!result.settings_updated);

        let settings =
            fs::read_to_string(profile_dir.join("claude/settings.json")).expect("read settings");
        assert!(settings.contains("/tmp/custom.sh"));
    }
}
