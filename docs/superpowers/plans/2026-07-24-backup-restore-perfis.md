# Backup e Restauração de Perfis — Plano de Implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adicionar os subcomandos `cloak backup` e `cloak restore`, que geram e restauram um artefato cifrado com a configuração e o conhecimento dos perfis.

**Architecture:** Um módulo novo `src/backup.rs` concentra seleção por allowlist, manifesto, empacotamento e criptografia. Ferramentas de sistema (`tar`, `gzip`, `gpg`, `date`, `hostname`) são invocadas via `std::process::Command`, coerente com a arquitetura do cloak, sem nenhuma dependência nova no `Cargo.toml`. O `main.rs` só ganha o dispatch; `cli.rs`, `config.rs` e `doctor.rs` ganham, respectivamente, os subcomandos, o bloco `[backup]` e a checagem das ferramentas.

**Tech Stack:** Rust 2021, `clap` (derive), `serde`/`serde_json`, `toml`, `color-eyre`, binários de sistema `tar`/`gzip`/`gpg`.

## Global Constraints

- Rust 2021, toolchain `1.93.1`; preservar compatibilidade com o fluxo atual de `cargo`.
- **Nenhuma dependência nova** em `Cargo.toml`. Exceção única e já aprovada: mover `tempfile` de `[dev-dependencies]` para `[dependencies]` (Task 0), porque `run_backup`, `run_restore` e `gpg_can_encrypt` precisam de diretório temporário em código de produção. Nenhuma outra alteração de dependência é permitida.
- **Testes não podem mutar env vars globais** (`std::env::set_var`) para controlar caminhos: os testes unitários rodam em paralelo na mesma process e `XDG_CONFIG_HOME` compartilhado gera flake. Funções que dependem da raiz de configuração recebem essa raiz por parâmetro (variante `*_at(root, ...)`), com um wrapper fino que resolve via `paths::`. Os testes exercitam a variante `_at`.
- Sem `unwrap`, `expect` ou `panic!` em fluxo de usuário; propagar com `color-eyre` e contexto explícito via `.wrap_err(...)`.
- Permissões Unix: `0700` para diretórios criados, `0600` para arquivos criados — usar `paths::ensure_secure_dir` e `paths::set_owner_only_file`.
- Não alterar nomes de subcomandos, variáveis de ambiente ou layout de perfis existentes.
- Paths de projeto na documentação são relativos ao repositório.
- Nunca assinar commits com atribuição de IA (sem trailer `Co-Authored-By`). A assinatura GPG do git é desejada e permanece; se um commit falhar por `gpg: signing failed: Timeout`, o executor deve pedir ao usuário para rodar `gpg-config unlock github` e então repetir o commit.
- Gates antes de finalizar cada task: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`.

---

## Arquitetura de Arquivos

| Arquivo | Responsabilidade | Ação |
|---|---|---|
| `Cargo.toml` | Mover `tempfile` para `[dependencies]` | Modificar |
| `src/backup.rs` | Manifesto, allowlist, empacotamento, gpg, backup e restore | Criar |
| `src/cli.rs` | Subcomandos `Backup` e `Restore` (clap) | Modificar |
| `src/config.rs` | Struct `BackupConfig` e campo `backup` no `Config` | Modificar |
| `src/main.rs` | `mod backup;` e arms de dispatch | Modificar |
| `src/doctor.rs` | Checagem de `tar`/`gzip`/`gpg` com teste real de cifragem | Modificar |
| `src/paths.rs` | `backups_dir()` (destino padrão) | Modificar |
| `tests/backup_integration.rs` | Roundtrip backup→restore end-to-end | Criar |

### Interfaces internas de `src/backup.rs` (contrato entre tasks)

```rust
// Opções vindas do CLI/dispatch
pub struct BackupOptions {
    pub profile: Option<String>,      // None = todos os perfis
    pub output: Option<PathBuf>,      // sobrepõe config e default
    pub include_credentials: bool,
    pub dry_run: bool,
}
pub struct RestoreOptions {
    pub archive: PathBuf,
    pub profile: Option<String>,      // None = todos os perfis do artefato
    pub force: bool,
    pub dry_run: bool,
    pub rewrite_paths: bool,          // false quando --no-rewrite-paths
}

// Pontos de entrada usados pelo main.rs
pub fn run_backup(config: &Config, opts: BackupOptions) -> Result<()>;
pub fn run_restore(config: &Config, opts: RestoreOptions) -> Result<()>;

