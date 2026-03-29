use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{eyre, Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CloakFile {
    profile: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub name: String,
    pub source: ProfileSource,
}

#[derive(Debug, Clone)]
pub enum ProfileSource {
    CloakFile(PathBuf),
    DefaultProfile,
}

pub fn resolve_profile(start: &Path, default_profile: &str) -> Result<ResolvedProfile> {
    if let Some((name, path)) = find_cloak_file(start)? {
        return Ok(ResolvedProfile {
            name,
            source: ProfileSource::CloakFile(path),
        });
    }

    Ok(ResolvedProfile {
        name: default_profile.to_string(),
        source: ProfileSource::DefaultProfile,
    })
}

pub fn find_cloak_file(start: &Path) -> Result<Option<(String, PathBuf)>> {
    let mut current = normalize_start(start);

    loop {
        let candidate = current.join(".cloak");
        if candidate.is_file() {
            let profile_name = read_profile_from_file(&candidate)?;
            return Ok(Some((profile_name, candidate)));
        }

        if !current.pop() {
            return Ok(None);
        }
    }
}

pub fn write_cloak_file(dir: &Path, profile: &str) -> Result<PathBuf> {
    let path = dir.join(".cloak");
    let content = format!("profile = \"{}\"\n", profile);
    fs::write(&path, content).wrap_err_with(|| format!("failed writing {}", path.display()))?;
    Ok(path)
}

fn normalize_start(start: &Path) -> PathBuf {
    let resolved = start.to_path_buf();

    if resolved.is_file() {
        resolved
            .parent()
            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf)
    } else {
        resolved
    }
}

fn read_profile_from_file(path: &Path) -> Result<String> {
    let raw =
        fs::read_to_string(path).wrap_err_with(|| format!("failed reading {}", path.display()))?;
    let parsed: CloakFile = toml::from_str(&raw)
        .wrap_err_with(|| format!("invalid .cloak format at {}", path.display()))?;

    let profile = parsed.profile.trim();
    if profile.is_empty() {
        return Err(eyre!("profile in {} cannot be empty", path.display()));
    }

    Ok(profile.to_string())
}

#[cfg(test)]
mod tests {
    use super::{find_cloak_file, resolve_profile, ProfileSource};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs as unix_fs;

    use tempfile::tempdir;

    #[test]
    fn test_find_cloak_walks_up() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        let deep = repo.join("src/deep");

        fs::create_dir_all(&deep).expect("mkdir");
        fs::write(repo.join(".cloak"), "profile = \"work\"\n").expect("write");

        let found = find_cloak_file(&deep).expect("find");
        let (profile, path) = found.expect("must find .cloak");

        assert_eq!(profile, "work");
        assert_eq!(path, repo.join(".cloak"));
    }

    #[test]
    fn test_find_cloak_uses_closest() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        let sub = root.join("sub");
        fs::create_dir_all(&sub).expect("mkdir");

        fs::write(root.join(".cloak"), "profile = \"root\"\n").expect("write root");
        fs::write(sub.join(".cloak"), "profile = \"sub\"\n").expect("write sub");

        let found = find_cloak_file(&sub).expect("find");
        let (profile, path) = found.expect("must find .cloak");

        assert_eq!(profile, "sub");
        assert_eq!(path, sub.join(".cloak"));
    }

    #[test]
    fn test_fallback_to_default() {
        let tmp = tempdir().expect("tempdir");

        let resolved = resolve_profile(tmp.path(), "personal").expect("resolve profile");

        assert_eq!(resolved.name, "personal");
        assert!(matches!(resolved.source, ProfileSource::DefaultProfile));
    }

    #[test]
    fn test_find_cloak_errors_on_invalid_file() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join(".cloak"), "this is invalid toml").expect("write");

        let err = find_cloak_file(tmp.path()).expect_err("should fail on invalid .cloak");
        assert!(
            err.to_string().contains("invalid .cloak format"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn test_write_cloak_file_creates_readable_file() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();

        let path = super::write_cloak_file(dir, "work").expect("write");
        assert_eq!(path, dir.join(".cloak"));

        let content = fs::read_to_string(&path).expect("read");
        assert_eq!(content, "profile = \"work\"\n");

        let found = find_cloak_file(dir).expect("find");
        let (profile, _) = found.expect("must find .cloak");
        assert_eq!(profile, "work");
    }

    #[test]
    fn test_write_cloak_file_overwrites_existing() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();

        super::write_cloak_file(dir, "old").expect("write first");
        super::write_cloak_file(dir, "new").expect("write second");

        let found = find_cloak_file(dir).expect("find");
        let (profile, _) = found.expect("must find .cloak");
        assert_eq!(profile, "new");
    }

    #[test]
    fn test_resolve_profile_uses_cloak_file_when_present() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        fs::write(dir.join(".cloak"), "profile = \"work\"\n").expect("write");

        let resolved = resolve_profile(dir, "default").expect("resolve");
        assert_eq!(resolved.name, "work");
        assert!(matches!(resolved.source, ProfileSource::CloakFile(_)));
    }

    #[test]
    fn test_find_cloak_returns_none_for_empty_tree() {
        let tmp = tempdir().expect("tempdir");
        let deep = tmp.path().join("a/b/c");
        fs::create_dir_all(&deep).expect("mkdir");

        let found = find_cloak_file(&deep).expect("find");
        assert!(found.is_none());
    }

    #[test]
    fn test_find_cloak_errors_on_empty_profile_name() {
        let tmp = tempdir().expect("tempdir");
        fs::write(tmp.path().join(".cloak"), "profile = \"\"\n").expect("write");

        let err = find_cloak_file(tmp.path()).expect_err("should fail");
        assert!(
            err.to_string().contains("cannot be empty"),
            "unexpected: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_find_cloak_keeps_logical_symlink_path() {
        let tmp = tempdir().expect("tempdir");
        let real_repo = tmp.path().join("real/repo");
        let logical_root = tmp.path().join("logical");
        let logical_repo = logical_root.join("repo");
        let logical_subdir = logical_repo.join("sub");

        fs::create_dir_all(real_repo.join("sub")).expect("create real repo");
        fs::create_dir_all(&logical_root).expect("create logical root");
        unix_fs::symlink(&real_repo, &logical_repo).expect("create symlink");
        fs::write(logical_root.join(".cloak"), "profile = \"work\"\n").expect("write .cloak");

        let found = find_cloak_file(&logical_subdir).expect("find");
        let (profile, path) = found.expect("must find .cloak");

        assert_eq!(profile, "work");
        assert_eq!(path, logical_root.join(".cloak"));
    }
}
