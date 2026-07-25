#[cfg(unix)]
mod backup_tests {
    use std::{
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    use tempfile::tempdir;

    fn cloak_bin() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_cloak"))
    }

    fn gpg_available() -> bool {
        which::which("gpg").is_ok()
    }

    fn write_config(xdg: &Path) {
        let cloak_dir = xdg.join("cloak");
        fs::create_dir_all(&cloak_dir).expect("mkdir cloak");
        fs::write(
            cloak_dir.join("config.toml"),
            r#"[general]
default_profile = "demo"

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"
"#,
        )
        .expect("write config");
    }

    fn seed_profile(xdg: &Path) {
        let claude = xdg.join("cloak/profiles/demo/claude");
        fs::create_dir_all(claude.join("skills")).expect("mkdir");
        fs::create_dir_all(claude.join("sessions")).expect("mkdir sessions");
        fs::write(claude.join("settings.json"), r#"{"theme":"dark"}"#).expect("settings");
        fs::write(claude.join("skills/a.md"), "skill a").expect("skill");
        fs::write(claude.join("sessions/log.jsonl"), "junk").expect("session");
        fs::write(claude.join(".claude.json"), r#"{"mcpServers":{"time":{}}}"#)
            .expect("claude.json");
    }

    #[test]
    fn backup_then_restore_roundtrip() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let out_dir = tmp.path().join("backups");

        // backup
        let status = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .status()
            .expect("run backup");
        assert!(status.success(), "backup failed");

        // localizar o artefato
        let artifact = fs::read_dir(&out_dir)
            .expect("read out_dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "gpg").unwrap_or(false))
            .expect("artifact created");

        // apagar o perfil e restaurar
        fs::remove_dir_all(xdg.join("cloak/profiles/demo")).expect("rm profile");
        let status = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .status()
            .expect("run restore");
        assert!(status.success(), "restore failed");

        // conferir: incluído restaurado, lixo não
        let claude = xdg.join("cloak/profiles/demo/claude");
        assert!(claude.join("settings.json").exists(), "settings restored");
        assert!(claude.join("skills/a.md").exists(), "skill restored");
        assert!(
            !claude.join("sessions/log.jsonl").exists(),
            "session NOT restored"
        );
        assert!(
            !claude.join(".claude.json").exists(),
            ".claude.json NOT in backup"
        );
    }

    #[test]
    fn dry_run_writes_no_artifact() {
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let out_dir = tmp.path().join("backups");

        let status = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .arg("--dry-run")
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .status()
            .expect("run backup dry-run");
        assert!(status.success());
        assert!(
            !out_dir.exists()
                || fs::read_dir(&out_dir)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(true),
            "dry-run must not write artifacts"
        );
    }

    #[test]
    fn restore_refuses_existing_profile_without_force() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let out_dir = tmp.path().join("backups");

        let status = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .status()
            .expect("run backup");
        assert!(status.success());

        let artifact = fs::read_dir(&out_dir)
            .expect("read out")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "gpg").unwrap_or(false))
            .expect("artifact");

