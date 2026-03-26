use std::{fs, path::Path};

use base64::{
    engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD},
    Engine as _,
};
use color_eyre::eyre::{Context, Result};
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

pub fn inspect_profile_accounts(profile: &str, cfg: &Config) -> Result<Vec<CliAccountInfo>> {
    paths::validate_profile_name(profile)?;

    let mut cli_names: Vec<String> = cfg.cli.keys().cloned().collect();
    cli_names.sort();

    let mut accounts = Vec::with_capacity(cli_names.len());
    for cli_name in cli_names {
        let cli_dir = paths::profile_cli_dir(profile, &cli_name)?;
        let status = inspect_cli_account(&cli_name, &cli_dir)?;
        accounts.push(CliAccountInfo { cli_name, status });
    }

    Ok(accounts)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use serde_json::{json, Value};
    use tempfile::tempdir;

    use super::{inspect_claude, inspect_codex, inspect_gemini, AccountStatus};

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

    fn fake_jwt(claims: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
        format!("{header}.{payload}.signature")
    }
}
