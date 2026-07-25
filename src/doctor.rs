use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use color_eyre::eyre::{Context, Result};
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, ContentArrangement, Table,
};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::{account, config::Config, paths};

struct BinarySummary {
    configured: usize,
    found: usize,
    missing: usize,
}

struct ProfileSummary {
    profiles: usize,
    cli_dirs_present: usize,
    cli_dirs_missing: usize,
    credentials_detected: usize,
    credentials_missing: usize,
}

pub struct BackupToolsSummary {
    pub tar: bool,
    pub gzip: bool,
    pub gpg: bool,
    pub gpg_encrypts: bool,
}

struct CodexProfileIssue {
    profile: String,
    file: String,
    severity: &'static str,
    issue: &'static str,
    suggested_action: &'static str,
}

pub fn run_doctor(config: &Config, config_path: &Path, config_created: bool) -> Result<()> {
    println!("{}", format_main_heading("Doctor"));
    if config_created {
        print_detail_line(
            "Config",
            &format!("{} (created with defaults)", config_path.display()),
        );
    } else {
        print_detail_line("Config", &config_path.display().to_string());
    }

    let binary_summary = check_binaries(config);
    let profile_summary = check_profiles(config)?;

    println!();
    println!("{}", format_section_title("Summary"));
    print_detail_line("CLI blocks", &binary_summary.configured.to_string());
    print_detail_line(
        "Binaries",
        &format!(
            "{} found, {} missing",
            binary_summary.found, binary_summary.missing
        ),
    );
    print_detail_line("Profiles", &profile_summary.profiles.to_string());
    print_detail_line(
        "CLI dirs",
        &format!(
            "{} present, {} missing",
            profile_summary.cli_dirs_present, profile_summary.cli_dirs_missing
        ),
    );
    print_detail_line(
        "Credentials",
        &format!(
            "{} detected, {} missing",
            profile_summary.credentials_detected, profile_summary.credentials_missing
        ),
    );

    println!();
    println!("{}", format_section_title("Binaries"));
    print_binaries_table(config);
    println!();
    println!("{}", format_section_title("Profiles"));
    print_profiles_table(config)?;

    let codex_issues = collect_codex_profile_issues()?;
    if !codex_issues.is_empty() {
        println!();
        println!("{}", format_section_title("Codex Profile Hygiene"));
        print_codex_profile_issues(&codex_issues);
    }

    let backup_tools = check_backup_tools();
    println!();
    println!("{}", format_section_title("Backup Tools"));
    print_detail_line("tar", if backup_tools.tar { "found" } else { "MISSING" });
    print_detail_line(
        "gzip",
        if backup_tools.gzip {
            "found"
        } else {
            "MISSING"
        },
    );
    print_detail_line("gpg", if backup_tools.gpg { "found" } else { "MISSING" });
    print_detail_line(
        "gpg encrypt",
        if backup_tools.gpg_encrypts {
            "ok"
        } else {
            "FAILED"
        },
    );

    Ok(())
}

fn check_binaries(config: &Config) -> BinarySummary {
    let mut cli_names: Vec<&String> = config.cli.keys().collect();
    cli_names.sort();
    let mut found = 0usize;
    let mut missing = 0usize;

    for cli_name in cli_names {
        let cli_cfg = &config.cli[cli_name];
        match which::which(&cli_cfg.binary) {
            Ok(_) => found += 1,
            Err(_) => missing += 1,
        };
    }

    BinarySummary {
        configured: found + missing,
        found,
        missing,
    }
}

