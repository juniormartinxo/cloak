use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use color_eyre::eyre::{eyre, Context, Result};
use serde::{Deserialize, Serialize};

// `Config` ainda não tem chamador aqui: entra nas Tasks 8/9, quando o
// manifesto passa a ser montado a partir do fluxo real de `backup`.
#[allow(unused_imports)]
use crate::{account, config::Config, paths};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncoveredEntry {
    pub path: String,
    pub size_bytes: u64,
}

#[allow(dead_code)]
const FORMAT_VERSION: u32 = 1;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub cloak_version: String,
    pub created_at: String,
    pub hostname: String,
    pub uid: Option<u32>,
    pub home: String,
    pub profile_root: String,
    pub include_credentials: bool,
    pub profiles: Vec<ProfileManifest>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileManifest {
    pub name: String,
    pub oauth_account: Option<String>,
    pub mcp_servers: Vec<String>,
    pub uncovered: Vec<UncoveredEntry>,
}

/// Lê os MCP registrados a partir de uma raiz de perfis explícita.
/// A raiz entra por parâmetro para manter os testes livres de env global.
fn read_mcp_servers_at(profile_root: &Path, profile: &str) -> Vec<String> {
    let mut servers = Vec::new();
    let claude_json = profile_root
        .join(profile)
        .join("claude")
        .join(".claude.json");
    if let Ok(raw) = fs::read_to_string(&claude_json) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(map) = value.get("mcpServers").and_then(|v| v.as_object()) {
                servers.extend(map.keys().cloned());
            }
        }
    }
    servers
}

/// Wrapper que resolve a raiz padrão de perfis. É o ponto usado pelo resto do código.
#[allow(dead_code)]
fn read_mcp_servers(profile: &str) -> Vec<String> {
    match paths::profiles_dir() {
        Ok(root) => read_mcp_servers_at(&root, profile),
        Err(_) => Vec::new(),
    }
}

#[allow(dead_code)]
fn build_profile_manifest(profile: &str, uncovered: Vec<UncoveredEntry>) -> ProfileManifest {
    ProfileManifest {
        name: profile.to_string(),
        oauth_account: account::profile_email(profile),
        mcp_servers: read_mcp_servers(profile),
        uncovered,
    }
}

const PASSPHRASE_ENV: &str = "CLOAK_BACKUP_PASSPHRASE";

#[allow(dead_code)]
fn resolve_passphrase() -> Option<String> {
    std::env::var(PASSPHRASE_ENV).ok().filter(|v| !v.is_empty())
}

#[allow(dead_code)]
fn ensure_tool(name: &str) -> Result<()> {
    which::which(name)
        .map(|_| ())
        .wrap_err_with(|| format!("required tool '{name}' not found in PATH"))
}