        // perfil ainda existe → restore sem --force deve falhar
        let status = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .status()
            .expect("run restore");
        assert!(
            !status.success(),
            "restore must refuse existing profile without --force"
        );
    }

    #[test]
    fn backup_real_run_produces_encrypted_artifact() {
        // REGRESSAO: o backup real ja esteve quebrado (tar empacotando o
        // proprio arquivo de saida) e nenhuma revisao de codigo pegou.
        // Este teste exercita o caminho completo, nao apenas --dry-run.
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let out_dir = tmp.path().join("backups");

        let output = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run backup");

        assert!(
            output.status.success(),
            "backup did not complete successfully — this is exactly the regression \
             where tar packaged its own growing output file and aborted with \
             'file changed as we read it' (exit 1)\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let artifacts: Vec<PathBuf> = fs::read_dir(&out_dir)
            .expect("read out_dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "gpg").unwrap_or(false))
            .collect();
        assert_eq!(
            artifacts.len(),
            1,
            "expected exactly one .gpg artifact in {}, found {:?}",
            out_dir.display(),
            artifacts
        );
        let artifact = &artifacts[0];

        let bytes = fs::read(artifact).expect("read artifact bytes");
        assert!(
            bytes.len() >= 2,
            "artifact suspiciously small ({} bytes): backup likely produced an empty file",
            bytes.len()
        );
        assert_ne!(
            &bytes[0..2],
            &[0x1f, 0x8b],
            "artifact starts with the gzip magic bytes — it is a plain tar.gz, NOT \
             encrypted; the gpg step did not actually run (or ran on the wrong file)"
        );

        // Confirma tambem que o artefato nao e' extraivel diretamente como
        // tar.gz: se fosse, a cifragem simplesmente nao aconteceu.
        let direct_list = Command::new("tar")
            .arg("-tzf")
            .arg(artifact)
            .output()
            .expect("run tar -tzf on artifact");
        assert!(
            !direct_list.status.success(),
            "artifact was directly extractable with `tar -tzf`; it must be gpg-encrypted"
        );

        let mode = fs::metadata(artifact)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "backup artifact must be 0600, got {mode:o}");
    }

    fn seed_profile_with_uncovered_entries(xdg: &Path) {
        let claude = xdg.join("cloak/profiles/demo/claude");
        fs::create_dir_all(claude.join("skills")).expect("mkdir skills");
        fs::create_dir_all(claude.join("sessions")).expect("mkdir sessions");
        fs::write(claude.join("settings.json"), r#"{"theme":"dark"}"#).expect("settings");
        fs::write(claude.join("skills/a.md"), "skill a").expect("skill");
        fs::write(claude.join("sessions/log.jsonl"), "junk").expect("session");
        fs::write(claude.join("mystery.bin"), "???").expect("mystery");
    }

    fn gpg_decrypt_to(archive: &Path, dest: &Path, passphrase: &str) {
        let mut child = Command::new("gpg")
            .arg("--batch")
            .arg("--yes")
            .arg("--pinentry-mode")
            .arg("loopback")
            .arg("--passphrase-fd")
            .arg("0")
            .arg("-o")
            .arg(dest)
            .arg("--decrypt")
            .arg(archive)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn gpg decrypt");
        child
            .stdin
            .take()
            .expect("gpg stdin")
            .write_all(passphrase.as_bytes())
            .expect("write passphrase to gpg");
        let output = child.wait_with_output().expect("wait gpg decrypt");
        assert!(
            output.status.success(),
            "gpg decrypt failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn tar_list_entries(archive: &Path) -> Vec<String> {
        let output = Command::new("tar")
            .arg("-tzf")
            .arg(archive)
            .output()
            .expect("run tar -tzf");
        assert!(
            output.status.success(),
            "tar -tzf failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.trim_start_matches("./").to_string())
            .collect()
    }

    fn find_artifact(out_dir: &Path) -> PathBuf {
        fs::read_dir(out_dir)
            .expect("read out_dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "gpg").unwrap_or(false))
            .expect("artifact created")
    }

    #[test]
    fn backup_allowlist_filters_report_and_archive_contents() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile_with_uncovered_entries(&xdg);
        let out_dir = tmp.path().join("backups");

        let output = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run backup");
        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("sessions"),
            "report must list the fully-uncovered sessions dir:\n{stdout}"
        );
        assert!(
            stdout.contains("mystery.bin"),
            "report must list the uncovered mystery.bin file:\n{stdout}"
        );

        let artifact = find_artifact(&out_dir);
        let tar_tmp = tmp.path().join("decrypted.tar.gz");
        gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
        let entries = tar_list_entries(&tar_tmp);

        assert!(
            entries
                .iter()
                .any(|e| e == "profiles/demo/claude/settings.json"),
            "settings.json missing from archive: {entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|e| e == "profiles/demo/claude/skills/a.md"),
            "skills/a.md missing from archive: {entries:?}"
        );
        assert!(
            !entries.iter().any(|e| e.contains("sessions/log.jsonl")),
            "sessions/log.jsonl must NOT be in archive: {entries:?}"
        );
        assert!(
            !entries.iter().any(|e| e.contains("mystery.bin")),
            "mystery.bin must NOT be in archive: {entries:?}"
        );
    }

    fn seed_profile_with_credentials(xdg: &Path) {
        let claude = xdg.join("cloak/profiles/demo/claude");
        fs::create_dir_all(&claude).expect("mkdir claude");
        fs::write(claude.join("settings.json"), r#"{"theme":"dark"}"#).expect("settings");
        fs::write(
            claude.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"tok"}}"#,
        )
        .expect("credentials");
    }

    #[test]
    fn backup_excludes_credentials_by_default_and_includes_with_flag() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }

        // Sem a flag: credenciais nao entram no artefato.
        {
            let tmp = tempdir().expect("tempdir");
            let xdg = tmp.path().join("xdg");
            write_config(&xdg);
            seed_profile_with_credentials(&xdg);
            let out_dir = tmp.path().join("backups");

            let status = Command::new(cloak_bin())
                .arg("backup")
                .arg("--output")
                .arg(&out_dir)
                .env("XDG_CONFIG_HOME", &xdg)
                .env("HOME", tmp.path())
                .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
                .status()
                .expect("run backup");
            assert!(status.success(), "backup failed");

            let artifact = find_artifact(&out_dir);
            let tar_tmp = tmp.path().join("decrypted.tar.gz");
            gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
            let entries = tar_list_entries(&tar_tmp);
            assert!(
                !entries.iter().any(|e| e.contains(".credentials.json")),
                "credentials must NOT be in archive without --include-credentials: {entries:?}"
            );
        }

        // Com a flag: credenciais entram no artefato.
        {
            let tmp = tempdir().expect("tempdir");
            let xdg = tmp.path().join("xdg");
            write_config(&xdg);
            seed_profile_with_credentials(&xdg);
            let out_dir = tmp.path().join("backups");

            let status = Command::new(cloak_bin())
                .arg("backup")
                .arg("--output")
                .arg(&out_dir)
                .arg("--include-credentials")
                .env("XDG_CONFIG_HOME", &xdg)
                .env("HOME", tmp.path())
                .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
                .status()
                .expect("run backup");
            assert!(status.success(), "backup failed");

            let artifact = find_artifact(&out_dir);
            let tar_tmp = tmp.path().join("decrypted.tar.gz");
            gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
            let entries = tar_list_entries(&tar_tmp);
            assert!(
                entries.iter().any(|e| e.contains(".credentials.json")),
                "credentials must be in archive with --include-credentials: {entries:?}"
            );
        }
    }

    #[test]
    fn restore_sets_secure_directory_and_file_permissions() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let out_dir = tmp.path().join("backups");

        let status = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .status()
            .expect("run backup");
        assert!(status.success(), "backup failed");

        let artifact = find_artifact(&out_dir);

        fs::remove_dir_all(xdg.join("cloak/profiles/demo")).expect("rm profile");

        let status = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .status()
            .expect("run restore");
        assert!(status.success(), "restore failed");

        let profile_dir = xdg.join("cloak/profiles/demo");
        let claude_dir = profile_dir.join("claude");
        let skills_dir = claude_dir.join("skills");
        let settings_file = claude_dir.join("settings.json");
        let skill_file = skills_dir.join("a.md");

        for dir in [&profile_dir, &claude_dir, &skills_dir] {
            let mode = fs::metadata(dir).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o700,
                "restored directory {} must be 0700, got {mode:o}",
                dir.display()
            );
        }
        for file in [&settings_file, &skill_file] {
            let mode = fs::metadata(file).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o600,
                "restored file {} must be 0600, got {mode:o}",
                file.display()
            );
        }
    }

    #[test]
    fn backup_includes_cloak_file_at_profile_root() {
        // REGRESSAO: arquivos na raiz do perfil eram ignorados pelo laco de CLIs,
        // ficando fora do backup E fora do relatorio de nao-cobertos.
        // Monte um perfil que tenha `.cloak` na raiz, rode backup real,
        // decifre o artefato e assegure que `.cloak` esta la.
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let profile_root_cloak_file = xdg.join("cloak/profiles/demo/.cloak");
        fs::write(&profile_root_cloak_file, "demo\n").expect("write .cloak");
        let out_dir = tmp.path().join("backups");

        let output = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run backup");
        assert!(
            output.status.success(),
            "backup failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("(tudo coberto pela allowlist)"),
            "report must not claim full coverage while a loose non-.cloak \
             invariant would be violated if .cloak were missing; got:\n{stdout}"
        );

        let artifact = find_artifact(&out_dir);
        let tar_tmp = tmp.path().join("decrypted.tar.gz");
        gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
        let entries = tar_list_entries(&tar_tmp);

        assert!(
            entries.iter().any(|e| e == "profiles/demo/.cloak"),
            ".cloak at profile root missing from archive: {entries:?}"
        );
    }

    #[test]
    fn backup_does_not_leave_partial_artifact_on_encryption_failure() {
        // Falha de cifragem nao pode deixar arquivo com o nome final no output_dir.
        // Provoca a falha com um binario `gpg` falso no PATH que sempre sai com
        // status != 0 (determinístico, sem dependencia de timing/sinais).
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let out_dir = tmp.path().join("backups");

        // PATH com um `gpg` falso na frente, que sempre falha, mas preserva o
        // restante do PATH real para que `tar` e outras ferramentas continuem
        // resolviveis.
        let fake_bin_dir = tmp.path().join("fake-bin");
        fs::create_dir_all(&fake_bin_dir).expect("mkdir fake-bin");
        let fake_gpg = fake_bin_dir.join("gpg");
        fs::write(
            &fake_gpg,
            "#!/bin/sh\necho 'fake gpg: passphrase rejected' >&2\nexit 1\n",
        )
        .expect("write fake gpg");
        let mut perms = fs::metadata(&fake_gpg).expect("metadata").permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&fake_gpg, perms).expect("chmod fake gpg");

        let real_path = std::env::var("PATH").unwrap_or_default();
        let fake_path = format!("{}:{}", fake_bin_dir.display(), real_path);

        let output = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .env("PATH", fake_path)
            .output()
            .expect("run backup");

        assert!(
            !output.status.success(),
            "backup must fail when gpg fails\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let leftovers: Vec<PathBuf> = if out_dir.exists() {
            fs::read_dir(&out_dir)
                .expect("read out_dir")
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    name.contains(".gpg")
                })
                .collect()
        } else {
            Vec::new()
        };
        assert!(
            leftovers.is_empty(),
            "no .gpg or .gpg.partial file may remain in output_dir after an \
             encryption failure; found: {leftovers:?}"
        );
    }

    #[test]
    fn restore_wrong_passphrase_fails() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let out_dir = tmp.path().join("backups");

        let status = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "correct-pass")
            .status()
            .expect("run backup");
        assert!(status.success(), "backup failed");

        let artifact = find_artifact(&out_dir);

        fs::remove_dir_all(xdg.join("cloak/profiles/demo")).expect("rm profile");

        let output = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "wrong-pass")
            .output()
            .expect("run restore");
        assert!(
            !output.status.success(),
            "restore with wrong passphrase must fail\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            !xdg.join("cloak/profiles/demo").exists(),
            "profile must not exist after a failed restore"
        );
    }
}
