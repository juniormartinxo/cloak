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

#[allow(clippy::only_used_in_recursion)]
fn walk_files(root: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(current).wrap_err_with(|| format!("failed reading {}", current.display()))?
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

#[allow(dead_code)]
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
        let rel = file.strip_prefix(cli_dir).unwrap_or(&file).to_path_buf();
        if is_allowed(cli_name, &rel, extra) {
            included.push(file);
        }
    }

    // Entradas de topo não cobertas, para o relatório (dir vira uma linha só).
    for entry in
        fs::read_dir(cli_dir).wrap_err_with(|| format!("failed reading {}", cli_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let covered = if path.is_dir() {
            included.iter().any(|inc| inc.starts_with(&path))
        } else {
            is_allowed(cli_name, Path::new(&name), extra)
        };
        if !covered {
            let size = if path.is_dir() {
                dir_size(&path)
            } else {
                path.metadata().map(|m| m.len()).unwrap_or(0)
            };
            uncovered.push(UncoveredEntry {
                path: name,
                size_bytes: size,
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
    }
}
