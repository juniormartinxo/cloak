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
        // O gemini aninha TUDO em `<perfil>/gemini/.gemini/` (`GEMINI_CLI_HOME`
        // aponta para `<perfil>/gemini`, e a CLI cria `.gemini/` dentro).
        // Os padroes de `COMMON_ALLOW` casam so no topo do diretorio do CLI,
        // entao nenhum deles alcanca nada de um perfil gemini: sem este arm o
        // artefato saia vazio e a subarvore inteira virava uma linha agregada
        // `gemini/.gemini` no relatorio de nao-cobertos.
        //
        // O conjunto abaixo re-enraiza `settings.json` e `*.md` um nivel
        // abaixo. `settings.json` e' o arquivo de configuracao que `account.rs`
        // e `doctor.rs` ja leem; `*.md` cobre o `GEMINI.md`, o equivalente do
        // `CLAUDE.md`/`AGENTS.md` que motivou a feature. Fora ficam
        // `oauth_creds.json` e `.env` (credenciais, so com a flag),
        // `history/`, `tmp/` e caches de IDE (volume: 1,5 GB medido em
        // `~/.gemini/antigravity-ide`), e `installation_id`, `state.json`,
        // `projects.json` e `trustedFolders.json` (estado da maquina de
        // origem, que a CLI reconstroi).
        "gemini" => patterns.extend([".gemini/settings.json", ".gemini/*.md"]),
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
            } else if !is_allowed(cli_name, Path::new(&rel), extra) {
                // Um diretório SEM arquivos faz `subtree_has_included_file`
                // devolver false, então um `claude/skills/` vazio era agregado
                // como não coberto e o relatório imprimia
                // `claude/skills (0 bytes)` sob "NÃO incluído (fora da
                // allowlist)" — mesmo `skills/` sendo padrão built-in. Falso
                // positivo treina o usuário a ignorar o relatório, que é a
                // única defesa contra omissão silenciosa. Um padrão de
                // diretório (`skills/`) casa com o próprio diretório, então
                // `is_allowed` distingue "vazio e coberto" de "vazio e fora".
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

/// Arquivos que carregam credencial e por isso ficam fora da allowlist,
/// entrando apenas com `--include-credentials`.
///
/// Os caminhos sao relativos ao diretorio do CLI dentro do perfil. As duas
/// entradas do gemini saem de `account.rs::inspect_gemini`, que le
/// `.gemini/oauth_creds.json` (OAuth) e `.gemini/.env` (`GEMINI_API_KEY` /
/// `GOOGLE_API_KEY`).
const CREDENTIAL_FILES: &[(&str, &str)] = &[
    ("claude", ".credentials.json"),
    ("codex", "auth.json"),
    ("gemini", ".gemini/oauth_creds.json"),
    ("gemini", ".gemini/.env"),
];

/// Padroes que se somam a allowlist built-in para UM diretorio de CLI nesta
/// execucao: os do usuario (`[backup].include`) mais, quando
/// `--include-credentials` foi passado, as credenciais daquele CLI.
///
/// As credenciais entram por aqui, e nao por uma copia a parte, porque o
/// relatorio de nao-cobertos e o `ProfileManifest.uncovered` sao derivados da
/// allowlist. Copiar por fora deixava o manifesto afirmando
/// "NAO incluido (fora da allowlist): claude/.credentials.json" para um
/// arquivo que estava dentro do payload — a inversao acontecia justamente no
/// arquivo mais sensivel, onde o relatorio e' a unica rede de seguranca.
fn cli_extra_patterns(
    cli_name: &str,
    config_include: &[String],
    include_credentials: bool,
) -> Vec<String> {
    let mut patterns = config_include.to_vec();
    if include_credentials {
        patterns.extend(
            CREDENTIAL_FILES
                .iter()
                .filter(|(cli, _)| *cli == cli_name)
                .map(|(_, file)| (*file).to_string()),
        );
    }
    patterns
}

/// Resolve o diretório de saída e informa se ele é o default do cloak.
///
/// O booleano existe por causa das permissões: só o default é um diretório do
/// cloak. Ver `prepare_output_dir`.
fn resolve_output_dir(config: &Config, opts: &BackupOptions) -> Result<(PathBuf, bool)> {
    if let Some(dir) = &opts.output {
        return Ok((dir.clone(), false));
    }
    if let Some(backup) = &config.backup {
        if let Some(dir) = &backup.output_dir {
            return Ok((PathBuf::from(dir), false));
        }
    }
    Ok((paths::backups_dir()?, true))
}

/// Garante que o diretório de saída exista, aplicando `0700` apenas quando ele
/// pertence ao cloak.
///
/// `paths::ensure_secure_dir` faz `create_dir_all` (no-op se já existe) e
/// depois `set_owner_only_dir` INCONDICIONAL. Usado direto, isso mutava as
/// permissões de um diretório que o cloak não criou — `cloak backup --output
/// ~/Downloads`, `--output .` ou um `output_dir` sincronizado no OneDrive —
/// sem aviso e sem opt-out. Em diretório compartilhado isso quebra outros
/// usuários e processos; em filesystem que rejeita chmod vira falha rígida do
/// backup. O artefato em si já é `0600`, então o chmod do diretório não
/// agregava proteção nenhuma.
///
/// Regra: o default (`~/.config/cloak/backups`) é do cloak e é sempre travado;
/// um diretório informado pelo usuário só é travado se o cloak for quem o
/// criar — se já existia, fica exatamente como estava.
fn prepare_output_dir(dir: &Path, is_default: bool) -> Result<()> {
    if is_default || !dir.exists() {
        return paths::ensure_secure_dir(dir);
    }
    Ok(())
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

/// Plano de backup de um perfil: o que entra, o que ficou de fora e o que a
/// allowlist casou mas não pôde ser lido.
///
/// A seleção é separada da cópia porque o `--dry-run` é anunciado como "ver o
/// que entraria no backup, sem gerar nenhum arquivo". Antes, o laço de cópia
/// dos incluídos, a cópia das credenciais, a cópia do config global e a
/// escrita do manifesto rodavam TODOS antes do ramo de dry-run — em perfil
/// real, uma cópia de vários megabytes para diretório temporário, apagada logo
/// em seguida.
struct ProfileBackupPlan {
    name: String,
    /// Caminhos absolutos na origem, já filtrados pela allowlist.
    included: Vec<PathBuf>,
    uncovered: Vec<UncoveredEntry>,
    /// Casaram com a allowlist mas não podem ser lidos (symlink quebrado,
    /// arquivo removido durante o backup).
    unreadable: Vec<String>,
}

/// Monta o plano de um perfil. NÃO escreve nada.
fn plan_profile_backup(
    profile: &str,
    profile_root: &Path,
    extra_includes: &[String],
    include_credentials: bool,
) -> Result<ProfileBackupPlan> {
    let profile_dir = paths::profile_dir(profile)?;
    if !profile_dir.exists() {
        return Err(eyre!("profile '{profile}' does not exist"));
    }
    let _ = profile_root;

    let mut included = Vec::new();
    let mut uncovered = Vec::new();
    let mut unreadable = Vec::new();

    // Arquivos soltos na raiz do perfil (ex.: `.cloak`) ficam fora do laco
    // de CLIs abaixo, que so trata diretorios. `.cloak` esta na allowlist
    // da spec e precisa ser copiado; qualquer outro arquivo solto que nao
    // case entra no relatorio de nao-cobertos, com o caminho relativo a
    // raiz do perfil — sem isso, o relatorio podia afirmar cobertura total
    // havendo arquivo de fora.
    for path in collect_profile_root_files(&profile_dir)? {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == ".cloak" {
            included.push(path);
        } else {
            uncovered.push(UncoveredEntry {
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
        let cli_extra = cli_extra_patterns(&cli_name, extra_includes, include_credentials);
        let (cli_included, cli_uncovered) =
            collect_profile_entries(&cli_path, &cli_name, &cli_extra)?;
        included.extend(cli_included);
        for u in cli_uncovered {
            uncovered.push(UncoveredEntry {
                path: format!("{cli_name}/{}", u.path),
                size_bytes: u.size_bytes,
            });
        }
    }

    // Um arquivo ilegivel e' PULADO, nao fatal.
    //
    // Um symlink quebrado (`skills/x.md -> /nonexistent/y.md`, sobra comum
    // depois de desinstalar um plugin ou skill) e' classificado como arquivo
    // por `walk_files` e casa com a allowlist, mas `fs::copy` falha com
    // ENOENT — e abortar tornava o backup impossivel ate o usuario descobrir
    // e apagar o arquivo, com uma mensagem que nao sugeria remedio.
    // `exists()` segue o symlink, entao a checagem aqui pega esse caso sem
    // precisar tentar a copia (o dry-run tambem precisa reportar).
    included.retain(|src| {
        if src.exists() {
            return true;
        }
        let shown = src.strip_prefix(&profile_dir).unwrap_or(src);
        unreadable.push(format!(
            "{} (arquivo inacessivel)",
            shown.to_string_lossy().replace('\\', "/")
        ));
        false
    });

    Ok(ProfileBackupPlan {
        name: profile.to_string(),
        included,
        uncovered,
        unreadable,
    })
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

    // FASE 1 — seleção e relatório. Nenhuma escrita em disco.
    println!("Backup");
    let mut plans = Vec::new();
    for profile in &profiles {
        let plan = plan_profile_backup(
            profile,
            &profile_root,
            &extra_includes,
            opts.include_credentials,
        )?;
        print_backup_profile_report(&plan.name, &plan.uncovered);
        print_unreadable_report(&plan.name, &plan.unreadable);
        plans.push(plan);
    }

    if opts.dry_run {
        println!("dry-run: nenhum artefato gerado");
        return Ok(());
    }

    // FASE 2 — materialização. A partir daqui o cloak escreve em disco.
    //
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
    for plan in plans {
        let profile_dir = paths::profile_dir(&plan.name)?;
        // Falhas de cópia descobertas só agora (arquivo removido entre o
        // planejamento e a cópia, permissão negada) somam ao que já foi
        // reportado na fase 1, com o mesmo tratamento: pular, não abortar.
        let mut unreadable = Vec::new();
        for src in &plan.included {
            let rel = src.strip_prefix(&profile_root).unwrap_or(src);
            let dest = staging_profiles.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .wrap_err_with(|| format!("failed creating {}", parent.display()))?;
            }
            if let Err(err) = fs::copy(src, &dest) {
                // Uma falha no meio da escrita deixa destino parcial.
                let _ = fs::remove_file(&dest);
                let shown = src.strip_prefix(&profile_dir).unwrap_or(src);
                unreadable.push(format!(
                    "{} ({err})",
                    shown.to_string_lossy().replace('\\', "/")
                ));
            }
        }
        print_unreadable_report(&plan.name, &unreadable);
        profile_manifests.push(build_profile_manifest(&plan.name, plan.uncovered));
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

    // tar.gz intermediário e cifragem.
    let (output_dir, output_is_default) = resolve_output_dir(config, &opts)?;
    prepare_output_dir(&output_dir, output_is_default)?;
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

/// Arquivos que casavam com a allowlist mas não puderam ser lidos.
///
/// Vai para stderr: é um aviso de backup incompleto, e stderr sobrevive a
/// `cloak backup > relatorio.txt` e é o que o cron envia por e-mail.
fn print_unreadable_report(profile: &str, unreadable: &[String]) {
    if unreadable.is_empty() {
        return;
    }
    eprintln!(
        "  AVISO: {} arquivo(s) do perfil '{profile}' casavam com a allowlist mas nao \
         puderam ser lidos e FICARAM DE FORA do artefato (symlink quebrado ou arquivo \
         removido durante o backup):",
        unreadable.len()
    );
    for item in unreadable {
        eprintln!("    {profile}/{item}");
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
    // Ultimo caractere ja emitido em `out`. `before` sozinho nao serve como
    // fronteira esquerda: a partir da segunda iteracao `rest` comeca logo
    // depois do match anterior, entao `before` fica vazio e
    // `next_back()` devolve `None`, que `is_none_or` aceita como fronteira.
    // Era assim que `x/home/old/home/old` virava `x/home/old/home/new`: a
    // segunda ocorrencia esta' aninhada dentro da primeira, ja rejeitada.
    let mut last_emitted: Option<char> = None;

    while let Some(idx) = rest.find(from) {
        let (before, tail) = rest.split_at(idx);
        let after = &tail[from.len()..];

        let left_ok = before
            .chars()
            .next_back()
            .or(last_emitted)
            .is_none_or(|c| !continues_path_component(c));
        let right_ok = after
            .chars()
            .next()
            .is_none_or(|c| !continues_path_component(c));

        out.push_str(before);
        let emitted = if left_ok && right_ok { to } else { from };
        out.push_str(emitted);
        last_emitted = emitted.chars().next_back().or(last_emitted);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Resultado de tentar reescrever os paths de um arquivo da área extraída.
#[derive(Debug)]
enum RewriteOutcome {
    /// Nada a fazer: extensão fora de `REWRITE_EXTENSIONS` ou sem ocorrência.
    Unchanged,
    Rewritten,
    /// O arquivo não pôde ser lido como texto e ficou INTACTO.
    Skipped(String),
}

fn rewrite_paths_in_file(file: &Path, from: &str, to: &str) -> Result<RewriteOutcome> {
    let is_text = file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| REWRITE_EXTENSIONS.contains(&e))
        .unwrap_or(false);
    if !is_text {
        return Ok(RewriteOutcome::Unchanged);
    }
    // Leitura que falha NAO e' fatal. `fs::read_to_string` exige UTF-8, e um
    // skill `.md` salvo em latin-1 ou um `.sh` com um byte nao-UTF-8 num
    // comentario derrubava `rewrite_tree` e `run_restore` inteiros — DEPOIS da
    // decifragem, sem restaurar nada e sem indicar que `--no-rewrite-paths` e'
    // o contorno. Um arquivo que apenas nao pode ser reescrito e' pulado: ele
    // chega ao destino como veio no artefato, so' com os paths da maquina de
    // origem, e o restore reporta isso ao usuario.
    let content = match fs::read_to_string(file) {
        Ok(content) => content,
        Err(err) => {
            return Ok(RewriteOutcome::Skipped(format!(
                "{} ({err})",
                file.display()
            )))
        }
    };
    let updated = replace_path_root(&content, from, to);
    if updated == content {
        return Ok(RewriteOutcome::Unchanged);
    }
    // A escrita, ao contrario da leitura, continua fatal: `fs::write` trunca
    // antes de escrever, entao uma falha aqui pode deixar o arquivo pela
    // metade. Copiar um arquivo truncado para o perfil do usuario e' pior do
    // que abortar antes de tocar no destino.
    fs::write(file, updated).wrap_err_with(|| format!("failed writing {}", file.display()))?;
    Ok(RewriteOutcome::Rewritten)
}

fn rewrite_tree(
    dir: &Path,
    from: &str,
    to: &str,
    changed: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<()> {
    for entry in fs::read_dir(dir).wrap_err_with(|| format!("failed reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            rewrite_tree(&path, from, to, changed, skipped)?;
        } else {
            match rewrite_paths_in_file(&path, from, to)? {
                RewriteOutcome::Rewritten => changed.push(path.display().to_string()),
                RewriteOutcome::Skipped(detail) => skipped.push(detail),
                RewriteOutcome::Unchanged => {}
            }
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

/// Recusa um artefato cujo formato de backup é mais novo do que este cloak
/// entende.
///
/// NÃO é contornável por `--force`: um cloak antigo não tem como adivinhar a
/// semântica de um formato futuro, e escrever no perfil do usuário com base em
/// palpite é pior do que recusar.
fn ensure_supported_format(format_version: u32) -> Result<()> {
    if format_version > FORMAT_VERSION {
        return Err(eyre!(
            "este artefato usa o formato de backup v{} e este cloak suporta ate v{}; \
             atualize o cloak para restaurar",
            format_version,
            FORMAT_VERSION
        ));
    }
    Ok(())
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

    ensure_supported_format(manifest.format_version)?;

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
    // O problema de identidade vira DADO, não abort imediato: o `--dry-run` é
    // documentado como "ver o plano de restauração sem tocar no destino", e
    // abortar aqui fazia a prévia recusar exatamente o caso em que ela mais
    // importa (restaurar por cima de uma instalação existente). O único
    // contorno era `--force --dry-run`, o que treina o usuário a digitar
    // `--force` em restores reais.
    let identity_issue: Option<String> = match (manifest.uid, dest_uid) {
        (Some(backup_uid), Some(current_uid)) if backup_uid != current_uid => Some(format!(
            "identidade divergente: backup do uid {backup_uid} sendo restaurado \
             por uid {current_uid}; use --force para prosseguir"
        )),
        (Some(_), Some(_)) => None,
        _ => Some(
            "não foi possível verificar a identidade do backup \
             (uid de origem ou destino indeterminado); use --force para prosseguir"
                .to_string(),
        ),
    };
    if let Some(issue) = &identity_issue {
        if !opts.force && !opts.dry_run {
            return Err(eyre!("{issue}"));
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

    let extracted_profiles = extracted.join("profiles");

    // Conflitos também viram dado, pelo mesmo motivo da checagem de identidade.
    let mut plans: Vec<(&ProfileManifest, Vec<String>, bool)> = Vec::new();
    for pm in &restore_profiles {
        let mut conflicts = Vec::new();
        let dest_profile = paths::profile_dir(&pm.name)?;
        if dest_profile.exists() {
            conflicts.push(format!(
                "profile '{}' already exists at destination; use --force to overwrite",
                pm.name
            ));
        }
        // Conta OAuth divergente é aviso de acidente.
        if let (Some(backup_acc), Some(dest_acc)) =
            (&pm.oauth_account, account::profile_email(&pm.name))
        {
            if backup_acc != &dest_acc {
                conflicts.push(format!(
                    "conta divergente no perfil '{}': backup {} vs destino {}; use --force",
                    pm.name, backup_acc, dest_acc
                ));
            }
        }
        if !conflicts.is_empty() && !opts.force && !opts.dry_run {
            return Err(eyre!("{}", conflicts.join("; ")));
        }
        let has_content = extracted_profiles.join(&pm.name).exists();
        plans.push((pm, conflicts, has_content));
    }

    if opts.dry_run {
        print_restore_plan(&plans, identity_issue.as_deref(), opts.force);
        println!("dry-run: destino não foi alterado");
        return Ok(());
    }

    // Reescrita de paths na área extraída antes de copiar.
    if opts.rewrite_paths && extracted_profiles.exists() {
        let mut changed = Vec::new();
        let mut skipped = Vec::new();
        let from_root = &manifest.profile_root;
        let to_root = profile_root.to_string_lossy();
        rewrite_tree(
            &extracted_profiles,
            from_root,
            &to_root,
            &mut changed,
            &mut skipped,
        )?;
        // Também reescrever o $HOME de origem, se diferente.
        let to_home = home.to_string_lossy();
        if manifest.home != to_home {
            rewrite_tree(
                &extracted_profiles,
                &manifest.home,
                &to_home,
                &mut changed,
                &mut skipped,
            )?;
        }
        if !changed.is_empty() {
            println!("  paths reescritos em {} arquivo(s)", changed.len());
        }
        // As duas passadas veem os mesmos arquivos ilegíveis; reportar uma vez.
        skipped.sort();
        skipped.dedup();
        if !skipped.is_empty() {
            eprintln!(
                "  AVISO: {} arquivo(s) nao puderam ser lidos como texto e foram \
                 restaurados SEM reescrita de paths — podem continuar apontando \
                 para a maquina de origem (use --no-rewrite-paths para pular a \
                 reescrita de todos):",
                skipped.len()
            );
            for detail in &skipped {
                eprintln!("    {detail}");
            }
        }
    }

    // Copiar cada perfil selecionado para o destino, aplicando permissões.
    let mut restored = 0usize;
    let mut skipped_profiles: Vec<String> = Vec::new();
    for pm in &restore_profiles {
        let src = extracted_profiles.join(&pm.name);
        // Perfil listado no manifesto sem diretório no artefato: pular EM
        // SILÊNCIO era o pior caso desta feature. O `cloak restore` imprimia
        // só o cabeçalho, não restaurava nada e retornava 0 — o usuário
        // acreditava que tinha funcionado. Acontece com artefato truncado e
        // com perfil cujo conteúdo era todo não-coberto, caso em que
        // `profiles/<nome>/` nunca chega a ser criado no payload.
        if !src.exists() {
            eprintln!(
                "  AVISO: perfil '{}' esta' no manifesto mas nao tem diretorio no \
                 artefato (profiles/{} ausente); NADA foi restaurado para ele",
                pm.name, pm.name
            );
            skipped_profiles.push(pm.name.clone());
            continue;
        }
        let dest = paths::profile_dir(&pm.name)?;
        // Levantado ANTES de copiar: depois da cópia, os arquivos do backup
        // já existem no destino e não dá mais para distinguir o que era pré-existente.
        let preserved = collect_preserved_files(&src, &dest)?;
        copy_tree_secure(&src, &dest)?;
        restored += 1;
        print_reconstruction_report(pm);
        print_preserved_report(&pm.name, &preserved);
    }

    // Zero perfis restaurados é uma falha, não um sucesso vazio: o comando
    // existe para restaurar, e devolver 0 sem escrever nada faz um script
    // que checa `$?` acreditar que o restore aconteceu.
    if restored == 0 {
        let detail = if skipped_profiles.is_empty() {
            "o manifesto nao lista nenhum perfil".to_string()
        } else {
            format!(
                "sem conteudo no artefato para: {}",
                skipped_profiles.join(", ")
            )
        };
        return Err(eyre!(
            "nenhum perfil foi restaurado ({detail}); o artefato pode estar truncado \
             ou ter sido gerado por uma versao do cloak que nao cobria esses perfis"
        ));
    }
    if !skipped_profiles.is_empty() {
        eprintln!(
            "  {} de {} perfil(is) do manifesto foram pulados: {}",
            skipped_profiles.len(),
            restore_profiles.len(),
            skipped_profiles.join(", ")
        );
    }

    Ok(())
}

/// Arquivos que já existem no destino e NÃO vêm no artefato.
///
/// O restore é um merge: nada do destino é apagado. Isso evita destruir
/// credenciais renovadas ou trabalho criado depois do backup, mas significa
/// que o perfil resultante mistura estado antigo e novo. O usuário precisa
/// saber disso — daí o relatório.
/// Imprime o plano do `restore --dry-run`, incluindo os conflitos detectados.
///
/// O plano nomeia o que um restore real encontraria pela frente em vez de
/// abortar na primeira checagem, que era o comportamento anterior.
fn print_restore_plan(
    plans: &[(&ProfileManifest, Vec<String>, bool)],
    identity_issue: Option<&str>,
    force: bool,
) {
    println!("  perfis a restaurar: {}", plans.len());
    let mut needs_force = false;
    if let Some(issue) = identity_issue {
        println!("  conflito de identidade: {issue}");
        needs_force = true;
    }
    for (pm, conflicts, has_content) in plans {
        println!("    {} (MCP: {})", pm.name, pm.mcp_servers.join(", "));
        if !has_content {
            println!(
                "      AVISO: o artefato nao tem conteudo para este perfil \
                 (profiles/{} ausente); nada seria restaurado",
                pm.name
            );
        }
        for conflict in conflicts {
            println!("      conflito: {conflict}");
            needs_force = true;
        }
    }
    if needs_force && !force {
        println!("  → um restore real destes conflitos exigiria --force");
    }
}

/// Retorna `true` se pelo menos um arquivo dentro de `dir` (recursivamente)
/// também existe em `mirror`.
///
/// É o espelho de `subtree_has_included_file`, e serve ao mesmo propósito:
/// decidir se uma subárvore pode virar uma linha única de relatório.
fn subtree_has_mirrored_file(dir: &Path, mirror: &Path) -> Result<bool> {
    for entry in fs::read_dir(dir).wrap_err_with(|| format!("failed reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let mirrored = mirror.join(entry.file_name());
        if path.is_dir() {
            if subtree_has_mirrored_file(&path, &mirrored)? {
                return Ok(true);
            }
        } else if mirrored.exists() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn collect_preserved(
    dir: &Path,
    mirror: &Path,
    rel_prefix: &str,
    out: &mut Vec<UncoveredEntry>,
) -> Result<()> {
    for entry in fs::read_dir(dir).wrap_err_with(|| format!("failed reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let mirrored = mirror.join(entry.file_name());
        let rel = if rel_prefix.is_empty() {
            name
        } else {
            format!("{rel_prefix}/{name}")
        };
        if path.is_dir() {
            if subtree_has_mirrored_file(&path, &mirrored)? {
                collect_preserved(&path, &mirrored, &rel, out)?;
            } else {
                out.push(UncoveredEntry {
                    path: rel,
                    size_bytes: dir_size(&path),
                });
            }
        } else if !mirrored.exists() {
            out.push(UncoveredEntry {
                path: rel,
                size_bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
            });
        }
    }
    Ok(())
}

/// Arquivos que já existem no destino e NÃO vêm no artefato.
///
/// O restore é um merge: nada do destino é apagado. Isso evita destruir
/// credenciais renovadas ou trabalho criado depois do backup, mas significa
/// que o perfil resultante mistura estado antigo e novo. O usuário precisa
/// saber disso — daí o relatório.
///
/// A agregação por subárvore é a mesma regra de `collect_uncovered`, e existe
/// pelo mesmo motivo: sessões, logs e `plugins/cache` deliberadamente nunca
/// estão no artefato, então enumerar folha a folha imprimia uma linha para
/// cada um dos 45.153 arquivos medidos em perfis reais. Um relatório que não
/// é lido não neutraliza nada.
fn collect_preserved_files(src: &Path, dest: &Path) -> Result<Vec<UncoveredEntry>> {
    let mut preserved = Vec::new();
    if !dest.exists() {
        return Ok(preserved);
    }
    collect_preserved(dest, src, "", &mut preserved)?;
    preserved.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(preserved)
}

fn print_preserved_report(profile: &str, preserved: &[UncoveredEntry]) {
    if preserved.is_empty() {
        return;
    }
    println!(
        "    {} item(ns) já existiam no destino e NÃO estavam no backup — preservados:",
        preserved.len()
    );
    for entry in preserved {
        println!(
            "      {profile}/{} ({} bytes)",
            entry.path, entry.size_bytes
        );
    }
}

/// Aplica as permissoes do destino a um arquivo restaurado: 0600 no caso
/// comum, 0700 quando a origem era executavel.
///
/// Forcar 0600 em tudo tirava o bit de execucao de arquivos que precisam dele:
/// `statusline-command.sh` (na allowlist do claude e criado pelo proprio cloak
/// com 0700), hooks referenciados em `codex/hooks.json` e executaveis sob
/// `skills/` e `.agents/`. Todos voltavam do restore falhando com
/// "Permission denied" no primeiro uso.
///
/// A permissao nunca e' afrouxada para grupo/outros: um `0755` na origem volta
/// como `0700`.
fn apply_restored_file_permissions(src: &Path, target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let executable = fs::metadata(src)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
        if executable {
            return fs::set_permissions(target, fs::Permissions::from_mode(0o700))
                .wrap_err_with(|| format!("failed setting permissions on {}", target.display()));
        }
    }

    #[cfg(not(unix))]
    let _ = src;

    paths::set_owner_only_file(target)
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
            apply_restored_file_permissions(&path, &target)?;
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
    fn test_ensure_supported_format_rejects_newer_version() {
        // O teste anterior montava um Manifest com FORMAT_VERSION + 1 e afirmava
        // `manifest.format_version > FORMAT_VERSION` — aritmetica com
        // constantes, sem exercitar a guarda. Apagar a guarda inteira deixava
        // ele verde, entao o comportamento fail-closed estava sem protecao
        // contra regressao.
        let err = ensure_supported_format(FORMAT_VERSION + 1)
            .expect_err("formato mais novo precisa ser recusado");
        let msg = err.to_string();
        assert!(
            msg.contains(&(FORMAT_VERSION + 1).to_string()),
            "a mensagem precisa nomear a versao do artefato: {msg}"
        );
        assert!(
            msg.contains("atualize o cloak"),
            "a mensagem precisa dizer o que fazer: {msg}"
        );
    }

    #[test]
    fn test_ensure_supported_format_accepts_current_and_older() {
        assert!(ensure_supported_format(FORMAT_VERSION).is_ok());
        assert!(ensure_supported_format(0).is_ok());
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
    fn test_allowlist_covers_gemini_config_and_memory() {
        // REGRESSAO: nao existia arm "gemini" e o perfil inteiro ficava fora do
        // backup. O gemini aninha TUDO em `<perfil>/gemini/.gemini/`, onde os
        // padroes de topo do COMMON_ALLOW (`settings.json`, `*.md`) nao chegam.
        assert!(is_allowed(
            "gemini",
            Path::new(".gemini/settings.json"),
            &[]
        ));
        assert!(is_allowed("gemini", Path::new(".gemini/GEMINI.md"), &[]));
        // Volume, estado de maquina e historico continuam fora.
        assert!(!is_allowed(
            "gemini",
            Path::new(".gemini/tmp/a/b.json"),
            &[]
        ));
        assert!(!is_allowed(
            "gemini",
            Path::new(".gemini/history/a/b.json"),
            &[]
        ));
        assert!(!is_allowed(
            "gemini",
            Path::new(".gemini/installation_id"),
            &[]
        ));
        assert!(!is_allowed("gemini", Path::new(".gemini/state.json"), &[]));
        assert!(!is_allowed(
            "gemini",
            Path::new(".gemini/projects.json"),
            &[]
        ));
        // Credencial so entra por --include-credentials, nunca pela allowlist.
        assert!(!is_allowed(
            "gemini",
            Path::new(".gemini/oauth_creds.json"),
            &[]
        ));
        assert!(!is_allowed("gemini", Path::new(".gemini/.env"), &[]));
    }

    #[test]
    fn test_gemini_profile_is_not_entirely_uncovered() {
        // REGRESSAO: um perfil so-gemini produzia UMA linha agregada
        // `gemini/.gemini` no relatorio e zero arquivos no artefato.
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("gemini");
        fs::create_dir_all(cli_dir.join(".gemini/tmp")).expect("mkdir");
        fs::write(cli_dir.join(".gemini/settings.json"), "{}").expect("settings");
        fs::write(cli_dir.join(".gemini/GEMINI.md"), "memoria").expect("md");
        fs::write(cli_dir.join(".gemini/tmp/x.json"), "lixo").expect("tmp");

        let (included, uncovered) =
            collect_profile_entries(&cli_dir, "gemini", &[]).expect("collect");
        let inc: Vec<String> = included
            .iter()
            .map(|p| {
                p.strip_prefix(&cli_dir)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(
            inc.contains(&".gemini/settings.json".to_string()),
            "settings do gemini precisa entrar no artefato; obtido: {inc:?}"
        );
        assert!(
            inc.contains(&".gemini/GEMINI.md".to_string()),
            "memoria do gemini precisa entrar no artefato; obtido: {inc:?}"
        );

        let unc: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();
        assert!(
            !unc.contains(&".gemini"),
            "a subarvore .gemini nao pode ser reportada inteira como fora; obtido: {unc:?}"
        );
        assert!(
            unc.contains(&".gemini/tmp"),
            "o lixo dentro de .gemini continua fora e precisa ser reportado; obtido: {unc:?}"
        );
    }

    #[test]
    fn test_cli_extra_patterns_gates_credentials_behind_the_flag() {
        // Sem a flag, a credencial nao e' coberta por nada.
        let without = cli_extra_patterns("claude", &[], false);
        assert!(!is_allowed(
            "claude",
            Path::new(".credentials.json"),
            &without
        ));

        // Com a flag, ela passa a ser coberta — e' isso que impede o relatorio
        // e o manifesto de negarem um arquivo que esta' dentro do payload.
        let with = cli_extra_patterns("claude", &[], true);
        assert!(is_allowed("claude", Path::new(".credentials.json"), &with));

        let codex = cli_extra_patterns("codex", &[], true);
        assert!(is_allowed("codex", Path::new("auth.json"), &codex));

        let gemini = cli_extra_patterns("gemini", &[], true);
        assert!(is_allowed(
            "gemini",
            Path::new(".gemini/oauth_creds.json"),
            &gemini
        ));
        assert!(is_allowed("gemini", Path::new(".gemini/.env"), &gemini));

        // A credencial de um CLI nao vaza para outro.
        assert!(!is_allowed("codex", Path::new(".credentials.json"), &codex));

        // Os padroes do usuario continuam somados.
        let user = cli_extra_patterns("claude", &["extra/*.json".to_string()], false);
        assert!(is_allowed("claude", Path::new("extra/a.json"), &user));
    }

    #[test]
    fn test_included_credential_is_not_listed_as_uncovered() {
        // REGRESSAO: o relatorio e o manifesto afirmavam
        // "NAO incluido (fora da allowlist): claude/.credentials.json"
        // para um arquivo que estava dentro do payload.
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("claude");
        fs::create_dir_all(&cli_dir).expect("mkdir");
        fs::write(cli_dir.join("settings.json"), "{}").expect("settings");
        fs::write(cli_dir.join(".credentials.json"), "{\"tok\":1}").expect("creds");

        let with = cli_extra_patterns("claude", &[], true);
        let (included, uncovered) =
            collect_profile_entries(&cli_dir, "claude", &with).expect("collect");
        let inc: Vec<String> = included
            .iter()
            .map(|p| {
                p.strip_prefix(&cli_dir)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(inc.contains(&".credentials.json".to_string()));
        let unc: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();
        assert!(
            !unc.contains(&".credentials.json"),
            "credencial copiada para o artefato nao pode ser reportada como fora: {unc:?}"
        );

        // Sem a flag, ela continua fora e continua aparecendo no relatorio.
        let without = cli_extra_patterns("claude", &[], false);
        let (included, uncovered) =
            collect_profile_entries(&cli_dir, "claude", &without).expect("collect");
        let inc: Vec<String> = included
            .iter()
            .map(|p| {
                p.strip_prefix(&cli_dir)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(!inc.contains(&".credentials.json".to_string()));
        let unc: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();
        assert!(unc.contains(&".credentials.json"), "obtido: {unc:?}");
    }

    #[test]
    fn test_gemini_credential_is_not_swallowed_by_aggregated_subtree() {
        // Caso limite da agregacao: `.gemini/` so com a credencial. Sem a flag
        // a subarvore vira uma linha agregada; com a flag o arquivo entra no
        // payload e nao pode continuar sendo reportado como fora.
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("gemini");
        fs::create_dir_all(cli_dir.join(".gemini")).expect("mkdir");
        fs::write(cli_dir.join(".gemini/oauth_creds.json"), "{}").expect("creds");

        let with = cli_extra_patterns("gemini", &[], true);
        let (included, uncovered) =
            collect_profile_entries(&cli_dir, "gemini", &with).expect("collect");
        assert_eq!(included.len(), 1, "a credencial precisa entrar no artefato");
        assert!(
            uncovered.is_empty(),
            "nada pode restar como nao coberto: {uncovered:?}"
        );
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
        assert!(matches!(changed, RewriteOutcome::Rewritten));
        let content = fs::read_to_string(&file).expect("read");
        assert!(content.contains("/home/new/.config/cloak/profiles/x/claude/p"));
        assert!(!content.contains("/home/old"));
    }

    #[test]
    fn test_rewrite_paths_in_file_skips_non_utf8_instead_of_failing() {
        // REGRESSAO: `fs::read_to_string` propagava com `?` e derrubava
        // `rewrite_tree` e `run_restore` inteiros DEPOIS da decifragem, sem
        // restaurar nada. Um skill `.md` em latin-1 ou um `.sh` com byte
        // nao-UTF-8 num comentario bastava.
        let tmp = tempdir().expect("tempdir");
        let file = tmp.path().join("latin1.md");
        // "instalação" em latin-1: 0xE7 e 0xE3 sao sequencias UTF-8 invalidas.
        let raw: Vec<u8> = b"instala\xe7\xe3o em /home/old/x\n".to_vec();
        fs::write(&file, &raw).expect("write latin1");

        let outcome =
            rewrite_paths_in_file(&file, "/home/old", "/home/new").expect("nao pode ser fatal");
        assert!(
            matches!(outcome, RewriteOutcome::Skipped(_)),
            "arquivo ilegivel deve ser pulado e reportado; obtido: {outcome:?}"
        );
        assert_eq!(
            fs::read(&file).expect("read back"),
            raw,
            "o arquivo pulado precisa ficar intacto"
        );
    }

    #[test]
    fn test_rewrite_tree_reports_skipped_and_keeps_rewriting_the_rest() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join("bad.md"), b"\xe7 /home/old/a").expect("bad");
        fs::write(tmp.path().join("good.json"), r#"{"p":"/home/old/b"}"#).expect("good");

        let mut changed = Vec::new();
        let mut skipped = Vec::new();
        rewrite_tree(
            tmp.path(),
            "/home/old",
            "/home/new",
            &mut changed,
            &mut skipped,
        )
        .expect("rewrite_tree nao pode ser fatal");

        assert_eq!(
            changed.len(),
            1,
            "o arquivo legivel foi reescrito: {changed:?}"
        );
        assert_eq!(skipped.len(), 1, "o ilegivel foi reportado: {skipped:?}");
        assert!(skipped[0].contains("bad.md"), "obtido: {skipped:?}");
        let good = fs::read_to_string(tmp.path().join("good.json")).expect("read good");
        assert!(good.contains("/home/new/b"), "obtido: {good}");
    }

    #[test]
    fn test_rewrite_paths_no_match_returns_false() {
        let tmp = tempdir().expect("tempdir");
        let file = tmp.path().join("f.json");
        fs::write(&file, r#"{"k":"/unrelated/path"}"#).expect("write");
        let changed = rewrite_paths_in_file(&file, "/home/old", "/home/new").expect("rewrite");
        assert!(matches!(changed, RewriteOutcome::Unchanged));
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
    fn test_replace_path_root_left_boundary_survives_after_first_match() {
        // REGRESSAO: `rest` avancava para depois de cada match, entao na
        // iteracao seguinte `before` ficava vazio e `next_back()` devolvia
        // `None`, que `is_none_or` tratava como fronteira valida. A segunda
        // ocorrencia — aninhada dentro do proprio path ja rejeitado — era
        // reescrita.
        let out = replace_path_root("x/home/old/home/old", "/home/old", "/home/new");
        assert_eq!(
            out, "x/home/old/home/old",
            "as duas ocorrencias sao componentes de um path alheio: {out}"
        );
    }

    #[test]
    fn test_replace_path_root_rewrites_second_occurrence_when_boundary_is_valid() {
        // A fronteira nao pode virar bloqueio geral: duas raizes legitimas
        // separadas continuam sendo reescritas.
        let out = replace_path_root("\"/home/old/a\" \"/home/old/b\"", "/home/old", "/home/new");
        assert_eq!(out, "\"/home/new/a\" \"/home/new/b\"");
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
        assert!(matches!(changed, RewriteOutcome::Rewritten));
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
        let paths: Vec<&str> = preserved.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["claude/token-novo.json"]);
    }

    #[test]
    fn test_collect_preserved_files_aggregates_fully_preserved_subtrees() {
        // REGRESSAO: uma linha por arquivo folha. Como sessions, logs e
        // plugins/cache deliberadamente nunca estao no artefato, um restore
        // --force sobre um perfil real imprimia uma linha para cada um deles
        // (45.153 arquivos medidos na spec). Um relatorio que nao e' lido nao
        // neutraliza nada — a mesma regra de `collect_uncovered` vale aqui.
        let tmp = tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        fs::create_dir_all(src.join("claude/plugins")).expect("mkdir src");
        fs::create_dir_all(dest.join("claude/sessions")).expect("mkdir sessions");
        fs::create_dir_all(dest.join("claude/plugins/cache/a/b")).expect("mkdir cache");

        // Coberto pelo artefato: nao e' preservado.
        fs::write(src.join("claude/settings.json"), "novo").expect("w1");
        fs::write(dest.join("claude/settings.json"), "antigo").expect("w2");
        fs::write(src.join("claude/plugins/installed_plugins.json"), "{}").expect("w3");
        fs::write(dest.join("claude/plugins/installed_plugins.json"), "{}").expect("w4");

        // Subarvores inteiramente ausentes do artefato.
        for n in 0..5 {
            fs::write(dest.join(format!("claude/sessions/s{n}.jsonl")), "x").expect("session");
        }
        fs::write(dest.join("claude/plugins/cache/a/b/x.bin"), "x").expect("cache1");
        fs::write(dest.join("claude/plugins/cache/z.bin"), "z").expect("cache2");
        // Arquivo solto preservado ao lado de um coberto.
        fs::write(dest.join("claude/token-novo.json"), "x").expect("w5");

        let preserved = collect_preserved_files(&src, &dest).expect("collect");
        let paths: Vec<&str> = preserved.iter().map(|p| p.path.as_str()).collect();

        assert!(
            paths.contains(&"claude/sessions"),
            "subarvore inteiramente preservada vira UMA linha: {paths:?}"
        );
        assert!(
            paths.contains(&"claude/plugins/cache"),
            "cache tambem agrega, dentro de um diretorio misto: {paths:?}"
        );
        assert!(
            paths.contains(&"claude/token-novo.json"),
            "arquivo solto preservado continua nomeado: {paths:?}"
        );
        assert!(
            !paths
                .iter()
                .any(|p| p.contains("s0.jsonl") || p.contains("x.bin")),
            "folhas dentro de subarvore agregada nao podem ser enumeradas: {paths:?}"
        );
        // Tamanho agregado precisa estar preenchido, como no relatorio de
        // nao-cobertos.
        let sessions = preserved
            .iter()
            .find(|p| p.path == "claude/sessions")
            .expect("sessions entry");
        assert!(sessions.size_bytes > 0, "tamanho agregado ausente");
    }

    #[test]
    fn test_empty_allowlisted_dir_is_not_reported_as_uncovered() {
        // REGRESSAO: `subtree_has_included_file` devolve false para diretorio
        // sem arquivos, entao um `claude/skills/` vazio era agregado como
        // nao-coberto e o relatorio imprimia `claude/skills (0 bytes)` sob
        // "NAO incluido (fora da allowlist)", mesmo `skills/` sendo built-in.
        let tmp = tempdir().expect("tempdir");
        let cli_dir = tmp.path().join("claude");
        fs::create_dir_all(cli_dir.join("skills")).expect("mkdir skills");
        fs::create_dir_all(cli_dir.join(".agents")).expect("mkdir .agents");
        fs::create_dir_all(cli_dir.join("sessions")).expect("mkdir sessions");
        fs::write(cli_dir.join("settings.json"), "{}").expect("settings");

        let (_included, uncovered) =
            collect_profile_entries(&cli_dir, "claude", &[]).expect("collect");
        let paths: Vec<&str> = uncovered.iter().map(|u| u.path.as_str()).collect();

        assert!(
            !paths.contains(&"skills"),
            "diretorio vazio DA allowlist nao e' 'fora da allowlist': {paths:?}"
        );
        assert!(
            !paths.contains(&".agents"),
            "diretorio vazio DA allowlist nao e' 'fora da allowlist': {paths:?}"
        );
        assert!(
            paths.contains(&"sessions"),
            "diretorio vazio FORA da allowlist continua sendo reportado: {paths:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_prepare_output_dir_does_not_chmod_preexisting_user_dir() {
        use std::os::unix::fs::PermissionsExt;
        // REGRESSAO: `paths::ensure_secure_dir` chmod 0700 incondicional
        // mutava as permissoes de um diretorio que o cloak nao criou
        // (`--output ~/Downloads`, `--output .`, um output_dir sincronizado).
        let tmp = tempdir().expect("tempdir");
        let user_dir = tmp.path().join("Downloads");
        fs::create_dir(&user_dir).expect("mkdir");
        fs::set_permissions(&user_dir, fs::Permissions::from_mode(0o755)).expect("chmod");

        prepare_output_dir(&user_dir, false).expect("prepare");

        let mode = fs::metadata(&user_dir)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o755,
            "diretorio pre-existente do usuario nao pode ter as permissoes mutadas"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_prepare_output_dir_locks_dirs_created_by_cloak() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempdir().expect("tempdir");
        let mode_of = |p: &Path| fs::metadata(p).expect("metadata").permissions().mode() & 0o777;

        // Diretorio default do cloak: sempre 0700, mesmo se ja existir.
        let default_dir = tmp.path().join("backups");
        fs::create_dir(&default_dir).expect("mkdir");
        fs::set_permissions(&default_dir, fs::Permissions::from_mode(0o755)).expect("chmod");
        prepare_output_dir(&default_dir, true).expect("prepare default");
        assert_eq!(mode_of(&default_dir), 0o700);

        // Diretorio do usuario que o cloak precisa criar: nasce 0700.
        let new_dir = tmp.path().join("novo/destino");
        prepare_output_dir(&new_dir, false).expect("prepare new");
        assert_eq!(mode_of(&new_dir), 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn test_copy_tree_secure_preserves_execute_bit_without_loosening() {
        use std::os::unix::fs::PermissionsExt;
        // REGRESSAO: o restore forcava 0600 em TODO arquivo, tirando o bit de
        // execucao. `statusline-command.sh` esta' na allowlist do claude e e'
        // criado pelo proprio cloak com 0700; hooks de `codex/hooks.json` e
        // executaveis sob `skills/` e `.agents/` voltavam sem permissao de
        // execucao e falhavam com "Permission denied" no primeiro uso.
        let tmp = tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        fs::create_dir_all(src.join("skills")).expect("mkdir");
        fs::write(src.join("settings.json"), "{}").expect("settings");
        fs::write(src.join("statusline-command.sh"), "#!/bin/sh\n").expect("script");
        fs::write(src.join("skills/tool"), "#!/bin/sh\n").expect("tool");
        fs::set_permissions(
            src.join("statusline-command.sh"),
            fs::Permissions::from_mode(0o700),
        )
        .expect("chmod script");
        // Executavel frouxo na origem: o bit de execucao volta, o acesso de
        // grupo/outros nao.
        fs::set_permissions(src.join("skills/tool"), fs::Permissions::from_mode(0o755))
            .expect("chmod tool");

        copy_tree_secure(&src, &dest).expect("copy");

        let mode = |p: &Path| fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(
            mode(&dest.join("settings.json")),
            0o600,
            "arquivo comum continua 0600"
        );
        assert_eq!(
            mode(&dest.join("statusline-command.sh")),
            0o700,
            "script executavel precisa voltar executavel"
        );
        assert_eq!(
            mode(&dest.join("skills/tool")),
            0o700,
            "executavel frouxo volta como 0700, nunca 0755"
        );
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