// Manifesto (serde_json)
pub struct Manifest {
    pub format_version: u32,
    pub cloak_version: String,
    pub created_at: String,           // "YYYYMMDD-HHMMSS" UTC
    pub hostname: String,
    pub uid: Option<u32>,
    pub home: String,                 // $HOME de origem
    pub profile_root: String,         // caminho absoluto de profiles/
    pub include_credentials: bool,
    pub profiles: Vec<ProfileManifest>,
}
pub struct ProfileManifest {
    pub name: String,
    pub oauth_account: Option<String>,
    pub mcp_servers: Vec<String>,
    pub uncovered: Vec<UncoveredEntry>,
}
pub struct UncoveredEntry { pub path: String, pub size_bytes: u64 }

// Helpers testáveis (privados exceto onde indicado)
fn allowlist_patterns(cli_name: &str) -> Vec<&'static str>;
fn is_allowed(cli_name: &str, rel: &Path, extra: &[String]) -> bool;
fn rewrite_paths_in_file(file: &Path, from: &str, to: &str) -> Result<bool>;
fn gpg_encrypt(input: &Path, output: &Path, passphrase: Option<&str>) -> Result<()>;
fn gpg_decrypt(input: &Path, output: &Path, passphrase: Option<&str>) -> Result<()>;
fn resolve_passphrase() -> Option<String>;   // lê env CLOAK_BACKUP_PASSPHRASE
```

**Convenção de passphrase (usada por gpg_encrypt/gpg_decrypt e pelos testes):** se a env `CLOAK_BACKUP_PASSPHRASE` estiver definida, o gpg roda em modo não-interativo (`--batch --pinentry-mode loopback --passphrase-fd 0`, passphrase via stdin); caso contrário, o gpg usa o pinentry interativo padrão. Os testes de integração sempre definem `CLOAK_BACKUP_PASSPHRASE`.

---

## Task 0: Mover `tempfile` para dependência de produção

**Files:**
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `tempfile` utilizável em código não-teste (`src/backup.rs`, `src/doctor.rs`).

**Contexto:** `tempfile` está hoje em `[dev-dependencies]` e só é usado dentro de `#[cfg(test)]`. As Tasks 8, 9 e 10 precisam de diretório temporário 0700 em código de produção. Esta é a única alteração de dependência aprovada para este plano.

- [ ] **Step 1: Mover a linha**

Em `Cargo.toml`, remover `tempfile = "3"` de `[dev-dependencies]` e adicioná-la ao final de `[dependencies]`, mantendo a ordem existente das demais. O bloco `[dev-dependencies]` fica vazio — remover o cabeçalho se não restar nenhuma entrada.

Resultado esperado do bloco:

```toml
[dependencies]
base64 = "0.22"
clap = { version = "4", features = ["derive"] }
clap_complete = "4"
toml = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "6"
color-eyre = "0.6"
owo-colors = "4"
which = "8"
comfy-table = "7"
rustyline = { version = "14", default-features = false }
tempfile = "3"
```

- [ ] **Step 2: Verificar que tudo ainda compila e passa**

Run: `cargo build 2>&1 | tail -5 && cargo test 2>&1 | tail -15`
Expected: build OK; toda a suíte existente continua passando (os testes que já usavam `tempfile` seguem funcionando, agora via dependência normal).

- [ ] **Step 3: Verificar que `Cargo.lock` não ganhou pacotes novos**

Run: `git diff --stat Cargo.lock`
Expected: `Cargo.lock` sem mudanças, ou apenas reordenação — nenhum pacote novo. `tempfile` já estava no lock.

- [ ] **Step 4: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add Cargo.toml Cargo.lock
git commit -m "build: move tempfile para dependencia de producao"
```

---

## Task 1: Bloco `[backup]` na configuração

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs` (módulo `#[cfg(test)]` no final)

**Interfaces:**
- Consumes: struct `Config` existente.
- Produces: `Config.backup: Option<BackupConfig>`; `BackupConfig { output_dir: Option<String>, upload_command: Option<String>, include: Vec<String> }`.

- [ ] **Step 1: Escrever o teste que falha**

No módulo de testes de `src/config.rs`, adicionar:

```rust
    #[test]
    fn test_parse_config_reads_backup_block() {
        let raw = r#"
[general]
default_profile = "personal"

[cli.claude]
binary = "claude"
config_dir_env = "CLAUDE_CONFIG_DIR"

[backup]
output_dir = "/tmp/cloak-backups"
upload_command = "rclone copy {archive} gdrive:cloak/"
include = ["extra/*.json"]
"#;
        let parsed = parse_config_str(raw, Path::new("config.toml")).expect("parse");
        let backup = parsed.backup.expect("backup block present");
        assert_eq!(backup.output_dir.as_deref(), Some("/tmp/cloak-backups"));
        assert_eq!(
            backup.upload_command.as_deref(),
            Some("rclone copy {archive} gdrive:cloak/")
        );
        assert_eq!(backup.include, vec!["extra/*.json".to_string()]);
    }

    #[test]
    fn test_parse_config_without_backup_block_is_none() {
        let parsed = parse_config_str(DEFAULT_CONFIG_TOML, Path::new("config.toml"))
            .expect("default parses");
        assert!(parsed.backup.is_none());
    }
```

