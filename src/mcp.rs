use std::io::Write;
use std::process::{Command, Stdio};

use color_eyre::eyre::{eyre, Result};

use crate::{cli::McpTransport, config::Config, exec};

/// Result of a successful MCP install invocation.
///
/// `AlreadyExists` is returned when the underlying CLI reports the server is
/// already registered. The install is treated as idempotent in that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpInstallOutcome {
    Installed,
    AlreadyExists,
}

#[derive(Debug)]
pub struct McpInstallRequest<'a> {
    pub cli_name: &'a str,
    pub server_name: &'a str,
    pub transport: McpTransport,
    pub url: Option<&'a str>,
    pub env: &'a [String],
    pub headers: &'a [String],
    pub bearer_token_env_var: Option<&'a str>,
    pub command: &'a [String],
}

pub fn install_for_profile(
    request: &McpInstallRequest<'_>,
    profile: &str,
    config: &Config,
) -> Result<McpInstallOutcome> {
    let args = build_install_args(request)?;
    let mut cmd = exec::prepare_cli_command(request.cli_name, profile, config)?;
    cmd.args(&args);

    run_install_command(cmd, request.cli_name, profile)
}

pub fn raw_install_for_profile(
    cli_name: &str,
    server_name: &str,
    command: &[String],
    profile: &str,
    config: &Config,
) -> Result<McpInstallOutcome> {
    if command.is_empty() {
        return Err(eyre!("raw MCP installs require a command after `--`"));
    }

    let mut cmd =
        exec::prepare_raw_command_with_profile_env(cli_name, profile, &command[0], config)?;
    cmd.args(&command[1..]);

    run_install_command(cmd, cli_name, profile).map_err(|err| {
        eyre!(
            "raw MCP install '{}' failed for CLI '{}' in profile '{}': {}",
            server_name,
            cli_name,
            profile,
            err
        )
    })
}

/// Runs an MCP install command and classifies the outcome.
///
/// stdout is inherited so the underlying CLI's progress remains visible; stderr
/// is captured to detect the "already exists" idempotency case. On real failure
/// the captured stderr is forwarded to the user so nothing is silently dropped.
fn run_install_command(
    mut cmd: Command,
    cli_name: &str,
    profile: &str,
) -> Result<McpInstallOutcome> {
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::piped());

    let child = cmd.spawn().map_err(|err| {
        eyre!(
            "failed running '{}' MCP install for profile '{}': {}",
            cli_name,
            profile,
            err
        )
    })?;

    let output = child.wait_with_output().map_err(|err| {
        eyre!(
            "failed waiting for '{}' MCP install for profile '{}': {}",
            cli_name,
            profile,
            err
        )
    })?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        // Forward stderr (warnings, info) to the user as the underlying CLI would have.
        forward_stderr(&stderr);
        return Ok(McpInstallOutcome::Installed);
    }

    if stderr_indicates_already_exists(&stderr) {
        return Ok(McpInstallOutcome::AlreadyExists);
    }

    forward_stderr(&stderr);
    Err(eyre!(
        "MCP install failed for CLI '{}' in profile '{}'",
        cli_name,
        profile
    ))
}

fn forward_stderr(stderr: &str) {
    if stderr.is_empty() {
        return;
    }
    let mut handle = std::io::stderr().lock();
    let _ = handle.write_all(stderr.as_bytes());
    let _ = handle.flush();
}

/// Returns true when the CLI's stderr signals the server is already registered.
/// Matches the messages emitted by `claude mcp add` and `codex mcp add` when
/// re-installing an existing server.
pub fn stderr_indicates_already_exists(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("already exists")
}

pub fn remove_for_profile(
    cli_name: &str,
    server_name: &str,
    profile: &str,
    config: &Config,
) -> Result<()> {
    let args = build_remove_args(cli_name, server_name)?;
    let mut cmd = exec::prepare_cli_command(cli_name, profile, config)?;
    cmd.args(&args);

    let status = cmd.status().map_err(|err| {
        eyre!(
            "failed running '{}' MCP remove for profile '{}': {}",
            cli_name,
            profile,
            err
        )
    })?;

    if status.success() {
        return Ok(());
    }

    Err(eyre!(
        "MCP remove failed for CLI '{}' in profile '{}'",
        cli_name,
        profile
    ))
}

