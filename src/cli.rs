use clap::{CommandFactory, Parser, Subcommand};
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

    /// Check config, binaries and profiles
    Doctor,

    /// Generate shell completions
    Completions { shell: Shell },
}

#[derive(Subcommand, Debug)]
pub enum ProfileCommands {
    /// List profiles
    List,

    /// Show which account(s) a profile is authenticated with
    Account { name: String },

    /// Show usage limits for supported CLIs in a profile
    Limits { name: String },

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

pub fn command_for_completions() -> clap::Command {
    Cli::command()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands, ProfileCommands};

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
    fn test_profile_limits_parses_name() {
        let parsed = Cli::parse_from(["cloak", "profile", "limits", "work"]);

        match parsed.command {
            Commands::Profile(ProfileCommands::Limits { name }) => assert_eq!(name, "work"),
            _ => panic!("expected profile limits command"),
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
}
