use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Parser, Debug)]
#[command(
    name = "cloak",
    version,
    about = "LLM CLI profile manager by directory"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute a CLI with the profile resolved from current directory
    #[command(trailing_var_arg = true)]
    Exec {
        /// Registered CLI name (for example: claude, codex)
        cli: String,
        /// Optional explicit profile. Must be passed before forwarded CLI args.
        #[arg(long)]
        profile: Option<String>,
        /// Arguments forwarded to target CLI
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Set or change the profile for the current directory (.cloak)
    #[command(visible_alias = "init")]
    Use {
        /// Profile to bind to this directory
        profile: String,
    },

    /// Manage profiles
    #[command(subcommand)]
    Profile(ProfileCommands),

    /// Shortcut for interactive authentication using the resolved profile
    Login {
        /// Registered CLI name (for example: claude, codex)
        cli: String,
        /// Optional explicit profile. If omitted, profile is resolved from CWD.
        profile: Option<String>,
    },

    /// Manage MCP server installation for supported CLIs
    #[command(subcommand)]
    Mcp(McpCommands),

    /// View or rank usage limits
    Limits {
        /// Profile name to show limits for, or 'rank' to rank all profiles
        profile: Option<String>,

        /// Display timestamps in a specific UTC offset (e.g. -3 for UTC-3, 5 for UTC+5)
        #[arg(long, allow_hyphen_values = true)]
        utc: Option<i32>,
    },

    /// Check config, binaries and profiles
    Doctor,

    /// Generate shell completions
    Completions { shell: Shell },

    /// Configure per-agent permissions
    Permission {
        #[command(subcommand)]
        command: PermissionCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProfileCommands {
    /// List profiles
    List,

    /// Show which account(s) a profile is authenticated with
    Account { name: String },

    /// Create profile directory structure
    Create { name: String },

    /// Delete profile directory
    Delete {
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Show profile that would be used for current directory
    Show,
}

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// Install an MCP server from the built-in registry (interactive)
    ///
    /// Running `cloak mcp add` without arguments prints the catalog.
    Add {
        /// MCP entry name (use `cloak mcp add` to see the catalog)
        name: Option<String>,

        /// Restrict target CLIs (comma-separated, e.g. `codex,claude`)
        #[arg(long = "for", value_delimiter = ',')]
        targets: Vec<String>,

        /// Install only for this profile (conflicts with --all-profiles/--no-all-profiles)
        #[arg(long, conflicts_with_all = ["all_profiles", "no_all_profiles"])]
        profile: Option<String>,

        /// Force installation for every existing profile (default when interactive)
        #[arg(long, conflicts_with = "no_all_profiles")]
        all_profiles: bool,

        /// Restrict installation to the profile resolved from the current directory
        #[arg(long)]
        no_all_profiles: bool,

        /// Accept defaults without prompting (all supported CLIs, all profiles)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Print the resolved command(s) for each target CLI without installing
        #[arg(long)]
        show: bool,
    },

    /// Install an MCP server using the native CLI syntax for each supported tool
    #[command(trailing_var_arg = true)]
    Install {
        /// Registered CLI name (for example: claude, codex)
        cli: String,

        /// MCP server name
        name: String,

        /// Optional explicit profile. If omitted, profile is resolved from CWD.
        #[arg(long)]
        profile: Option<String>,

        /// Install for every existing profile instead of the resolved profile only
        #[arg(long)]
        all_profiles: bool,

        /// MCP transport
        #[arg(long, value_enum, default_value_t = McpTransport::Stdio)]
        transport: McpTransport,

        /// Streamable HTTP/SSE endpoint
        #[arg(long)]
        url: Option<String>,

        /// Environment variables for stdio MCP servers (`KEY=VALUE`)
        #[arg(short = 'e', long = "env")]
        env: Vec<String>,

        /// Headers for HTTP/SSE MCP servers (`Name: value`)
        #[arg(short = 'H', long = "header")]
        header: Vec<String>,

        /// Bearer token environment variable for Codex HTTP MCP servers
        #[arg(long)]
        bearer_token_env_var: Option<String>,

        /// Run the command after `--` directly with profile env, bypassing the CLI's native `mcp add`
        #[arg(long)]
        raw: bool,

        /// Stdio command forwarded after `--`
        #[arg(allow_hyphen_values = true)]
        command: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PermissionCommands {
    /// Interactive setup of execution permissions for one agent
    Ask {
        /// Agent name (for example: codex)
        #[arg(long)]
        agent: Option<String>,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

pub fn command_for_completions() -> clap::Command {
    Cli::command()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands, McpCommands, McpTransport, ProfileCommands};

    #[test]
    fn test_exec_parses_forwarded_args() {
        let parsed = Cli::parse_from([
            "cloak",
            "exec",
            "claude",
            "--",
            "--model",
            "sonnet",
            "fix this bug",
        ]);

        match parsed.command {
            Commands::Exec { cli, profile, args } => {
                assert_eq!(cli, "claude");
                assert_eq!(profile, None);
                assert_eq!(args, vec!["--model", "sonnet", "fix this bug"]);
            }
            _ => panic!("expected exec command"),
        }
    }

    #[test]
    fn test_exec_parses_explicit_profile_flag() {
        let parsed = Cli::parse_from([
            "cloak",
            "exec",
            "codex",
            "--profile",
            "work",
            "fix this bug",
        ]);

        match parsed.command {
            Commands::Exec { cli, profile, args } => {
                assert_eq!(cli, "codex");
                assert_eq!(profile.as_deref(), Some("work"));
                assert_eq!(args, vec!["fix this bug"]);
            }
            _ => panic!("expected exec command"),
        }
    }

    #[test]
    fn test_exec_forwards_target_profile_flag_after_separator() {
        let parsed = Cli::parse_from([
            "cloak",
            "exec",
            "codex",
            "--",
            "--profile",
            "target-profile",
        ]);

        match parsed.command {
            Commands::Exec { cli, profile, args } => {
                assert_eq!(cli, "codex");
                assert_eq!(profile, None);
                assert_eq!(args, vec!["--profile", "target-profile"]);
            }
            _ => panic!("expected exec command"),
        }
    }

    #[test]
    fn test_use_parses_profile() {
        let parsed = Cli::parse_from(["cloak", "use", "work"]);

        match parsed.command {
            Commands::Use { profile } => assert_eq!(profile, "work"),
            _ => panic!("expected use command"),
        }
    }

    #[test]
    fn test_profile_account_parses_name() {
        let parsed = Cli::parse_from(["cloak", "profile", "account", "work"]);

        match parsed.command {
            Commands::Profile(ProfileCommands::Account { name }) => assert_eq!(name, "work"),
            _ => panic!("expected profile account command"),
        }
    }

    #[test]
    fn test_limits_parses_profile() {
        let parsed = Cli::parse_from(["cloak", "limits", "work"]);

        match parsed.command {
            Commands::Limits { profile, utc } => {
                assert_eq!(profile.as_deref(), Some("work"));
                assert_eq!(utc, None);
            }
            _ => panic!("expected limits command"),
        }
    }

    #[test]
    fn test_limits_parses_positive_utc_offset() {
        let parsed = Cli::parse_from(["cloak", "limits", "work", "--utc", "5"]);

        match parsed.command {
            Commands::Limits { profile, utc } => {
                assert_eq!(profile.as_deref(), Some("work"));
                assert_eq!(utc, Some(5));
            }
            _ => panic!("expected limits command"),
        }
    }

    #[test]
    fn test_limits_parses_negative_utc_offset() {
        let parsed = Cli::parse_from(["cloak", "limits", "work", "--utc", "-3"]);

        match parsed.command {
            Commands::Limits { profile, utc } => {
                assert_eq!(profile.as_deref(), Some("work"));
                assert_eq!(utc, Some(-3));
            }
            _ => panic!("expected limits command"),
        }
    }

    #[test]
    fn test_init_alias_maps_to_use() {
        let parsed = Cli::parse_from(["cloak", "init", "work"]);

        match parsed.command {
            Commands::Use { profile } => assert_eq!(profile, "work"),
            _ => panic!("expected use command from init alias"),
        }
    }

    #[test]
    fn test_mcp_install_parses_stdio_command() {
        let parsed = Cli::parse_from([
            "cloak",
            "mcp",
            "install",
            "codex",
            "filesystem",
            "--profile",
            "work",
            "-e",
            "API_KEY=secret",
            "--",
            "npx",
            "@modelcontextprotocol/server-filesystem",
            "/tmp",
        ]);

        match parsed.command {
            Commands::Mcp(McpCommands::Install {
                cli,
                name,
                profile,
                all_profiles,
                transport,
                url,
                env,
                header,
                bearer_token_env_var,
                raw,
                command,
            }) => {
                assert_eq!(cli, "codex");
                assert_eq!(name, "filesystem");
                assert_eq!(profile.as_deref(), Some("work"));
                assert!(!all_profiles);
                assert!(!raw);
                assert_eq!(transport, McpTransport::Stdio);
                assert_eq!(url, None);
                assert_eq!(env, vec!["API_KEY=secret"]);
                assert!(header.is_empty());
                assert_eq!(bearer_token_env_var, None);
                assert_eq!(
                    command,
                    vec!["npx", "@modelcontextprotocol/server-filesystem", "/tmp",]
                );
            }
            _ => panic!("expected mcp install command"),
        }
    }

    #[test]
    fn test_mcp_install_parses_raw_flag() {
        let parsed = Cli::parse_from([
            "cloak",
            "mcp",
            "install",
            "codex",
            "shadcn",
            "--all-profiles",
            "--raw",
            "--",
            "npx",
            "shadcn@latest",
            "mcp",
            "init",
            "--client",
            "codex",
        ]);

        match parsed.command {
            Commands::Mcp(McpCommands::Install {
                cli,
                name,
                all_profiles,
                raw,
                command,
                ..
            }) => {
                assert_eq!(cli, "codex");
                assert_eq!(name, "shadcn");
                assert!(all_profiles);
                assert!(raw);
                assert_eq!(
                    command,
                    vec!["npx", "shadcn@latest", "mcp", "init", "--client", "codex"]
                );
            }
            _ => panic!("expected mcp install command"),
        }
    }

    #[test]
    fn test_mcp_add_parses_without_name_for_catalog_listing() {
        let parsed = Cli::parse_from(["cloak", "mcp", "add"]);
        match parsed.command {
            Commands::Mcp(McpCommands::Add {
                name,
                targets,
                profile,
                all_profiles,
                no_all_profiles,
                yes,
                show,
            }) => {
                assert!(name.is_none());
                assert!(targets.is_empty());
                assert!(profile.is_none());
                assert!(!all_profiles);
                assert!(!no_all_profiles);
                assert!(!yes);
                assert!(!show);
            }
            _ => panic!("expected mcp add command"),
        }
    }

    #[test]
    fn test_mcp_add_parses_targets_and_profile_scope() {
        let parsed = Cli::parse_from([
            "cloak",
            "mcp",
            "add",
            "gitnexus",
            "--for",
            "codex,claude",
            "--no-all-profiles",
            "--yes",
        ]);
        match parsed.command {
            Commands::Mcp(McpCommands::Add {
                name,
                targets,
                profile,
                all_profiles,
                no_all_profiles,
                yes,
                show,
            }) => {
                assert_eq!(name.as_deref(), Some("gitnexus"));
                assert_eq!(targets, vec!["codex".to_string(), "claude".to_string()]);
                assert!(profile.is_none());
                assert!(!all_profiles);
                assert!(no_all_profiles);
                assert!(yes);
                assert!(!show);
            }
            _ => panic!("expected mcp add command"),
        }
    }

    #[test]
    fn test_mcp_add_profile_conflicts_with_all_profiles() {
        let err = Cli::try_parse_from([
            "cloak",
            "mcp",
            "add",
            "gitnexus",
            "--profile",
            "work",
            "--all-profiles",
        ])
        .err();
        assert!(err.is_some(), "expected conflict error");
    }

    #[test]
    fn test_mcp_install_parses_http_options() {
        let parsed = Cli::parse_from([
            "cloak",
            "mcp",
            "install",
            "claude",
            "sentry",
            "--all-profiles",
            "--transport",
            "http",
            "--url",
            "https://mcp.sentry.dev/mcp",
            "-H",
            "Authorization: Bearer token",
        ]);

        match parsed.command {
            Commands::Mcp(McpCommands::Install {
                cli,
                name,
                all_profiles,
                transport,
                url,
                header,
                ..
            }) => {
                assert_eq!(cli, "claude");
                assert_eq!(name, "sentry");
                assert!(all_profiles);
                assert_eq!(transport, McpTransport::Http);
                assert_eq!(url.as_deref(), Some("https://mcp.sentry.dev/mcp"));
                assert_eq!(header, vec!["Authorization: Bearer token"]);
            }
            _ => panic!("expected mcp install command"),
        }
    }
}