fn gpg_can_encrypt() -> bool {
    if which::which("gpg").is_err() {
        return false;
    }
    let Ok(tmp) = tempfile::tempdir() else {
        return false;
    };
    let plain = tmp.path().join("probe.txt");
    let enc = tmp.path().join("probe.txt.gpg");
    if std::fs::write(&plain, b"probe").is_err() {
        return false;
    }
    let mut child = match Command::new("gpg")
        .args([
            "--batch",
            "--yes",
            "--pinentry-mode",
            "loopback",
            "--passphrase-fd",
            "0",
            "--symmetric",
            "--cipher-algo",
            "AES256",
            "-o",
        ])
        .arg(&enc)
        .arg(&plain)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(stdin) = child.stdin.as_mut() {
        if stdin.write_all(b"probe-pass").is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
    }
    matches!(child.wait(), Ok(s) if s.success()) && enc.exists()
}

pub fn check_backup_tools() -> BackupToolsSummary {
    BackupToolsSummary {
        tar: which::which("tar").is_ok(),
        gzip: which::which("gzip").is_ok(),
        gpg: which::which("gpg").is_ok(),
        gpg_encrypts: gpg_can_encrypt(),
    }
}

fn print_binaries_table(config: &Config) {
    let mut cli_names: Vec<&String> = config.cli.keys().collect();
    cli_names.sort();
    let mut table = new_ui_table(vec!["CLI", "Binary", "Status", "Location"]);

    for cli_name in cli_names {
        let cli_cfg = &config.cli[cli_name];
        match which::which(&cli_cfg.binary) {
            Ok(path) => {
                table.add_row(vec![
                    Cell::new(format_cli_label(cli_name)),
                    Cell::new(&cli_cfg.binary),
                    Cell::new("found"),
                    Cell::new(path.display().to_string()),
                ]);
            }
            Err(_) => {
                table.add_row(vec![
                    Cell::new(format_cli_label(cli_name)),
                    Cell::new(&cli_cfg.binary),
                    Cell::new("missing"),
                    Cell::new("not found in PATH"),
                ]);
            }
        };
    }

    println!("{table}");
}

fn check_profiles(config: &Config) -> Result<ProfileSummary> {
    let profiles_root = paths::profiles_dir()?;

    if !profiles_root.exists() {
        return Ok(ProfileSummary {
            profiles: 0,
            cli_dirs_present: 0,
            cli_dirs_missing: 0,
            credentials_detected: 0,
            credentials_missing: 0,
        });
    }

    let mut profile_dirs = collect_dirs(&profiles_root)?;
    profile_dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    if profile_dirs.is_empty() {
        return Ok(ProfileSummary {
            profiles: 0,
            cli_dirs_present: 0,
            cli_dirs_missing: 0,
            credentials_detected: 0,
            credentials_missing: 0,
        });
    }

    let cli_names = crate::config::profile_managed_cli_names(config);

    if cli_names.is_empty() {
        return Ok(ProfileSummary {
            profiles: profile_dirs.len(),
            cli_dirs_present: 0,
            cli_dirs_missing: 0,
            credentials_detected: 0,
            credentials_missing: 0,
        });
    }

    let mut cli_dirs_present = 0usize;
    let mut cli_dirs_missing = 0usize;
    let mut credentials_detected = 0usize;
    let mut credentials_missing = 0usize;

    for profile_dir in profile_dirs {
        for cli_name in &cli_names {
            let cli_dir = profile_dir.join(cli_name);
            if !cli_dir.exists() {
                cli_dirs_missing += 1;
                continue;
            }

            cli_dirs_present += 1;

            if has_credentials_hint(cli_name, &cli_dir)? {
                credentials_detected += 1;
            } else {
                credentials_missing += 1;
            }
        }
    }

    Ok(ProfileSummary {
        profiles: collect_dirs(&profiles_root)?.len(),
        cli_dirs_present,
        cli_dirs_missing,
        credentials_detected,
        credentials_missing,
    })
}

