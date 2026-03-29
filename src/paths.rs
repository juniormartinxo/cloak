use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{eyre, Context, Result};

pub fn cloak_config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| eyre!("unable to resolve XDG config directory"))?;
    Ok(base.join("cloak"))
}

pub fn config_file_path() -> Result<PathBuf> {
    Ok(cloak_config_dir()?.join("config.toml"))
}

pub fn profiles_dir() -> Result<PathBuf> {
    Ok(cloak_config_dir()?.join("profiles"))
}

pub fn profile_dir(profile: &str) -> Result<PathBuf> {
    validate_profile_name(profile)?;
    Ok(profiles_dir()?.join(profile))
}

pub fn profile_cli_dir(profile: &str, cli_name: &str) -> Result<PathBuf> {
    validate_cli_name(cli_name)?;
    Ok(profile_dir(profile)?.join(cli_name))
}

pub fn ensure_secure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .wrap_err_with(|| format!("failed creating directory {}", path.display()))?;

    #[cfg(unix)]
    set_owner_only_dir(path)?;

    Ok(())
}

pub fn validate_profile_name(name: &str) -> Result<()> {
    let value = name.trim();
    if value.is_empty() {
        return Err(eyre!("profile name cannot be empty"));
    }

    if value.contains('/') || value.contains('\\') {
        return Err(eyre!("profile name cannot contain path separators"));
    }

    if value == "." || value == ".." {
        return Err(eyre!("profile name cannot be '.' or '..'"));
    }

    if value.starts_with('-') {
        return Err(eyre!("profile name cannot start with '-'"));
    }

    let valid = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));

    if !valid {
        return Err(eyre!(
            "profile name must use only [a-zA-Z0-9._-] characters"
        ));
    }

    Ok(())
}

pub fn validate_cli_name(name: &str) -> Result<()> {
    let value = name.trim();
    if value.is_empty() {
        return Err(eyre!("CLI name cannot be empty"));
    }

    if value.contains('/') || value.contains('\\') {
        return Err(eyre!("CLI name cannot contain path separators"));
    }

    if value == "." || value == ".." {
        return Err(eyre!("CLI name cannot be '.' or '..'"));
    }

    if value.starts_with('-') {
        return Err(eyre!("CLI name cannot start with '-'"));
    }

    let valid = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));

    if !valid {
        return Err(eyre!("CLI name must use only [a-zA-Z0-9_-] characters"));
    }

    Ok(())
}

#[cfg(unix)]
pub fn set_owner_only_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .wrap_err_with(|| format!("failed setting permissions on {}", path.display()))
}

#[cfg(not(unix))]
pub fn set_owner_only_dir(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn set_owner_only_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .wrap_err_with(|| format!("failed setting permissions on {}", path.display()))
}

#[cfg(not(unix))]
pub fn set_owner_only_file(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_cli_name, validate_profile_name};

    #[test]
    fn test_validate_profile_name_accepts_safe_names() {
        assert!(validate_profile_name("work").is_ok());
        assert!(validate_profile_name("personal-1").is_ok());
        assert!(validate_profile_name("client_x.prod").is_ok());
    }

    #[test]
    fn test_validate_profile_name_rejects_path_chars() {
        assert!(validate_profile_name("work/dev").is_err());
        assert!(validate_profile_name("work\\dev").is_err());
    }

    #[test]
    fn test_validate_profile_name_rejects_invalid_values() {
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("..").is_err());
        assert!(validate_profile_name("-work").is_err());
        assert!(validate_profile_name("hello world").is_err());
    }

    #[test]
    fn test_validate_cli_name_accepts_safe_names() {
        assert!(validate_cli_name("claude").is_ok());
        assert!(validate_cli_name("codex-1").is_ok());
        assert!(validate_cli_name("gemini_dev").is_ok());
    }

    #[test]
    fn test_validate_cli_name_rejects_path_chars() {
        assert!(validate_cli_name("claude/dev").is_err());
        assert!(validate_cli_name("claude\\dev").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_secure_dir_creates_dir_with_0700_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("secure");

        super::ensure_secure_dir(&target).expect("ensure");
        assert!(target.is_dir());

        let mode = std::fs::metadata(&target)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_secure_dir_creates_nested_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("a/b/c");

        super::ensure_secure_dir(&target).expect("ensure");
        assert!(target.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn test_set_owner_only_dir_applies_0700() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("dir");
        std::fs::create_dir(&target).expect("mkdir");

        super::set_owner_only_dir(&target).expect("set perms");

        let mode = std::fs::metadata(&target)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn test_set_owner_only_file_applies_0600() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("secret.txt");
        std::fs::write(&file, "data").expect("write");

        super::set_owner_only_file(&file).expect("set perms");

        let mode = std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn test_profile_cli_dir_rejects_invalid_profile_name() {
        assert!(super::profile_cli_dir("../escape", "claude").is_err());
    }

    #[test]
    fn test_profile_cli_dir_rejects_invalid_cli_name() {
        assert!(super::profile_cli_dir("work", "../escape").is_err());
    }

    #[test]
    fn test_validate_cli_name_rejects_invalid_values() {
        assert!(validate_cli_name("").is_err());
        assert!(validate_cli_name("..").is_err());
        assert!(validate_cli_name("-claude").is_err());
        assert!(validate_cli_name("claude.beta").is_err());
        assert!(validate_cli_name("hello world").is_err());
    }
}
