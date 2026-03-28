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

    let (binary, launch_args) =
        resolve_effective_launch(cli_name, binary, &rendered_launch_args, cli_cfg);

    let mut cmd = Command::new(binary);
    cmd.args(launch_args);
    cmd.args(args);

    if let Some(config_dir_env) = &cli_cfg.config_dir_env {
        cmd.env(config_dir_env, &profile_dir);
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
    binary: std::path::PathBuf,
    launch_args: &[String],
    cli_cfg: &crate::config::CliConfig,
) -> (std::path::PathBuf, Vec<String>) {
    if !is_cursor_wsl_wrapper(cli_name, binary.as_path()) {
        return (binary, launch_args.to_vec());
    }

    let has_user_data_arg = cli_cfg
        .launch_args
        .iter()
        .any(|arg| arg == "--user-data-dir");
    let has_extensions_arg = cli_cfg
        .launch_args
        .iter()
        .any(|arg| arg == "--extensions-dir");

    if !has_user_data_arg && !has_extensions_arg {
        return (binary, launch_args.to_vec());
    }

    eprintln!(
        "Warning: Cursor is running through the Windows WSL wrapper ({}). \
Launching Cursor.exe directly with isolated --user-data-dir and without --extensions-dir so extension auth can use the cloak profile while installed extensions remain available.",
        binary.display()
    );

    let direct_binary = resolve_cursor_windows_exe(binary.as_path()).unwrap_or(binary);
    let filtered_args = strip_flag_with_value(launch_args, "--extensions-dir");

    (direct_binary, filtered_args)
}

fn resolve_cursor_windows_exe(binary: &std::path::Path) -> Option<std::path::PathBuf> {
    let exe_path = binary
        .parent()?
        .parent()?
        .parent()?
        .parent()?
        .join("Cursor.exe");

    Some(exe_path)
}

fn strip_flag_with_value(args: &[String], flag: &str) -> Vec<String> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut skip_next = false;

    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }

        if arg == flag {
            skip_next = true;
            continue;
        }

        filtered.push(arg.clone());
    }

    filtered
}

fn is_cursor_wsl_wrapper(cli_name: &str, binary: &std::path::Path) -> bool {
    if cli_name != "cursor" {
        return false;
    }

    if std::env::var_os("WSL_DISTRO_NAME").is_none() {
        return false;
    }

    binary.to_string_lossy().starts_with("/mnt/c/")
}

fn should_launch_detached(cli_name: &str) -> bool {
    matches!(cli_name, "cursor") && is_interactive_terminal()
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
        resolve_cursor_windows_exe, resolve_effective_launch, should_launch_detached,
        strip_flag_with_value, TemplateContext,
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

        assert!(!is_cursor_wsl_wrapper(
            "cursor",
            Path::new("/usr/bin/cursor")
        ));

        unsafe {
            std::env::remove_var("WSL_DISTRO_NAME");
        }
    }

    #[test]
    fn cursor_wsl_warning_condition_depends_on_editor_isolation_args() {
        let cfg = CliConfig {
            binary: "cursor".to_string(),
            config_dir_env: None,
            remove_env_vars: vec![],
            extra_env: Default::default(),
            launch_args: vec![
                "--user-data-dir".to_string(),
                "{profile_dir}".to_string(),
                "--new-window".to_string(),
            ],
        };

        assert!(cfg.launch_args.iter().any(|arg| arg == "--user-data-dir"));
        assert!(!cfg.launch_args.iter().any(|arg| arg == "--extensions-dir"));
    }

    #[test]
    fn strips_extensions_dir_pair_but_keeps_other_args() {
        let args = vec![
            "--user-data-dir".to_string(),
            "/tmp/profile".to_string(),
            "--extensions-dir".to_string(),
            "/tmp/profile/extensions".to_string(),
            "--new-window".to_string(),
        ];

        assert_eq!(
            strip_flag_with_value(&args, "--extensions-dir"),
            vec![
                "--user-data-dir".to_string(),
                "/tmp/profile".to_string(),
                "--new-window".to_string(),
            ]
        );
    }

    #[test]
    fn resolves_cursor_windows_exe_from_wsl_wrapper_path() {
        let binary =
            Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor");

        assert_eq!(
            resolve_cursor_windows_exe(binary),
            Some(Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/Cursor.exe").into())
        );
    }

    #[test]
    fn rewrites_cursor_wsl_launch_to_drop_extensions_dir() {
        unsafe {
            std::env::set_var("WSL_DISTRO_NAME", "Ubuntu");
        }

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

        let (binary, args) = resolve_effective_launch(
            "cursor",
            Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor")
                .into(),
            &rendered_args,
            &cfg,
        );

        assert_eq!(
            binary,
            Path::new("/mnt/c/Users/test/AppData/Local/Programs/cursor/Cursor.exe")
        );
        assert_eq!(
            args,
            vec![
                "--user-data-dir".to_string(),
                "/tmp/profile".to_string(),
                "--new-window".to_string(),
            ]
        );

        unsafe {
            std::env::remove_var("WSL_DISTRO_NAME");
        }
    }

    #[test]
    fn launches_cursor_detached_only_for_interactive_terminal_flow() {
        assert!(should_launch_detached("cursor") == is_interactive_terminal());
        assert!(!should_launch_detached("codex"));
    }
}