pub fn build_remove_args(cli_name: &str, server_name: &str) -> Result<Vec<String>> {
    if server_name.trim().is_empty() {
        return Err(eyre!("MCP server name cannot be empty"));
    }

    match cli_name {
        "codex" => Ok(vec![
            "mcp".to_string(),
            "remove".to_string(),
            server_name.to_string(),
        ]),
        "claude" => Ok(vec![
            "mcp".to_string(),
            "remove".to_string(),
            "--scope".to_string(),
            "user".to_string(),
            server_name.to_string(),
        ]),
        other => Err(eyre!(
            "CLI '{}' does not have a known native MCP removal flow in cloak yet",
            other
        )),
    }
}

pub fn build_install_args(request: &McpInstallRequest<'_>) -> Result<Vec<String>> {
    validate_request(request)?;

    match request.cli_name {
        "codex" => build_codex_install_args(request),
        "claude" => build_claude_install_args(request),
        other => Err(eyre!(
            "CLI '{}' does not have a known native MCP installation flow in cloak yet",
            other
        )),
    }
}

fn validate_request(request: &McpInstallRequest<'_>) -> Result<()> {
    if request.server_name.trim().is_empty() {
        return Err(eyre!("MCP server name cannot be empty"));
    }

    match request.transport {
        McpTransport::Stdio => {
            if request.url.is_some() {
                return Err(eyre!("--url is only valid for HTTP/SSE MCP transports"));
            }
            if request.command.is_empty() {
                return Err(eyre!("stdio MCP installs require a command after `--`"));
            }
            if request
                .env
                .iter()
                .any(|value| value.split_once('=').is_none())
            {
                return Err(eyre!("--env must use KEY=VALUE format"));
            }
            if !request.headers.is_empty() {
                return Err(eyre!("--header is only valid for HTTP/SSE MCP transports"));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            if request.url.is_none() {
                return Err(eyre!("--url is required for HTTP/SSE MCP installs"));
            }
            if !request.command.is_empty() {
                return Err(eyre!(
                    "HTTP/SSE MCP installs do not accept a stdio command after `--`"
                ));
            }
            if !request.env.is_empty() {
                return Err(eyre!("--env is only valid for stdio MCP installs"));
            }
            if request
                .headers
                .iter()
                .any(|value| value.split_once(':').is_none())
            {
                return Err(eyre!("--header must use `Name: value` format"));
            }
        }
    }

    Ok(())
}

fn build_codex_install_args(request: &McpInstallRequest<'_>) -> Result<Vec<String>> {
    let mut args = vec![
        "mcp".to_string(),
        "add".to_string(),
        request.server_name.to_string(),
    ];

    match request.transport {
        McpTransport::Stdio => {
            for item in request.env {
                args.push("--env".to_string());
                args.push(item.clone());
            }
            args.push("--".to_string());
            args.extend(request.command.iter().cloned());
        }
        McpTransport::Http => {
            if !request.headers.is_empty() {
                return Err(eyre!(
                    "Codex HTTP MCP installs do not accept arbitrary headers; use --bearer-token-env-var when needed"
                ));
            }
            args.push("--url".to_string());
            args.push(request.url.expect("validated url").to_string());
            if let Some(env_var) = request.bearer_token_env_var {
                args.push("--bearer-token-env-var".to_string());
                args.push(env_var.to_string());
            }
        }
        McpTransport::Sse => {
            return Err(eyre!(
                "Codex MCP installs currently support only stdio and HTTP transports"
            ));
        }
    }

    if request.bearer_token_env_var.is_some() && request.transport != McpTransport::Http {
        return Err(eyre!(
            "--bearer-token-env-var is only valid for Codex HTTP MCP installs"
        ));
    }

    Ok(args)
}

fn build_claude_install_args(request: &McpInstallRequest<'_>) -> Result<Vec<String>> {
    if request.bearer_token_env_var.is_some() {
        return Err(eyre!(
            "Claude MCP installs do not support --bearer-token-env-var; use --header instead"
        ));
    }

    let mut args = vec![
        "mcp".to_string(),
        "add".to_string(),
        "--scope".to_string(),
        "user".to_string(),
    ];

    match request.transport {
        McpTransport::Stdio => {
            for item in request.env {
                args.push("-e".to_string());
                args.push(item.clone());
            }
            args.push(request.server_name.to_string());
            args.push("--".to_string());
            args.extend(request.command.iter().cloned());
        }
        McpTransport::Http | McpTransport::Sse => {
            args.push("--transport".to_string());
            args.push(match request.transport {
                McpTransport::Http => "http".to_string(),
                McpTransport::Sse => "sse".to_string(),
                McpTransport::Stdio => unreachable!("handled above"),
            });
            for header in request.headers {
                args.push("-H".to_string());
                args.push(header.clone());
            }
            args.push(request.server_name.to_string());
            args.push(request.url.expect("validated url").to_string());
        }
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use crate::cli::McpTransport;

    use super::{
        build_install_args, build_remove_args, stderr_indicates_already_exists, McpInstallRequest,
    };

    #[test]
    fn detects_claude_already_exists_message() {
        let stderr = "MCP server gitnexus already exists in user config\n";
        assert!(stderr_indicates_already_exists(stderr));
    }

    #[test]
    fn detects_codex_already_exists_message() {
        let stderr = "Error: server `gitnexus` already exists\n";
        assert!(stderr_indicates_already_exists(stderr));
    }

    #[test]
    fn detects_already_exists_case_insensitively() {
        let stderr = "Server ALREADY EXISTS in user config\n";
        assert!(stderr_indicates_already_exists(stderr));
    }

    #[test]
    fn does_not_match_unrelated_errors() {
        let stderr = "Error: invalid transport\n";
        assert!(!stderr_indicates_already_exists(stderr));
    }

    #[test]
    fn empty_stderr_does_not_match() {
        assert!(!stderr_indicates_already_exists(""));
    }

    #[test]
    fn remove_args_for_codex_are_plain() {
        let args = build_remove_args("codex", "gitnexus").expect("build");
        assert_eq!(args, vec!["mcp", "remove", "gitnexus"]);
    }

    #[test]
    fn remove_args_for_claude_carry_user_scope() {
        let args = build_remove_args("claude", "gitnexus").expect("build");
        assert_eq!(args, vec!["mcp", "remove", "--scope", "user", "gitnexus"]);
    }

    #[test]
    fn remove_args_reject_unknown_cli() {
        let err = build_remove_args("gemini", "gitnexus").expect_err("must fail");
        assert!(err.to_string().contains("does not have a known"));
    }

    #[test]
    fn remove_args_reject_empty_name() {
        let err = build_remove_args("codex", "  ").expect_err("must fail");
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn codex_stdio_args_match_native_shape() {
        let request = McpInstallRequest {
            cli_name: "codex",
            server_name: "filesystem",
            transport: McpTransport::Stdio,
            url: None,
            env: &["API_KEY=secret".to_string()],
            headers: &[],
            bearer_token_env_var: None,
            command: &[
                "npx".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "/tmp".to_string(),
            ],
        };

        let args = build_install_args(&request).expect("build args");
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "filesystem",
                "--env",
                "API_KEY=secret",
                "--",
                "npx",
                "@modelcontextprotocol/server-filesystem",
                "/tmp",
            ]
        );
    }

    #[test]
    fn codex_http_rejects_headers() {
        let request = McpInstallRequest {
            cli_name: "codex",
            server_name: "remote",
            transport: McpTransport::Http,
            url: Some("https://example.com/mcp"),
            env: &[],
            headers: &["Authorization: Bearer token".to_string()],
            bearer_token_env_var: None,
            command: &[],
        };

        let err = build_install_args(&request).expect_err("must fail");
        assert!(err.to_string().contains("do not accept arbitrary headers"));
    }

    #[test]
    fn claude_http_args_match_native_shape() {
        let request = McpInstallRequest {
            cli_name: "claude",
            server_name: "sentry",
            transport: McpTransport::Http,
            url: Some("https://mcp.sentry.dev/mcp"),
            env: &[],
            headers: &["Authorization: Bearer token".to_string()],
            bearer_token_env_var: None,
            command: &[],
        };

        let args = build_install_args(&request).expect("build args");
        assert_eq!(
            args,
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "--transport",
                "http",
                "-H",
                "Authorization: Bearer token",
                "sentry",
                "https://mcp.sentry.dev/mcp",
            ]
        );
    }

    #[test]
    fn claude_stdio_rejects_bearer_token_flag() {
        let request = McpInstallRequest {
            cli_name: "claude",
            server_name: "filesystem",
            transport: McpTransport::Stdio,
            url: None,
            env: &[],
            headers: &[],
            bearer_token_env_var: Some("SENTRY_TOKEN"),
            command: &["npx".to_string(), "server".to_string()],
        };

        let err = build_install_args(&request).expect_err("must fail");
        assert!(err
            .to_string()
            .contains("do not support --bearer-token-env-var"));
    }
}