Adicionar `BackupConfig` ao `use super::{...}` do módulo de testes.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib config:: 2>&1 | tail -20`
Expected: FAIL — `no field 'backup' on type 'Config'` / `BackupConfig` não existe.

- [ ] **Step 3: Implementar**

Em `src/config.rs`, adicionar o campo ao `Config` (logo após `agents`):

```rust
    #[serde(default)]
    pub backup: Option<BackupConfig>,
```

E a struct nova, após `AgentPermissions`/`Default`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupConfig {
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub upload_command: Option<String>,
    #[serde(default)]
    pub include: Vec<String>,
}
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test --lib config:: 2>&1 | tail -20`
Expected: PASS (todos os testes de config, inclusive os dois novos).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): adiciona bloco [backup] a configuracao"
```

---

## Task 2: Subcomandos `Backup` e `Restore` no CLI

**Files:**
- Modify: `src/cli.rs`
- Test: `src/cli.rs` (módulo `#[cfg(test)]`)

**Interfaces:**
- Produces: variantes `Commands::Backup { profile, output, include_credentials, dry_run }` e `Commands::Restore { archive, profile, force, dry_run, no_rewrite_paths }`.

- [ ] **Step 1: Escrever o teste que falha**

No módulo de testes de `src/cli.rs`, adicionar `Cli`/`Commands` ao `use super::{...}` se necessário e:

```rust
    #[test]
    fn test_backup_parses_flags() {
        let parsed = Cli::parse_from([
            "cloak", "backup", "--profile", "work", "--dry-run",
        ]);
        match parsed.command {
            Commands::Backup { profile, dry_run, include_credentials, output } => {
                assert_eq!(profile.as_deref(), Some("work"));
                assert!(dry_run);
                assert!(!include_credentials);
                assert!(output.is_none());
            }
            _ => panic!("expected backup command"),
        }
    }

    #[test]
    fn test_restore_parses_archive_and_flags() {
        let parsed = Cli::parse_from([
            "cloak", "restore", "/tmp/backup.tar.gz.gpg", "--force", "--no-rewrite-paths",
        ]);
        match parsed.command {
            Commands::Restore { archive, force, no_rewrite_paths, profile, dry_run } => {
                assert_eq!(archive.to_str(), Some("/tmp/backup.tar.gz.gpg"));
                assert!(force);
                assert!(no_rewrite_paths);
                assert!(profile.is_none());
                assert!(!dry_run);
            }
            _ => panic!("expected restore command"),
        }
    }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib cli:: 2>&1 | tail -20`
Expected: FAIL — variantes `Backup`/`Restore` inexistentes.

- [ ] **Step 3: Implementar**

Em `src/cli.rs`, garantir `use std::path::PathBuf;` no topo. Adicionar ao enum `Commands` (após `Permission`):

```rust
    /// Generate an encrypted backup of profile configuration and knowledge
    Backup {
        /// Back up only this profile (default: all profiles)
        #[arg(long)]
        profile: Option<String>,

        /// Directory to write the artifact into (overrides config and default)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Include OAuth credentials in the backup (requires encryption)
        #[arg(long)]
        include_credentials: bool,

        /// Print the inclusion/uncovered report without generating an artifact
        #[arg(long)]
        dry_run: bool,
    },

    /// Restore profiles from an encrypted backup artifact
    Restore {
        /// Path to the encrypted backup artifact
        archive: PathBuf,

        /// Restore only this profile (default: all profiles in the artifact)
        #[arg(long)]
        profile: Option<String>,

        /// Overwrite existing profiles and bypass identity checks
        #[arg(long)]
        force: bool,

        /// Print the restore plan without touching the destination
        #[arg(long)]
        dry_run: bool,

        /// Do not rewrite absolute paths from the origin machine
        #[arg(long)]
        no_rewrite_paths: bool,
    },
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test --lib cli:: 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "feat(cli): adiciona subcomandos backup e restore"
```

---

## Task 3: Destino padrão `backups_dir()`

**Files:**
- Modify: `src/paths.rs`
- Test: `src/paths.rs` (módulo `#[cfg(test)]`)

**Interfaces:**
- Produces: `pub fn backups_dir() -> Result<PathBuf>` → `<cloak_config_dir>/backups`.

- [ ] **Step 1: Escrever o teste que falha**

No módulo de testes de `src/paths.rs`:

