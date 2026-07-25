use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context, Result};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UncoveredEntry {
    pub path: String,
    pub size_bytes: u64,
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
