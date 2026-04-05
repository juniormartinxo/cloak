use std::{
    fs,
    io::{stderr, stdin, stdout, IsTerminal},
    process::{Command, Stdio},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use color_eyre::eyre::{eyre, Context, Result};

use crate::{
    config::{self, Config},
    paths,
};

pub fn exec_cli(cli_name: &str, profile: &str, args: &[String], config: &Config) -> Result<()> {
    let mut cmd = prepare_exec_command(cli_name, profile, args, config)?;

    if should_launch_detached(cli_name) {
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.spawn()
            .wrap_err("failed launching detached child process")?;
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

pub fn prepare_cli_command(cli_name: &str, profile: &str, config: &Config) -> Result<Command> {
    let (cmd, _, _, _) = prepare_cli_command_with_context(cli_name, profile, config)?;
    Ok(cmd)
}

pub fn prepare_exec_command(
    cli_name: &str,
    profile: &str,
    args: &[String],
    config: &Config,
) -> Result<Command> {
    let (mut cmd, binary, profile_dir, cli_cfg) =
        prepare_cli_command_with_context(cli_name, profile, config)?;
    let permissions = config::permissions_for_agent(config, cli_name);
    enforce_command_permissions(cli_name, args, &permissions)?;

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

    let launch_args = resolve_effective_launch(cli_name, &binary, &rendered_launch_args, &cli_cfg);
    let forwarded_args = resolve_forwarded_args(cli_name, args);
    cmd.args(launch_args);
    cmd.args(&forwarded_args);
    Ok(cmd)
}

fn prepare_cli_command_with_context(
    cli_name: &str,
    profile: &str,
    config: &Config,
) -> Result<(
    Command,
    std::path::PathBuf,
    std::path::PathBuf,
    crate::config::CliConfig,
)> {
    let cli_cfg = config
        .cli
        .get(cli_name)
        .cloned()
        .ok_or_else(|| eyre!("CLI '{}' not configured in config.toml", cli_name))?;

    config::ensure_profile_management_enabled(cli_name)?;

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

    let mut cmd = Command::new(&binary);

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

    Ok((cmd, binary, profile_dir, cli_cfg))
}

fn enforce_command_permissions(
    cli_name: &str,
    args: &[String],
    permissions: &crate::config::AgentPermissions,
) -> Result<()> {
    let Some(command) = forwarded_command_token(args)? else {
        return Ok(());
    };
    let command = command.as_str();

    if permissions
        .deny_commands
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(command))
    {
        return Err(eyre!(
            "command '{command}' is denied for agent '{cli_name}'"
        ));
    }

    if !permissions.allow_shell && command_is_shell(command) {
        return Err(eyre!("shell access is disabled for agent '{cli_name}'"));
    }

    if !permissions.allow_file_write && command_is_file_write(command) {
        return Err(eyre!(
            "file-write operations are disabled for agent '{cli_name}' (command '{command}')"
        ));
    }

    if !permissions.allow_network && command_is_network(command) {
        return Err(eyre!(
            "network access is disabled for agent '{cli_name}' (command '{command}')"
        ));
    }

    let explicitly_allowed = permissions
        .allowed_commands
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(command));

    if command_is_dangerous(command) && !explicitly_allowed {
        return Err(eyre!(
            "dangerous command '{command}' requires explicit allowlist entry for agent '{cli_name}'"
        ));
    }

    if permissions.allowed_commands.is_empty() {
        return Ok(());
    }

    if explicitly_allowed {
        return Ok(());
    }

    Err(eyre!(
        "command '{command}' is not permitted for agent '{cli_name}'"
    ))
}

fn command_is_shell(command: &str) -> bool {
    const SHELL_COMMANDS: [&str; 9] = [
        "sh",
        "bash",
        "zsh",
        "fish",
        "pwsh",
        "powershell",
        "cmd",
        "tcsh",
        "shell",
    ];

    SHELL_COMMANDS
        .iter()
        .any(|forbidden| forbidden.eq_ignore_ascii_case(command))
}

fn command_is_file_write(command: &str) -> bool {
    const FILE_WRITE_COMMANDS: &[&str] = &[
        "cp",
        "mv",
        "rm",
        "rmdir",
        "mkdir",
        "touch",
        "ln",
        "chmod",
        "chown",
        "chgrp",
        "truncate",
        "tee",
        "install",
        "apply_patch",
        "patch",
        "dd",
    ];

    FILE_WRITE_COMMANDS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(command))
}

