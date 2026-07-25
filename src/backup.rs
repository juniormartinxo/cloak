use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use color_eyre::eyre::{eyre, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{account, config::Config, paths};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncoveredEntry {
    pub path: String,
    pub size_bytes: u64,
}

const FORMAT_VERSION: u32 = 1;

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
fn read_mcp_servers(profile: &str) -> Vec<String> {
    match paths::profiles_dir() {
        Ok(root) => read_mcp_servers_at(&root, profile),
        Err(_) => Vec::new(),
    }
}

fn build_profile_manifest(profile: &str, uncovered: Vec<UncoveredEntry>) -> ProfileManifest {
    ProfileManifest {
        name: profile.to_string(),
        oauth_account: account::profile_email(profile),
        mcp_servers: read_mcp_servers(profile),
        uncovered,
    }
}

const PASSPHRASE_ENV: &str = "CLOAK_BACKUP_PASSPHRASE";

fn resolve_passphrase() -> Option<String> {
    std::env::var(PASSPHRASE_ENV).ok().filter(|v| !v.is_empty())
}

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
        let write_result = match child.stdin.as_mut() {
            Some(stdin) => stdin
                .write_all(pw.as_bytes())
                .wrap_err("failed writing passphrase to gpg"),
            None => Err(eyre!("failed to open gpg stdin")),
        };
        if let Err(err) = write_result {
            // Colhe o filho para nao deixar zumbi antes de propagar.
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
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

fn gpg_decrypt(input: &Path, output: &Path, passphrase: Option<&str>) -> Result<()> {
    let input_s = input.to_string_lossy();
    let output_s = output.to_string_lossy();
    run_gpg(&["-o", &output_s, "--decrypt", &input_s], passphrase)
        .wrap_err("failed to decrypt backup archive")
}

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
#[cfg(unix)]
fn origin_uid(home: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(home).map(|m| m.uid()).ok()
}

#[cfg(not(unix))]
fn origin_uid(_home: &Path) -> Option<u32> {
    None
}

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

fn allowlist_patterns(cli_name: &str) -> Vec<&'static str> {
    let mut patterns: Vec<&'static str> = COMMON_ALLOW.to_vec();
    match cli_name {
        "claude" => patterns.extend([
            "statusline-command.sh",
            "plugins/installed_plugins.json",
            "plugins/known_marketplaces.json",
            "plugins/blocklist.json",
            "projects/*/memory/",
            "plans/",
        ]),
        "codex" => patterns.extend(["config.toml", "hooks.json", "memories/"]),
        _ => {}
    }
    patterns
}

/// Casa um padrão de allowlist contra um caminho relativo, segmento a segmento.
///
/// Um segmento `*` no padrão casa com qualquer segmento único do caminho (não
/// atravessa `/`) — é o que permite expressar `projects/*/memory/`, onde o
/// segundo segmento é o slug do projeto e varia por perfil.
///
/// Formas suportadas (todas usadas em `allowlist_patterns`):
/// - `nome.ext` ou `dir/nome.ext`: caminho exato, segmento a segmento.
/// - `*.ext` sem `/`: extensão solta, mas só no nível de topo — não desce em
///   subpastas (ex.: `*.md` casa `CLAUDE.md`, não `skills/a.md`).
/// - `dir/*.ext`: extensão dentro de um diretório específico.
/// - `dir/` (ou `dir/sub/*/mais/`): termina em `/`, casa o próprio diretório
///   e tudo abaixo dele — o prefixo pode conter segmentos `*`.
fn matches_pattern(pattern: &str, rel: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('/') {
        // Diretório: casa o próprio dir e tudo abaixo dele. `rel` pode ser
        // mais profundo que o prefixo — só os segmentos do prefixo precisam bater.
        let prefix_segs: Vec<&str> = prefix.split('/').collect();
        let rel_segs: Vec<&str> = rel.split('/').collect();
        if rel_segs.len() < prefix_segs.len() {
            return false;
        }
        return prefix_segs
            .iter()
            .zip(rel_segs.iter())
            .all(|(p, r)| *p == "*" || p == r);
    }
    if !pattern.contains('/') {
        if let Some(ext) = pattern.strip_prefix("*.") {
            // Glob simples "*.ext" só no nível informado (sem subpastas).
            return !rel.contains('/') && rel.ends_with(&format!(".{ext}"));
        }
        return rel == pattern;
    }
    // Caminho com múltiplos segmentos: exige a mesma profundidade e casa
    // segmento a segmento, com `*` coringa de segmento único e `*.ext`
    // coringa de extensão no último (ou qualquer) segmento.
    let pattern_segs: Vec<&str> = pattern.split('/').collect();
    let rel_segs: Vec<&str> = rel.split('/').collect();
    if pattern_segs.len() != rel_segs.len() {
        return false;
    }
    pattern_segs
        .iter()
        .zip(rel_segs.iter())
        .all(|(p, r)| match p.strip_prefix("*.") {
            Some(ext) => r.ends_with(&format!(".{ext}")),
            None => *p == "*" || p == r,
        })
}

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

/// Retorna `true` se pelo menos um arquivo dentro de `dir` (recursivamente)
/// é coberto pela allowlist.
///
/// Usado para decidir, subárvore por subárvore, se ela pode ser agregada
/// numa linha única de relatório (nada coberto lá dentro) ou se precisa ser
/// percorrida filho a filho (há cobertura parcial).
fn subtree_has_included_file(
    dir: &Path,
    rel_prefix: &str,
    cli_name: &str,
    extra: &[String],
) -> Result<bool> {
    for entry in fs::read_dir(dir).wrap_err_with(|| format!("failed reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if rel_prefix.is_empty() {
            name
        } else {
            format!("{rel_prefix}/{name}")
        };
        if path.is_dir() {
            if subtree_has_included_file(&path, &rel, cli_name, extra)? {
                return Ok(true);
            }
        } else if is_allowed(cli_name, Path::new(&rel), extra) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Preenche `out` com as maiores subárvores inteiramente não cobertas a
/// partir de `dir`, em vez de enumerar cada arquivo folha não coberto.
///
/// CUIDADO: um diretório NÃO pode ser considerado "coberto" só porque algum
/// arquivo dentro dele entrou. Em `claude/plugins/`, por exemplo, três
/// arquivos casam com a allowlist e ~45 mil arquivos de cache não casam.
/// Agregar por diretório com `.any()` no nível de topo faria esses arquivos
/// sumirem em silêncio (nem no backup, nem no relatório) — mas enumerar cada
/// um deles gera relatórios de dezenas de milhares de linhas, ilegíveis.
///
/// Regra recursiva: diretório totalmente descoberto vira UMA linha (com o
/// tamanho agregado) e não desce mais; diretório com cobertura mista desce e
/// aplica a mesma regra a cada filho; arquivo não coberto vira uma linha.
fn collect_uncovered(
    dir: &Path,
    rel_prefix: &str,
    cli_name: &str,
    extra: &[String],
    out: &mut Vec<UncoveredEntry>,
) -> Result<()> {
    for entry in fs::read_dir(dir).wrap_err_with(|| format!("failed reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if rel_prefix.is_empty() {
            name
        } else {
            format!("{rel_prefix}/{name}")
        };
        if path.is_dir() {
            if subtree_has_included_file(&path, &rel, cli_name, extra)? {
                collect_uncovered(&path, &rel, cli_name, extra, out)?;
            } else {
                out.push(UncoveredEntry {
                    path: rel,
                    size_bytes: dir_size(&path),
                });
            }
        } else if !is_allowed(cli_name, Path::new(&rel), extra) {
            out.push(UncoveredEntry {
                path: rel,
                size_bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
            });
        }
    }
    Ok(())
}

/// Lista as entradas que são ARQUIVO diretamente na raiz do diretório do
/// perfil (não desce em subdiretórios — esses são as pastas por CLI, tratadas
/// à parte por `collect_profile_entries`).
fn collect_profile_root_files(profile_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(profile_dir)
        .wrap_err_with(|| format!("failed reading {}", profile_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            files.push(path);
        }
    }
    Ok(files)
}

fn collect_profile_entries(
    cli_dir: &Path,
    cli_name: &str,
    extra: &[String],
) -> Result<(Vec<PathBuf>, Vec<UncoveredEntry>)> {
    let mut included = Vec::new();

    let mut all_files = Vec::new();
    walk_files(cli_dir, &mut all_files)?;
    for file in all_files {
        let rel = file.strip_prefix(cli_dir).unwrap_or(&file).to_path_buf();
        if is_allowed(cli_name, &rel, extra) {
            included.push(file);
        }
    }

    let mut uncovered = Vec::new();
    collect_uncovered(cli_dir, "", cli_name, extra, &mut uncovered)?;
    uncovered.sort_by(|a, b| a.path.cmp(&b.path));

    Ok((included, uncovered))
}

pub struct BackupOptions {
    pub profile: Option<String>,
    pub output: Option<PathBuf>,
    pub include_credentials: bool,
    pub dry_run: bool,
}

const CREDENTIAL_FILES: &[(&str, &str)] =
    &[("claude", ".credentials.json"), ("codex", "auth.json")];

fn resolve_output_dir(config: &Config, opts: &BackupOptions) -> Result<PathBuf> {
    if let Some(dir) = &opts.output {
        return Ok(dir.clone());
    }
    if let Some(backup) = &config.backup {
        if let Some(dir) = &backup.output_dir {
            return Ok(PathBuf::from(dir));
        }
    }
    paths::backups_dir()
}

fn target_profiles(opts: &BackupOptions) -> Result<Vec<String>> {
    let root = paths::profiles_dir()?;
    let mut profiles = Vec::new();
    if let Some(p) = &opts.profile {
        paths::validate_profile_name(p)?;
        profiles.push(p.clone());
        return Ok(profiles);
    }
    if root.exists() {
        for entry in
            fs::read_dir(&root).wrap_err_with(|| format!("failed reading {}", root.display()))?
        {
            let entry = entry?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    profiles.push(name.to_string());
                }
            }
        }
    }
    profiles.sort();
    if profiles.is_empty() {
        return Err(eyre!("no profiles found to back up"));
    }
    Ok(profiles)
}

pub fn run_backup(config: &Config, opts: BackupOptions) -> Result<()> {
    ensure_tool("tar")?;
    ensure_tool("gpg")?;

    let profiles = target_profiles(&opts)?;
    let profile_root = paths::profiles_dir()?;
    let home = dirs::home_dir().ok_or_else(|| eyre!("unable to resolve home directory"))?;
    let extra_includes = config
        .backup
        .as_ref()
        .map(|b| b.include.clone())
        .unwrap_or_default();

    // Área de staging temporária.
    //
    // ATENÇÃO: `tempfile::tempdir()` cria o diretório com 0777 & !umask, o que
    // com o umask padrão 022 resulta em 0755 — legível por qualquer usuário da
    // máquina. A raiz do staging guarda `manifest.json`, `config.toml` e o
    // `archive.tar.gz` AINDA EM CLARO antes da cifragem. Sem o chmod abaixo,
    // esses dados ficam expostos localmente durante toda a execução do backup.
    // Travar a raiz em 0700 basta: sem permissão de travessia, o conteúdo
    // aninhado fica inacessível a terceiros.
    // O conteúdo a empacotar fica em `payload/`, e o tar.gz é escrito como
    // IRMÃO de `payload/`, nunca dentro dele. Se o tar for gravado no mesmo
    // diretório que ele está empacotando, o tar tenta incluir o próprio arquivo
    // de saída enquanto ele cresce e aborta com
    // `tar: .: file changed as we read it` (exit 1) — o backup nunca completa.
    let staging = tempfile::tempdir().wrap_err("failed to create staging dir")?;
    paths::set_owner_only_dir(staging.path())?;
    let payload = staging.path().join("payload");
    paths::ensure_secure_dir(&payload)?;
    let staging_profiles = payload.join("profiles");
    paths::ensure_secure_dir(&staging_profiles)?;

    let mut profile_manifests = Vec::new();

    println!("Backup");
    for profile in &profiles {
        let profile_dir = paths::profile_dir(profile)?;
        if !profile_dir.exists() {
            return Err(eyre!("profile '{profile}' does not exist"));
        }
        let mut all_uncovered = Vec::new();

        // Arquivos soltos na raiz do perfil (ex.: `.cloak`) ficam fora do laco
        // de CLIs abaixo, que so trata diretorios. `.cloak` esta na allowlist
        // da spec e precisa ser copiado; qualquer outro arquivo solto que nao
        // case entra no relatorio de nao-cobertos, com o caminho relativo a
        // raiz do perfil — sem isso, o relatorio podia afirmar cobertura total
        // havendo arquivo de fora.
        let root_files = collect_profile_root_files(&profile_dir)?;
        for path in root_files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name == ".cloak" {
                let rel = path.strip_prefix(&profile_root).unwrap_or(&path);
                let dest = staging_profiles.join(rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .wrap_err_with(|| format!("failed creating {}", parent.display()))?;
                }
                fs::copy(&path, &dest)
                    .wrap_err_with(|| format!("failed copying {}", path.display()))?;
            } else {
                all_uncovered.push(UncoveredEntry {
                    path: name,
                    size_bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
                });
            }
        }

        for entry in fs::read_dir(&profile_dir)
            .wrap_err_with(|| format!("failed reading {}", profile_dir.display()))?
        {
            let entry = entry?;
            let cli_path = entry.path();
            if !cli_path.is_dir() {
                continue;
            }
            let cli_name = entry.file_name().to_string_lossy().into_owned();
            let (included, uncovered) =
                collect_profile_entries(&cli_path, &cli_name, &extra_includes)?;

            for src in included {
                let rel = src.strip_prefix(&profile_root).unwrap_or(&src);
                let dest = staging_profiles.join(rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .wrap_err_with(|| format!("failed creating {}", parent.display()))?;
                }
                fs::copy(&src, &dest)
                    .wrap_err_with(|| format!("failed copying {}", src.display()))?;
            }

            for u in uncovered {
                all_uncovered.push(UncoveredEntry {
                    path: format!("{cli_name}/{}", u.path),
                    size_bytes: u.size_bytes,
                });
            }
        }

        // Credenciais: incluídas apenas com a flag.
        if opts.include_credentials {
            for (cli_name, file) in CREDENTIAL_FILES {
                let src = profile_dir.join(cli_name).join(file);
                if src.exists() {
                    let rel = src.strip_prefix(&profile_root).unwrap_or(&src);
                    let dest = staging_profiles.join(rel);
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(&src, &dest)?;
                }
            }
        }

        print_backup_profile_report(profile, &all_uncovered);
        profile_manifests.push(build_profile_manifest(profile, all_uncovered));
    }

    // Config global do cloak.
    let global_config = paths::config_file_path()?;
    if global_config.exists() {
        fs::copy(&global_config, payload.join("config.toml"))
            .wrap_err("failed copying global config.toml")?;
    }

    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        cloak_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: timestamp_utc()?,
        hostname: origin_hostname(),
        uid: origin_uid(&home),
        home: home.to_string_lossy().into_owned(),
        profile_root: profile_root.to_string_lossy().into_owned(),
        include_credentials: opts.include_credentials,
        profiles: profile_manifests,
    };
    let manifest_json =
        serde_json::to_string_pretty(&manifest).wrap_err("failed serializing manifest")?;
    fs::write(payload.join("manifest.json"), manifest_json).wrap_err("failed writing manifest")?;

    if opts.dry_run {
        println!("dry-run: nenhum artefato gerado");
        return Ok(());
    }

    // tar.gz intermediário e cifragem.
    let output_dir = resolve_output_dir(config, &opts)?;
    paths::ensure_secure_dir(&output_dir)?;
    let filename = format!("cloak-backup-{}.tar.gz.gpg", manifest.created_at);
    let final_path = output_dir.join(&filename);
    // Cifra para um nome temporario e renomeia no fim: o nome final so passa a
    // existir depois da cifragem completa. Sem isso, uma interrupcao deixa um
    // artefato truncado com o nome definitivo — e, num output_dir sincronizado,
    // ele seria o mais recente do diretorio e venceria "restaurar o ultimo backup".
    let partial_path = output_dir.join(format!("{filename}.partial"));

    let tar_tmp = staging.path().join("archive.tar.gz");
    create_tar_gz(&payload, &tar_tmp)?;

    let passphrase = resolve_passphrase();
    if let Err(e) = gpg_encrypt(&tar_tmp, &partial_path, passphrase.as_deref()) {
        let _ = fs::remove_file(&partial_path);
        return Err(e);
    }
    paths::set_owner_only_file(&partial_path)?;
    fs::rename(&partial_path, &final_path)
        .wrap_err_with(|| format!("failed finalizing artifact at {}", final_path.display()))?;
    let _ = fs::remove_file(&tar_tmp);

    println!("Artefato: {}", final_path.display());

    // Upload opcional.
    if let Some(backup) = &config.backup {
        if let Some(cmd_template) = &backup.upload_command {
            run_upload_command(cmd_template, &final_path)?;
        }
    }

    Ok(())
}