```rust
    #[test]
    fn test_backups_dir_is_under_cloak_config() {
        // Não mutar XDG_CONFIG_HOME: os testes rodam em paralelo na mesma
        // process. A relação com cloak_config_dir vale para qualquer raiz.
        let dir = super::backups_dir().expect("backups dir");
        let base = super::cloak_config_dir().expect("config dir");
        assert_eq!(dir, base.join("backups"));
        assert!(dir.ends_with("cloak/backups"), "unexpected: {}", dir.display());
    }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib paths::tests::test_backups_dir 2>&1 | tail -15`
Expected: FAIL — `backups_dir` não existe.

- [ ] **Step 3: Implementar**

Em `src/paths.rs`, após `profiles_dir()`:

```rust
pub fn backups_dir() -> Result<PathBuf> {
    Ok(cloak_config_dir()?.join("backups"))
}
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test --lib paths::tests::test_backups_dir 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/paths.rs
git commit -m "feat(paths): adiciona backups_dir como destino padrao"
```

---

## Task 4: Módulo `backup.rs` — allowlist e relatório de não-cobertos

**Files:**
- Create: `src/backup.rs`
- Modify: `src/main.rs` (adicionar `mod backup;`)
- Test: `src/backup.rs` (módulo `#[cfg(test)]`)

**Interfaces:**
- Produces: `allowlist_patterns`, `is_allowed`, `struct UncoveredEntry`, `fn collect_profile_entries(profile_dir: &Path, cli_name: &str, extra: &[String]) -> Result<(Vec<PathBuf>, Vec<UncoveredEntry>)>` — retorna (arquivos incluídos, entradas de topo não cobertas). Os caminhos incluídos são absolutos; `UncoveredEntry.path` é relativo à raiz do CLI.

- [ ] **Step 1: Adicionar o módulo ao crate**

Em `src/main.rs`, na lista de `mod` (linhas 1-10), inserir em ordem alfabética:

```rust
mod backup;
```

- [ ] **Step 2: Escrever o teste que falha**

Criar `src/backup.rs` com o esqueleto e os testes:

```rust
use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{eyre, Context, Result};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncoveredEntry {
    pub path: String,
    pub size_bytes: u64,
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
            .map(|p| p.strip_prefix(&cli_dir).unwrap().to_string_lossy().into_owned())
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
            .map(|p| p.strip_prefix(&cli_dir).unwrap().to_string_lossy().into_owned())
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
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test --lib backup:: 2>&1 | tail -25`
Expected: FAIL — `is_allowed` / `collect_profile_entries` não definidos.

- [ ] **Step 4: Implementar**

Em `src/backup.rs`, acima do módulo de testes:

```rust
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

fn walk_files(root: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)
        .wrap_err_with(|| format!("failed reading {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_files(root, &path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_profile_entries(
    cli_dir: &Path,
    cli_name: &str,
    extra: &[String],
) -> Result<(Vec<PathBuf>, Vec<UncoveredEntry>)> {
    let mut included = Vec::new();
    let mut uncovered = Vec::new();

    let mut all_files = Vec::new();
    walk_files(cli_dir, cli_dir, &mut all_files)?;
    for file in all_files {
        let rel = file
            .strip_prefix(cli_dir)
            .unwrap_or(&file)
            .to_path_buf();
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

    for entry in fs::read_dir(cli_dir)
        .wrap_err_with(|| format!("failed reading {}", cli_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if path.is_dir() {
            let mut files = Vec::new();
            walk_files(cli_dir, &path, &mut files)?;
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
```

- [ ] **Step 5: Rodar e ver passar**

Run: `cargo test --lib backup:: 2>&1 | tail -25`
Expected: PASS (3 testes).

- [ ] **Step 6: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/backup.rs src/main.rs
git commit -m "feat(backup): allowlist de selecao e relatorio de nao-cobertos"
```

---

## Task 5: Manifesto do backup

**Files:**
- Modify: `src/backup.rs`
- Test: `src/backup.rs` (módulo `#[cfg(test)]`)

**Interfaces:**
- Consumes: `account::profile_email` (lê `/oauthAccount/emailAddress`), `config::Config`.
- Produces: `struct Manifest`, `struct ProfileManifest`, `fn build_profile_manifest(profile: &str, uncovered: Vec<UncoveredEntry>) -> ProfileManifest`, `fn read_mcp_servers_at(profile_root: &Path, profile: &str) -> Vec<String>` e o wrapper `fn read_mcp_servers(profile: &str) -> Vec<String>`.

**Nota de design (constraint global):** a leitura recebe a raiz por parâmetro (`_at`) para que o teste não precise mutar `XDG_CONFIG_HOME`. O wrapper resolve a raiz via `paths::profiles_dir()` e é o que o resto do código chama.

- [ ] **Step 1: Escrever o teste que falha**

