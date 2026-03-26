use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{eyre, Context, Result};
use serde::Deserialize;

use crate::paths;

pub const RECOMMENDED_CLI_NAMES: [&str; 3] = ["claude", "codex", "gemini"];

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    #[serde(default)]
    pub cli: HashMap<String, CliConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneralConfig {
    pub default_profile: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CliConfig {
    pub binary: String,
    pub config_dir_env: String,

    #[serde(default)]
    pub remove_env_vars: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
    pub created: bool,
}

const DEFAULT_CLAUDE_BLOCK: &str = r#"[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"
remove_env_vars = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]"#;

const DEFAULT_CODEX_BLOCK: &str = r#"[cli.codex]
binary = "codex"
config_dir_env = "CODEX_HOME"
remove_env_vars = ["OPENAI_API_KEY"]"#;

const DEFAULT_GEMINI_BLOCK: &str = r#"[cli.gemini]
binary = "gemini"
config_dir_env = "GEMINI_CLI_HOME"
remove_env_vars = ["GEMINI_API_KEY", "GOOGLE_API_KEY"]"#;

const DEFAULT_CONFIG_TOML: &str = r#"[general]
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
"#;

pub fn load_config_from_path(path: &Path) -> Result<Config> {
    let raw = fs::read_to_string(path)
        .wrap_err_with(|| format!("failed reading config file {}", path.display()))?;
    parse_config_str(&raw, path)
}

pub fn missing_recommended_cli_names(config: &Config) -> Vec<String> {
    let mut missing = Vec::new();

    for cli_name in RECOMMENDED_CLI_NAMES {
        if !config.cli.contains_key(cli_name) {
            missing.push(cli_name.to_string());
        }
    }

    missing
}

pub fn append_default_cli_blocks(config_path: &Path, cli_names: &[String]) -> Result<Vec<String>> {
    if cli_names.is_empty() {
        return Ok(Vec::new());
    }

    let mut raw = fs::read_to_string(config_path)
        .wrap_err_with(|| format!("failed reading config file {}", config_path.display()))?;

    if !raw.ends_with('\n') {
        raw.push('\n');
    }

    let mut appended = Vec::new();

    for cli_name in cli_names {
        let Some(block) = default_cli_block(cli_name) else {
            continue;
        };

        raw.push('\n');
        raw.push_str(block);
        raw.push('\n');
        appended.push(cli_name.clone());
    }

    if appended.is_empty() {
        return Ok(appended);
    }

    fs::write(config_path, raw)
        .wrap_err_with(|| format!("failed writing config file {}", config_path.display()))?;
    paths::set_owner_only_file(config_path)?;

    // Re-parse to ensure the written file is still valid.
    let _ = load_config_from_path(config_path)?;

    Ok(appended)
}

fn default_cli_block(cli_name: &str) -> Option<&'static str> {
    match cli_name {
        "claude" => Some(DEFAULT_CLAUDE_BLOCK),
        "codex" => Some(DEFAULT_CODEX_BLOCK),
        "gemini" => Some(DEFAULT_GEMINI_BLOCK),
        _ => None,
    }
}

pub fn load_or_create_config() -> Result<LoadedConfig> {
    let config_path = paths::config_file_path()?;

    if !config_path.exists() {
        create_default_config(&config_path)?;
        let config = parse_config_str(DEFAULT_CONFIG_TOML, &config_path)?;
        return Ok(LoadedConfig {
            config,
            path: config_path,
            created: true,
        });
    }

    let config = load_config_from_path(&config_path)?;

    Ok(LoadedConfig {
        config,
        path: config_path,
        created: false,
    })
}

fn create_default_config(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| eyre!("invalid config path: {}", path.display()))?;

    paths::ensure_secure_dir(parent)?;
    fs::write(path, DEFAULT_CONFIG_TOML)
        .wrap_err_with(|| format!("failed writing default config to {}", path.display()))?;

    #[cfg(unix)]
    paths::set_owner_only_file(path)?;

    Ok(())
}

fn parse_config_str(raw: &str, path: &Path) -> Result<Config> {
    let parsed: Config =
        toml::from_str(raw).wrap_err_with(|| format!("invalid config at {}", path.display()))?;

    if parsed.general.default_profile.trim().is_empty() {
        return Err(eyre!("general.default_profile cannot be empty"));
    }

    if parsed.cli.is_empty() {
        return Err(eyre!("at least one [cli.<name>] entry is required"));
    }

    for cli_name in parsed.cli.keys() {
        paths::validate_cli_name(cli_name).wrap_err_with(|| {
            format!(
                "invalid CLI entry name '{}' in {}",
                cli_name,
                path.display()
            )
        })?;
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::{
        append_default_cli_blocks, load_config_from_path, missing_recommended_cli_names,
        parse_config_str, DEFAULT_CONFIG_TOML,
    };

    #[test]
    fn test_parse_config_default_is_valid() {
        let parsed = parse_config_str(DEFAULT_CONFIG_TOML, Path::new("config.toml"))
            .expect("default config must parse");
        assert!(parsed.cli.contains_key("claude"));
        assert!(parsed.cli.contains_key("codex"));
        assert!(parsed.cli.contains_key("gemini"));
    }

    #[test]
    fn test_parse_config_rejects_empty_default_profile() {
        let raw = r#"
[general]
default_profile = ""

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"
"#;

        let err = parse_config_str(raw, Path::new("config.toml")).expect_err("must fail");
        assert!(
            err.to_string().contains("default_profile cannot be empty"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_config_rejects_missing_cli_entries() {
        let raw = r#"
[general]
default_profile = "personal"
"#;

        let err = parse_config_str(raw, Path::new("config.toml")).expect_err("must fail");
        assert!(
            err.to_string()
                .contains("at least one [cli.<name>] entry is required"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_parse_config_rejects_invalid_cli_entry_name() {
        let raw = r#"
[general]
default_profile = "personal"

[cli."../../etc"]
binary = "bad"
config_dir_env = "BAD_HOME"
"#;

        let err = parse_config_str(raw, Path::new("config.toml")).expect_err("must fail");
        assert!(
            err.to_string()
                .contains("invalid CLI entry name '../../etc'"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_missing_recommended_cli_names_detects_absent_gemini() {
        let raw = r#"
[general]
default_profile = "personal"

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"

[cli.codex]
binary = "codex"
config_dir_env = "CODEX_HOME"
"#;

        let parsed = parse_config_str(raw, Path::new("config.toml")).expect("parse");
        let missing = missing_recommended_cli_names(&parsed);
        assert_eq!(missing, vec!["gemini"]);
    }

    #[test]
    fn test_append_default_cli_blocks_adds_gemini_block() {
        let tmp = tempdir().expect("tempdir");
        let config_path = tmp.path().join("config.toml");
        let base = r#"
[general]
default_profile = "personal"

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"

[cli.codex]
binary = "codex"
config_dir_env = "CODEX_HOME"
"#;
        fs::write(&config_path, base).expect("write config");

        let appended = append_default_cli_blocks(&config_path, &[String::from("gemini")])
            .expect("append blocks");
        assert_eq!(appended, vec!["gemini"]);

        let reloaded = load_config_from_path(&config_path).expect("reload config");
        assert!(reloaded.cli.contains_key("gemini"));
    }
}
