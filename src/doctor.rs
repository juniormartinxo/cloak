use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context, Result};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::{config::Config, paths};

pub fn run_doctor(config: &Config, config_path: &Path, config_created: bool) -> Result<()> {
    if config_created {
        println!(
            "{} Config: {} (created with defaults)",
            ok_mark(),
            config_path.display()
        );
    } else {
        println!("{} Config: {}", ok_mark(), config_path.display());
    }

    check_binaries(config);
    check_profiles(config)?;

    Ok(())
}

fn check_binaries(config: &Config) {
    let mut cli_names: Vec<&String> = config.cli.keys().collect();
    cli_names.sort();

    for cli_name in cli_names {
        let cli_cfg = &config.cli[cli_name];
        match which::which(&cli_cfg.binary) {
            Ok(path) => println!("{} {} found at {}", ok_mark(), cli_name, path.display()),
            Err(_) => println!(
                "{} {} binary '{}' not found in PATH",
                err_mark(),
                cli_name,
                cli_cfg.binary
            ),
        }
    }
}

fn check_profiles(config: &Config) -> Result<()> {
    let profiles_root = paths::profiles_dir()?;

    if !profiles_root.exists() {
        println!(
            "{} No profiles found. Run: cloak profile create <name>",
            err_mark()
        );
        return Ok(());
    }

    let mut profile_dirs = collect_dirs(&profiles_root)?;
    profile_dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    if profile_dirs.is_empty() {
        println!(
            "{} No profiles found. Run: cloak profile create <name>",
            err_mark()
        );
        return Ok(());
    }

    for profile_dir in profile_dirs {
        let profile_name = profile_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<invalid>");

        println!("{} Profile '{}'", info_mark(), profile_name);

        let mut cli_names: Vec<&String> = config.cli.keys().collect();
        cli_names.sort();

        for cli_name in cli_names {
            let cli_dir = profile_dir.join(cli_name);
            if !cli_dir.exists() {
                println!("  {} missing dir {}", err_mark(), cli_dir.display());
                continue;
            }

            println!("  {} dir {}", ok_mark(), cli_dir.display());

            if has_credentials_hint(cli_name, &cli_dir)? {
                println!("  {} credentials detected for {}", ok_mark(), cli_name);
            } else {
                println!(
                    "  {} no credential file detected for {}",
                    warn_mark(),
                    cli_name
                );
            }
        }
    }

    Ok(())
}

fn has_credentials_hint(cli_name: &str, cli_dir: &Path) -> Result<bool> {
    let present = match cli_name {
        "claude" => cli_dir.join(".credentials.json").exists(),
        "codex" => cli_dir.join("auth.json").exists(),
        "gemini" => gemini_credentials_hint(cli_dir)?,
        _ => {
            let mut entries = fs::read_dir(cli_dir)
                .wrap_err_with(|| format!("failed reading {}", cli_dir.display()))?;
            entries.next().is_some()
        }
    };

    Ok(present)
}

