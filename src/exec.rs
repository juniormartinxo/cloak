use std::{
    fs,
    io::{stdin, stderr, stdout, IsTerminal},
    process::{Command, Stdio},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use color_eyre::eyre::{eyre, Context, Result};

use crate::{config::Config, paths};

pub fn exec_cli(cli_name: &str, profile: &str, args: &[String], config: &Config) -> Result<()> {
    let cli_cfg = config
        .cli
        .get(cli_name)
        .ok_or_else(|| eyre!("CLI '{}' not configured in config.toml", cli_name))?;

    let binary = which::which(&cli_cfg.binary).wrap_err_with(|| {
        format!(
            "'{}' not found in PATH. Install it or set cli.{}.binary in config.",
            cli_cfg.binary, cli_name
        )
    })?;

    let profile_dir = paths::profile_cli_dir(profile, cli_name)?;
    ensure_profile_cli_dir(&profile_dir, profile, cli_name)?;
    let template_context = TemplateContext {
        cli_name,
        profile,
        profile_dir: &profile_dir,
    };

    let rendered_launch_args: Vec<String> = cli_cfg
        .launch_args
        .iter()
        .map(|arg| render_template(arg, &template_context))
        .collect();

    let launch_args = resolve_effective_launch(cli_name, &binary, &rendered_launch_args, cli_cfg);
    let forwarded_args = resolve_forwarded_args(cli_name, args);

    let mut cmd = Command::new(&binary);
    cmd.args(launch_args);
    cmd.args(&forwarded_args);

    if let Some(config_dir_env) = &cli_cfg.config_dir_env {
        cmd.env(config_dir_env, &profile_dir);
    }

    if let Some(agent_folder) = resolve_remote_agent_folder(cli_name, &binary, &profile_dir) {
        cmd.env("VSCODE_AGENT_FOLDER", agent_folder);
    }

    for (name, value) in &cli_cfg.extra_env {
        cmd.env(name, render_template(value, &template_context));
    }

    for var in &cli_cfg.remove_env_vars {
        cmd.env_remove(var);
    }

    if should_launch_detached(cli_name) {
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.spawn().wrap_err("failed launching detached child process")?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        let err = cmd.exec();
        Err(eyre!("exec failed: {}", err))
    }

    #[cfg(not(unix))]
    {
        let status = cmd.status().wrap_err("failed running child process")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn resolve_effective_launch(
    cli_name: &str,
    _binary: &std::path::Path,
    launch_args: &[String],
    _cli_cfg: &crate::config::CliConfig,
) -> Vec<String> {
    let _ = cli_name;
    launch_args.to_vec()
}

fn should_launch_detached(cli_name: &str) -> bool {
    matches!(cli_name, "cursor") && is_interactive_terminal()
}

fn resolve_forwarded_args(cli_name: &str, args: &[String]) -> Vec<String> {
    if cli_name == "cursor" && args.is_empty() {
        return vec![".".to_string()];
    }

    args.to_vec()
}

pub(crate) fn resolve_remote_agent_folder(
    cli_name: &str,
    binary: &std::path::Path,
    profile_dir: &std::path::Path,
) -> Option<std::path::PathBuf> {
    if !is_cursor_wsl_wrapper(cli_name, binary) {
        return None;
    }

    Some(profile_dir.join(".cursor-server"))
}

pub(crate) fn is_cursor_wsl_wrapper(cli_name: &str, binary: &std::path::Path) -> bool {
    if cli_name != "cursor" {
        return false;
    }

    if std::env::var_os("WSL_DISTRO_NAME").is_none() {
        return false;
    }

    binary.to_string_lossy().starts_with("/mnt/c/")
}

fn is_interactive_terminal() -> bool {
    stdin().is_terminal() || stdout().is_terminal() || stderr().is_terminal()
}

pub(crate) struct TemplateContext<'a> {
    pub(crate) cli_name: &'a str,
    pub(crate) profile: &'a str,
    pub(crate) profile_dir: &'a std::path::Path,
}

pub(crate) fn render_template(template: &str, context: &TemplateContext<'_>) -> String {
    let profile_dir = context.profile_dir.display().to_string();

    template
        .replace("{profile_dir}", &profile_dir)
        .replace("{profile_name}", context.profile)
        .replace("{cli_name}", context.cli_name)
}

fn ensure_profile_cli_dir(path: &std::path::Path, profile: &str, cli_name: &str) -> Result<()> {
    let existed_before = path.exists();

    if let Some(profile_dir) = path.parent() {
        paths::ensure_secure_dir(profile_dir)?;
    }

    paths::ensure_secure_dir(path)?;

    if !existed_before {
        eprintln!(
            "Profile '{}' initialized for '{}' at {}",
            profile,
            cli_name,
            display_path(path)
        );
    }

    Ok(())
}

fn display_path(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(home) {
            return format!("~/{}", stripped.display());
        }
    }

    path.display().to_string()
}

#[allow(dead_code)]
fn _is_profile_dir_empty(path: &std::path::Path) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }

    let mut entries =
        fs::read_dir(path).wrap_err_with(|| format!("failed reading {}", path.display()))?;
    Ok(entries.next().is_none())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::CliConfig;

    use super::{
        is_cursor_wsl_wrapper, is_interactive_terminal, render_template,
        resolve_effective_launch, resolve_forwarded_args, resolve_remote_agent_folder,
        should_launch_detached, TemplateContext,
    };

    #[test]
    fn render_template_expands_profile_placeholders() {
        let context = TemplateContext {
            cli_name: "cursor",
            profile: "work",
            profile_dir: Path::new("/tmp/profiles/work/cursor"),
        };

        let rendered = render_template(
            "{cli_name}:{profile_name}:{profile_dir}:{profile_dir}/extensions",
            &context,
        );

        assert_eq!(
            rendered,
            "cursor:work:/tmp/profiles/work/cursor:/tmp/profiles/work/cursor/extensions"
        );
    }

    #[test]
    fn keeps_cursor_wsl_wrapper_launch_args_unchanged() {
        let cfg = CliConfig {
            binary: "cursor".to_string(),
            config_dir_env: None,
            remove_env_vars: vec![],
            extra_env: Default::default(),
            launch_args: vec![
                "--user-data-dir".to_string(),
                "{profile_dir}".to_string(),
                "--extensions-dir".to_string(),
                "{profile_dir}/extensions".to_string(),
                "--new-window".to_string(),
            ],
        };

        let rendered_args = vec![
            "--user-data-dir".to_string(),
            "/tmp/profile".to_string(),
            "--extensions-dir".to_string(),
            "/tmp/profile/extensions".to_string(),
            "--new-window".to_string(),
        ];

        let args = resolve_effective_launch(
            "cursor",
            Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor"),
            &rendered_args,
            &cfg,
        );

        assert_eq!(
            args,
            vec![
                "--user-data-dir".to_string(),
                "/tmp/profile".to_string(),
                "--extensions-dir".to_string(),
                "/tmp/profile/extensions".to_string(),
                "--new-window".to_string(),
            ]
        );
    }

    #[test]
    fn launches_cursor_detached_only_for_interactive_terminal_flow() {
        assert!(should_launch_detached("cursor") == is_interactive_terminal());
        assert!(!should_launch_detached("codex"));
    }

    #[test]
    fn injects_current_directory_for_cursor_when_no_args_are_provided() {
        assert_eq!(resolve_forwarded_args("cursor", &[]), vec![".".to_string()]);
        assert_eq!(
            resolve_forwarded_args("cursor", &["src".to_string()]),
            vec!["src".to_string()]
        );
        assert!(resolve_forwarded_args("codex", &[]).is_empty());
    }

    #[test]
    fn detects_cursor_wsl_wrapper_only_for_cursor_on_wsl_windows_path() {
        unsafe {
            std::env::set_var("WSL_DISTRO_NAME", "Ubuntu");
        }

        assert!(is_cursor_wsl_wrapper(
            "cursor",
            Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor")
        ));
        assert!(!is_cursor_wsl_wrapper(
            "code",
            Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor")
        ));
        assert!(!is_cursor_wsl_wrapper("cursor", Path::new("/usr/bin/cursor")));

        unsafe {
            std::env::remove_var("WSL_DISTRO_NAME");
        }
    }

    #[test]
    fn derives_profile_specific_cursor_remote_agent_folder_on_wsl() {
        unsafe {
            std::env::set_var("WSL_DISTRO_NAME", "Ubuntu");
        }

        assert_eq!(
            resolve_remote_agent_folder(
                "cursor",
                Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor"),
                Path::new("/tmp/profiles/work/cursor")
            ),
            Some(Path::new("/tmp/profiles/work/cursor/.cursor-server").into())
        );
        assert_eq!(
            resolve_remote_agent_folder(
                "codex",
                Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor"),
                Path::new("/tmp/profiles/work/cursor")
            ),
            None
        );

        unsafe {
            std::env::remove_var("WSL_DISTRO_NAME");
        }
    }
}
