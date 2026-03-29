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
    fn exec_applies_templated_launch_args_and_extra_env() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let repo = tmp.path().join("repo-cursor");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&repo).expect("create repo dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_editor_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(repo.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let output = Command::new(cloak_bin())
            .arg("exec")
            .arg("cursor")
            .arg("--")
            .arg("repo-cursor")
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
            .join("work")
            .join("cursor");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!(
                "CURSOR_USER_DATA_DIR={}",
                expected_profile_home.display()
            )),
            "missing CURSOR_USER_DATA_DIR in stdout:\n{stdout}"
        );
        assert!(
            stdout.contains(&format!(
                "CURSOR_EXTENSIONS_DIR={}/extensions",
                expected_profile_home.display()
            )),
            "missing CURSOR_EXTENSIONS_DIR in stdout:\n{stdout}"
        );
        assert!(
            stdout.contains(&format!(
                "ARGS=--user-data-dir {} --extensions-dir {}/extensions --new-window repo-cursor",
                expected_profile_home.display(),
                expected_profile_home.display()
            )),
            "launch args were not rendered as expected:\n{stdout}"
        );
    }

    #[test]
    fn exec_cursor_defaults_to_current_directory_when_no_target_is_forwarded() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let repo = tmp.path().join("repo-cursor-default-dir");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&repo).expect("create repo dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_editor_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(repo.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let output = Command::new(cloak_bin())
            .arg("exec")
            .arg("cursor")
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
            .join("work")
            .join("cursor");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!(
                "ARGS=--user-data-dir {} --extensions-dir {}/extensions --new-window .",
                expected_profile_home.display(),
                expected_profile_home.display()
            )),
            "cursor should default to current directory when no target is passed:\n{stdout}"
        );
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
    fn profile_list_renders_table_with_default_marker() {
        let tmp = tempdir().expect("tempdir");
        let xdg_config_home = tmp.path().join("xdg");
        let profiles_root = xdg_config_home.join("cloak").join("profiles");

        write_standard_config(&xdg_config_home);
        fs::create_dir_all(profiles_root.join("personal")).expect("create personal profile");
        fs::create_dir_all(profiles_root.join("work")).expect("create work profile");

        let output = Command::new(cloak_bin())
            .arg("profile")
            .arg("list")
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak profile list");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Profiles"), "missing heading:\n{stdout}");
        assert!(
            stdout.contains("  Default: personal"),
            "missing default line:\n{stdout}"
        );
        assert!(
            stdout.contains("  Count: 2"),
            "missing count line:\n{stdout}"
        );
        assert!(
            stdout.contains("Profile"),
            "missing table header:\n{stdout}"
        );
        assert!(
            stdout.contains("personal"),
            "missing personal row:\n{stdout}"
        );
        assert!(stdout.contains("work"), "missing work row:\n{stdout}");
        assert!(stdout.contains("yes"), "missing default marker:\n{stdout}");
    }

    #[test]
    fn profile_show_renders_cli_configuration_table() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let repo = tmp.path().join("repo-show");
        let xdg_config_home = tmp.path().join("xdg");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&repo).expect("create repo dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_editor_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(repo.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let output = Command::new(cloak_bin())
            .arg("profile")
            .arg("show")
            .current_dir(&repo)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak profile show");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Profile 'work'"),
            "missing profile heading:\n{stdout}"
        );
        assert!(
            stdout.contains("  Source: from"),
            "missing source line:\n{stdout}"
        );
        assert!(
            stdout.contains("CLI Configuration"),
            "missing configuration section:\n{stdout}"
        );
        assert!(stdout.contains("Cursor"), "missing cursor row:\n{stdout}");
        assert!(
            stdout.contains("CURSOR_USER_DATA_DIR"),
            "missing extra env row:\n{stdout}"
        );
        assert!(
            stdout.contains("launch_args"),
            "missing launch args row:\n{stdout}"
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
            stdout.contains("Accounts"),
            "missing accounts section:\n{stdout}"
        );
        assert!(stdout.contains("Claude"), "missing claude row:\n{stdout}");
        assert!(
            stdout.contains("credentials detected, but account identifier unavailable"),
            "missing claude output:\n{stdout}"
        );
        assert!(stdout.contains("Codex"), "missing codex row:\n{stdout}");
        assert!(
            stdout.contains("Jane Doe <jane@example.com>"),
            "missing codex identity:\n{stdout}"
        );
        assert!(stdout.contains("Gemini"), "missing gemini row:\n{stdout}");
        assert!(
            stdout.contains("Gem User <gem@example.com>"),
            "missing gemini identity:\n{stdout}"
        );
    }

    #[test]
    fn profile_limits_shows_codex_rate_limits() {
        let tmp = tempdir().expect("tempdir");
        let xdg_config_home = tmp.path().join("xdg");
        let profiles_root = xdg_config_home.join("cloak").join("profiles");
        let work_dir = profiles_root.join("work");

        write_standard_config(&xdg_config_home);

        fs::create_dir_all(work_dir.join("codex/sessions/2026/03/28")).expect("create codex dir");
        fs::write(
            work_dir.join("codex/auth.json"),
            json!({
                "auth_mode": "chatgpt",
                "tokens": {
                    "account_id": "acct_123"
                }
            })
            .to_string(),
        )
        .expect("write codex auth");

        let session = json!({
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
                    }
                }
            }
        });

        fs::write(
            work_dir.join("codex/sessions/2026/03/28/rollout-a.jsonl"),
            format!("{session}\n"),
        )
        .expect("write codex session");

        let output = Command::new(cloak_bin())
            .arg("profile")
            .arg("limits")
            .arg("work")
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak profile limits");

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
        assert!(stdout.contains("Codex"), "missing codex summary:\n{stdout}");
        assert!(
            stdout.contains("  Status: usage snapshot available"),
            "missing codex status:\n{stdout}"
        );
        assert!(
            stdout.contains("  Details: plan: team"),
            "missing codex detail:\n{stdout}"
        );
        assert!(
            stdout.contains("  Observed: 2026-03-28T15:23:12.299Z"),
            "missing observed timestamp:\n{stdout}"
        );
        assert!(
            stdout.contains("  Limit: Codex Team"),
            "missing limit label:\n{stdout}"
        );
        assert!(stdout.contains("╭"), "missing table border:\n{stdout}");
        assert!(
            stdout.contains("primary"),
            "missing primary limit window:\n{stdout}"
        );
        assert!(
            stdout.contains("5h"),
            "missing primary window shorthand:\n{stdout}"
        );
        assert!(
            stdout.contains("99%"),
            "missing remaining percentage:\n{stdout}"
        );
        assert!(
            stdout.contains("2026-03-28 17:42:39 UTC"),
            "missing primary reset timestamp:\n{stdout}"
        );
        assert!(
            stdout.contains("secondary"),
            "missing secondary limit window:\n{stdout}"
        );
    }

    #[test]
    fn profile_limits_shows_claude_rate_limits() {
        let tmp = tempdir().expect("tempdir");
        let xdg_config_home = tmp.path().join("xdg");
        let profiles_root = xdg_config_home.join("cloak").join("profiles");
        let work_dir = profiles_root.join("work");

        write_standard_config(&xdg_config_home);

        fs::create_dir_all(work_dir.join("claude")).expect("create claude dir");
        fs::write(
            work_dir.join("claude/.credentials.json"),
            json!({
                "claudeAiOauth": {
                    "subscriptionType": "team",
                    "rateLimitTier": "default_raven"
                }
            })
            .to_string(),
        )
        .expect("write claude credentials");
        fs::write(
            work_dir.join("claude/usage-limits.json"),
            json!({
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
            })
            .to_string(),
        )
        .expect("write claude usage snapshot");

        let output = Command::new(cloak_bin())
            .arg("profile")
            .arg("limits")
            .arg("work")
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak profile limits");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Claude"),
            "missing claude summary:\n{stdout}"
        );
        assert!(
            stdout.contains("  Status: usage snapshot available"),
            "missing claude status:\n{stdout}"
        );
        assert!(
            stdout.contains("  Details: plan: team, tier: default_raven"),
            "missing claude details:\n{stdout}"
        );
        assert!(
            stdout.contains("  Observed: 2026-03-28T18:12:44Z"),
            "missing claude observed timestamp:\n{stdout}"
        );
        assert!(stdout.contains("╭"), "missing table border:\n{stdout}");
        assert!(
            stdout.contains("five_hour"),
            "missing claude five-hour window:\n{stdout}"
        );
        assert!(
            stdout.contains("12.5%"),
            "missing claude used percentage:\n{stdout}"
        );
        assert!(
            stdout.contains("2026-03-28 17:42:39 UTC"),
            "missing claude reset timestamp:\n{stdout}"
        );
        assert!(
            stdout.contains("seven_day"),
            "missing claude seven-day window:\n{stdout}"
        );
    }

    #[test]
    fn doctor_renders_tables_for_binaries_and_profiles() {
        let tmp = tempdir().expect("tempdir");
        let bin_dir = tmp.path().join("bin");
        let xdg_config_home = tmp.path().join("xdg");
        let profiles_root = xdg_config_home.join("cloak").join("profiles");
        let work_dir = profiles_root.join("work").join("mock");

        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&work_dir).expect("create profile dir");

        let mock_binary = create_mock_binary(&bin_dir);
        write_config(&xdg_config_home, &mock_binary, "personal");
        fs::write(work_dir.join("some-config.json"), "{}").expect("write hint file");

        let output = Command::new(cloak_bin())
            .arg("doctor")
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .output()
            .expect("run cloak doctor");

        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Recommended CLI Blocks"),
            "missing recommended blocks section:\n{stdout}"
        );
        assert!(
            stdout.contains("Doctor"),
            "missing doctor heading:\n{stdout}"
        );
        assert!(
            stdout.contains("Summary"),
            "missing summary section:\n{stdout}"
        );
        assert!(
            stdout.contains("Binaries"),
            "missing binaries section:\n{stdout}"
        );
        assert!(
            stdout.contains("Profiles"),
            "missing profiles section:\n{stdout}"
        );
        assert!(
            stdout.contains("Profile 'work'"),
            "missing per-profile section:\n{stdout}"
        );
        assert!(
            stdout.contains("  CLI blocks: 1"),
            "missing cli block summary:\n{stdout}"
        );
        assert!(
            stdout.contains("  Binaries: 1 found, 0 missing"),
            "missing binaries summary:\n{stdout}"
        );
        assert!(stdout.contains("mock"), "missing mock row:\n{stdout}");
        assert!(stdout.contains("found"), "missing binary status:\n{stdout}");
        assert!(
            stdout.contains("/bin/mock-cli.sh"),
            "missing binary location:\n{stdout}"
        );
        assert!(
            stdout.contains("credentials detected"),
            "missing credential hint:\n{stdout}"
        );
        assert!(
            stdout.contains("Mock"),
            "missing profile cli row:\n{stdout}"
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
echo "CURSOR_USER_DATA_DIR=$CURSOR_USER_DATA_DIR"
echo "CURSOR_EXTENSIONS_DIR=$CURSOR_EXTENSIONS_DIR"
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

    fn write_editor_config(xdg_config_home: &Path, mock_binary: &Path, default_profile: &str) {
        let cloak_dir = xdg_config_home.join("cloak");
        fs::create_dir_all(&cloak_dir).expect("create cloak config dir");

        let config = format!(
            "[general]\ndefault_profile = \"{}\"\n\n[cli.cursor]\nbinary = \"{}\"\nremove_env_vars = [\"OPENAI_API_KEY\"]\nlaunch_args = [\"--user-data-dir\", \"{{profile_dir}}\", \"--extensions-dir\", \"{{profile_dir}}/extensions\", \"--new-window\"]\n\n[cli.cursor.extra_env]\nCURSOR_USER_DATA_DIR = \"{{profile_dir}}\"\nCURSOR_EXTENSIONS_DIR = \"{{profile_dir}}/extensions\"\n",
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