fn run_gpg(args: &[&str], passphrase: Option<&str>) -> Result<()> {
    ensure_tool("gpg")?;
    let mut cmd = Command::new("gpg");
    if let Some(pw) = passphrase {
        cmd.arg("--batch")
            .arg("--yes")
            .arg("--pinentry-mode")
            .arg("loopback")
            .arg("--passphrase-fd")
            .arg("0");
        cmd.args(args);
        cmd.stdin(Stdio::piped());
        let mut child = cmd.spawn().wrap_err("failed to spawn gpg")?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| eyre!("failed to open gpg stdin"))?
            .write_all(pw.as_bytes())
            .wrap_err("failed writing passphrase to gpg")?;
        let status = child.wait().wrap_err("failed waiting for gpg")?;
        if !status.success() {
            return Err(eyre!("gpg exited with status {status}"));
        }
    } else {
        cmd.args(args);
        let status = cmd.status().wrap_err("failed to run gpg (interactive)")?;
        if !status.success() {
            return Err(eyre!(
                "gpg failed (status {status}); pinentry may have timed out or been cancelled"
            ));
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn gpg_encrypt(input: &Path, output: &Path, passphrase: Option<&str>) -> Result<()> {
    let input_s = input.to_string_lossy();
    let output_s = output.to_string_lossy();
    run_gpg(
        &[
            "--symmetric",
            "--cipher-algo",
            "AES256",
            "-o",
            &output_s,
            &input_s,
        ],
        passphrase,
    )
    .wrap_err("failed to encrypt backup archive")
}

#[allow(dead_code)]
fn gpg_decrypt(input: &Path, output: &Path, passphrase: Option<&str>) -> Result<()> {
    let input_s = input.to_string_lossy();
    let output_s = output.to_string_lossy();
    run_gpg(&["-o", &output_s, "--decrypt", &input_s], passphrase)
        .wrap_err("failed to decrypt backup archive")
}

#[allow(dead_code)]
fn create_tar_gz(src_dir: &Path, output: &Path) -> Result<()> {
    ensure_tool("tar")?;
    let src_s = src_dir.to_string_lossy();
    let out_s = output.to_string_lossy();
    let status = Command::new("tar")
        .arg("-czf")
        .arg(out_s.as_ref())
        .arg("-C")
        .arg(src_s.as_ref())
        .arg(".")
        .status()
        .wrap_err("failed to run tar")?;
    if !status.success() {
        return Err(eyre!("tar failed while creating archive (status {status})"));
    }
    Ok(())
}

#[allow(dead_code)]
fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    ensure_tool("tar")?;
    let archive_s = archive.to_string_lossy();
    let dest_s = dest.to_string_lossy();
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive_s.as_ref())
        .arg("-C")
        .arg(dest_s.as_ref())
        .status()
        .wrap_err("failed to run tar")?;
    if !status.success() {
        return Err(eyre!(
            "tar failed while extracting archive (status {status})"
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn origin_hostname() -> String {
    Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Retorna `None` quando o uid não pôde ser determinado.
///
/// NÃO use `0` como fallback: `0` é o uid real do root, então uma leitura
/// falha se tornaria indistinguível de "restaurando como root" e a checagem
/// de identidade do restore passaria por engano (fail-open). `None` força o
/// restore a tratar o caso como não-verificável (fail-safe).
#[allow(dead_code)]
#[cfg(unix)]
fn origin_uid(home: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(home).map(|m| m.uid()).ok()
}

#[allow(dead_code)]
#[cfg(not(unix))]
fn origin_uid(_home: &Path) -> Option<u32> {
    None
}

#[allow(dead_code)]
fn timestamp_utc() -> Result<String> {
    let output = Command::new("date")
        .args(["-u", "+%Y%m%d-%H%M%S"])
        .output()
        .wrap_err("failed to run date")?;
    if !output.status.success() {
        return Err(eyre!("date command failed"));
    }
    let ts = String::from_utf8(output.stdout)
        .wrap_err("date returned non-utf8")?
        .trim()
        .to_string();
    Ok(ts)
}

const COMMON_ALLOW: &[&str] = &[
    "settings.json",
    "keybindings.json",
    "*.md",
    "skills/",
    ".agents/",
];

#[allow(dead_code)]
fn allowlist_patterns(cli_name: &str) -> Vec<&'static str> {
    let mut patterns: Vec<&'static str> = COMMON_ALLOW.to_vec();
    match cli_name {
        "claude" => patterns.extend([
            "statusline-command.sh",
            "plugins/installed_plugins.json",
            "plugins/known_marketplaces.json",
            "plugins/blocklist.json",
        ]),
        "codex" => patterns.extend(["config.toml", "hooks.json", "memories/"]),
        _ => {}
    }
    patterns
}

fn matches_pattern(pattern: &str, rel: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('/') {
        // Diretório: casa o próprio dir e tudo abaixo dele.
        return rel == prefix || rel.starts_with(&format!("{prefix}/"));
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        // Glob simples "*.ext" só no nível informado (sem subpastas).
        return !rel.contains('/') && rel.ends_with(&format!(".{ext}"));
    }
    if let Some((dir, glob)) = pattern.rsplit_once('/') {
        if let Some(ext) = glob.strip_prefix("*.") {
            if let Some((rdir, rfile)) = rel.rsplit_once('/') {
                return rdir == dir && rfile.ends_with(&format!(".{ext}"));
            }
            return false;
        }
    }
    rel == pattern
}

#[allow(dead_code)]
fn is_allowed(cli_name: &str, rel: &Path, extra: &[String]) -> bool {
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let builtin = allowlist_patterns(cli_name);
    builtin.iter().any(|p| matches_pattern(p, &rel_str))
        || extra.iter().any(|p| matches_pattern(p, &rel_str))
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            total += dir_size(&p);
        } else if let Ok(meta) = p.metadata() {
            total += meta.len();
        }
    }
    total
}

fn walk_files(current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(current).wrap_err_with(|| format!("failed reading {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn collect_profile_entries(
    cli_dir: &Path,
    cli_name: &str,
    extra: &[String],
) -> Result<(Vec<PathBuf>, Vec<UncoveredEntry>)> {
    let mut included = Vec::new();
    let mut uncovered = Vec::new();

    let mut all_files = Vec::new();
    walk_files(cli_dir, &mut all_files)?;
    for file in all_files {
        let rel = file.strip_prefix(cli_dir).unwrap_or(&file).to_path_buf();
        if is_allowed(cli_name, &rel, extra) {
            included.push(file);
        }
    }

    // Entradas não cobertas, para o relatório.
    //
    // CUIDADO: um diretório de topo NÃO pode ser considerado "coberto" só
    // porque algum arquivo dentro dele entrou. Em `claude/plugins/`, por
    // exemplo, três arquivos casam com a allowlist e qualquer outro arquivo
    // ali não casa. Agregar por diretório com `.any()` faria esses arquivos
    // sumirem em silêncio — nem no backup, nem no relatório —, destruindo a
    // única garantia que torna a allowlist aceitável para backup.
    //
    // Regra: diretório totalmente descoberto vira UMA linha (com o tamanho
    // agregado); diretório parcialmente coberto é percorrido e reporta os
    // arquivos específicos que ficaram de fora.
    let included_set: std::collections::HashSet<&Path> =
        included.iter().map(|p| p.as_path()).collect();

    for entry in
        fs::read_dir(cli_dir).wrap_err_with(|| format!("failed reading {}", cli_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if path.is_dir() {
            let mut files = Vec::new();
            walk_files(&path, &mut files)?;
            let any_included = files.iter().any(|f| included_set.contains(f.as_path()));
            if !any_included {
                // Nada dentro entrou: uma linha só para o diretório inteiro.
                uncovered.push(UncoveredEntry {
                    path: name,
                    size_bytes: dir_size(&path),
                });
            } else {
                // Cobertura parcial: reportar cada arquivo que ficou de fora.
                for f in files {
                    if included_set.contains(f.as_path()) {
                        continue;
                    }
                    let rel = f.strip_prefix(cli_dir).unwrap_or(&f);
                    uncovered.push(UncoveredEntry {
                        path: rel.to_string_lossy().replace('\\', "/"),
                        size_bytes: f.metadata().map(|m| m.len()).unwrap_or(0),
                    });
                }
            }
        } else if !is_allowed(cli_name, Path::new(&name), extra) {
            uncovered.push(UncoveredEntry {
                path: name,
                size_bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
            });
        }
    }
    uncovered.sort_by(|a, b| a.path.cmp(&b.path));

    Ok((included, uncovered))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_is_allowed_matches_common_and_cli_specific() {
        assert!(is_allowed("claude", Path::new("settings.json"), &[]));
        assert!(is_allowed("claude", Path::new("skills/foo.md"), &[]));
        assert!(is_allowed("claude", Path::new("CLAUDE.md"), &[]));
        assert!(is_allowed("codex", Path::new("memories/note.md"), &[]));
        assert!(!is_allowed("claude", Path::new("sessions/x.jsonl"), &[]));
        assert!(!is_allowed("claude", Path::new("cache/blob"), &[]));
    }

    #[test]
    fn test_is_allowed_honors_extra_includes() {
        assert!(!is_allowed("codex", Path::new("extra/a.json"), &[]));
        assert!(is_allowed(
            "codex",
            Path::new("extra/a.json"),
            &["extra/*.json".to_string()]
        ));
    }

    #[test]
    fn test_collect_profile_entries_splits_included_and_uncovered() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("claude");
        fs::create_dir_all(cli_dir.join("skills")).expect("mkdir skills");
        fs::create_dir_all(cli_dir.join("sessions")).expect("mkdir sessions");
        fs::write(cli_dir.join("settings.json"), "{}").expect("settings");
        fs::write(cli_dir.join("skills/s.md"), "x").expect("skill");
        fs::write(cli_dir.join("sessions/log.jsonl"), "x").expect("session");
        fs::write(cli_dir.join("mystery.bin"), "x").expect("mystery");

        let (included, uncovered) =
            collect_profile_entries(&cli_dir, "claude", &[]).expect("collect");

        let inc: Vec<String> = included
            .iter()
            .map(|p| {
                p.strip_prefix(&cli_dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(inc.contains(&"settings.json".to_string()));
        assert!(inc.contains(&"skills/s.md".to_string()));
        assert!(!inc.iter().any(|p| p.starts_with("sessions")));

        let unc: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();
        assert!(unc.contains(&"mystery.bin"));
        // Diretório totalmente descoberto vira uma linha só.
        assert!(unc.contains(&"sessions"));
    }

    #[test]
    fn test_read_mcp_servers_at_reads_claude_json() {
        // Sem set_var: a raiz entra por parâmetro, então o teste é puro
        // e não corre com outros testes em paralelo.
        let tmp = tempdir().expect("tempdir");
        let profile_root = tmp.path().join("profiles");
        let claude_dir = profile_root.join("demo/claude");
        fs::create_dir_all(&claude_dir).expect("mkdir");
        fs::write(
            claude_dir.join(".claude.json"),
            r#"{"mcpServers":{"time":{},"gitnexus":{}}}"#,
        )
        .expect("write claude.json");

        let mut servers = read_mcp_servers_at(&profile_root, "demo");
        servers.sort();
        assert_eq!(servers, vec!["gitnexus".to_string(), "time".to_string()]);
    }

    #[test]
    fn test_read_mcp_servers_at_missing_file_returns_empty() {
        let tmp = tempdir().expect("tempdir");
        let servers = read_mcp_servers_at(tmp.path(), "inexistente");
        assert!(servers.is_empty());
    }

    #[test]
    fn test_manifest_serializes_roundtrip() {
        let manifest = Manifest {
            format_version: 1,
            cloak_version: "0.3.1".into(),
            created_at: "20260724-120000".into(),
            hostname: "host".into(),
            uid: Some(1000),
            home: "/home/x".into(),
            profile_root: "/home/x/.config/cloak/profiles".into(),
            include_credentials: false,
            profiles: vec![ProfileManifest {
                name: "demo".into(),
                oauth_account: Some("a@b.com".into()),
                mcp_servers: vec!["time".into()],
                uncovered: vec![],
            }],
        };
        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: Manifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.profiles[0].name, "demo");
        assert_eq!(back.profiles[0].oauth_account.as_deref(), Some("a@b.com"));
    }

    #[test]
    fn test_origin_uid_returns_none_for_unreadable_path() {
        // Caminho inexistente: nao pode virar Some(0), que colidiria com root.
        let missing = Path::new("/definitivamente/nao/existe/cloak-test");
        assert_eq!(origin_uid(missing), None);
    }

    #[test]
    fn test_origin_uid_returns_some_for_real_path() {
        let tmp = tempdir().expect("tempdir");
        assert!(origin_uid(tmp.path()).is_some());
    }

    fn gpg_available() -> bool {
        which::which("gpg").is_ok()
    }

    #[test]
    fn test_gpg_encrypt_decrypt_roundtrip() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let plain = tmp.path().join("plain.txt");
        let enc = tmp.path().join("plain.txt.gpg");
        let dec = tmp.path().join("decrypted.txt");
        fs::write(&plain, b"segredo-de-teste").expect("write plain");

        gpg_encrypt(&plain, &enc, Some("pw123")).expect("encrypt");
        assert!(enc.exists());
        gpg_decrypt(&enc, &dec, Some("pw123")).expect("decrypt");
        assert_eq!(fs::read(&dec).expect("read dec"), b"segredo-de-teste");
    }

    #[test]
    fn test_gpg_decrypt_wrong_passphrase_fails() {
        if !gpg_available() {
            eprintln!("skipping: gpg not available");
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let plain = tmp.path().join("p.txt");
        let enc = tmp.path().join("p.txt.gpg");
        let dec = tmp.path().join("d.txt");
        fs::write(&plain, b"x").expect("write");
        gpg_encrypt(&plain, &enc, Some("right")).expect("encrypt");
        assert!(gpg_decrypt(&enc, &dec, Some("wrong")).is_err());
    }

    #[test]
    fn test_tar_gz_roundtrip() {
        let tmp = tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("sub")).expect("mkdir");
        fs::write(src.join("a.txt"), b"hello").expect("a");
        fs::write(src.join("sub/b.txt"), b"world").expect("b");

        let archive = tmp.path().join("out.tar.gz");
        create_tar_gz(&src, &archive).expect("create");
        assert!(archive.exists());

        let dest = tmp.path().join("dest");
        fs::create_dir_all(&dest).expect("mkdir dest");
        extract_tar_gz(&archive, &dest).expect("extract");
        assert_eq!(fs::read(dest.join("a.txt")).expect("a"), b"hello");
        assert_eq!(fs::read(dest.join("sub/b.txt")).expect("b"), b"world");
    }

    #[test]
    fn test_timestamp_utc_format() {
        let ts = timestamp_utc().expect("timestamp");
        assert_eq!(ts.len(), 15, "expected YYYYMMDD-HHMMSS, got {ts}");
        assert_eq!(ts.as_bytes()[8], b'-');
    }

    #[test]
    fn test_collect_profile_entries_reports_uncovered_inside_partially_covered_dir() {
        // Regressão: um diretório com MISTURA de arquivos cobertos e não
        // cobertos não pode marcar o diretório inteiro como coberto — os
        // arquivos de fora sumiriam do backup E do relatório.
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("claude");
        fs::create_dir_all(cli_dir.join("plugins")).expect("mkdir plugins");
        // Este casa com a allowlist de claude.
        fs::write(cli_dir.join("plugins/installed_plugins.json"), "{}").expect("inst");
        // Este NÃO casa com nada.
        fs::write(cli_dir.join("plugins/secret_api_key.json"), "shh").expect("secret");

        let (included, uncovered) =
            collect_profile_entries(&cli_dir, "claude", &[]).expect("collect");

        let inc: Vec<String> = included
            .iter()
            .map(|p| {
                p.strip_prefix(&cli_dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(inc.contains(&"plugins/installed_plugins.json".to_string()));
        assert!(!inc.contains(&"plugins/secret_api_key.json".to_string()));

        let unc: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();
        assert!(
            unc.contains(&"plugins/secret_api_key.json"),
            "arquivo nao coberto dentro de diretorio parcialmente coberto \
             precisa aparecer no relatorio; obtido: {unc:?}"
        );
    }
}