fn print_backup_profile_report(profile: &str, uncovered: &[UncoveredEntry]) {
    println!("  perfil: {profile}");
    if uncovered.is_empty() {
        println!("    (tudo coberto pela allowlist)");
        return;
    }
    println!("    NÃO incluído (fora da allowlist):");
    for u in uncovered {
        println!("      {} ({} bytes)", u.path, u.size_bytes);
    }
}

/// Escapa um valor para interpolação segura em `sh -c`.
///
/// O destino típico é `/mnt/c/Users/...` num mount WSL, onde nome de usuário
/// com espaço é comum. Sem aspas, o caminho quebra em várias palavras para o
/// shell e o upload falha de forma incompreensível.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn run_upload_command(template: &str, archive: &Path) -> Result<()> {
    let rendered = template.replace("{archive}", &shell_quote(&archive.to_string_lossy()));
    println!("upload: {rendered}");
    let status = Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .status()
        .wrap_err("failed to run upload_command")?;
    if !status.success() {
        return Err(eyre!(
            "upload_command failed (status {status}); local artifact kept at {}",
            archive.display()
        ));
    }
    Ok(())
}

const REWRITE_EXTENSIONS: &[&str] = &["json", "toml", "md", "sh"];

/// Um caractere que CONTINUA um componente de caminho.
///
/// A checagem de fronteira é definida por exclusão, não por uma lista de
/// terminadores. Enumerar terminadores falha em silêncio: `REWRITE_EXTENSIONS`
/// inclui `.sh` e `.md`, onde paths aparecem sem aspas ao redor — crase em
/// code-span markdown, `)` em link, `;` em shell. Qualquer terminador esquecido
/// faria a reescrita não acontecer, e o perfil restaurado continuaria apontando
/// para o home da máquina antiga sem nenhum aviso.
fn continues_path_component(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// Substitui `from` por `to` apenas quando `from` é uma raiz de caminho
/// completa, não parte de outro nome.
///
/// Duas fronteiras são exigidas:
/// - à direita, para que `from = "/home/ana"` não corrompa `/home/anastacia/x`
///   (que viraria `/home/<novo>stacia/x`);
/// - à esquerda, para que `/home/old` não case dentro de `/backup/home/old/x`,
///   que é um caminho distinto e não deve ser reescrito.
fn replace_path_root(content: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return content.to_string();
    }

    let mut out = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(idx) = rest.find(from) {
        let (before, tail) = rest.split_at(idx);
        let after = &tail[from.len()..];

        let left_ok = before
            .chars()
            .next_back()
            .is_none_or(|c| !continues_path_component(c));
        let right_ok = after
            .chars()
            .next()
            .is_none_or(|c| !continues_path_component(c));

        out.push_str(before);
        if left_ok && right_ok {
            out.push_str(to);
        } else {
            out.push_str(from);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn rewrite_paths_in_file(file: &Path, from: &str, to: &str) -> Result<bool> {
    let is_text = file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| REWRITE_EXTENSIONS.contains(&e))
        .unwrap_or(false);
    if !is_text {
        return Ok(false);
    }
    let content =
        fs::read_to_string(file).wrap_err_with(|| format!("failed reading {}", file.display()))?;
    let updated = replace_path_root(&content, from, to);
    if updated == content {
        return Ok(false);
    }
    fs::write(file, updated).wrap_err_with(|| format!("failed writing {}", file.display()))?;
    Ok(true)
}

fn rewrite_tree(dir: &Path, from: &str, to: &str, changed: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir).wrap_err_with(|| format!("failed reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            rewrite_tree(&path, from, to, changed)?;
        } else if rewrite_paths_in_file(&path, from, to)? {
            changed.push(path.display().to_string());
        }
    }
    Ok(())
}

pub struct RestoreOptions {
    pub archive: PathBuf,
    pub profile: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub rewrite_paths: bool,
}

pub fn run_restore(_config: &Config, opts: RestoreOptions) -> Result<()> {
    ensure_tool("tar")?;
    ensure_tool("gpg")?;

    if !opts.archive.exists() {
        return Err(eyre!("archive not found: {}", opts.archive.display()));
    }

    // Área de staging temporária.
    //
    // Mesma questão descrita em `run_backup`: `tempfile::tempdir()` sai em
    // 0755 com o umask padrão, e aqui a raiz guarda o conteúdo decifrado do
    // backup do usuário (dados sensíveis em claro) até ser copiado para o
    // destino. Trava-se a raiz em 0700 imediatamente após a criação.
    let staging = tempfile::tempdir().wrap_err("failed creating restore staging dir")?;
    paths::set_owner_only_dir(staging.path())?;
    let tar_tmp = staging.path().join("archive.tar.gz");
    let passphrase = resolve_passphrase();
    gpg_decrypt(&opts.archive, &tar_tmp, passphrase.as_deref())?;

    let extracted = staging.path().join("extracted");
    paths::ensure_secure_dir(&extracted)?;
    extract_tar_gz(&tar_tmp, &extracted)?;

    let manifest_path = extracted.join("manifest.json");
    let manifest_raw = fs::read_to_string(&manifest_path)
        .wrap_err("manifest.json missing or unreadable in archive")?;
    let manifest: Manifest =
        serde_json::from_str(&manifest_raw).wrap_err("failed parsing manifest.json")?;

    // Checagem de formato: NAO contornavel por --force. Um cloak antigo nao
    // tem como adivinhar a semantica de um formato futuro, e escrever no
    // perfil do usuario com base em palpite e' pior do que recusar.
    if manifest.format_version > FORMAT_VERSION {
        return Err(eyre!(
            "este artefato usa o formato de backup v{} e este cloak suporta ate v{}; \
             atualize o cloak para restaurar",
            manifest.format_version,
            FORMAT_VERSION
        ));
    }

    println!("Restore");
    println!("  origem: {} @ {}", manifest.hostname, manifest.created_at);

    // Identidade: uid do destino.
    //
    // `uid` é Option: `None` significa "não foi possível determinar", que é
    // diferente de "é o uid 0". Tratar desconhecido como verificado seria
    // fail-open — exatamente o que a checagem existe para impedir. Portanto
    // qualquer lado desconhecido exige `--force` explícito.
    let home = dirs::home_dir().ok_or_else(|| eyre!("unable to resolve home directory"))?;
    let dest_uid = origin_uid(&home);
    if !opts.force {
        match (manifest.uid, dest_uid) {
            (Some(backup_uid), Some(current_uid)) if backup_uid != current_uid => {
                return Err(eyre!(
                    "identidade divergente: backup do uid {} sendo restaurado por uid {}; \
                     use --force para prosseguir",
                    backup_uid,
                    current_uid
                ));
            }
            (Some(_), Some(_)) => {}
            _ => {
                return Err(eyre!(
                    "não foi possível verificar a identidade do backup \
                     (uid de origem ou destino indeterminado); use --force para prosseguir"
                ));
            }
        }
    }

    let profile_root = paths::profiles_dir()?;
    let restore_profiles: Vec<&ProfileManifest> = match &opts.profile {
        Some(name) => {
            let found = manifest
                .profiles
                .iter()
                .find(|p| &p.name == name)
                .ok_or_else(|| eyre!("profile '{name}' not present in archive"))?;
            vec![found]
        }
        None => manifest.profiles.iter().collect(),
    };

    for pm in &restore_profiles {
        let dest_profile = paths::profile_dir(&pm.name)?;
        if dest_profile.exists() && !opts.force {
            return Err(eyre!(
                "profile '{}' already exists at destination; use --force to overwrite",
                pm.name
            ));
        }
        // Conta OAuth divergente é aviso de acidente.
        if let (Some(backup_acc), Some(dest_acc)) =
            (&pm.oauth_account, account::profile_email(&pm.name))
        {
            if backup_acc != &dest_acc && !opts.force {
                return Err(eyre!(
                    "conta divergente no perfil '{}': backup {} vs destino {}; use --force",
                    pm.name,
                    backup_acc,
                    dest_acc
                ));
            }
        }
    }

    if opts.dry_run {
        println!("  perfis a restaurar: {}", restore_profiles.len());
        for pm in &restore_profiles {
            println!("    {} (MCP: {})", pm.name, pm.mcp_servers.join(", "));
        }
        println!("dry-run: destino não foi alterado");
        return Ok(());
    }

    // Reescrita de paths na área extraída antes de copiar.
    let extracted_profiles = extracted.join("profiles");
    if opts.rewrite_paths && extracted_profiles.exists() {
        let mut changed = Vec::new();
        let from_root = &manifest.profile_root;
        let to_root = profile_root.to_string_lossy();
        rewrite_tree(&extracted_profiles, from_root, &to_root, &mut changed)?;
        // Também reescrever o $HOME de origem, se diferente.
        let to_home = home.to_string_lossy();
        if manifest.home != to_home {
            rewrite_tree(&extracted_profiles, &manifest.home, &to_home, &mut changed)?;
        }
        if !changed.is_empty() {
            println!("  paths reescritos em {} arquivo(s)", changed.len());
        }
    }

    // Copiar cada perfil selecionado para o destino, aplicando permissões.
    for pm in &restore_profiles {
        let src = extracted_profiles.join(&pm.name);
        if !src.exists() {
            continue;
        }
        let dest = paths::profile_dir(&pm.name)?;
        // Levantado ANTES de copiar: depois da cópia, os arquivos do backup
        // já existem no destino e não dá mais para distinguir o que era pré-existente.
        let preserved = collect_preserved_files(&src, &dest)?;
        copy_tree_secure(&src, &dest)?;
        print_reconstruction_report(pm);
        print_preserved_report(&pm.name, &preserved);
    }

    Ok(())
}

/// Arquivos que já existem no destino e NÃO vêm no artefato.
///
/// O restore é um merge: nada do destino é apagado. Isso evita destruir
/// credenciais renovadas ou trabalho criado depois do backup, mas significa
/// que o perfil resultante mistura estado antigo e novo. O usuário precisa
/// saber disso — daí o relatório.
fn collect_preserved_files(src: &Path, dest: &Path) -> Result<Vec<String>> {
    let mut preserved = Vec::new();
    if !dest.exists() {
        return Ok(preserved);
    }

    let mut dest_files = Vec::new();
    walk_files(dest, &mut dest_files)?;
    for file in dest_files {
        let Ok(rel) = file.strip_prefix(dest) else {
            continue;
        };
        if !src.join(rel).exists() {
            preserved.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    preserved.sort();
    Ok(preserved)
}

fn print_preserved_report(profile: &str, preserved: &[String]) {
    if preserved.is_empty() {
        return;
    }
    println!(
        "    {} arquivo(s) já existiam no destino e NÃO estavam no backup — preservados:",
        preserved.len()
    );
    for path in preserved {
        println!("      {profile}/{path}");
    }
}

fn copy_tree_secure(src: &Path, dest: &Path) -> Result<()> {
    paths::ensure_secure_dir(dest)?;
    for entry in fs::read_dir(src).wrap_err_with(|| format!("failed reading {}", src.display()))? {
        let entry = entry?;
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_tree_secure(&path, &target)?;
        } else {
            fs::copy(&path, &target)
                .wrap_err_with(|| format!("failed copying {}", path.display()))?;
            paths::set_owner_only_file(&target)?;
        }
    }
    Ok(())
}

fn print_reconstruction_report(pm: &ProfileManifest) {
    println!("  perfil '{}' restaurado", pm.name);
    if !pm.mcp_servers.is_empty() {
        println!(
            "    MCP registrados (reconciliados na 1ª execução): {}",
            pm.mcp_servers.join(", ")
        );
    }
    println!("    plugins/marketplaces serão rebaixados pela CLI na 1ª execução");
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
    fn test_restore_rejects_newer_format_version() {
        // Um cloak antigo nao pode adivinhar a semantica de um formato futuro.
        let manifest = Manifest {
            format_version: FORMAT_VERSION + 1,
            cloak_version: "0.3.1".into(),
            created_at: "20260725-120000".into(),
            hostname: "h".into(),
            uid: Some(1000),
            home: "/home/x".into(),
            profile_root: "/home/x/.config/cloak/profiles".into(),
            include_credentials: false,
            profiles: vec![],
        };
        assert!(manifest.format_version > FORMAT_VERSION);
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
    fn test_allowlist_covers_claude_project_memories() {
        // REGRESSAO: as memorias do Claude vivem fundo em projects/<slug>/memory/
        // e ficaram fora do backup — exatamente o conteudo que motivou a feature.
        assert!(is_allowed(
            "claude",
            Path::new("projects/-home-user-proj/memory/MEMORY.md"),
            &[]
        ));
        assert!(is_allowed(
            "claude",
            Path::new("projects/-home-user-proj/memory/uma-memoria.md"),
            &[]
        ));
        assert!(is_allowed("claude", Path::new("plans/algum-plano.md"), &[]));
        // Transcricoes de subagente continuam fora.
        assert!(!is_allowed(
            "claude",
            Path::new("projects/-home-user-proj/abc-123/subagents/x.jsonl"),
            &[]
        ));
    }

    #[test]
    fn test_uncovered_report_aggregates_fully_uncovered_subtrees() {
        // REGRESSAO: enumerar arquivo a arquivo gerava 45 mil linhas nos perfis reais.
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("claude");
        // plugins/ e' parcialmente coberto: um manifesto casa, o cache nao.
        fs::create_dir_all(cli_dir.join("plugins/cache/a/b")).expect("mkdir cache");
        fs::write(cli_dir.join("plugins/installed_plugins.json"), "{}").expect("w1");
        fs::write(cli_dir.join("plugins/cache/a/b/x.bin"), "x").expect("w2");
        fs::write(cli_dir.join("plugins/cache/a/b/y.bin"), "y").expect("w3");
        fs::write(cli_dir.join("plugins/cache/z.bin"), "z").expect("w4");

        let (_included, uncovered) =
            collect_profile_entries(&cli_dir, "claude", &[]).expect("collect");
        let paths: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();

        assert!(
            paths.contains(&"plugins/cache"),
            "subarvore totalmente descoberta deve virar UMA linha; obtido: {paths:?}"
        );
        assert!(
            !paths
                .iter()
                .any(|p| p.contains("x.bin") || p.contains("z.bin")),
            "arquivos dentro de subarvore agregada nao devem ser enumerados; obtido: {paths:?}"
        );
    }

    #[test]
    fn test_uncovered_report_still_names_loose_file_next_to_covered() {
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("claude");
        fs::create_dir_all(&cli_dir).expect("mkdir");
        fs::write(cli_dir.join("settings.json"), "{}").expect("w1");
        fs::write(cli_dir.join("mystery.bin"), "x").expect("w2");

        let (_included, uncovered) =
            collect_profile_entries(&cli_dir, "claude", &[]).expect("collect");
        let paths: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();
        assert!(paths.contains(&"mystery.bin"), "obtido: {paths:?}");
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

    #[cfg(unix)]
    #[test]
    fn test_shell_quote_wraps_paths_with_spaces() {
        assert_eq!(
            shell_quote("/mnt/c/Users/Ana Paula/b.gpg"),
            "'/mnt/c/Users/Ana Paula/b.gpg'"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_shell_quote_escapes_single_quote() {
        // Aspas simples internas nao podem encerrar o literal.
        assert_eq!(shell_quote("/tmp/it's.gpg"), r#"'/tmp/it'\''s.gpg'"#);
    }

    #[test]
    fn test_rewrite_paths_replaces_root_and_reports_change() {
        let tmp = tempdir().expect("tempdir");
        let file = tmp.path().join("installed_plugins.json");
        fs::write(
            &file,
            r#"{"installPath":"/home/old/.config/cloak/profiles/x/claude/p"}"#,
        )
        .expect("write");

        let changed = rewrite_paths_in_file(
            &file,
            "/home/old/.config/cloak/profiles",
            "/home/new/.config/cloak/profiles",
        )
        .expect("rewrite");
        assert!(changed);
        let content = fs::read_to_string(&file).expect("read");
        assert!(content.contains("/home/new/.config/cloak/profiles/x/claude/p"));
        assert!(!content.contains("/home/old"));
    }

    #[test]
    fn test_rewrite_paths_no_match_returns_false() {
        let tmp = tempdir().expect("tempdir");
        let file = tmp.path().join("f.json");
        fs::write(&file, r#"{"k":"/unrelated/path"}"#).expect("write");
        let changed = rewrite_paths_in_file(&file, "/home/old", "/home/new").expect("rewrite");
        assert!(!changed);
    }

    #[test]
    fn test_replace_path_root_respects_component_boundary() {
        // /home/ana nao pode casar dentro de /home/anastacia
        let content = r#"{"a":"/home/ana/x","b":"/home/anastacia/y"}"#;
        let out = replace_path_root(content, "/home/ana", "/home/bob");
        assert!(out.contains(r#""/home/bob/x""#), "obtido: {out}");
        assert!(
            out.contains(r#""/home/anastacia/y""#),
            "prefixo alheio corrompido: {out}"
        );
    }

    #[test]
    fn test_replace_path_root_handles_end_of_string() {
        let out = replace_path_root("/home/ana", "/home/ana", "/home/bob");
        assert_eq!(out, "/home/bob");
    }

    #[test]
    fn test_replace_path_root_rewrites_in_markdown_and_shell_contexts() {
        let from = "/home/old";
        let to = "/home/new";
        // Crase de code-span markdown.
        assert_eq!(
            replace_path_root("`/home/old/x`", from, to),
            "`/home/new/x`"
        );
        // Link markdown.
        assert_eq!(
            replace_path_root("[p](/home/old/x)", from, to),
            "[p](/home/new/x)"
        );
        // Shell com ponto-e-virgula.
        assert_eq!(
            replace_path_root("export R=/home/old;", from, to),
            "export R=/home/new;"
        );
    }

    #[test]
    fn test_replace_path_root_ignores_path_nested_under_other_prefix() {
        // /home/old aqui e' componente intermediario de um caminho distinto.
        let out = replace_path_root("/backup/home/old/x", "/home/old", "/home/new");
        assert_eq!(out, "/backup/home/old/x");
    }

    #[test]
    fn test_replace_path_root_still_rejects_sibling_prefix() {
        // Regressao do achado anterior: nao pode casar dentro de nome maior.
        let out = replace_path_root("/home/anastacia/y", "/home/ana", "/home/bob");
        assert_eq!(out, "/home/anastacia/y");
    }

    #[test]
    fn test_rewrite_paths_in_file_does_not_corrupt_sibling_prefix() {
        let tmp = tempdir().expect("tempdir");
        let file = tmp.path().join("f.json");
        fs::write(&file, r#"{"a":"/home/ana/x","b":"/home/anastacia/y"}"#).expect("write");
        let changed = rewrite_paths_in_file(&file, "/home/ana", "/home/bob").expect("rewrite");
        assert!(changed);
        let content = fs::read_to_string(&file).expect("read");
        assert!(content.contains("/home/bob/x"));
        assert!(content.contains("/home/anastacia/y"), "obtido: {content}");
    }

    #[test]
    fn test_collect_preserved_files_lists_only_files_absent_from_artifact() {
        let tmp = tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        fs::create_dir_all(src.join("claude")).expect("mkdir src");
        fs::create_dir_all(dest.join("claude")).expect("mkdir dest");
        // Existe nos dois: nao e' "preservado", vai ser sobrescrito.
        fs::write(src.join("claude/settings.json"), "novo").expect("w1");
        fs::write(dest.join("claude/settings.json"), "antigo").expect("w2");
        // So no destino: preservado, precisa ser reportado.
        fs::write(dest.join("claude/token-novo.json"), "x").expect("w3");

        let preserved = collect_preserved_files(&src, &dest).expect("collect");
        assert_eq!(preserved, vec!["claude/token-novo.json".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn test_set_owner_only_dir_locks_tempdir_root() {
        use std::os::unix::fs::PermissionsExt;
        // tempfile::tempdir() sozinho nao garante 0700 (com umask 022 sai 0755),
        // por isso run_backup precisa travar a raiz explicitamente.
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::paths::set_owner_only_dir(tmp.path()).expect("chmod");
        let mode = std::fs::metadata(tmp.path())
            .expect("meta")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }
}