Adicionar ao módulo de testes de `src/backup.rs`:

```rust
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
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib backup:: 2>&1 | tail -25`
Expected: FAIL — `Manifest`, `read_mcp_servers` inexistentes.

- [ ] **Step 3: Implementar**

Em `src/backup.rs`, adicionar os `use` e as structs. No topo, junto aos imports:

```rust
use serde::{Deserialize, Serialize};

use crate::{account, config::Config, paths};
```

E as definições:

```rust
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
    let claude_json = profile_root.join(profile).join("claude").join(".claude.json");
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

fn build_profile_manifest(
    profile: &str,
    uncovered: Vec<UncoveredEntry>,
) -> ProfileManifest {
    ProfileManifest {
        name: profile.to_string(),
        oauth_account: account::profile_email(profile),
        mcp_servers: read_mcp_servers(profile),
        uncovered,
    }
}
```

> Nota: `read_mcp_servers` cobre `claude`. Codex expõe `mcp_servers` em `config.toml`, mas esse arquivo entra no backup pela allowlist e é reconciliado pelo próprio codex; para o manifesto, o foco é o `.claude.json`, que **não** é copiado. Não adicionar leitura de codex aqui (YAGNI).

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test --lib backup:: 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/backup.rs
git commit -m "feat(backup): manifesto com conta OAuth, MCP e nao-cobertos"
```

---

## Task 6: Wrapper de gpg (cifrar/decifrar)

**Files:**
- Modify: `src/backup.rs`
- Test: `src/backup.rs` (módulo `#[cfg(test)]`, teste ignorado sem gpg)

**Interfaces:**
- Produces: `fn resolve_passphrase() -> Option<String>`, `fn gpg_encrypt(input, output, passphrase) -> Result<()>`, `fn gpg_decrypt(input, output, passphrase) -> Result<()>`, `fn ensure_tool(name: &str) -> Result<()>`.

- [ ] **Step 1: Escrever o teste que falha**

Adicionar ao módulo de testes:

```rust
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
```

Adicionar `which` ao teste requer que `which` seja acessível — já é dependência do crate.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib backup::tests::test_gpg 2>&1 | tail -25`
Expected: FAIL — `gpg_encrypt`/`gpg_decrypt` inexistentes.

- [ ] **Step 3: Implementar**

Adicionar `use std::process::{Command, Stdio};` e `use std::io::Write;` aos imports de `src/backup.rs`. Implementar:

```rust
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
        let mut child = cmd
            .spawn()
            .wrap_err("failed to spawn gpg")?;
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
        let status = cmd
            .status()
            .wrap_err("failed to run gpg (interactive)")?;
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
```

Adicionar `use which;` não é necessário — usar caminho completo `which::which`.

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test --lib backup::tests::test_gpg 2>&1 | tail -25`
Expected: PASS (ou skip com aviso se gpg ausente).

- [ ] **Step 5: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/backup.rs
git commit -m "feat(backup): wrapper gpg simetrico com passphrase interativa ou por env"
```

---

## Task 7: Empacotamento (tar+gzip) e metadados de origem

**Files:**
- Modify: `src/backup.rs`
- Test: `src/backup.rs` (módulo `#[cfg(test)]`)

**Interfaces:**
- Produces: `fn create_tar_gz(src_dir: &Path, output: &Path) -> Result<()>`, `fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()>`, `fn origin_hostname() -> String`, `fn origin_uid(home: &Path) -> u32`, `fn timestamp_utc() -> Result<String>`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
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
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib backup::tests::test_tar 2>&1 | tail -20`
Expected: FAIL — funções inexistentes.

- [ ] **Step 3: Implementar**

`create_tar_gz` empacota o **conteúdo** de `src_dir` (o `-C` garante caminhos relativos):

```rust
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
        return Err(eyre!("tar failed while extracting archive (status {status})"));
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
    if !output.success_or_err() {
        return Err(eyre!("date command failed"));
    }
    let ts = String::from_utf8(output.stdout)
        .wrap_err("date returned non-utf8")?
        .trim()
        .to_string();
    Ok(ts)
}
```

Substituir a chamada inexistente `output.success_or_err()` por checagem de `output.status.success()`:

```rust
    if !output.status.success() {
        return Err(eyre!("date command failed"));
    }
```

(Corrigir isso já ao digitar — o helper `success_or_err` não existe.)

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test --lib backup::tests::test_tar backup::tests::test_timestamp 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/backup.rs
git commit -m "feat(backup): empacotamento tar+gzip e metadados de origem"
```

---

## Task 8: Orquestração de `run_backup`

**Files:**
- Modify: `src/backup.rs`
- Modify: `src/main.rs` (dispatch de `Commands::Backup`)
- Test: coberto pela integração na Task 11 (roundtrip). Sem teste unitário novo aqui.