fn command_is_network(command: &str) -> bool {
    const NETWORK_COMMANDS: &[&str] = &[
        "curl", "wget", "ssh", "scp", "sftp", "nc", "ncat", "git", "npm", "pnpm", "yarn", "pip",
        "python", "python3", "node", "deno", "bun", "go", "ruby", "rustc", "curl.exe", "nc.exe",
    ];

    NETWORK_COMMANDS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(command))
}

fn command_is_dangerous(command: &str) -> bool {
    const DANGEROUS_COMMANDS: &[&str] = &[
        "rm", "rmdir", "mv", "dd", "truncate", "chmod", "chown", "chgrp",
    ];

    DANGEROUS_COMMANDS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(command))
}

fn forwarded_command_token(args: &[String]) -> Result<Option<String>> {
    for arg in args {
        if !arg.starts_with('-') {
            return Ok(Some(arg.to_ascii_lowercase()));
        }
    }

    Ok(None)
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

pub fn prepare_raw_command_with_profile_env(
    cli_name: &str,
    profile: &str,
    program: &str,
    config: &Config,
) -> Result<Command> {
    let cli_cfg = config
        .cli
        .get(cli_name)
        .cloned()
        .ok_or_else(|| eyre!("CLI '{}' not configured in config.toml", cli_name))?;

    config::ensure_profile_management_enabled(cli_name)?;

    let profile_dir = paths::profile_cli_dir(profile, cli_name)?;
    ensure_profile_cli_dir(&profile_dir, profile, cli_name)?;
    let template_context = TemplateContext {
        cli_name,
        profile,
        profile_dir: &profile_dir,
    };

    let mut cmd = Command::new(program);

    if let Some(config_dir_env) = &cli_cfg.config_dir_env {
        cmd.env(config_dir_env, &profile_dir);
    }

    for (name, value) in &cli_cfg.extra_env {
        cmd.env(name, render_template(value, &template_context));
    }

    for var in &cli_cfg.remove_env_vars {
        cmd.env_remove(var);
    }

    Ok(cmd)
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
    use std::{collections::HashMap, path::Path};

    use crate::config::{AgentPermissions, CliConfig, Config, GeneralConfig};

    use super::{
        command_is_dangerous, command_is_file_write, command_is_network, command_is_shell,
        enforce_command_permissions, is_cursor_wsl_wrapper, is_interactive_terminal,
        render_template, resolve_effective_launch, resolve_forwarded_args,
        resolve_remote_agent_folder, should_launch_detached, TemplateContext,
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
        assert!(!is_cursor_wsl_wrapper(
            "cursor",
            Path::new("/usr/bin/cursor")
        ));

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
                Path::new(
                    "/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor"
                ),
                Path::new("/tmp/profiles/work/cursor")
            ),
            Some(Path::new("/tmp/profiles/work/cursor/.cursor-server").into())
        );
        assert_eq!(
            resolve_remote_agent_folder(
                "codex",
                Path::new(
                    "/mnt/c/Users/test/AppData/Local/Programs/cursor/resources/app/bin/cursor"
                ),
                Path::new("/tmp/profiles/work/cursor")
            ),
            None
        );

        unsafe {
            std::env::remove_var("WSL_DISTRO_NAME");
        }
    }

    #[test]
    fn prepare_cli_command_rejects_temporarily_disabled_profile_managed_cli() {
        let cfg = Config {
            general: GeneralConfig {
                default_profile: "personal".to_string(),
            },
            cli: HashMap::from([(
                "cursor".to_string(),
                CliConfig {
                    binary: "cursor".to_string(),
                    config_dir_env: None,
                    remove_env_vars: vec![],
                    extra_env: HashMap::new(),
                    launch_args: vec![],
                },
            )]),
            agents: HashMap::new(),
        };

        let err = super::prepare_cli_command("cursor", "work", &cfg).expect_err("must fail");
        assert!(
            err.to_string()
                .contains("profile management for CLI 'cursor' is temporarily disabled"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn enforce_command_permissions_rejects_shell_when_disabled() {
        let permissions = AgentPermissions {
            allow_shell: false,
            allow_file_write: true,
            allow_network: true,
            allowed_commands: Vec::new(),
            deny_commands: Vec::new(),
        };

        let err = enforce_command_permissions("codex", &["bash".to_string()], &permissions)
            .expect_err("shell command should be blocked");
        assert!(err.to_string().contains("shell access is disabled"));
    }

    #[test]
    fn enforce_command_permissions_allows_shell_when_enabled() {
        let permissions = AgentPermissions {
            allow_shell: true,
            allow_file_write: true,
            allow_network: true,
            allowed_commands: Vec::new(),
            deny_commands: Vec::new(),
        };

        let result = enforce_command_permissions("codex", &["bash".to_string()], &permissions);
        assert!(result.is_ok());
    }

    #[test]
    fn enforce_command_permissions_rejects_file_write_when_disabled() {
        let permissions = AgentPermissions {
            allow_shell: true,
            allow_file_write: false,
            allow_network: true,
            allowed_commands: Vec::new(),
            deny_commands: Vec::new(),
        };

        let err = enforce_command_permissions("codex", &["rm".to_string()], &permissions)
            .expect_err("file write command should be blocked");
        assert!(err
            .to_string()
            .contains("file-write operations are disabled for agent"));
    }

    #[test]
    fn enforce_command_permissions_rejects_network_when_disabled() {
        let permissions = AgentPermissions {
            allow_shell: true,
            allow_file_write: true,
            allow_network: false,
            allowed_commands: Vec::new(),
            deny_commands: Vec::new(),
        };

        let err = enforce_command_permissions("codex", &["curl".to_string()], &permissions)
            .expect_err("network command should be blocked");
        assert!(err
            .to_string()
            .contains("network access is disabled for agent"));
    }

    #[test]
    fn enforce_command_permissions_blocks_dangerous_commands_by_default() {
        let permissions = AgentPermissions {
            allow_shell: true,
            allow_file_write: true,
            allow_network: true,
            allowed_commands: Vec::new(),
            deny_commands: Vec::new(),
        };

        let err = enforce_command_permissions("codex", &["rm".to_string()], &permissions)
            .expect_err("dangerous command should require explicit allowlist");
        assert!(err
            .to_string()
            .contains("requires explicit allowlist entry"));
    }

    #[test]
    fn enforce_command_permissions_allows_dangerous_command_when_explicitly_allowlisted() {
        let permissions = AgentPermissions {
            allow_shell: true,
            allow_file_write: true,
            allow_network: true,
            allowed_commands: vec!["rm".to_string()],
            deny_commands: Vec::new(),
        };

        let result = enforce_command_permissions("codex", &["rm".to_string()], &permissions);
        assert!(result.is_ok());
    }

    #[test]
    fn classify_shell_commands() {
        assert!(command_is_shell("bash"));
        assert!(!command_is_shell("ask"));
    }

    #[test]
    fn classify_file_write_commands() {
        assert!(command_is_file_write("rm"));
        assert!(!command_is_file_write("ask"));
    }

    #[test]
    fn classify_network_commands() {
        assert!(command_is_network("curl"));
        assert!(!command_is_network("ask"));
    }

    #[test]
    fn classify_dangerous_commands() {
        assert!(command_is_dangerous("rm"));
        assert!(command_is_dangerous("mv"));
        assert!(!command_is_dangerous("cp"));
    }
}
