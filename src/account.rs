use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use color_eyre::eyre::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::{config::Config, paths};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliAccountInfo {
    pub cli_name: String,
    pub status: AccountStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountStatus {
    Identified { display: String },
    CredentialsPresent { detail: String },
    NoCredentials,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexRateLimitSnapshot {
    pub observed_at: String,
    pub plan_type: Option<String>,
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub windows: Vec<CodexRateLimitWindow>,
    pub credits: Option<CodexCreditsSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodexRateLimitWindow {
    pub label: &'static str,
    pub window_minutes: u64,
    pub used_percent: f64,
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCreditsSummary {
    pub used: Option<String>,
    pub remaining: Option<String>,
    pub total: Option<String>,
    pub resets_at: Option<i64>,
    pub opaque: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodexRateLimitStatus {
    Available(Box<CodexRateLimitSnapshot>),
    NoUsageData,
    NotAuthenticated,
    NotConfigured,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeRateLimitSnapshot {
    pub observed_at: String,
    pub plan_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub windows: Vec<ClaudeRateLimitWindow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeRateLimitWindow {
    pub label: &'static str,
    pub window_minutes: u64,
    pub used_percent: f64,
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClaudeRateLimitStatus {
    Available(Box<ClaudeRateLimitSnapshot>),
    NoUsageData,
    NotAuthenticated,
    NotConfigured,
}

pub fn inspect_profile_accounts(profile: &str, cfg: &Config) -> Result<Vec<CliAccountInfo>> {
    paths::validate_profile_name(profile)?;

    let cli_names = crate::config::profile_managed_cli_names(cfg);

    let mut accounts = Vec::with_capacity(cli_names.len());
    for cli_name in cli_names {
        let cli_dir = paths::profile_cli_dir(profile, &cli_name)?;
        let status = inspect_cli_account(&cli_name, &cli_dir)?;
        accounts.push(CliAccountInfo { cli_name, status });
    }

    Ok(accounts)
}

pub fn profile_email(profile: &str) -> Option<String> {
    let cli_dir = paths::profile_cli_dir(profile, "claude").ok()?;
    let config_path = cli_dir.join(".claude.json");
    let parsed = read_json(&config_path).ok()?;
    first_nonempty_str(&parsed, &["/oauthAccount/emailAddress"]).map(ToString::to_string)
}

pub fn inspect_profile_codex_limits(profile: &str, cfg: &Config) -> Result<CodexRateLimitStatus> {
    paths::validate_profile_name(profile)?;

    if !cfg.cli.contains_key("codex") {
        return Ok(CodexRateLimitStatus::NotConfigured);
    }

    let cli_dir = paths::profile_cli_dir(profile, "codex")?;
    inspect_codex_limits(&cli_dir)
}

pub fn inspect_profile_claude_limits(profile: &str, cfg: &Config) -> Result<ClaudeRateLimitStatus> {
    paths::validate_profile_name(profile)?;

    if !cfg.cli.contains_key("claude") {
        return Ok(ClaudeRateLimitStatus::NotConfigured);
    }

    let cli_dir = paths::profile_cli_dir(profile, "claude")?;
    inspect_claude_limits(&cli_dir)
}

fn inspect_cli_account(cli_name: &str, cli_dir: &Path) -> Result<AccountStatus> {
    match cli_name {
        "claude" => inspect_claude(cli_dir),
        "codex" => inspect_codex(cli_dir),
        "gemini" => inspect_gemini(cli_dir),
        _ => inspect_unknown(cli_dir),
    }
}

fn inspect_claude(cli_dir: &Path) -> Result<AccountStatus> {
    let credentials_path = cli_dir.join(".credentials.json");
    if !credentials_path.exists() {
        return Ok(AccountStatus::NoCredentials);
    }

    let parsed = read_json(&credentials_path)?;

    if let Some(display) = display_from_paths(
        &parsed,
        &[
            "/email",
            "/user/email",
            "/account/email",
            "/claudeAiOauth/email",
        ],
        &[
            "/name",
            "/user/name",
            "/account/name",
            "/claudeAiOauth/name",
        ],
    ) {
        return Ok(AccountStatus::Identified { display });
    }

    if let Some(display) = claude_identity_from_config(cli_dir) {
        return Ok(AccountStatus::Identified { display });
    }

    let subscription = first_nonempty_str(
        &parsed,
        &[
            "/claudeAiOauth/subscriptionType",
            "/claudeAiOauth/rateLimitTier",
        ],
    );

    let detail = match subscription {
        Some(value) => {
            format!("credentials detected, but account identifier unavailable (plan: {value})")
        }
        None => "credentials detected, but account identifier unavailable".to_string(),
    };

    Ok(AccountStatus::CredentialsPresent { detail })
}

fn claude_identity_from_config(cli_dir: &Path) -> Option<String> {
    let config_path = cli_dir.join(".claude.json");
    let parsed = read_json(&config_path).ok()?;
    display_from_paths(
        &parsed,
        &["/oauthAccount/emailAddress"],
        &["/oauthAccount/displayName"],
    )
}

fn inspect_codex(cli_dir: &Path) -> Result<AccountStatus> {
    let auth_path = cli_dir.join("auth.json");
    if !auth_path.exists() {
        return Ok(AccountStatus::NoCredentials);
    }

    let parsed = read_json(&auth_path)?;

    if let Some(display) = parsed
        .pointer("/tokens/id_token")
        .and_then(Value::as_str)
        .and_then(jwt_claims_from_token)
        .and_then(display_from_claims)
    {
        return Ok(AccountStatus::Identified { display });
    }

    if let Some(account_id) = parsed.pointer("/tokens/account_id").and_then(Value::as_str) {
        return Ok(AccountStatus::CredentialsPresent {
            detail: format!("authenticated (account id: {account_id})"),
        });
    }

    if parsed
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(AccountStatus::CredentialsPresent {
            detail: "API key configured (account not identifiable from local auth files)"
                .to_string(),
        });
    }

    Ok(AccountStatus::CredentialsPresent {
        detail: "credentials detected, but no account identifier was found".to_string(),
    })
}

fn inspect_codex_limits(cli_dir: &Path) -> Result<CodexRateLimitStatus> {
    if let Some(snapshot) = latest_codex_rate_limit_snapshot(&cli_dir.join("sessions"))? {
        return Ok(CodexRateLimitStatus::Available(Box::new(snapshot)));
    }

    if cli_dir.join("auth.json").exists() {
        return Ok(CodexRateLimitStatus::NoUsageData);
    }

    Ok(CodexRateLimitStatus::NotAuthenticated)
}

fn inspect_claude_limits(cli_dir: &Path) -> Result<ClaudeRateLimitStatus> {
    let snapshot_path = cli_dir.join("usage-limits.json");
    if snapshot_path.exists() {
        let raw = read_json(&snapshot_path)?;
        if let Some(snapshot) = parse_claude_rate_limit_snapshot(&raw, cli_dir) {
            return Ok(ClaudeRateLimitStatus::Available(Box::new(snapshot)));
        }
    }

    if cli_dir.join(".credentials.json").exists() {
        return Ok(ClaudeRateLimitStatus::NoUsageData);
    }

    Ok(ClaudeRateLimitStatus::NotAuthenticated)
}

fn inspect_gemini(cli_dir: &Path) -> Result<AccountStatus> {
    let gemini_home = cli_dir.join(".gemini");
    let oauth_path = gemini_home.join("oauth_creds.json");
    if oauth_path.exists() {
        let parsed = read_json(&oauth_path)?;
        if let Some(display) = parsed
            .get("id_token")
            .and_then(Value::as_str)
            .and_then(jwt_claims_from_token)
            .and_then(display_from_claims)
        {
            return Ok(AccountStatus::Identified { display });
        }

        return Ok(AccountStatus::CredentialsPresent {
            detail: "OAuth credentials detected, but no account identifier was found".to_string(),
        });
    }

    let env_path = gemini_home.join(".env");
    if env_path.exists() {
        let raw = fs::read_to_string(&env_path)
            .wrap_err_with(|| format!("failed reading {}", env_path.display()))?;
        let has_api_key = raw
            .lines()
            .map(str::trim)
            .any(|line| line.starts_with("GEMINI_API_KEY=") || line.starts_with("GOOGLE_API_KEY="));
        if has_api_key {
            return Ok(AccountStatus::CredentialsPresent {
                detail: "API key configured (account not identifiable from local auth files)"
                    .to_string(),
            });
        }
    }

    let settings_path = gemini_home.join("settings.json");
    if settings_path.exists() {
        let parsed = read_json(&settings_path)?;
        if let Some(auth_type) = parsed
            .get("security")
            .and_then(|v| v.get("auth"))
            .and_then(|v| v.get("selectedType"))
            .and_then(Value::as_str)
            .or_else(|| parsed.get("selectedAuthType").and_then(Value::as_str))
        {
            return Ok(AccountStatus::CredentialsPresent {
                detail: format!("auth configured as {auth_type} (account unavailable)"),
            });
        }
    }

    Ok(AccountStatus::NoCredentials)
}

fn inspect_unknown(cli_dir: &Path) -> Result<AccountStatus> {
    if !cli_dir.exists() || !cli_dir.is_dir() {
        return Ok(AccountStatus::NoCredentials);
    }

    let mut entries =
        fs::read_dir(cli_dir).wrap_err_with(|| format!("failed reading {}", cli_dir.display()))?;

    if entries.next().is_some() {
        return Ok(AccountStatus::CredentialsPresent {
            detail: "credentials detected, but this CLI is not yet supported by `profile account`"
                .to_string(),
        });
    }

    Ok(AccountStatus::NoCredentials)
}

fn latest_codex_rate_limit_snapshot(session_root: &Path) -> Result<Option<CodexRateLimitSnapshot>> {
    if !session_root.exists() {
        return Ok(None);
    }

    let mut session_files = collect_session_jsonl_files(session_root)?;
    session_files.sort();

    let mut latest: Option<CodexRateLimitSnapshot> = None;

    for session_file in session_files {
        let file = fs::File::open(&session_file)
            .wrap_err_with(|| format!("failed opening {}", session_file.display()))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line =
                line.wrap_err_with(|| format!("failed reading {}", session_file.display()))?;

            let Some(snapshot) = parse_codex_rate_limit_snapshot(&line) else {
                continue;
            };

            let should_replace = latest
                .as_ref()
                .is_none_or(|current| snapshot.observed_at > current.observed_at);
            if should_replace {
                latest = Some(snapshot);
            }
        }
    }

    Ok(latest)
}

fn collect_session_jsonl_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        for entry in
            fs::read_dir(&dir).wrap_err_with(|| format!("failed reading {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn parse_codex_rate_limit_snapshot(raw: &str) -> Option<CodexRateLimitSnapshot> {
    let parsed: SessionLogEntry = serde_json::from_str(raw).ok()?;
    if parsed.entry_type.as_deref()? != "event_msg" {
        return None;
    }

    let payload = parsed.payload?;
    if payload.payload_type.as_deref()? != "token_count" {
        return None;
    }

    let observed_at = parsed.timestamp?;
    let rate_limits = payload.rate_limits?;

    let mut windows = Vec::new();
    if let Some(window) = rate_limits
        .primary
        .and_then(|value| value.into_window("primary"))
    {
        windows.push(window);
    }
    if let Some(window) = rate_limits
        .secondary
        .and_then(|value| value.into_window("secondary"))
    {
        windows.push(window);
    }

    Some(CodexRateLimitSnapshot {
        observed_at,
        plan_type: rate_limits
            .plan_type
            .filter(|value| !value.trim().is_empty()),
        limit_id: rate_limits
            .limit_id
            .filter(|value| !value.trim().is_empty()),
        limit_name: rate_limits
            .limit_name
            .filter(|value| !value.trim().is_empty()),
        windows,
        credits: rate_limits
            .credits
            .as_ref()
            .and_then(extract_codex_credits_summary),
    })
}

fn parse_claude_rate_limit_snapshot(
    value: &Value,
    cli_dir: &Path,
) -> Option<ClaudeRateLimitSnapshot> {
    let observed_at = value.pointer("/observed_at")?.as_str()?.to_string();

    let mut windows = Vec::new();

    if let Some(window) = value
        .pointer("/rate_limits/five_hour")
        .and_then(|window| parse_claude_window(window, "five_hour", 300))
    {
        windows.push(window);
    }

    if let Some(window) = value
        .pointer("/rate_limits/seven_day")
        .and_then(|window| parse_claude_window(window, "seven_day", 10_080))
    {
        windows.push(window);
    }

    if windows.is_empty() {
        return None;
    }

    let (plan_type, rate_limit_tier) = read_claude_plan_metadata(cli_dir).ok().unwrap_or_default();

    Some(ClaudeRateLimitSnapshot {
        observed_at,
        plan_type,
        rate_limit_tier,
        windows,
    })
}

fn parse_claude_window(
    value: &Value,
    label: &'static str,
    window_minutes: u64,
) -> Option<ClaudeRateLimitWindow> {
    Some(ClaudeRateLimitWindow {
        label,
        window_minutes,
        used_percent: first_f64(value, &["/used_percentage", "/usedPercent"])?,
        resets_at: first_i64(value, &["/resets_at", "/resetsAt"])?,
    })
}

fn read_claude_plan_metadata(cli_dir: &Path) -> Result<(Option<String>, Option<String>)> {
    let credentials_path = cli_dir.join(".credentials.json");
    if !credentials_path.exists() {
        return Ok((None, None));
    }

    let parsed = read_json(&credentials_path)?;
    Ok((
        first_nonempty_str(&parsed, &["/claudeAiOauth/subscriptionType"]).map(ToString::to_string),
        first_nonempty_str(&parsed, &["/claudeAiOauth/rateLimitTier"]).map(ToString::to_string),
    ))
}

fn extract_codex_credits_summary(value: &Value) -> Option<CodexCreditsSummary> {
    if value.is_null() {
        return None;
    }

    let summary = CodexCreditsSummary {
        used: first_scalar_string(
            value,
            &[
                "/used",
                "/used_usd",
                "/usedUsd",
                "/spent",
                "/spent_usd",
                "/spentUsd",
            ],
        ),
        remaining: first_scalar_string(
            value,
            &[
                "/remaining",
                "/remaining_usd",
                "/remainingUsd",
                "/available",
                "/available_usd",
                "/availableUsd",
            ],
        ),
        total: first_scalar_string(
            value,
            &[
                "/total",
                "/total_usd",
                "/totalUsd",
                "/limit",
                "/limit_usd",
                "/limitUsd",
                "/budget",
                "/budget_usd",
                "/budgetUsd",
            ],
        ),
        resets_at: first_i64(value, &["/resets_at", "/resetsAt", "/reset_at", "/resetAt"]),
        opaque: false,
    };

    if summary.used.is_some()
        || summary.remaining.is_some()
        || summary.total.is_some()
        || summary.resets_at.is_some()
    {
        return Some(summary);
    }

    Some(CodexCreditsSummary {
        used: None,
        remaining: None,
        total: None,
        resets_at: None,
        opaque: true,
    })
}

fn read_json(path: &Path) -> Result<Value> {
    let raw =
        fs::read_to_string(path).wrap_err_with(|| format!("failed reading {}", path.display()))?;
    serde_json::from_str(&raw).wrap_err_with(|| format!("invalid JSON at {}", path.display()))
}

fn jwt_claims_from_token(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn display_from_claims(claims: Value) -> Option<String> {
    display_from_paths(&claims, &["/email"], &["/name", "/given_name"])
        .or_else(|| first_nonempty_str(&claims, &["/sub"]).map(ToString::to_string))
}

fn display_from_paths(value: &Value, email_paths: &[&str], name_paths: &[&str]) -> Option<String> {
    let email = first_nonempty_str(value, email_paths);
    let name = first_nonempty_str(value, name_paths);

    match (name, email) {
        (Some(name), Some(email)) => Some(format!("{name} <{email}>")),
        (_, Some(email)) => Some(email.to_string()),
        (Some(name), None) => Some(name.to_string()),
        (None, None) => None,
    }
}

fn first_nonempty_str<'a>(value: &'a Value, paths: &[&str]) -> Option<&'a str> {
    paths.iter().find_map(|path| {
        value
            .pointer(path)
            .and_then(Value::as_str)
            .filter(|candidate| !candidate.trim().is_empty())
    })
}

fn first_scalar_string(value: &Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| match value.pointer(path) {
        Some(Value::String(candidate)) if !candidate.trim().is_empty() => Some(candidate.clone()),
        Some(Value::Number(candidate)) => Some(candidate.to_string()),
        Some(Value::Bool(candidate)) => Some(candidate.to_string()),
        _ => None,
    })
}

fn first_i64(value: &Value, paths: &[&str]) -> Option<i64> {
    paths.iter().find_map(|path| {
        value.pointer(path).and_then(|candidate| match candidate {
            Value::Number(value) => value.as_i64(),
            Value::String(value) => value.parse().ok(),
            _ => None,
        })
    })
}

fn first_f64(value: &Value, paths: &[&str]) -> Option<f64> {
    paths.iter().find_map(|path| {
        value.pointer(path).and_then(|candidate| match candidate {
            Value::Number(value) => value.as_f64(),
            Value::String(value) => value.parse().ok(),
            _ => None,
        })
    })
}

#[derive(Debug, Deserialize)]
struct SessionLogEntry {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    entry_type: Option<String>,
    payload: Option<TokenCountPayload>,
}

#[derive(Debug, Deserialize)]
struct TokenCountPayload {
    #[serde(rename = "type")]
    payload_type: Option<String>,
    rate_limits: Option<RawRateLimits>,
}

#[derive(Debug, Deserialize)]
struct RawRateLimits {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    primary: Option<RawRateLimitWindow>,
    secondary: Option<RawRateLimitWindow>,
    credits: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawRateLimitWindow {
    used_percent: Option<f64>,
    window_minutes: Option<u64>,
    resets_at: Option<i64>,
}

impl RawRateLimitWindow {
    fn into_window(self, label: &'static str) -> Option<CodexRateLimitWindow> {
        Some(CodexRateLimitWindow {
            label,
            window_minutes: self.window_minutes?,
            used_percent: self.used_percent?,
            resets_at: self.resets_at?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::{
        display_from_claims, display_from_paths, extract_codex_credits_summary, first_nonempty_str,
        first_scalar_string, inspect_claude, inspect_claude_limits, inspect_codex,
        inspect_codex_limits, inspect_gemini, inspect_unknown, jwt_claims_from_token,
        parse_codex_rate_limit_snapshot, AccountStatus, ClaudeRateLimitSnapshot,
        ClaudeRateLimitStatus, ClaudeRateLimitWindow, CodexCreditsSummary, CodexRateLimitSnapshot,
        CodexRateLimitStatus, CodexRateLimitWindow,
    };

    #[test]
    fn test_inspect_codex_extracts_name_and_email_from_id_token() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        let auth = json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": fake_jwt(json!({
                    "name": "Jane Doe",
                    "email": "jane@example.com",
                    "sub": "user_123"
                })),
                "account_id": "acct_123"
            }
        });

        fs::write(cli_dir.join("auth.json"), auth.to_string()).expect("write auth.json");

        let status = inspect_codex(cli_dir).expect("inspect");
        assert_eq!(
            status,
            AccountStatus::Identified {
                display: "Jane Doe <jane@example.com>".to_string()
            }
        );
    }

    #[test]
    fn test_inspect_gemini_falls_back_to_api_key_hint() {
        let tmp = tempdir().expect("tempdir");
        let gemini_home = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_home).expect("create .gemini dir");
        fs::write(gemini_home.join(".env"), "GEMINI_API_KEY=secret\n").expect("write .env");

        let status = inspect_gemini(tmp.path()).expect("inspect");
        assert_eq!(
            status,
            AccountStatus::CredentialsPresent {
                detail: "API key configured (account not identifiable from local auth files)"
                    .to_string()
            }
        );
    }

    #[test]
    fn test_inspect_claude_reports_plan_when_identity_is_unavailable() {
        let tmp = tempdir().expect("tempdir");
        let credentials = json!({
            "claudeAiOauth": {
                "accessToken": "opaque-token",
                "subscriptionType": "max"
            }
        });

        fs::write(
            tmp.path().join(".credentials.json"),
            credentials.to_string(),
        )
        .expect("write .credentials.json");

        let status = inspect_claude(tmp.path()).expect("inspect");
        assert_eq!(
            status,
            AccountStatus::CredentialsPresent {
                detail: "credentials detected, but account identifier unavailable (plan: max)"
                    .to_string()
            }
        );
    }

    #[test]
    fn test_inspect_claude_extracts_identity_from_claude_json() {
        let tmp = tempdir().expect("tempdir");
        let credentials = json!({
            "claudeAiOauth": {
                "accessToken": "opaque-token",
                "subscriptionType": "max"
            }
        });
        let config = json!({
            "oauthAccount": {
                "emailAddress": "jane@example.com",
                "displayName": "Jane Doe"
            }
        });

        fs::write(
            tmp.path().join(".credentials.json"),
            credentials.to_string(),
        )
        .expect("write .credentials.json");
        fs::write(tmp.path().join(".claude.json"), config.to_string()).expect("write .claude.json");

        let status = inspect_claude(tmp.path()).expect("inspect");
        assert_eq!(
            status,
            AccountStatus::Identified {
                display: "Jane Doe <jane@example.com>".to_string()
            }
        );
    }

    #[test]
    fn test_inspect_codex_limits_reads_latest_session_snapshot() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        let session_dir = cli_dir.join("sessions/2026/03/28");
        fs::create_dir_all(&session_dir).expect("create sessions dir");
        fs::write(cli_dir.join("auth.json"), r#"{"auth_mode":"chatgpt"}"#).expect("write auth");

        let older = json!({
            "timestamp": "2026-03-28T15:21:04.789Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "rate_limits": {
                    "limit_id": "codex",
                    "plan_type": "team",
                    "primary": {
                        "used_percent": 1.0,
                        "window_minutes": 300,
                        "resets_at": 1774719759i64
                    },
                    "secondary": {
                        "used_percent": 29.0,
                        "window_minutes": 10080,
                        "resets_at": 1775223377i64
                    },
                    "credits": null
                }
            }
        });

        let newer = json!({
            "timestamp": "2026-03-28T15:23:12.299Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "rate_limits": {
                    "limit_id": "codex",
                    "limit_name": "Codex Team",
                    "plan_type": "team",
                    "primary": {
                        "used_percent": 1.0,
                        "window_minutes": 300,
                        "resets_at": 1774719759i64
                    },
                    "secondary": {
                        "used_percent": 30.0,
                        "window_minutes": 10080,
                        "resets_at": 1775223377i64
                    },
                    "credits": {
                        "used_usd": 12.5,
                        "remaining_usd": 87.5,
                        "limit_usd": 100,
                        "resets_at": 1775223377i64
                    }
                }
            }
        });

        fs::write(
            session_dir.join("rollout-a.jsonl"),
            format!("{}\n{}\n", older, newer),
        )
        .expect("write session");

        let status = inspect_codex_limits(cli_dir).expect("inspect");
        assert_eq!(
            status,
            CodexRateLimitStatus::Available(Box::new(CodexRateLimitSnapshot {
                observed_at: "2026-03-28T15:23:12.299Z".to_string(),
                plan_type: Some("team".to_string()),
                limit_id: Some("codex".to_string()),
                limit_name: Some("Codex Team".to_string()),
                windows: vec![
                    CodexRateLimitWindow {
                        label: "primary",
                        window_minutes: 300,
                        used_percent: 1.0,
                        resets_at: 1774719759,
                    },
                    CodexRateLimitWindow {
                        label: "secondary",
                        window_minutes: 10080,
                        used_percent: 30.0,
                        resets_at: 1775223377,
                    },
                ],
                credits: Some(CodexCreditsSummary {
                    used: Some("12.5".to_string()),
                    remaining: Some("87.5".to_string()),
                    total: Some("100".to_string()),
                    resets_at: Some(1775223377),
                    opaque: false,
                }),
            }))
        );
    }

    #[test]
    fn test_inspect_codex_limits_reports_no_usage_data_when_auth_exists() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        fs::write(cli_dir.join("auth.json"), r#"{"auth_mode":"chatgpt"}"#).expect("write auth");

        let status = inspect_codex_limits(cli_dir).expect("inspect");
        assert_eq!(status, CodexRateLimitStatus::NoUsageData);
    }

    #[test]
    fn test_inspect_codex_limits_reports_not_authenticated_without_auth_or_sessions() {
        let tmp = tempdir().expect("tempdir");

        let status = inspect_codex_limits(tmp.path()).expect("inspect");
        assert_eq!(status, CodexRateLimitStatus::NotAuthenticated);
    }

    #[test]
    fn test_inspect_claude_limits_reads_usage_snapshot() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        let credentials = json!({
            "claudeAiOauth": {
                "subscriptionType": "team",
                "rateLimitTier": "default_raven"
            }
        });
        let snapshot = json!({
            "observed_at": "2026-03-28T18:12:44Z",
            "rate_limits": {
                "five_hour": {
                    "used_percentage": 12.5,
                    "resets_at": 1774719759i64
                },
                "seven_day": {
                    "used_percentage": 37.0,
                    "resets_at": 1775223377i64
                }
            }
        });

        fs::write(cli_dir.join(".credentials.json"), credentials.to_string())
            .expect("write .credentials.json");
        fs::write(cli_dir.join("usage-limits.json"), snapshot.to_string())
            .expect("write usage-limits.json");

        let status = inspect_claude_limits(cli_dir).expect("inspect");
        assert_eq!(
            status,
            ClaudeRateLimitStatus::Available(Box::new(ClaudeRateLimitSnapshot {
                observed_at: "2026-03-28T18:12:44Z".to_string(),
                plan_type: Some("team".to_string()),
                rate_limit_tier: Some("default_raven".to_string()),
                windows: vec![
                    ClaudeRateLimitWindow {
                        label: "five_hour",
                        window_minutes: 300,
                        used_percent: 12.5,
                        resets_at: 1774719759,
                    },
                    ClaudeRateLimitWindow {
                        label: "seven_day",
                        window_minutes: 10_080,
                        used_percent: 37.0,
                        resets_at: 1775223377,
                    },
                ],
            }))
        );
    }

    #[test]
    fn test_inspect_claude_limits_reports_no_usage_data_when_authenticated() {
        let tmp = tempdir().expect("tempdir");
        fs::write(
            tmp.path().join(".credentials.json"),
            r#"{"claudeAiOauth":{"subscriptionType":"team"}}"#,
        )
        .expect("write .credentials.json");

        let status = inspect_claude_limits(tmp.path()).expect("inspect");
        assert_eq!(status, ClaudeRateLimitStatus::NoUsageData);
    }

    #[test]
    fn test_inspect_claude_limits_reports_not_authenticated_without_credentials_or_snapshot() {
        let tmp = tempdir().expect("tempdir");

        let status = inspect_claude_limits(tmp.path()).expect("inspect");
        assert_eq!(status, ClaudeRateLimitStatus::NotAuthenticated);
    }

    #[test]
    fn test_jwt_claims_from_token_parses_valid_jwt() {
        let token = fake_jwt(json!({"email": "test@example.com", "sub": "user_1"}));
        let claims = jwt_claims_from_token(&token).expect("should parse");
        assert_eq!(claims["email"], "test@example.com");
        assert_eq!(claims["sub"], "user_1");
    }

    #[test]
    fn test_jwt_claims_from_token_returns_none_for_invalid_token() {
        assert!(jwt_claims_from_token("not-a-jwt").is_none());
        assert!(jwt_claims_from_token("header.!!!invalid-base64!!!.sig").is_none());
    }

    #[test]
    fn test_display_from_claims_prefers_email() {
        let claims = json!({"email": "jane@example.com", "name": "Jane", "sub": "user_1"});
        assert_eq!(
            display_from_claims(claims),
            Some("Jane <jane@example.com>".to_string())
        );
    }

    #[test]
    fn test_display_from_claims_falls_back_to_sub() {
        let claims = json!({"sub": "user_1"});
        assert_eq!(display_from_claims(claims), Some("user_1".to_string()));
    }

    #[test]
    fn test_display_from_claims_returns_none_when_empty() {
        let claims = json!({});
        assert_eq!(display_from_claims(claims), None);
    }

    #[test]
    fn test_display_from_paths_email_only() {
        let value = json!({"email": "test@example.com"});
        assert_eq!(
            display_from_paths(&value, &["/email"], &["/name"]),
            Some("test@example.com".to_string())
        );
    }

    #[test]
    fn test_display_from_paths_name_only() {
        let value = json!({"name": "Jane"});
        assert_eq!(
            display_from_paths(&value, &["/email"], &["/name"]),
            Some("Jane".to_string())
        );
    }

    #[test]
    fn test_display_from_paths_name_and_email() {
        let value = json!({"email": "jane@example.com", "name": "Jane"});
        assert_eq!(
            display_from_paths(&value, &["/email"], &["/name"]),
            Some("Jane <jane@example.com>".to_string())
        );
    }

    #[test]
    fn test_display_from_paths_returns_none_when_no_match() {
        let value = json!({"other": "data"});
        assert_eq!(display_from_paths(&value, &["/email"], &["/name"]), None);
    }

    #[test]
    fn test_first_nonempty_str_finds_first_match() {
        let value = json!({"a": "", "b": "found", "c": "also"});
        assert_eq!(
            first_nonempty_str(&value, &["/a", "/b", "/c"]),
            Some("found")
        );
    }

    #[test]
    fn test_first_nonempty_str_skips_empty_and_whitespace() {
        let value = json!({"a": "", "b": "  ", "c": "ok"});
        assert_eq!(first_nonempty_str(&value, &["/a", "/b", "/c"]), Some("ok"));
    }

    #[test]
    fn test_first_nonempty_str_returns_none_when_nothing_found() {
        let value = json!({"a": ""});
        assert_eq!(first_nonempty_str(&value, &["/a", "/missing"]), None);
    }

    #[test]
    fn test_first_scalar_string_extracts_string() {
        let value = json!({"amount": "42.5"});
        assert_eq!(
            first_scalar_string(&value, &["/amount"]),
            Some("42.5".to_string())
        );
    }

    #[test]
    fn test_first_scalar_string_extracts_number() {
        let value = json!({"amount": 42.5});
        assert_eq!(
            first_scalar_string(&value, &["/amount"]),
            Some("42.5".to_string())
        );
    }

    #[test]
    fn test_first_scalar_string_extracts_bool() {
        let value = json!({"active": true});
        assert_eq!(
            first_scalar_string(&value, &["/active"]),
            Some("true".to_string())
        );
    }

    #[test]
    fn test_first_scalar_string_skips_empty_strings() {
        let value = json!({"a": "", "b": "ok"});
        assert_eq!(
            first_scalar_string(&value, &["/a", "/b"]),
            Some("ok".to_string())
        );
    }

    #[test]
    fn test_extract_codex_credits_summary_with_values() {
        let value = json!({
            "used_usd": 12.5,
            "remaining_usd": 87.5,
            "limit_usd": 100,
            "resets_at": 1774719759i64
        });
        let summary = extract_codex_credits_summary(&value).expect("should extract");
        assert_eq!(summary.used, Some("12.5".to_string()));
        assert_eq!(summary.remaining, Some("87.5".to_string()));
        assert_eq!(summary.total, Some("100".to_string()));
        assert_eq!(summary.resets_at, Some(1774719759));
        assert!(!summary.opaque);
    }

    #[test]
    fn test_extract_codex_credits_summary_returns_opaque_for_empty_object() {
        let value = json!({});
        let summary = extract_codex_credits_summary(&value).expect("should extract");
        assert!(summary.opaque);
    }

    #[test]
    fn test_extract_codex_credits_summary_returns_none_for_null() {
        assert!(extract_codex_credits_summary(&Value::Null).is_none());
    }

    #[test]
    fn test_parse_codex_rate_limit_snapshot_valid_entry() {
        let entry = json!({
            "timestamp": "2026-03-28T15:23:12.299Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "rate_limits": {
                    "limit_id": "codex",
                    "plan_type": "team",
                    "primary": {
                        "used_percent": 1.0,
                        "window_minutes": 300,
                        "resets_at": 1774719759i64
                    }
                }
            }
        });
        let snapshot = parse_codex_rate_limit_snapshot(&entry.to_string()).expect("should parse");
        assert_eq!(snapshot.observed_at, "2026-03-28T15:23:12.299Z");
        assert_eq!(snapshot.plan_type, Some("team".to_string()));
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].label, "primary");
    }

    #[test]
    fn test_parse_codex_rate_limit_snapshot_ignores_non_event_msg() {
        let entry = json!({
            "timestamp": "2026-03-28T15:23:12.299Z",
            "type": "other_type",
            "payload": { "type": "token_count" }
        });
        assert!(parse_codex_rate_limit_snapshot(&entry.to_string()).is_none());
    }

    #[test]
    fn test_parse_codex_rate_limit_snapshot_ignores_non_token_count_payload() {
        let entry = json!({
            "timestamp": "2026-03-28T15:23:12.299Z",
            "type": "event_msg",
            "payload": { "type": "something_else" }
        });
        assert!(parse_codex_rate_limit_snapshot(&entry.to_string()).is_none());
    }

    #[test]
    fn test_parse_codex_rate_limit_snapshot_ignores_invalid_json() {
        assert!(parse_codex_rate_limit_snapshot("not json at all").is_none());
    }

    #[test]
    fn test_inspect_unknown_detects_files_in_directory() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path();
        fs::write(cli_dir.join("some-config"), "data").expect("write");

        let status = inspect_unknown(cli_dir).expect("inspect");
        assert!(matches!(status, AccountStatus::CredentialsPresent { .. }));
    }

    #[test]
    fn test_inspect_unknown_reports_no_credentials_for_empty_dir() {
        let tmp = tempdir().expect("tempdir");

        let status = inspect_unknown(tmp.path()).expect("inspect");
        assert_eq!(status, AccountStatus::NoCredentials);
    }

    #[test]
    fn test_inspect_unknown_reports_no_credentials_for_missing_dir() {
        let tmp = tempdir().expect("tempdir");
        let missing = tmp.path().join("nonexistent");

        let status = inspect_unknown(&missing).expect("inspect");
        assert_eq!(status, AccountStatus::NoCredentials);
    }

    fn fake_jwt(claims: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
        format!("{header}.{payload}.signature")
    }
}