**Interfaces:**
- Consumes: tudo das tasks 3-7, `config::Config`, `paths`.
- Produces: `pub struct BackupOptions`, `pub fn run_backup(config: &Config, opts: BackupOptions) -> Result<()>`.

- [ ] **Step 1: Implementar `BackupOptions` e `run_backup`**

Em `src/backup.rs`:

```rust
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
        for entry in fs::read_dir(&root)
            .wrap_err_with(|| format!("failed reading {}", root.display()))?
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
    let staging = tempfile::tempdir().wrap_err("failed to create staging dir")?;
    paths::set_owner_only_dir(staging.path())?;
    let staging_profiles = staging.path().join("profiles");
    paths::ensure_secure_dir(&staging_profiles)?;

    let mut profile_manifests = Vec::new();

    println!("{}", "Backup".to_string());
    for profile in &profiles {
        let profile_dir = paths::profile_dir(profile)?;
        if !profile_dir.exists() {
            return Err(eyre!("profile '{profile}' does not exist"));
        }
        let mut all_uncovered = Vec::new();

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
        fs::copy(&global_config, staging.path().join("config.toml"))
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
    fs::write(staging.path().join("manifest.json"), manifest_json)
        .wrap_err("failed writing manifest")?;

    if opts.dry_run {
        println!("dry-run: nenhum artefato gerado");
        return Ok(());
    }

    // tar.gz intermediário e cifragem.
    let output_dir = resolve_output_dir(config, &opts)?;
    paths::ensure_secure_dir(&output_dir)?;
    let filename = format!("cloak-backup-{}.tar.gz.gpg", manifest.created_at);
    let final_path = output_dir.join(&filename);

    let tar_tmp = staging.path().join("archive.tar.gz");
    create_tar_gz(staging.path(), &tar_tmp)?;

    let passphrase = resolve_passphrase();
    if let Err(e) = gpg_encrypt(&tar_tmp, &final_path, passphrase.as_deref()) {
        let _ = fs::remove_file(&final_path);
        return Err(e);
    }
    let _ = fs::remove_file(&tar_tmp);
    paths::set_owner_only_file(&final_path)?;

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
    let rendered = template.replace(
        "{archive}",
        &shell_quote(&archive.to_string_lossy()),
    );
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
```

Adicionar `use crate::{account, config::Config, paths};` já feito na Task 5; garantir que `dirs` está acessível (é dependência do crate — usar `dirs::home_dir()`).

- [ ] **Step 2: Wire no dispatch do main.rs**

Em `src/main.rs`, dentro do `match cli.command`, após o arm `Commands::Doctor => {...}` e antes de `Commands::Completions`:

```rust
        Commands::Backup {
            profile,
            output,
            include_credentials,
            dry_run,
        } => {
            backup::run_backup(
                &loaded.config,
                backup::BackupOptions {
                    profile,
                    output,
                    include_credentials,
                    dry_run,
                },
            )?;
        }
```

- [ ] **Step 3: Compilar**

Run: `cargo build 2>&1 | tail -25`
Expected: compila sem erros. (Restore ainda não referenciado; ok.)

- [ ] **Step 4: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/backup.rs src/main.rs
git commit -m "feat(backup): orquestracao de run_backup e dispatch"
```

---

## Task 9: Reescrita de paths e `run_restore`

**Files:**
- Modify: `src/backup.rs`
- Modify: `src/main.rs` (dispatch de `Commands::Restore`)
- Test: `src/backup.rs` (unit test de `rewrite_paths_in_file`)

**Interfaces:**
- Consumes: `extract_tar_gz`, `gpg_decrypt`, `Manifest`, `paths`.
- Produces: `pub struct RestoreOptions`, `pub fn run_restore(config: &Config, opts: RestoreOptions) -> Result<()>`, `fn rewrite_paths_in_file(file: &Path, from: &str, to: &str) -> Result<bool>`.

- [ ] **Step 1: Escrever o teste que falha**

```rust
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
        let changed =
            rewrite_paths_in_file(&file, "/home/old", "/home/new").expect("rewrite");
        assert!(!changed);
    }
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib backup::tests::test_rewrite 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Implementar reescrita e restore**

