use std::{fs, process::Command};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use color_eyre::eyre::{eyre, Context, Result};

use crate::{config::Config, paths};

pub fn exec_cli(cli_name: &str, profile: &str, args: &[String], config: &Config) -> Result<()> {
    let cli_cfg = config
        .cli
        .get(cli_name)
        .ok_or_else(|| eyre!("CLI '{}' not configured in config.toml", cli_name))?;

    let binary = which::which(&cli_cfg.binary).wrap_err_with(|| {
        format!(
            "'{}' not found in PATH. Install it or set cli.{}.binary in config.",
            cli_cfg.binary, cli_name
        )
    })?;

    let profile_dir = paths::profile_cli_dir(profile, cli_name)?;
    ensure_profile_cli_dir(&profile_dir, profile, cli_name)?;

    let mut cmd = Command::new(binary);
    cmd.args(args);
    cmd.env(&cli_cfg.config_dir_env, &profile_dir);

    for var in &cli_cfg.remove_env_vars {
        cmd.env_remove(var);
    }

    #[cfg(unix)]
    {
        let err = cmd.exec();
        Err(eyre!("exec failed: {}", err))
    }

    #[cfg(not(unix))]
    {
        let status = cmd.status().wrap_err("failed running child process")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn ensure_profile_cli_dir(path: &std::path::Path, profile: &str, cli_name: &str) -> Result<()> {
    let existed_before = path.exists();

    if let Some(profile_dir) = path.parent() {
        paths::ensure_secure_dir(profile_dir)?;
    }

    paths::ensure_secure_dir(path)?;

    if !existed_before {
        eprintln!(
            "Profile '{}' initialized for '{}' at {}",
            profile,
            cli_name,
            display_path(path)
        );
    }

    Ok(())
}

fn display_path(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(home) {
            return format!("~/{}", stripped.display());
        }
    }

    path.display().to_string()
}

#[allow(dead_code)]
fn _is_profile_dir_empty(path: &std::path::Path) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }

    let mut entries =
        fs::read_dir(path).wrap_err_with(|| format!("failed reading {}", path.display()))?;
    Ok(entries.next().is_none())
}