fn print_profiles_table(config: &Config) -> Result<()> {
    let profiles_root = paths::profiles_dir()?;

    if !profiles_root.exists() {
        print_detail_line(
            "Status",
            "No profiles found. Run: cloak profile create <name>",
        );
        return Ok(());
    }

    let mut profile_dirs = collect_dirs(&profiles_root)?;
    profile_dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    if profile_dirs.is_empty() {
        print_detail_line(
            "Status",
            "No profiles found. Run: cloak profile create <name>",
        );
        return Ok(());
    }

    let cli_names = crate::config::profile_managed_cli_names(config);

    if cli_names.is_empty() {
        print_detail_line("Status", "No profile-managed CLI is currently enabled.");
        return Ok(());
    }

    for (index, profile_dir) in profile_dirs.iter().enumerate() {
        let profile_name = profile_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<invalid>");

        if index > 0 {
            println!();
        }

        let title = match account::profile_email(profile_name) {
            Some(email) => format!("Profile '{}' <{}>", profile_name, email),
            None => format!("Profile '{}'", profile_name),
        };
        println!("{}", format_section_title(&title));
        let mut table = new_ui_table(vec!["CLI", "Directory", "Credentials"]);

        for cli_name in &cli_names {
            let cli_dir = profile_dir.join(cli_name);
            if !cli_dir.exists() {
                table.add_row(vec![
                    Cell::new(format_cli_label(cli_name)),
                    Cell::new(format!("missing dir {}", cli_dir.display())),
                    Cell::new("n/a"),
                ]);
                continue;
            }

            let credentials = if has_credentials_hint(cli_name, &cli_dir)? {
                "credentials detected"
            } else {
                "no credential file detected"
            };

            table.add_row(vec![
                Cell::new(format_cli_label(cli_name)),
                Cell::new(cli_dir.display().to_string()),
                Cell::new(credentials),
            ]);
        }

        println!("{table}");
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

fn collect_codex_profile_issues() -> Result<Vec<CodexProfileIssue>> {
    let profiles_root = paths::profiles_dir()?;
    if !profiles_root.exists() {
        return Ok(Vec::new());
    }

    let mut issues = Vec::new();
    let mut profile_dirs = collect_dirs(&profiles_root)?;
    profile_dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for profile_dir in profile_dirs {
        let profile_name = profile_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<invalid>")
            .to_string();
        let codex_dir = profile_dir.join("codex");
        if !codex_dir.exists() {
            continue;
        }

        let config_path = codex_dir.join("config.toml");
        if config_path.exists() {
            collect_codex_config_issues(&profile_name, &config_path, &mut issues)?;
        }
        collect_codex_backup_issues(&profile_name, &codex_dir, &mut issues)?;
    }

    Ok(issues)
}

fn collect_codex_config_issues(
    profile_name: &str,
    config_path: &Path,
    issues: &mut Vec<CodexProfileIssue>,
) -> Result<()> {
    let raw = fs::read_to_string(config_path)
        .wrap_err_with(|| format!("failed reading {}", config_path.display()))?;
    let parsed: toml::Value = match toml::from_str(&raw) {
        Ok(value) => value,
        Err(_) => {
            issues.push(CodexProfileIssue {
                profile: profile_name.to_string(),
                file: "config.toml".to_string(),
                severity: "warning",
                issue: "invalid Codex config TOML",
                suggested_action: "inspect config.toml manually",
            });
            return Ok(());
        }
    };

    collect_codex_trusted_project_issues(profile_name, &parsed, issues);
    collect_codex_removed_feature_issues(profile_name, &parsed, issues);
    Ok(())
}

fn collect_codex_trusted_project_issues(
    profile_name: &str,
    parsed: &toml::Value,
    issues: &mut Vec<CodexProfileIssue>,
) {
    let Some(projects) = parsed.get("projects").and_then(toml::Value::as_table) else {
        return;
    };

    let home_dir = dirs::home_dir();
    let temp_dir = std::env::temp_dir();

    for (project_path, raw) in projects {
        let trust_level = raw
            .as_table()
            .and_then(|table| table.get("trust_level"))
            .and_then(toml::Value::as_str);
        if trust_level != Some("trusted") {
            continue;
        }

        let path = Path::new(project_path);
        if home_dir.as_deref() == Some(path) {
            issues.push(CodexProfileIssue {
                profile: profile_name.to_string(),
                file: "config.toml".to_string(),
                severity: "warning",
                issue: "trusted project is home directory",
                suggested_action: "remove this project entry",
            });
        } else if path.starts_with(&temp_dir) {
            issues.push(CodexProfileIssue {
                profile: profile_name.to_string(),
                file: "config.toml".to_string(),
                severity: "warning",
                issue: "trusted project is under temp directory",
                suggested_action: "remove stale temp entry",
            });
        }
    }
}

fn collect_codex_removed_feature_issues(
    profile_name: &str,
    parsed: &toml::Value,
    issues: &mut Vec<CodexProfileIssue>,
) {
    let js_repl_enabled = parsed
        .get("features")
        .and_then(toml::Value::as_table)
        .and_then(|features| features.get("js_repl"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    if js_repl_enabled {
        issues.push(CodexProfileIssue {
            profile: profile_name.to_string(),
            file: "config.toml".to_string(),
            severity: "info",
            issue: "removed Codex feature flag js_repl",
            suggested_action: "remove features.js_repl",
        });
    }
}

fn collect_codex_backup_issues(
    profile_name: &str,
    codex_dir: &Path,
    issues: &mut Vec<CodexProfileIssue>,
) -> Result<()> {
    for entry in fs::read_dir(codex_dir)
        .wrap_err_with(|| format!("failed reading {}", codex_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if is_stale_codex_config_backup(file_name) {
            issues.push(CodexProfileIssue {
                profile: profile_name.to_string(),
                file: file_name.to_string(),
                severity: "info",
                issue: "stale Codex config backup file",
                suggested_action: "archive or delete manually",
            });
        }
    }
    Ok(())
}

fn is_stale_codex_config_backup(file_name: &str) -> bool {
    file_name.starts_with("config.toml.")
        || matches!(file_name, "config.toml.bak" | "config.toml.old")
}

fn print_codex_profile_issues(issues: &[CodexProfileIssue]) {
    let mut table = new_ui_table(vec![
        "Profile",
        "File",
        "Severity",
        "Issue",
        "Suggested action",
    ]);

    for issue in issues {
        table.add_row(vec![
            Cell::new(&issue.profile),
            Cell::new(&issue.file),
            Cell::new(issue.severity),
            Cell::new(issue.issue),
            Cell::new(issue.suggested_action),
        ]);
    }

    println!("{table}");
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

fn format_main_heading(title: &str) -> String {
    if io::stdout().is_terminal() {
        title.bold().underline().to_string()
    } else {
        title.to_string()
    }
}

fn format_section_title(title: &str) -> String {
    if io::stdout().is_terminal() {
        title.bold().cyan().to_string()
    } else {
        title.to_string()
    }
}

fn print_detail_line(label: &str, value: &str) {
    let label = if io::stdout().is_terminal() {
        format!("  {}", label.bold().bright_black())
    } else {
        format!("  {label}")
    };
    println!("{label}: {value}");
}

fn format_cli_label(cli_name: &str) -> String {
    match cli_name {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "gemini" => "Gemini".to_string(),
        other => capitalize_label(other),
    }
}

fn capitalize_label(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn new_ui_table(header: Vec<&str>) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(header);
    table
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

    #[test]
    fn test_check_backup_tools_reports_presence() {
        let summary = super::check_backup_tools();
        // tar e gzip praticamente sempre presentes no ambiente de teste Unix.
        assert_eq!(summary.tar, which::which("tar").is_ok());
        assert_eq!(summary.gpg, which::which("gpg").is_ok());
        // Se gpg está presente e cifra, gpg_encrypts deve ser true.
        if summary.gpg {
            assert_eq!(summary.gpg_encrypts, super::gpg_can_encrypt());
        }
    }
}