```rust
const REWRITE_EXTENSIONS: &[&str] = &["json", "toml", "md", "sh"];

fn rewrite_paths_in_file(file: &Path, from: &str, to: &str) -> Result<bool> {
    let is_text = file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| REWRITE_EXTENSIONS.contains(&e))
        .unwrap_or(false);
    if !is_text {
        return Ok(false);
    }
    let content = fs::read_to_string(file)
        .wrap_err_with(|| format!("failed reading {}", file.display()))?;
    let updated = replace_path_root(&content, from, to);
    if updated == content {
        return Ok(false);
    }
    fs::write(file, updated)
        .wrap_err_with(|| format!("failed writing {}", file.display()))?;
    Ok(true)
}

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

        let left_ok = before.chars().next_back().is_none_or(|c| !continues_path_component(c));
        let right_ok = after.chars().next().is_none_or(|c| !continues_path_component(c));

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

fn rewrite_tree(dir: &Path, from: &str, to: &str, changed: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)
        .wrap_err_with(|| format!("failed reading {}", dir.display()))?
    {
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

    let staging = tempfile::tempdir().wrap_err("failed creating restore staging dir")?;
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
                    pm.name, backup_acc, dest_acc
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

fn copy_tree_secure(src: &Path, dest: &Path) -> Result<()> {
    paths::ensure_secure_dir(dest)?;
    for entry in fs::read_dir(src)
        .wrap_err_with(|| format!("failed reading {}", src.display()))?
    {
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
        println!("      {path}");
    }
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
```

- [ ] **Step 4: Wire no dispatch do main.rs**

Após o arm `Commands::Backup`:

```rust
        Commands::Restore {
            archive,
            profile,
            force,
            dry_run,
            no_rewrite_paths,
        } => {
            backup::run_restore(
                &loaded.config,
                backup::RestoreOptions {
                    archive,
                    profile,
                    force,
                    dry_run,
                    rewrite_paths: !no_rewrite_paths,
                },
            )?;
        }
```

- [ ] **Step 5: Rodar unit tests e compilar**

Run: `cargo test --lib backup::tests::test_rewrite 2>&1 | tail -20 && cargo build 2>&1 | tail -10`
Expected: testes PASS, build OK.

- [ ] **Step 6: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/backup.rs src/main.rs
git commit -m "feat(backup): run_restore com reescrita de paths e checagem de identidade"
```

---

## Task 10: Checagem de ferramentas no `doctor`

**Files:**
- Modify: `src/doctor.rs`
- Test: `src/doctor.rs` (módulo `#[cfg(test)]`)

**Interfaces:**
- Consumes: nada das tasks anteriores (independente).
- Produces: `fn check_backup_tools() -> BackupToolsSummary { tar: bool, gzip: bool, gpg: bool, gpg_encrypts: bool }` e `fn gpg_can_encrypt() -> bool`.

- [ ] **Step 1: Escrever o teste que falha**

No módulo de testes de `src/doctor.rs` (criar `#[cfg(test)] mod tests` se não existir):

```rust
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
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test --lib doctor::tests::test_check_backup_tools 2>&1 | tail -20`
Expected: FAIL — `check_backup_tools` inexistente.

- [ ] **Step 3: Implementar**

Em `src/doctor.rs`, adicionar `use std::process::{Command, Stdio};` (se ausente) e `use std::io::Write;`, e:

```rust
pub struct BackupToolsSummary {
    pub tar: bool,
    pub gzip: bool,
    pub gpg: bool,
    pub gpg_encrypts: bool,
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
```

- [ ] **Step 4: Exibir no `run_doctor`**

Dentro de `run_doctor`, antes do `Ok(())` final, adicionar uma seção:

```rust
    let backup_tools = check_backup_tools();
    println!();
    println!("{}", format_section_title("Backup Tools"));
    print_detail_line("tar", if backup_tools.tar { "found" } else { "MISSING" });
    print_detail_line("gzip", if backup_tools.gzip { "found" } else { "MISSING" });
    print_detail_line("gpg", if backup_tools.gpg { "found" } else { "MISSING" });
    print_detail_line(
        "gpg encrypt",
        if backup_tools.gpg_encrypts { "ok" } else { "FAILED" },
    );
```

- [ ] **Step 5: Rodar e ver passar**

Run: `cargo test --lib doctor:: 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add src/doctor.rs
git commit -m "feat(doctor): checa tar/gzip/gpg e capacidade real de cifragem"
```

---

## Task 11: Teste de integração end-to-end

**Files:**
- Create: `tests/backup_integration.rs`

**Interfaces:**
- Consumes: o binário `cloak` compilado (`env!("CARGO_BIN_EXE_cloak")`).

- [ ] **Step 1: Escrever o teste que falha (roundtrip)**

Criar `tests/backup_integration.rs`:

```rust
#[cfg(unix)]
mod backup_tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
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
        assert!(!claude.join("sessions/log.jsonl").exists(), "session NOT restored");
        assert!(!claude.join(".claude.json").exists(), ".claude.json NOT in backup");
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
            !out_dir.exists() || fs::read_dir(&out_dir).map(|mut d| d.next().is_none()).unwrap_or(true),
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
        assert!(!status.success(), "restore must refuse existing profile without --force");
    }
}
```

