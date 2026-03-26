#[cfg(unix)]
mod unix_exec_tests {
    use std::{
        fs,
        os::unix::{fs as unix_fs, fs::PermissionsExt},
        path::{Path, PathBuf},
        process::Command,
    };

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn exec_sets_profile_env_and_removes_api_key() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let repo = tmp.path().join("repo");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&repo).expect("create repo dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(repo.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let output = Command::new(cloak_bin())
            .arg("exec")
            .arg("mock")
            .arg("--")
            .arg("alpha")
            .arg("beta")
            .current_dir(&repo)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .env("OPENAI_API_KEY", "SHOULD_NOT_LEAK")
            .output()
            .expect("run cloak exec");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_profile_home = xdg_config_home
            .join("cloak")
            .join("profiles")
            .join("work")
            .join("mock");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!("MOCK_HOME={}", expected_profile_home.display())),
            "missing MOCK_HOME in stdout:\n{stdout}"
        );
        assert!(
            stdout.contains("OPENAI_API_KEY=<unset>"),
            "OPENAI_API_KEY should be removed:\n{stdout}"
        );
        assert!(
            stdout.contains("ARGS=alpha beta"),
            "args were not forwarded as expected:\n{stdout}"
        );

        assert!(expected_profile_home.is_dir(), "profile cli dir must exist");

        let mode = fs::metadata(&expected_profile_home)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "profile cli dir must be 0700");
    }

    #[test]
    fn exec_sets_gemini_home_and_removes_gemini_keys() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let repo = tmp.path().join("repo-gemini");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&repo).expect("create repo dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_gemini_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(repo.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let output = Command::new(cloak_bin())
            .arg("exec")
            .arg("gemini")
            .arg("--")
            .arg("ask")
            .arg("hello")
            .current_dir(&repo)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .env("GEMINI_API_KEY", "SHOULD_NOT_LEAK")
            .env("GOOGLE_API_KEY", "SHOULD_NOT_LEAK")
            .output()
            .expect("run cloak exec");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_profile_home = xdg_config_home
            .join("cloak")
            .join("profiles")
            .join("work")
            .join("gemini");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!(
                "GEMINI_CLI_HOME={}",
                expected_profile_home.display()
            )),
            "missing GEMINI_CLI_HOME in stdout:\n{stdout}"
        );
        assert!(
            stdout.contains("GEMINI_API_KEY=<unset>"),
            "GEMINI_API_KEY should be removed:\n{stdout}"
        );
        assert!(
            stdout.contains("GOOGLE_API_KEY=<unset>"),
            "GOOGLE_API_KEY should be removed:\n{stdout}"
        );
        assert!(
            stdout.contains("ARGS=ask hello"),
            "args were not forwarded as expected:\n{stdout}"
        );

        assert!(expected_profile_home.is_dir(), "profile cli dir must exist");

        let mode = fs::metadata(&expected_profile_home)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "profile cli dir must be 0700");
    }

    #[test]
    fn exec_explicit_profile_overrides_directory_resolution() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let repo = tmp.path().join("repo-explicit-profile");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&repo).expect("create repo dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(repo.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let output = Command::new(cloak_bin())
            .arg("exec")
            .arg("mock")
            .arg("--profile")
            .arg("override")
            .arg("alpha")
            .arg("beta")
            .current_dir(&repo)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak exec");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_profile_home = xdg_config_home
            .join("cloak")
            .join("profiles")
            .join("override")
            .join("mock");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!("MOCK_HOME={}", expected_profile_home.display())),
            "explicit profile path not found in stdout:\n{stdout}"
        );
        assert!(
            stdout.contains("ARGS=alpha beta"),
            "args were not forwarded as expected:\n{stdout}"
        );
    }

    #[test]
    fn exec_uses_default_profile_when_no_cloak_file_exists() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let repo = tmp.path().join("repo-no-cloak");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&repo).expect("create repo dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_config(&xdg_config_home, &mock_binary, "personal");

        let output = Command::new(cloak_bin())
            .arg("exec")
            .arg("mock")
            .current_dir(&repo)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak exec");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_profile_home = xdg_config_home
            .join("cloak")
            .join("profiles")
            .join("personal")
            .join("mock");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!("MOCK_HOME={}", expected_profile_home.display())),
            "default profile path not found in stdout:\n{stdout}"
        );
    }

    #[test]
    fn exec_prefers_logical_pwd_for_profile_resolution() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let real_repo = tmp.path().join("real/repo");
        let logical_root = tmp.path().join("logical");
        let logical_repo = logical_root.join("repo");
        let logical_subdir = logical_repo.join("sub");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(real_repo.join("sub")).expect("create real repo dir");
        fs::create_dir_all(&logical_root).expect("create logical root");
        unix_fs::symlink(&real_repo, &logical_repo).expect("create repo symlink");

        let mock_binary = create_mock_binary(&bin_dir);
        write_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(logical_root.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let output = Command::new(cloak_bin())
            .arg("exec")
            .arg("mock")
            .current_dir(real_repo.join("sub"))
            .env("PWD", &logical_subdir)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak exec");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let expected_profile_home = xdg_config_home
            .join("cloak")
            .join("profiles")
            .join("work")
            .join("mock");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!("MOCK_HOME={}", expected_profile_home.display())),
            "logical PWD .cloak should win over physical path:\n{stdout}"
        );
    }

    #[test]
    fn profile_account_shows_detected_accounts_per_cli() {
        let tmp = tempdir().expect("tempdir");
        let xdg_config_home = tmp.path().join("xdg");
        let profiles_root = xdg_config_home.join("cloak").join("profiles");
        let work_dir = profiles_root.join("work");

        write_standard_config(&xdg_config_home);

        fs::create_dir_all(work_dir.join("claude")).expect("create claude dir");
        fs::create_dir_all(work_dir.join("codex")).expect("create codex dir");
        fs::create_dir_all(work_dir.join("gemini/.gemini")).expect("create gemini dir");

        fs::write(
            work_dir.join("claude/.credentials.json"),
            json!({
                "claudeAiOauth": {
                    "accessToken": "opaque-token",
                    "subscriptionType": "max"
                }
            })
            .to_string(),
        )
        .expect("write claude credentials");

        fs::write(
            work_dir.join("codex/auth.json"),
            json!({
                "auth_mode": "chatgpt",
                "tokens": {
                    "id_token": fake_jwt(json!({
                        "name": "Jane Doe",
                        "email": "jane@example.com"
                    })),
                    "account_id": "acct_123"
                }
            })
            .to_string(),
        )
        .expect("write codex auth");

        fs::write(
            work_dir.join("gemini/.gemini/oauth_creds.json"),
            json!({
                "id_token": fake_jwt(json!({
                    "name": "Gem User",
                    "email": "gem@example.com"
                }))
            })
            .to_string(),
        )
        .expect("write gemini oauth");

        let output = Command::new(cloak_bin())
            .arg("profile")
            .arg("account")
            .arg("work")
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak profile account");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Profile 'work'"),
            "missing profile header:\n{stdout}"
        );
        assert!(
            stdout.contains("claude -> credentials detected, but account identifier unavailable"),
            "missing claude output:\n{stdout}"
        );
        assert!(
            stdout.contains("codex -> Jane Doe <jane@example.com>"),
            "missing codex identity:\n{stdout}"
        );
        assert!(
            stdout.contains("gemini -> Gem User <gem@example.com>"),
            "missing gemini identity:\n{stdout}"
        );
    }

    fn cloak_bin() -> PathBuf {
        if let Some(path) = std::env::var_os("CARGO_BIN_EXE_cloak").map(PathBuf::from) {
            return path;
        }

        let current = std::env::current_exe().expect("failed to read current_exe");
        let debug_dir = if current
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|name| name.to_str())
            == Some("deps")
        {
            current
                .parent()
                .and_then(|p| p.parent())
                .expect("failed to resolve target/debug directory")
                .to_path_buf()
        } else {
            current
                .parent()
                .expect("failed to resolve current executable parent")
                .to_path_buf()
        };

        let candidate = debug_dir.join("cloak");
        if candidate.is_file() {
            return candidate;
        }

        panic!(
            "could not resolve cloak binary. checked env CARGO_BIN_EXE_cloak and {}",
            candidate.display()
        );
    }

    fn create_mock_binary(bin_dir: &Path) -> PathBuf {
        let path = bin_dir.join("mock-cli.sh");
        let script = r#"#!/bin/sh
echo "MOCK_HOME=$MOCK_HOME"
echo "GEMINI_CLI_HOME=$GEMINI_CLI_HOME"
if [ -z "${OPENAI_API_KEY+x}" ]; then
  echo "OPENAI_API_KEY=<unset>"
else
  echo "OPENAI_API_KEY=$OPENAI_API_KEY"
fi
if [ -z "${GEMINI_API_KEY+x}" ]; then
  echo "GEMINI_API_KEY=<unset>"
else
  echo "GEMINI_API_KEY=$GEMINI_API_KEY"
fi
if [ -z "${GOOGLE_API_KEY+x}" ]; then
  echo "GOOGLE_API_KEY=<unset>"
else
  echo "GOOGLE_API_KEY=$GOOGLE_API_KEY"
fi
echo "ARGS=$*"
"#;

        fs::write(&path, script).expect("write mock script");

        let mut perms = fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("chmod");

        path
    }

    fn write_config(xdg_config_home: &Path, mock_binary: &Path, default_profile: &str) {
        let cloak_dir = xdg_config_home.join("cloak");
        fs::create_dir_all(&cloak_dir).expect("create cloak config dir");

        let config = format!(
            "[general]\ndefault_profile = \"{}\"\n\n[cli.mock]\nbinary = \"{}\"\nconfig_dir_env = \"MOCK_HOME\"\nremove_env_vars = [\"OPENAI_API_KEY\"]\n",
            default_profile,
            mock_binary.display()
        );

        fs::write(cloak_dir.join("config.toml"), config).expect("write config.toml");
    }

    fn write_gemini_config(xdg_config_home: &Path, mock_binary: &Path, default_profile: &str) {
        let cloak_dir = xdg_config_home.join("cloak");
        fs::create_dir_all(&cloak_dir).expect("create cloak config dir");

        let config = format!(
            "[general]\ndefault_profile = \"{}\"\n\n[cli.gemini]\nbinary = \"{}\"\nconfig_dir_env = \"GEMINI_CLI_HOME\"\nremove_env_vars = [\"GEMINI_API_KEY\", \"GOOGLE_API_KEY\"]\n",
            default_profile,
            mock_binary.display()
        );

        fs::write(cloak_dir.join("config.toml"), config).expect("write config.toml");
    }

    fn write_standard_config(xdg_config_home: &Path) {
        let cloak_dir = xdg_config_home.join("cloak");
        fs::create_dir_all(&cloak_dir).expect("create cloak config dir");

        let config = r#"[general]
default_profile = "personal"

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"

[cli.codex]
binary = "codex"
config_dir_env = "CODEX_HOME"

[cli.gemini]
binary = "gemini"
config_dir_env = "GEMINI_CLI_HOME"
"#;

        fs::write(cloak_dir.join("config.toml"), config).expect("write config.toml");
    }

    fn fake_jwt(claims: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
        format!("{header}.{payload}.signature")
    }
}
