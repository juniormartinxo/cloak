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
    fn dry_run_does_not_create_staging_or_copy_anything() {
        // REGRESSAO: o laco de copia dos incluidos, a copia das credenciais, a
        // copia do config global e a escrita do manifesto rodavam TODOS antes
        // do ramo de dry-run. Em perfil real isso e' uma copia de varios
        // megabytes para diretorio temporario, apagada logo em seguida — e a
        // documentacao anuncia o dry-run como "sem gerar nenhum arquivo".
        //
        // O controle e' o TMPDIR inexistente: o backup real falha ao criar o
        // staging, entao um dry-run que passa prova que ele nao criou staging
        // nem copiou nada.
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile_with_uncovered_entries(&xdg);
        let out_dir = tmp.path().join("backups");
        let missing_tmpdir = tmp.path().join("tmpdir-que-nao-existe");

        let output = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .arg("--dry-run")
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("TMPDIR", &missing_tmpdir)
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run backup dry-run");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "dry-run nao pode depender de staging\nstdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // E continua sendo um relatorio util, nao um curto-circuito.
        assert!(
            stdout.contains("mystery.bin") && stdout.contains("sessions"),
            "o dry-run precisa continuar imprimindo o relatorio:\n{stdout}"
        );
        assert!(
            !missing_tmpdir.exists(),
            "o dry-run nao pode criar area de staging"
        );

        // Controle: o backup real precisa do staging e falha com o mesmo TMPDIR.
        let output = Command::new(cloak_bin())
            .arg("backup")
            .arg("--output")
            .arg(&out_dir)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("TMPDIR", &missing_tmpdir)
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run backup");
        assert!(
            !output.status.success(),
            "controle invalido: o backup real deveria falhar sem TMPDIR utilizavel"
        );
    }

    #[test]
    fn backup_does_not_chmod_a_preexisting_output_dir() {
        // REGRESSAO: `paths::ensure_secure_dir` roda `set_owner_only_dir`
        // incondicional, entao `cloak backup --output ~/Downloads`, `--output .`
        // ou um `output_dir` sincronizado tinham as permissoes mutadas para
        // 0700 sem aviso e sem opt-out. O artefato ja e' 0600, entao o chmod do
        // diretorio nao agregava protecao.
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);

        // Diretorio pre-existente do usuario, compartilhado (0755).
        let out_dir = tmp.path().join("Downloads");
        fs::create_dir(&out_dir).expect("mkdir");
        fs::set_permissions(&out_dir, fs::Permissions::from_mode(0o755)).expect("chmod");

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

        let mode = fs::metadata(&out_dir)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o755,
            "o cloak nao pode mutar as permissoes de um diretorio que nao criou"
        );

        // O artefato continua sendo o que protege o conteudo.
        let artifact = find_artifact(&out_dir);
        let artifact_mode = fs::metadata(&artifact)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(artifact_mode, 0o600, "o artefato continua 0600");
    }

    #[test]
    fn restore_dry_run_prints_plan_over_existing_profile() {
        // REGRESSAO: a checagem de uid e a de perfil ja existente rodavam ANTES
        // do ramo de dry-run, entao `cloak restore <artefato> --dry-run` numa
        // maquina que ainda tem o perfil falhava com "already exists" e nunca
        // imprimia o plano. O unico contorno era `--force --dry-run`, o que
        // treina o usuario a digitar --force em restores reais.
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

        // O perfil continua no destino, com conteudo DIFERENTE do artefato:
        // um restore real sobrescreveria; o dry-run nao pode tocar.
        let settings = xdg.join("cloak/profiles/demo/claude/settings.json");
        fs::write(&settings, r#"{"theme":"MODIFICADO"}"#).expect("modify settings");

        let output = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .arg("--dry-run")
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run restore dry-run");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "dry-run precisa imprimir o plano em vez de abortar\nstdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("demo"),
            "o plano precisa nomear o perfil:\n{stdout}"
        );
        assert!(
            stdout.contains("conflito"),
            "o conflito detectado precisa aparecer no plano:\n{stdout}"
        );
        assert!(
            stdout.contains("--force"),
            "o plano precisa dizer o que um restore real exigiria:\n{stdout}"
        );
        assert_eq!(
            fs::read_to_string(&settings).expect("read settings"),
            r#"{"theme":"MODIFICADO"}"#,
            "o dry-run nao pode tocar no destino"
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

    fn tar_extract_to(archive: &Path, dest: &Path) {
        fs::create_dir_all(dest).expect("mkdir extract dest");
        let output = Command::new("tar")
            .arg("-xzf")
            .arg(archive)
            .arg("-C")
            .arg(dest)
            .output()
            .expect("run tar -xzf");
        assert!(
            output.status.success(),
            "tar -xzf failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Lê o `ProfileManifest.uncovered` serializado dentro do artefato.
    fn manifest_uncovered_paths(artifact: &Path, tmp: &Path, passphrase: &str) -> Vec<String> {
        let tar_tmp = tmp.join("manifest-check.tar.gz");
        gpg_decrypt_to(artifact, &tar_tmp, passphrase);
        let extracted = tmp.join("manifest-check");
        tar_extract_to(&tar_tmp, &extracted);
        let raw = fs::read_to_string(extracted.join("manifest.json")).expect("read manifest.json");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parse manifest.json");
        value["profiles"]
            .as_array()
            .expect("profiles array")
            .iter()
            .flat_map(|p| {
                p["uncovered"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
            })
            .filter_map(|u| u["path"].as_str().map(str::to_string))
            .collect()
    }

    fn tar_create_from(dir: &Path, archive: &Path) {
        let output = Command::new("tar")
            .arg("-czf")
            .arg(archive)
            .arg("-C")
            .arg(dir)
            .arg(".")
            .output()
            .expect("run tar -czf");
        assert!(
            output.status.success(),
            "tar -czf failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn gpg_encrypt_to(plain: &Path, dest: &Path, passphrase: &str) {
        let mut child = Command::new("gpg")
            .arg("--batch")
            .arg("--yes")
            .arg("--pinentry-mode")
            .arg("loopback")
            .arg("--passphrase-fd")
            .arg("0")
            .arg("--symmetric")
            .arg("-o")
            .arg(dest)
            .arg(plain)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn gpg encrypt");
        child
            .stdin
            .take()
            .expect("gpg stdin")
            .write_all(passphrase.as_bytes())
            .expect("write passphrase to gpg");
        let output = child.wait_with_output().expect("wait gpg encrypt");
        assert!(
            output.status.success(),
            "gpg encrypt failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
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

        // `statusline-command.sh` esta' na allowlist do claude e e' criado pelo
        // proprio cloak com 0700. Ele precisa voltar do restore executavel.
        let statusline = xdg.join("cloak/profiles/demo/claude/statusline-command.sh");
        fs::write(&statusline, "#!/bin/sh\necho hi\n").expect("write statusline");
        fs::set_permissions(&statusline, fs::Permissions::from_mode(0o700)).expect("chmod");

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
        let statusline_file = claude_dir.join("statusline-command.sh");

        for dir in [&profile_dir, &claude_dir, &skills_dir] {
            let mode = fs::metadata(dir).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o700,
                "restored directory {} must be 0700, got {mode:o}",
                dir.display()
            );
        }
        // Arquivo comum: 0600.
        for file in [&settings_file, &skill_file] {
            let mode = fs::metadata(file).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o600,
                "restored file {} must be 0600, got {mode:o}",
                file.display()
            );
        }
        // REGRESSAO: o restore forcava 0600 em TODO arquivo e o executavel
        // voltava sem permissao de execucao — "Permission denied" no 1o uso.
        let mode = fs::metadata(&statusline_file)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode,
            0o700,
            "executable {} must come back 0700, got {mode:o}",
            statusline_file.display()
        );
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

    fn seed_gemini_profile(xdg: &Path) {
        let gemini = xdg.join("cloak/profiles/demo/gemini/.gemini");
        fs::create_dir_all(gemini.join("tmp")).expect("mkdir .gemini/tmp");
        fs::create_dir_all(gemini.join("history")).expect("mkdir .gemini/history");
        fs::write(
            gemini.join("settings.json"),
            r#"{"model":{"name":"gemini-3.1-pro-preview"}}"#,
        )
        .expect("gemini settings");
        fs::write(gemini.join("GEMINI.md"), "memoria do gemini").expect("GEMINI.md");
        fs::write(gemini.join("oauth_creds.json"), r#"{"id_token":"x"}"#).expect("creds");
        fs::write(gemini.join("installation_id"), "abc").expect("installation_id");
        fs::write(gemini.join("tmp/scratch.json"), "lixo").expect("tmp file");
        fs::write(gemini.join("history/h.json"), "lixo").expect("history file");
    }

    #[test]
    fn backup_and_restore_cover_gemini_profile() {
        // REGRESSAO: `allowlist_patterns` so tinha arm para claude e codex, e o
        // gemini aninha tudo em `gemini/.gemini/`. O artefato saia SEM NADA de
        // um perfil gemini e a subarvore inteira virava uma linha agregada
        // `gemini/.gemini` no relatorio.
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_gemini_profile(&xdg);
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
            !stdout.contains("gemini/.gemini ("),
            "a subarvore .gemini inteira nao pode ser reportada como fora:\n{stdout}"
        );

        let artifact = find_artifact(&out_dir);
        let tar_tmp = tmp.path().join("decrypted.tar.gz");
        gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
        let entries = tar_list_entries(&tar_tmp);

        assert!(
            entries
                .iter()
                .any(|e| e == "profiles/demo/gemini/.gemini/settings.json"),
            "settings.json do gemini fora do artefato: {entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|e| e == "profiles/demo/gemini/.gemini/GEMINI.md"),
            "GEMINI.md fora do artefato: {entries:?}"
        );
        assert!(
            !entries.iter().any(|e| e.contains("oauth_creds.json")),
            "credencial do gemini nao pode entrar sem --include-credentials: {entries:?}"
        );
        assert!(
            !entries.iter().any(|e| e.contains("/tmp/")),
            "lixo de tmp/ nao pode entrar: {entries:?}"
        );
        assert!(
            !entries.iter().any(|e| e.contains("installation_id")),
            "estado da maquina de origem nao pode entrar: {entries:?}"
        );

        // Restore devolve o conteudo do gemini.
        fs::remove_dir_all(xdg.join("cloak/profiles/demo")).expect("rm profile");
        let output = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run restore");
        assert!(
            output.status.success(),
            "restore failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let gemini = xdg.join("cloak/profiles/demo/gemini/.gemini");
        assert!(
            gemini.join("settings.json").exists(),
            "settings.json do gemini nao voltou no restore"
        );
        assert!(
            gemini.join("GEMINI.md").exists(),
            "GEMINI.md nao voltou no restore"
        );
    }

    #[test]
    fn backup_includes_gemini_credentials_with_flag() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_gemini_profile(&xdg);
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
            entries
                .iter()
                .any(|e| e == "profiles/demo/gemini/.gemini/oauth_creds.json"),
            "credencial do gemini precisa entrar com --include-credentials: {entries:?}"
        );
    }

    #[test]
    fn included_credentials_are_not_reported_as_uncovered() {
        // REGRESSAO: com --include-credentials, o stdout imprimia
        // "NAO incluido (fora da allowlist): claude/.credentials.json" e o
        // manifesto serializava a mesma mentira, para um arquivo que estava
        // dentro do payload.
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }

        // Com a flag: nem no stdout nem no manifesto.
        {
            let tmp = tempdir().expect("tempdir");
            let xdg = tmp.path().join("xdg");
            write_config(&xdg);
            seed_profile_with_credentials(&xdg);
            let out_dir = tmp.path().join("backups");

            let output = Command::new(cloak_bin())
                .arg("backup")
                .arg("--output")
                .arg(&out_dir)
                .arg("--include-credentials")
                .env("XDG_CONFIG_HOME", &xdg)
                .env("HOME", tmp.path())
                .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
                .output()
                .expect("run backup");
            assert!(output.status.success(), "backup failed");

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                !stdout.contains(".credentials.json"),
                "credencial dentro do payload nao pode ser listada como fora \
                 da allowlist:\n{stdout}"
            );

            let artifact = find_artifact(&out_dir);
            let uncovered = manifest_uncovered_paths(&artifact, tmp.path(), "test-pass");
            assert!(
                !uncovered.iter().any(|p| p.contains(".credentials.json")),
                "manifesto nao pode negar credencial que esta no payload: {uncovered:?}"
            );

            let tar_tmp = tmp.path().join("decrypted.tar.gz");
            gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
            let entries = tar_list_entries(&tar_tmp);
            assert!(
                entries
                    .iter()
                    .any(|e| e == "profiles/demo/claude/.credentials.json"),
                "credencial precisa estar no payload: {entries:?}"
            );
        }

        // Sem a flag: continua aparecendo como nao coberta nos dois lugares.
        {
            let tmp = tempdir().expect("tempdir");
            let xdg = tmp.path().join("xdg");
            write_config(&xdg);
            seed_profile_with_credentials(&xdg);
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
            assert!(output.status.success(), "backup failed");

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("claude/.credentials.json"),
                "sem a flag a credencial precisa continuar no relatorio:\n{stdout}"
            );

            let artifact = find_artifact(&out_dir);
            let uncovered = manifest_uncovered_paths(&artifact, tmp.path(), "test-pass");
            assert!(
                uncovered.iter().any(|p| p == "claude/.credentials.json"),
                "sem a flag a credencial precisa continuar no manifesto: {uncovered:?}"
            );
        }
    }

    /// Perfil cujo conteúdo é INTEIRAMENTE não-coberto: `profiles/<nome>/`
    /// nunca chega a ser criado no payload, mas o manifesto lista o perfil.
    fn seed_fully_uncovered_profile(xdg: &Path, name: &str) {
        let claude = xdg.join(format!("cloak/profiles/{name}/claude"));
        fs::create_dir_all(claude.join("sessions")).expect("mkdir sessions");
        fs::write(claude.join("sessions/log.jsonl"), "junk").expect("session");
    }

    #[test]
    fn restore_fails_when_no_profile_has_content_in_artifact() {
        // REGRESSAO: `if !src.exists() { continue; }` descartava em silencio um
        // perfil listado no manifesto sem diretorio no artefato. O restore
        // imprimia so o cabecalho, nao restaurava nada e retornava 0.
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_fully_uncovered_profile(&xdg, "demo");
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

        let output = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run restore");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "restore que nao restaurou NENHUM perfil nao pode sair com 0\nstdout:\n{}\nstderr:\n{stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains("demo"),
            "o perfil pulado precisa ser nomeado:\n{stderr}"
        );
        assert!(
            stderr.contains("nenhum perfil foi restaurado"),
            "o motivo precisa ser explicito:\n{stderr}"
        );
    }

    #[test]
    fn restore_warns_about_skipped_profile_and_restores_the_rest() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        seed_fully_uncovered_profile(&xdg, "vazio");
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
        fs::remove_dir_all(xdg.join("cloak/profiles/demo")).expect("rm demo");
        fs::remove_dir_all(xdg.join("cloak/profiles/vazio")).expect("rm vazio");

        let output = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run restore");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "restore parcial deve concluir\nstdout:\n{}\nstderr:\n{stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains("vazio"),
            "o perfil pulado precisa ser nomeado no aviso:\n{stderr}"
        );
        assert!(
            xdg.join("cloak/profiles/demo/claude/settings.json")
                .exists(),
            "o perfil com conteudo precisa ter sido restaurado"
        );
    }

    #[test]
    fn backup_skips_broken_symlink_instead_of_aborting() {
        // REGRESSAO: `walk_files` classifica um symlink quebrado como arquivo,
        // `is_allowed` casa com `skills/` e o `fs::copy` falhava com ENOENT,
        // abortando o backup inteiro com exit 2 e sem produzir artefato.
        use std::os::unix::fs::symlink;

        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        let skills = xdg.join("cloak/profiles/demo/claude/skills");
        symlink("/nonexistent/target.md", skills.join("broken.md")).expect("symlink");
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

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "um symlink obsoleto nao pode tornar o backup impossivel\nstdout:\n{}\nstderr:\n{stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains("broken.md"),
            "o arquivo pulado precisa ser reportado:\n{stderr}"
        );

        let artifact = find_artifact(&out_dir);
        let tar_tmp = tmp.path().join("decrypted.tar.gz");
        gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
        let entries = tar_list_entries(&tar_tmp);
        assert!(
            entries
                .iter()
                .any(|e| e == "profiles/demo/claude/skills/a.md"),
            "o resto do perfil precisa continuar entrando: {entries:?}"
        );
        assert!(
            !entries.iter().any(|e| e.contains("broken.md")),
            "o symlink quebrado nao pode entrar no artefato: {entries:?}"
        );
    }

    #[test]
    fn restore_skips_non_utf8_file_instead_of_aborting() {
        // REGRESSAO: `rewrite_paths_in_file` usava `fs::read_to_string` e
        // propagava com `?`, derrubando `run_restore` DEPOIS da decifragem —
        // nada restaurado e nenhuma indicacao de `--no-rewrite-paths`.
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let xdg = tmp.path().join("xdg");
        write_config(&xdg);
        seed_profile(&xdg);
        // "instalação" em latin-1 dentro de um skill .md.
        let latin1: Vec<u8> = b"instala\xe7\xe3o\n".to_vec();
        let latin1_path = xdg.join("cloak/profiles/demo/claude/skills/latin1.md");
        fs::write(&latin1_path, &latin1).expect("write latin1");
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

        let output = Command::new(cloak_bin())
            .arg("restore")
            .arg(&artifact)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("HOME", tmp.path())
            .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
            .output()
            .expect("run restore");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "um arquivo nao-UTF-8 nao pode derrubar o restore\nstdout:\n{}\nstderr:\n{stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains("latin1.md"),
            "o arquivo pulado precisa ser reportado:\n{stderr}"
        );
        assert!(
            stderr.contains("--no-rewrite-paths"),
            "o aviso precisa apontar o contorno:\n{stderr}"
        );

        let restored = xdg.join("cloak/profiles/demo/claude/skills/latin1.md");
        assert_eq!(
            fs::read(&restored).expect("read restored"),
            latin1,
            "o arquivo pulado precisa chegar ao destino intacto"
        );
        assert!(
            xdg.join("cloak/profiles/demo/claude/settings.json")
                .exists(),
            "o resto do perfil precisa ter sido restaurado"
        );
    }

    #[test]
    fn restore_refuses_newer_format_version_even_with_force() {
        // O teste unitario anterior era tautologia: montava um Manifest com
        // FORMAT_VERSION + 1 e afirmava `> FORMAT_VERSION`, sem exercitar a
        // guarda. Este forja um artefato real com format_version bumpado e
        // exercita o caminho completo do `cloak restore`.
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

        // Reempacota o artefato com format_version = FORMAT_VERSION + 1.
        let tar_tmp = tmp.path().join("plain.tar.gz");
        gpg_decrypt_to(&artifact, &tar_tmp, "test-pass");
        let payload = tmp.path().join("payload");
        tar_extract_to(&tar_tmp, &payload);
        let manifest_path = payload.join("manifest.json");
        let raw = fs::read_to_string(&manifest_path).expect("read manifest");
        let mut value: serde_json::Value = serde_json::from_str(&raw).expect("parse manifest");
        let current = value["format_version"].as_u64().expect("format_version");
        value["format_version"] = serde_json::json!(current + 1);
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&value).expect("serialize"),
        )
        .expect("write manifest");

        let forged_tar = tmp.path().join("forged.tar.gz");
        tar_create_from(&payload, &forged_tar);
        let forged = tmp.path().join("forged.tar.gz.gpg");
        gpg_encrypt_to(&forged_tar, &forged, "test-pass");

        fs::remove_dir_all(xdg.join("cloak/profiles/demo")).expect("rm profile");

        // Sem --force e com --force: os dois precisam recusar.
        for extra in [Vec::new(), vec!["--force".to_string()]] {
            let mut cmd = Command::new(cloak_bin());
            cmd.arg("restore").arg(&forged);
            for a in &extra {
                cmd.arg(a);
            }
            let output = cmd
                .env("XDG_CONFIG_HOME", &xdg)
                .env("HOME", tmp.path())
                .env("CLOAK_BACKUP_PASSPHRASE", "test-pass")
                .output()
                .expect("run restore");

            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success(),
                "formato mais novo precisa ser recusado (extra: {extra:?})\nstdout:\n{}\nstderr:\n{stderr}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(
                stderr.contains("formato de backup"),
                "a mensagem precisa explicar o motivo (extra: {extra:?}):\n{stderr}"
            );
            assert!(
                !xdg.join("cloak/profiles/demo").exists(),
                "nada pode ser escrito no destino (extra: {extra:?})"
            );
        }
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