- [ ] **Step 2: Rodar e ver passar**

Run: `cargo test --test backup_integration -- --nocapture 2>&1 | tail -30`
Expected: PASS (ou skip nos que exigem gpg, se ausente).

- [ ] **Step 3: Suíte completa**

Run: `cargo test 2>&1 | tail -20`
Expected: todos os testes do crate PASS.

- [ ] **Step 4: Gates e commit**

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
git add tests/backup_integration.rs
git commit -m "test(backup): integracao roundtrip, dry-run e recusa sem --force"
```

---

## Task 12: Documentação de uso

**Files:**
- Modify: `docs/usage.md`, `docs/pt-br/uso.md`

**Interfaces:** nenhuma (documentação).

- [ ] **Step 1: Documentar em `docs/pt-br/uso.md`**

Adicionar uma seção "Backup e restauração" cobrindo:
- `cloak backup [--profile <nome>] [--output <dir>] [--include-credentials] [--dry-run]`
- `cloak restore <arquivo> [--profile <nome>] [--force] [--dry-run] [--no-rewrite-paths]`
- Bloco `[backup]` no `config.toml` (`output_dir`, `upload_command`, `include`).
- **Aviso destacado:** o artefato é cifrado com `gpg --symmetric`; **perder a passphrase torna o backup irrecuperável**.
- Credenciais ficam fora por padrão; rode `cloak login` na máquina nova.
- Env `CLOAK_BACKUP_PASSPHRASE` para uso não-interativo (cron/CI).

- [ ] **Step 2: Espelhar em `docs/usage.md`** (versão em inglês, mesmo conteúdo).

- [ ] **Step 3: Validar e commit**

```bash
git diff --check
git add docs/usage.md docs/pt-br/uso.md
git commit -m "docs(backup): documenta comandos backup e restore"
```

---

## Self-Review (preenchido)

**Cobertura do spec:**
- Comandos `backup`/`restore` e flags → Tasks 2, 8, 9. ✓
- Sem dependências novas → todas usam binários de sistema + crates já presentes (`which`, `dirs`, `serde_json`, `tempfile`). ✓
- Seleção por allowlist + relatório de não-cobertos → Task 4, 8. ✓
- `include` soma aos built-in → Task 1, 4 (`is_allowed` com `extra`). ✓
- `.claude.json` fora do backup, MCP/OAuth só no manifesto → Task 5, 8. ✓
- Manifesto dentro do envelope cifrado → Task 8 (manifest.json vai no tar, depois cifrado). ✓
- Criptografia simétrica sempre; passphrase interativa ou por env → Task 6, 8. ✓
- Credenciais fora por padrão, `--include-credentials` → Task 8. ✓
- Transporte: artefato local + `upload_command` com `{archive}` → Task 8 (`run_upload_command`). ✓
- Destino: `--output` > config > `~/.config/cloak/backups` (0700) → Task 3, 8. ✓
- Diretório de backup nunca engole a si mesmo → Task 4/8: allowlist opera sobre dirs de perfil, `output_dir` é externo por padrão. (Observação: se o usuário apontar `output_dir` para dentro de um perfil, o staging é feito antes da cifragem e o artefato só é escrito no final, fora do staging; não há recursão. Documentar limite em usage.) ✓
- Restore: decifra, valida manifesto, identidade (uid + conta), colisão, reescrita de paths, cópia com permissões, relatório de reconstrução → Task 9. ✓
- `--dry-run` no backup e no restore → Task 8, 9. ✓
- `--no-rewrite-paths` → Task 9. ✓
- `doctor` checa tar/gzip/gpg e cifragem real → Task 10. ✓
- Tratamento de erro: pinentry timeout limpa intermediário e não deixa parcial → Task 8 (`if let Err(e) = gpg_encrypt { remove_file; return Err }`). ✓
- Testes previstos no spec → Tasks 4,5,6,7,9 (unit) + Task 11 (integração). ✓

**Scan de placeholders:** sem TBD/TODO; todo passo de código tem código completo. Corrigido no plano o `output.success_or_err()` inexistente (Task 7, Step 3) apontando o uso de `output.status.success()`.

**Consistência de tipos:** `BackupOptions`/`RestoreOptions`, `Manifest`/`ProfileManifest`/`UncoveredEntry`, `is_allowed(cli, rel, extra)`, `collect_profile_entries`, `gpg_encrypt`/`gpg_decrypt`, `rewrite_paths_in_file(file, from, to)` consistentes entre a seção de interfaces e as tasks 4-11.

**Gaps conhecidos assumidos:** a reescrita de paths cobre a raiz `profile_root` e o `$HOME` de origem por substituição textual em arquivos `.json/.toml/.md/.sh`, conforme o risco já registrado no spec.