fn gemini_credentials_hint(cli_dir: &Path) -> Result<bool> {
    let gemini_home = cli_dir.join(".gemini");

    if gemini_home.join("oauth_creds.json").exists() {
        return Ok(true);
    }

    let env_file = gemini_home.join(".env");
    if env_file.exists() {
        let raw = fs::read_to_string(&env_file)
            .wrap_err_with(|| format!("failed reading {}", env_file.display()))?;
        let has_key = raw
            .lines()
            .map(str::trim)
            .any(|line| line.starts_with("GEMINI_API_KEY=") || line.starts_with("GOOGLE_API_KEY="));
        if has_key {
            return Ok(true);
        }
    }

    let settings_path = gemini_home.join("settings.json");
    if settings_path.exists() {
        let raw = fs::read_to_string(&settings_path)
            .wrap_err_with(|| format!("failed reading {}", settings_path.display()))?;
        let parsed: Value = serde_json::from_str(&raw)
            .wrap_err_with(|| format!("invalid JSON at {}", settings_path.display()))?;

        let selected_type = parsed
            .get("security")
            .and_then(|v| v.get("auth"))
            .and_then(|v| v.get("selectedType"))
            .and_then(Value::as_str)
            .or_else(|| parsed.get("selectedAuthType").and_then(Value::as_str));

        if selected_type
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn collect_dirs(path: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();

    for entry in
        fs::read_dir(path).wrap_err_with(|| format!("failed reading {}", path.display()))?
    {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            dirs.push(entry_path);
        }
    }

    Ok(dirs)
}

fn ok_mark() -> String {
    "✓".green().to_string()
}

fn err_mark() -> String {
    "✗".red().to_string()
}

fn warn_mark() -> String {
    "!".yellow().to_string()
}

fn info_mark() -> String {
    "•".blue().to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{collect_dirs, gemini_credentials_hint, has_credentials_hint};

    #[test]
    fn test_has_credentials_hint_detects_claude_credentials() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        fs::write(cli_dir.join(".credentials.json"), "{}").expect("write");

        assert!(has_credentials_hint("claude", cli_dir).expect("check"));
    }

    #[test]
    fn test_has_credentials_hint_reports_absent_claude_credentials() {
        let tmp = tempdir().expect("tempdir");

        assert!(!has_credentials_hint("claude", tmp.path()).expect("check"));
    }

    #[test]
    fn test_has_credentials_hint_detects_codex_auth() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        fs::write(cli_dir.join("auth.json"), "{}").expect("write");

        assert!(has_credentials_hint("codex", cli_dir).expect("check"));
    }

    #[test]
    fn test_has_credentials_hint_reports_absent_codex_auth() {
        let tmp = tempdir().expect("tempdir");

        assert!(!has_credentials_hint("codex", tmp.path()).expect("check"));
    }

    #[test]
    fn test_has_credentials_hint_detects_unknown_cli_with_files() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        fs::write(cli_dir.join("some-config.json"), "{}").expect("write");

        assert!(has_credentials_hint("aider", cli_dir).expect("check"));
    }

    #[test]
    fn test_has_credentials_hint_reports_empty_unknown_cli_dir() {
        let tmp = tempdir().expect("tempdir");

        assert!(!has_credentials_hint("aider", tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_detects_oauth_creds() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("mkdir");
        fs::write(gemini_home.join("oauth_creds.json"), "{}").expect("write");

        assert!(gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_detects_api_key_in_env() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("mkdir");
        fs::write(gemini_home.join(".env"), "GEMINI_API_KEY=secret\n").expect("write");

        assert!(gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_detects_google_api_key_in_env() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("mkdir");
        fs::write(gemini_home.join(".env"), "GOOGLE_API_KEY=secret\n").expect("write");

        assert!(gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_ignores_env_without_api_key() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("mkdir");
        fs::write(gemini_home.join(".env"), "OTHER_VAR=value\n").expect("write");

        assert!(!gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_detects_selected_auth_type_in_settings() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("mkdir");

        let settings = json!({
            "security": {
                "auth": {
                    "selectedType": "oauth"
                }
            }
        });
        fs::write(gemini_home.join("settings.json"), settings.to_string()).expect("write");

        assert!(gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_detects_legacy_selected_auth_type() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("mkdir");

        let settings = json!({ "selectedAuthType": "api_key" });
        fs::write(gemini_home.join("settings.json"), settings.to_string()).expect("write");

        assert!(gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_ignores_empty_selected_type() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("mkdir");

        let settings = json!({
            "security": {
                "auth": {
                    "selectedType": "  "
                }
            }
        });
        fs::write(gemini_home.join("settings.json"), settings.to_string()).expect("write");

        assert!(!gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_gemini_credentials_hint_reports_no_credentials_when_gemini_dir_is_absent() {
        let tmp = tempdir().expect("tempdir");

        assert!(!gemini_credentials_hint(tmp.path()).expect("check"));
    }

    #[test]
    fn test_collect_dirs_returns_only_directories() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();

        fs::create_dir(root.join("alpha")).expect("mkdir");
        fs::create_dir(root.join("beta")).expect("mkdir");
        fs::write(root.join("file.txt"), "data").expect("write");

        let mut dirs = collect_dirs(root).expect("collect");
        dirs.sort();

        let names: Vec<&str> = dirs
            .iter()
            .filter_map(|d| d.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_collect_dirs_returns_empty_for_empty_directory() {
        let tmp = tempdir().expect("tempdir");

        let dirs = collect_dirs(tmp.path()).expect("collect");
        assert!(dirs.is_empty());
    }
}
