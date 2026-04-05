mod account;
mod cli;
mod config;
mod doctor;
mod exec;
mod mcp;
mod paths;
mod profile;

use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use clap::Parser;
use clap_complete::generate;
use color_eyre::eyre::{eyre, Context, Result};
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, CellAlignment,
    ContentArrangement, Table,
};
use owo_colors::OwoColorize;
use serde_json::{json, Value};

use crate::{
    account::{
        inspect_profile_accounts, inspect_profile_claude_limits, inspect_profile_codex_limits,
        AccountStatus, ClaudeRateLimitSnapshot, ClaudeRateLimitStatus, CodexCreditsSummary,
        CodexRateLimitSnapshot, CodexRateLimitStatus,
    },
    cli::{Cli, Commands, McpCommands, McpTransport, PermissionCommands, ProfileCommands},
    profile::{ProfileSource, ResolvedProfile},
};

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    if let Commands::Completions { shell } = &cli.command {
        let mut cmd = crate::cli::command_for_completions();
        generate(*shell, &mut cmd, "cloak", &mut io::stdout());
        return Ok(());
    }

    let loaded = config::load_or_create_config()?;

    match cli.command {
        Commands::Exec { cli, profile, args } => {
            let selected_profile = match profile {
                Some(name) => {
                    paths::validate_profile_name(&name)?;
                    if !ensure_exec_profile_exists(&name, &loaded.config)? {
                        std::process::exit(1);
                    }
                    name
                }
                None => {
                    let cwd = current_dir()?;
                    profile::resolve_profile(&cwd, &loaded.config.general.default_profile)?.name
                }
            };

            maybe_provision_claude_statusline(&cli, &selected_profile, &loaded.config)?;
            exec::exec_cli(&cli, &selected_profile, &args, &loaded.config)?;
        }
        Commands::Use {
            profile: profile_name,
        } => {
            paths::validate_profile_name(&profile_name)?;

            let cwd = current_dir()?;
            let cloak_path = cwd.join(".cloak");

            if cloak_path.exists()
                && !confirm(&format!(
                    "{} already exists. Overwrite?",
                    cloak_path.display()
                ))?
            {
                println!("Aborted");
                return Ok(());
            }

            if !profile_exists(&profile_name)? {
                if confirm(&format!(
                    "Profile '{}' does not exist yet. Create it now?",
                    profile_name
                ))? {
                    create_profile(&profile_name, &loaded.config)?;
                } else {
                    return Err(eyre!(
                        "profile '{}' does not exist. Run: cloak profile create {}",
                        profile_name,
                        profile_name
                    ));
                }
            }

            let path = profile::write_cloak_file(&cwd, &profile_name)?;
            println!("{}", format_main_heading("Repository Profile"));
            print_detail_line("File", &display_path(&path));
            print_detail_line("Profile", &profile_name);
        }
        Commands::Profile(sub) => match sub {
            ProfileCommands::List => {
                let names = list_profiles()?;
                show_profile_list(&names, &loaded.config.general.default_profile);
            }
            ProfileCommands::Account { name } => {
                show_profile_accounts(&name, &loaded.config)?;
            }
            ProfileCommands::Create { name } => {
                create_profile(&name, &loaded.config)?;
            }
            ProfileCommands::Delete { name, yes } => {
                delete_profile(&name, yes, &loaded)?;
            }
            ProfileCommands::Show => {
                let cwd = current_dir()?;
                let resolved =
                    profile::resolve_profile(&cwd, &loaded.config.general.default_profile)?;
                show_profile(&resolved, &loaded.config)?;
            }
        },
        Commands::Login {
            cli,
            profile: explicit_profile,
        } => {
            let selected_profile = match explicit_profile {
                Some(name) => {
                    paths::validate_profile_name(&name)?;
                    name
                }
                None => {
                    let cwd = current_dir()?;
                    profile::resolve_profile(&cwd, &loaded.config.general.default_profile)?.name
                }
            };

            maybe_provision_claude_statusline(&cli, &selected_profile, &loaded.config)?;
            exec::exec_cli(&cli, &selected_profile, &[], &loaded.config)?;
        }
        Commands::Mcp(sub) => match sub {
            McpCommands::Install {
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
            } => {
                install_mcp(
                    &loaded.config,
                    InstallMcpParams {
                        cli_name: &cli,
                        server_name: &name,
                        profile: profile.as_deref(),
                        all_profiles,
                        transport,
                        url: url.as_deref(),
                        env: &env,
                        headers: &header,
                        bearer_token_env_var: bearer_token_env_var.as_deref(),
                        raw,
                        command: &command,
                    },
                )?;
            }
        },
        Commands::Limits { profile, utc } => match profile.as_deref() {
            Some("rank") => show_limits_rank(&loaded.config, utc.unwrap_or(0))?,
            Some(name) => show_profile_limits(name, &loaded.config, utc.unwrap_or(0))?,
            None => {
                let profiles = list_profiles()?;
                if profiles.is_empty() {
                    print_detail_line(
                        "Status",
                        "No profiles found. Run: cloak profile create <name>",
                    );
                } else {
                    for (i, p) in profiles.iter().enumerate() {
                        if i > 0 {
                            print_profile_separator();
                        }
                        show_profile_limits(p, &loaded.config, utc.unwrap_or(0))?;
                    }
                }
            }
        },
        Commands::Permission { command } => match command {
            PermissionCommands::Ask { agent } => {
                run_permission_questionnaire(agent, &loaded)?;
            }
        },
        Commands::Doctor => {
            let mut config_for_doctor = loaded.config.clone();
            let missing = config::missing_recommended_cli_names(&config_for_doctor);

            if !missing.is_empty() {
                println!("{}", format_section_title("Recommended CLI Blocks"));
                print_detail_line("Config", &display_path(&loaded.path));
                print_detail_line("Missing", &missing.join(", "));

                if is_interactive_terminal() {
                    if confirm(&format!(
                        "Append defaults for missing CLI entries ({})?",
                        missing.join(", ")
                    ))? {
                        let added = config::append_default_cli_blocks(&loaded.path, &missing)?;
                        if !added.is_empty() {
                            print_detail_line("Added", &added.join(", "));
                            config_for_doctor = config::load_config_from_path(&loaded.path)?;
                        }
                    } else {
                        print_detail_line("Status", "Skipped optional config migration.");
                    }
                } else {
                    print_detail_line(
                        "Status",
                        "Non-interactive terminal detected. Skipping optional migration prompt.",
                    );
                }
                println!();
            }

            doctor::run_doctor(&config_for_doctor, &loaded.path, loaded.created)?;
        }
        Commands::Completions { .. } => unreachable!("handled before config load"),
    }

    Ok(())
}

struct InstallMcpParams<'a> {
    cli_name: &'a str,
    server_name: &'a str,
    profile: Option<&'a str>,
    all_profiles: bool,
    transport: McpTransport,
    url: Option<&'a str>,
    env: &'a [String],
    headers: &'a [String],
    bearer_token_env_var: Option<&'a str>,
    raw: bool,
    command: &'a [String],
}

fn install_mcp(cfg: &config::Config, params: InstallMcpParams<'_>) -> Result<()> {
    if params.all_profiles && params.profile.is_some() {
        return Err(eyre!("use either --profile or --all-profiles, not both"));
    }

    let selected_profile = match params.profile {
        Some(name) => {
            paths::validate_profile_name(name)?;
            name.to_string()
        }
        None => {
            let cwd = current_dir()?;
            profile::resolve_profile(&cwd, &cfg.general.default_profile)?.name
        }
    };

    let install_all_profiles =
        params.all_profiles || should_install_for_all_profiles(params.profile, &selected_profile)?;

    let profiles = if install_all_profiles {
        let profiles = list_profiles()?;
        if profiles.is_empty() {
            return Err(eyre!("no profiles found. Run: cloak profile create <name>"));
        }
        profiles
    } else {
        vec![selected_profile]
    };

    if !params.raw {
        let request = mcp::McpInstallRequest {
            cli_name: params.cli_name,
            server_name: params.server_name,
            transport: params.transport,
            url: params.url,
            env: params.env,
            headers: params.headers,
            bearer_token_env_var: params.bearer_token_env_var,
            command: params.command,
        };
        let _ = mcp::build_install_args(&request)?;
    }

    println!("{}", format_main_heading("MCP Install"));
    print_detail_line("CLI", &format_cli_label(params.cli_name));
    print_detail_line("Server", params.server_name);
    if params.raw {
        print_detail_line("Mode", "raw");
    }
    print_detail_line(
        "Target",
        if install_all_profiles {
            "all profiles"
        } else {
            "selected profile"
        },
    );
    print_detail_line("Count", &profiles.len().to_string());

    let mut failures = Vec::new();
    for profile_name in &profiles {
        if !params.raw && params.cli_name == "claude" {
            maybe_provision_claude_statusline(params.cli_name, profile_name, cfg)?;
        }

        println!();
        println!(
            "{}",
            format_section_title(&format!("Profile '{}'", profile_name))
        );
        print_detail_line("Status", "installing");

        let result = if params.raw {
            mcp::raw_install_for_profile(
                params.cli_name,
                params.server_name,
                params.command,
                profile_name,
                cfg,
            )
        } else {
            let request = mcp::McpInstallRequest {
                cli_name: params.cli_name,
                server_name: params.server_name,
                transport: params.transport,
                url: params.url,
                env: params.env,
                headers: params.headers,
                bearer_token_env_var: params.bearer_token_env_var,
                command: params.command,
            };
            mcp::install_for_profile(&request, profile_name, cfg)
        };

        match result {
            Ok(()) => print_detail_line("Result", "installed"),
            Err(err) => {
                print_detail_line("Result", "failed");
                print_detail_line("Error", &err.to_string());
                failures.push(profile_name.clone());
            }
        }
    }

    if failures.is_empty() {
        return Ok(());
    }

    Err(eyre!(
        "MCP install failed for {} profile(s): {}",
        failures.len(),
        failures.join(", ")
    ))
}

fn should_install_for_all_profiles(
    explicit_profile: Option<&str>,
    selected_profile: &str,
) -> Result<bool> {
    if explicit_profile.is_some() || !is_interactive_terminal() {
        return Ok(false);
    }

    confirm(&format!(
        "Install this MCP for all profiles instead of only '{}'? ",
        selected_profile
    ))
}

fn ensure_exec_profile_exists(profile_name: &str, cfg: &config::Config) -> Result<bool> {
    if profile_exists(profile_name)? {
        return Ok(true);
    }

    let existing_profiles = list_profiles()?;

    println!("Profile '{}' does not exist.", profile_name);

    if existing_profiles.is_empty() {
        println!();
        println!("No profiles exist yet.");
    } else {
        println!();
        println!("Existing profiles:");
        for name in &existing_profiles {
            if name == &cfg.general.default_profile {
                println!("- {} (default)", name);
            } else {
                println!("- {}", name);
            }
        }
    }

    println!();
    if confirm("Create it now?")? {
        create_profile(profile_name, cfg)?;
        return Ok(true);
    }

    eprintln!();
    if existing_profiles.is_empty() {
        eprintln!("Profile '{}' was not created.", profile_name);
        eprintln!(
            "Run `cloak profile create {}` when you want to use it.",
            profile_name
        );
        return Ok(false);
    }

    eprintln!("Profile '{}' was not created.", profile_name);
    eprintln!(
        "Use one of the existing profiles: {}",
        existing_profiles.join(", ")
    );
    Ok(false)
}

fn show_profile_list(names: &[String], default_profile: &str) {
    println!("{}", format_main_heading("Profiles"));
    print_detail_line("Default", default_profile);
    print_detail_line("Count", &names.len().to_string());

    if names.is_empty() {
        print_detail_line(
            "Status",
            "No profiles found. Run: cloak profile create <name>",
        );
        return;
    }

    println!();
    let mut table = new_ui_table(vec!["Profile", "Default"]);
    for name in names {
        let default_marker = if name == default_profile { "yes" } else { "" };
        table.add_row(vec![Cell::new(name), Cell::new(default_marker)]);
    }
    println!("{table}");
}

fn create_profile(name: &str, cfg: &config::Config) -> Result<()> {
    paths::validate_profile_name(name)?;

    let profile_dir = paths::profile_dir(name)?;
    let existed = profile_dir.exists();

    paths::ensure_secure_dir(&profile_dir)?;

    let cli_names = config::profile_managed_cli_names(cfg);

    for cli_name in cli_names {
        let cli_dir = profile_dir.join(&cli_name);
        paths::ensure_secure_dir(&cli_dir)?;
    }

    let statusline_result = provision_default_claude_statusline(&profile_dir, cfg)?;
    println!("{}", format_main_heading(&format!("Profile '{}'", name)));

    if existed {
        print_detail_line("Status", "already exists");
    } else {
        print_detail_line("Status", "created");
        print_detail_line(
            "Next",
            &format!("Run `cloak login <cli> {name}` to authenticate."),
        );
    }
    print_detail_line("Location", &display_path(&profile_dir));

    if statusline_result.script_created {
        println!();
        println!("{}", format_section_title("Claude"));
        print_detail_line("Statusline", "script created");
        print_detail_line("Script", &display_path(&statusline_result.script_path));
    }

    if statusline_result.script_updated {
        if !statusline_result.script_created {
            println!();
            println!("{}", format_section_title("Claude"));
        }
        print_detail_line("Statusline", "script updated");
        print_detail_line("Script", &display_path(&statusline_result.script_path));
    }

    if statusline_result.settings_updated {
        if !statusline_result.script_created && !statusline_result.script_updated {
            println!();
            println!("{}", format_section_title("Claude"));
        }
        print_detail_line("Settings", &display_path(&statusline_result.settings_path));
    }

    Ok(())
}

fn run_permission_questionnaire(
    agent_arg: Option<String>,
    loaded: &config::LoadedConfig,
) -> Result<()> {
    let agent = resolve_permission_agent(agent_arg, &loaded.config)?;
    let current = config::permissions_for_agent(&loaded.config, &agent);

    println!(
        "{}",
        format_main_heading(&format!("Permissoes para '{agent}'"))
    );
    print_detail_line("Politica atual", "questionario interativo guiado");
    print_detail_line(
        "Observacao",
        "Comandos perigosos continuam bloqueados por padrao e exigem allowlist manual.",
    );
    println!();

    let updated = ask_permissions(&agent, &current)?;
    config::save_agent_permissions(&loaded.path, &agent, &updated)?;
    print_agent_permissions(&agent, &updated, &loaded.path)?;
    Ok(())
}

fn resolve_permission_agent(
    explicit_agent: Option<String>,
    cfg: &config::Config,
) -> Result<String> {
    if let Some(agent) = explicit_agent {
        paths::validate_cli_name(&agent)?;
        return Ok(agent);
    }

    let candidates = available_agents(cfg);
    if candidates.is_empty() {
        return Err(eyre!(
            "nenhum agente conhecido foi configurado ainda. use --agent <nome> para configurar um"
        ));
    }

    if candidates.len() == 1 {
        return Ok(candidates[0].clone());
    }

    if !is_interactive_terminal() {
        return Err(eyre!("--agent e obrigatorio em modo nao interativo"));
    }

    println!("Escolha um agente:");
    for (index, name) in candidates.iter().enumerate() {
        println!("  [{}] {}", index + 1, name);
    }

    print!("Digite o numero (1-{}): ", candidates.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input
        .trim()
        .parse::<usize>()
        .map_err(|_| eyre!("selecao invalida"))?;

    if choice == 0 || choice > candidates.len() {
        return Err(eyre!("selecao fora do intervalo"));
    }

    Ok(candidates[choice - 1].clone())
}

fn available_agents(cfg: &config::Config) -> Vec<String> {
    let mut agents: Vec<String> = cfg.agents.keys().cloned().collect();
    if !agents.iter().any(|agent| agent == "codex") {
        agents.push("codex".to_string());
    }

    agents.sort();
    agents.dedup();
    agents
}

fn ask_permissions(
    agent: &str,
    current: &config::AgentPermissions,
) -> Result<config::AgentPermissions> {
    println!("{}", format_section_title(&format!("Agente: {}", agent)));
    println!("Responda ao questionario abaixo.");
    println!("Use 's' para sim, 'n' para nao e Enter para manter o valor atual.");
    println!();

    let allow_shell = prompt_bool(
        1,
        6,
        "Permitir acesso ao shell",
        "Controla a execucao de comandos de shell como bash, sh, zsh e similares.",
        current.allow_shell,
    )?;
    let allow_file_write = prompt_bool(
        2,
        6,
        "Permitir operacoes de escrita em arquivos",
        "Controla comandos que criam, alteram ou removem arquivos e diretorios, como cp, mkdir e install. Comandos perigosos como rm, mv, chmod e dd continuam bloqueados por padrao e exigem liberacao manual na allowlist.",
        current.allow_file_write,
    )?;
    let allow_network = prompt_bool(
        3,
        6,
        "Permitir acesso a rede",
        "Controla comandos que podem acessar recursos externos, como curl, wget, git, npm, pnpm, node e python.",
        current.allow_network,
    )?;
    let include_dangerous_in_deny = prompt_bool(
        4,
        6,
        "Adicionar comandos perigosos em deny_commands",
        "Se ativado, rm, rmdir, mv, dd, truncate, chmod, chown e chgrp serao adicionados explicitamente em deny_commands no TOML salvo.",
        deny_commands_contain_all_dangerous(current),
    )?;

    let allowed_commands = prompt_command_list(
        5,
        6,
        "Comandos de topo explicitamente permitidos",
        "Informe uma lista separada por virgula. Comandos perigosos so executam se forem adicionados aqui manualmente e nao estiverem bloqueados em deny_commands.",
        &current.allowed_commands,
    )?;
    let deny_current = if include_dangerous_in_deny {
        merge_command_lists(current.deny_commands.clone(), dangerous_command_names())
    } else {
        current.deny_commands.clone()
    };
    let deny_commands = prompt_command_list(
        6,
        6,
        "Comandos de topo explicitamente bloqueados",
        "Informe uma lista separada por virgula para negar comandos especificos, mesmo quando outras permissoes estiverem liberadas.",
        &deny_current,
    )?;

    let allowed_commands = normalize_command_list(allowed_commands)?;
    let mut deny_commands = normalize_command_list(deny_commands)?;
    if include_dangerous_in_deny {
        deny_commands = merge_command_lists(deny_commands, dangerous_command_names());
    }

    let overlapping = overlap(&allowed_commands, &deny_commands);
    if !overlapping.is_empty() {
        return Err(eyre!(
            "ha comandos repetidos entre permitidos e bloqueados: {}",
            overlapping.join(", ")
        ));
    }

    Ok(config::AgentPermissions {
        allow_shell,
        allow_file_write,
        allow_network,
        allowed_commands,
        deny_commands,
    })
}

fn prompt_bool(
    step: usize,
    total: usize,
    question: &str,
    description: &str,
    default: bool,
) -> Result<bool> {
    let default_suffix = if default { "[S/n]" } else { "[s/N]" };
    println!("[{step}/{total}] {question}");
    println!("  {description}");
    println!("  Atual: {}", bool_status_label(default));
    print!("  Resposta {default_suffix}: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_ascii_lowercase();

    match answer.as_str() {
        "" => Ok(default),
        "s" | "sim" | "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => Err(eyre!("resposta invalida: use s ou n")),
    }
}

fn prompt_command_list(
    step: usize,
    total: usize,
    question: &str,
    description: &str,
    current: &[String],
) -> Result<Vec<String>> {
    let current_label = if current.is_empty() {
        "<nenhum>".to_string()
    } else {
        current.join(", ")
    };
    println!("[{step}/{total}] {question}");
    println!("  {description}");
    println!("  Atual: {current_label}");
    println!("  Enter vazio mantem o valor atual. '-' limpa a lista.");
    print!("  Lista: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();

    if trimmed == "-" {
        return Ok(Vec::new());
    }

    if trimmed.is_empty() {
        return Ok(current.to_vec());
    }

    let parsed: Vec<String> = trimmed
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();

    Ok(parsed)
}

fn normalize_command_list(values: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = Vec::new();

    for value in values {
        if value.starts_with('-') {
            return Err(eyre!("o comando nao pode comecar com '-': {value}"));
        }
        if !normalized.iter().any(|existing| existing == &value) {
            normalized.push(value);
        }
    }

    Ok(normalized)
}

fn overlap(a: &[String], b: &[String]) -> Vec<String> {
    a.iter()
        .filter(|value| b.iter().any(|other| other == *value))
        .cloned()
        .collect()
}

fn dangerous_command_names() -> Vec<String> {
    vec![
        "rm".to_string(),
        "rmdir".to_string(),
        "mv".to_string(),
        "dd".to_string(),
        "truncate".to_string(),
        "chmod".to_string(),
        "chown".to_string(),
        "chgrp".to_string(),
    ]
}

fn merge_command_lists(mut base: Vec<String>, extras: Vec<String>) -> Vec<String> {
    for extra in extras {
        if !base.iter().any(|existing| existing == &extra) {
            base.push(extra);
        }
    }

    base
}

fn deny_commands_contain_all_dangerous(current: &config::AgentPermissions) -> bool {
    dangerous_command_names().iter().all(|command| {
        current
            .deny_commands
            .iter()
            .any(|existing| existing == command)
    })
}

fn print_agent_permissions(
    agent: &str,
    permissions: &config::AgentPermissions,
    config_path: &Path,
) -> Result<()> {
    println!();
    println!("{}", format_section_title("Permissoes salvas"));
    print_detail_line("Agente", agent);
    print_detail_line("Shell", bool_status_label(permissions.allow_shell));
    print_detail_line(
        "Escrita em arquivos",
        bool_status_label(permissions.allow_file_write),
    );
    print_detail_line("Rede", bool_status_label(permissions.allow_network));

    if permissions.allowed_commands.is_empty() {
        print_detail_line("Comandos permitidos", "<todos os nao perigosos>");
    } else {
        print_detail_line(
            "Comandos permitidos",
            &permissions.allowed_commands.join(", "),
        );
    }

    if permissions.deny_commands.is_empty() {
        print_detail_line("Comandos bloqueados", "<nenhum>");
    } else {
        print_detail_line("Comandos bloqueados", &permissions.deny_commands.join(", "));
    }

    print_detail_line("Comandos perigosos", dangerous_commands_summary());
    println!("Salvo na configuracao.");
    println!();
    println!("{}", format_section_title("config.toml salvo"));
    print_detail_line("Arquivo", &display_path(config_path));
    println!();

    let raw = fs::read_to_string(config_path)
        .wrap_err_with(|| format!("falha ao ler {}", config_path.display()))?;
    print!("{raw}");
    if !raw.ends_with('\n') {
        println!();
    }

    Ok(())
}

fn bool_status_label(value: bool) -> &'static str {
    if value {
        "permitido"
    } else {
        "bloqueado"
    }
}

fn dangerous_commands_summary() -> &'static str {
    "rm, rmdir, mv, dd, truncate, chmod, chown e chgrp exigem allowlist manual"
}

fn delete_profile(name: &str, yes: bool, loaded: &config::LoadedConfig) -> Result<()> {
    paths::validate_profile_name(name)?;

    let profile_dir = paths::profile_dir(name)?;
    if !profile_dir.exists() {
        return Err(eyre!("profile '{}' does not exist", name));
    }

    if !yes
        && !confirm(&format!(
            "Delete profile '{}' at {}?",
            name,
            profile_dir.display()
        ))?
    {
        println!("Aborted");
        return Ok(());
    }

    let is_default = loaded.config.general.default_profile == name;
    println!("{}", format_main_heading(&format!("Profile '{}'", name)));

    if is_default {
        let all = list_profiles()?;
        let remaining: Vec<String> = all.into_iter().filter(|n| n != name).collect();

        if !remaining.is_empty() {
            let new_default = if yes || !is_interactive_terminal() {
                remaining[0].clone()
            } else {
                pick_new_default_profile(&remaining)?
            };

            config::update_default_profile(&loaded.path, &new_default)?;
            print_detail_line("Default", &format!("updated: {name} -> {new_default}"));
        } else {
            print_detail_line(
                "Warning",
                &format!("'{name}' is your default profile and no other profiles exist."),
            );
            print_detail_line(
                "Action",
                &format!(
                    "After deletion, create a new profile and update default_profile in {}",
                    display_path(&loaded.path)
                ),
            );
        }
    }

    fs::remove_dir_all(&profile_dir)
        .wrap_err_with(|| format!("failed deleting {}", profile_dir.display()))?;

    print_detail_line("Status", "deleted");
    print_detail_line(
        "Note",
        &format!(".cloak files in project directories may still reference '{name}'."),
    );
    print_detail_line(
        "Action",
        "Run `cloak use <profile>` in those directories to update them.",
    );
    Ok(())
}

fn pick_new_default_profile(remaining: &[String]) -> Result<String> {
    println!("Choose a new default profile:");
    for (i, name) in remaining.iter().enumerate() {
        println!("  [{}] {}", i + 1, name);
    }
    print!("Enter number (1-{}): ", remaining.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice: usize = input
        .trim()
        .parse()
        .map_err(|_| eyre!("invalid selection"))?;

    if choice < 1 || choice > remaining.len() {
        return Err(eyre!("selection out of range"));
    }

    Ok(remaining[choice - 1].clone())
}

fn show_profile(resolved: &ResolvedProfile, cfg: &config::Config) -> Result<()> {
    println!(
        "{}",
        format_main_heading(&format!("Profile '{}'", resolved.name))
    );
    match &resolved.source {
        ProfileSource::CloakFile(path) => {
            print_detail_line("Source", &format!("from {}", display_path(path)));
        }
        ProfileSource::DefaultProfile => {
            print_detail_line("Source", "fallback to default");
        }
    }

    let cli_names = config::profile_managed_cli_names(cfg);

    println!();
    println!("{}", format_section_title("CLI Configuration"));

    if cli_names.is_empty() {
        print_detail_line("Status", "no profile-managed CLI is currently enabled");
        return Ok(());
    }

    let mut table = new_ui_table(vec!["CLI", "Setting", "Value"]);

    for cli_name in cli_names {
        let cli_cfg = &cfg.cli[&cli_name];
        let cli_dir = paths::profile_cli_dir(&resolved.name, &cli_name)?;
        table.add_row(vec![
            Cell::new(format_cli_label(&cli_name)),
            Cell::new("profile_dir"),
            Cell::new(display_path(&cli_dir)),
        ]);

        if let Some(config_dir_env) = &cli_cfg.config_dir_env {
            table.add_row(vec![
                Cell::new(format_cli_label(&cli_name)),
                Cell::new(config_dir_env),
                Cell::new(display_path(&cli_dir)),
            ]);
        }

        let mut extra_env: Vec<_> = cli_cfg.extra_env.iter().collect();
        extra_env.sort_by(|a, b| a.0.cmp(b.0));
        for (name, value) in extra_env {
            table.add_row(vec![
                Cell::new(format_cli_label(&cli_name)),
                Cell::new(name),
                Cell::new(exec::render_template(
                    value,
                    &exec::TemplateContext {
                        cli_name: &cli_name,
                        profile: &resolved.name,
                        profile_dir: &cli_dir,
                    },
                )),
            ]);
        }

        let resolved_binary = which::which(&cli_cfg.binary)
            .unwrap_or_else(|_| std::path::PathBuf::from(&cli_cfg.binary));
        if let Some(agent_folder) =
            exec::resolve_remote_agent_folder(&cli_name, &resolved_binary, &cli_dir)
        {
            table.add_row(vec![
                Cell::new(format_cli_label(&cli_name)),
                Cell::new("VSCODE_AGENT_FOLDER"),
                Cell::new(display_path(&agent_folder)),
            ]);
        }

        if !cli_cfg.launch_args.is_empty() {
            let rendered_args = cli_cfg
                .launch_args
                .iter()
                .map(|arg| {
                    exec::render_template(
                        arg,
                        &exec::TemplateContext {
                            cli_name: &cli_name,
                            profile: &resolved.name,
                            profile_dir: &cli_dir,
                        },
                    )
                })
                .collect::<Vec<_>>()
                .join(" ");
            table.add_row(vec![
                Cell::new(format_cli_label(&cli_name)),
                Cell::new("launch_args"),
                Cell::new(rendered_args),
            ]);
        }
    }

    println!("{table}");

    Ok(())
}

fn show_profile_accounts(profile: &str, cfg: &config::Config) -> Result<()> {
    paths::validate_profile_name(profile)?;

    if !profile_exists(profile)? {
        return Err(eyre!("profile '{}' does not exist", profile));
    }

    println!("{}", format_main_heading(&format!("Profile '{}'", profile)));
    println!();
    println!("{}", format_section_title("Accounts"));
    let mut table = new_ui_table(vec!["CLI", "Account"]);

    for account in inspect_profile_accounts(profile, cfg)? {
        match account.status {
            AccountStatus::Identified { display } => {
                table.add_row(vec![
                    Cell::new(format_cli_label(&account.cli_name)),
                    Cell::new(display),
                ]);
            }
            AccountStatus::CredentialsPresent { detail } => {
                table.add_row(vec![
                    Cell::new(format_cli_label(&account.cli_name)),
                    Cell::new(detail),
                ]);
            }
            AccountStatus::NoCredentials => {
                table.add_row(vec![
                    Cell::new(format_cli_label(&account.cli_name)),
                    Cell::new("not authenticated"),
                ]);
            }
        }
    }

    println!("{table}");

    Ok(())
}

fn show_profile_limits(profile: &str, cfg: &config::Config, utc_offset: i32) -> Result<()> {
    paths::validate_profile_name(profile)?;

    if !profile_exists(profile)? {
        return Err(eyre!("profile '{}' does not exist", profile));
    }

    if !(-12..=14).contains(&utc_offset) {
        return Err(eyre!(
            "invalid UTC offset '{}': must be between -12 and +14",
            utc_offset
        ));
    }

    println!("{}", format_main_heading(&format!("Profile '{}'", profile)));

    let mut rendered_any = false;
    let mut rendered_sections = 0usize;

    if cfg.cli.contains_key("claude") {
        let provision_result =
            provision_default_claude_statusline(&paths::profile_dir(profile)?, cfg)?;

        print_limit_section_header("claude", &mut rendered_sections);
        match inspect_profile_claude_limits(profile, cfg)? {
            ClaudeRateLimitStatus::Available(snapshot) => {
                print_claude_limits_snapshot(&snapshot, utc_offset);
            }
            ClaudeRateLimitStatus::NoUsageData => {
                print_detail_line(
                    "Status",
                    "authenticated, but no local usage snapshot was found yet",
                );
                print_detail_line("Next", &missing_usage_snapshot_hint("claude"));
                if provision_result.script_created
                    || provision_result.script_updated
                    || provision_result.settings_updated
                {
                    print_detail_line("Note", "snapshot support was refreshed for this profile");
                }
            }
            ClaudeRateLimitStatus::NotAuthenticated => {
                print_detail_line("Status", "not authenticated");
            }
            ClaudeRateLimitStatus::NotConfigured => {}
        }
        rendered_any = true;
    }

    if cfg.cli.contains_key("codex") {
        print_limit_section_header("codex", &mut rendered_sections);
        match inspect_profile_codex_limits(profile, cfg)? {
            CodexRateLimitStatus::Available(snapshot) => {
                print_codex_limits_snapshot(&snapshot, utc_offset);
            }
            CodexRateLimitStatus::NoUsageData => {
                print_detail_line(
                    "Status",
                    "authenticated, but no local usage snapshot was found yet",
                );
                print_detail_line("Next", &missing_usage_snapshot_hint("codex"));
            }
            CodexRateLimitStatus::NotAuthenticated => {
                print_detail_line("Status", "not authenticated");
            }
            CodexRateLimitStatus::NotConfigured => {}
        }
        rendered_any = true;
    }

    if !rendered_any {
        println!("no supported CLI with usage limits is configured in config.toml");
    }

    Ok(())
}

fn show_limits_rank(cfg: &config::Config, utc_offset: i32) -> Result<()> {
    let profiles = list_profiles()?;

    println!("{}", format_main_heading("Weekly Limits Rank"));

    if profiles.is_empty() {
        print_detail_line(
            "Status",
            "No profiles found. Run: cloak profile create <name>",
        );
        return Ok(());
    }

    let now = unix_now();

    let mut rendered_any = false;

    if cfg.cli.contains_key("claude") {
        let mut claude_ranks: Vec<(String, Option<String>, bool, f64, u64, i64)> = Vec::new();

        for profile in &profiles {
            if let Ok(ClaudeRateLimitStatus::Available(snapshot)) =
                inspect_profile_claude_limits(profile, cfg)
            {
                if let Some(window) = snapshot.windows.iter().find(|w| w.window_minutes == 10080) {
                    let expired = usage_window_expired(window.resets_at, now);
                    let limit_info = format_limit_subject_detail(
                        snapshot.plan_type.as_deref(),
                        snapshot.rate_limit_tier.as_deref(),
                    );
                    let effective_used =
                        effective_used_percent(window.used_percent, window.resets_at, now);
                    claude_ranks.push((
                        profile.clone(),
                        limit_info,
                        expired,
                        effective_used,
                        window.window_minutes,
                        window.resets_at,
                    ));
                }
            }
        }

        if !claude_ranks.is_empty() {
            println!();
            println!("{}", format_section_title("Claude"));
            claude_ranks.sort_by(|a, b| {
                a.2.cmp(&b.2).then_with(|| {
                    let avail_a = 100.0 - a.3;
                    let avail_b = 100.0 - b.3;
                    avail_b
                        .partial_cmp(&avail_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });

            let mut table = new_ui_table(vec![
                "Profile",
                "Limit",
                "Snapshot",
                "Used",
                "Available",
                "Pacing",
                "Resets",
            ]);
            let mut any_expired = false;
            for (profile, limit_info, expired, used_percent, window_minutes, resets_at) in
                claude_ranks
            {
                if expired {
                    any_expired = true;
                }
                let available = (100.0 - used_percent).clamp(0.0, 100.0);
                let pacing = format_rank_pacing(available, window_minutes, resets_at, now);
                let resets_display = if expired {
                    "expired *".to_string()
                } else {
                    format_unix_timestamp_utc(resets_at, utc_offset)
                };
                table.add_row(vec![
                    Cell::new(&profile),
                    Cell::new(limit_info.as_deref().unwrap_or("-")),
                    Cell::new(if expired { "expired" } else { "fresh" }),
                    Cell::new(format_percent(used_percent)).set_alignment(CellAlignment::Right),
                    Cell::new(format_percent(available)).set_alignment(CellAlignment::Right),
                    Cell::new(pacing).set_alignment(CellAlignment::Right),
                    Cell::new(resets_display),
                ]);
            }
            println!("{table}");
            if any_expired {
                println!("  * {}", expired_usage_rank_hint("claude"));
            }
            rendered_any = true;
        }
    }

    if cfg.cli.contains_key("codex") {
        let mut codex_ranks: Vec<(String, Option<String>, bool, f64, u64, i64)> = Vec::new();

        for profile in &profiles {
            if let Ok(CodexRateLimitStatus::Available(snapshot)) =
                inspect_profile_codex_limits(profile, cfg)
            {
                if let Some(window) = snapshot.windows.iter().find(|w| w.window_minutes == 10080) {
                    let expired = usage_window_expired(window.resets_at, now);
                    let limit_info = snapshot
                        .limit_name
                        .clone()
                        .or_else(|| snapshot.limit_id.clone())
                        .or_else(|| snapshot.plan_type.as_ref().map(|p| format!("plan: {p}")));
                    let effective_used =
                        effective_used_percent(window.used_percent, window.resets_at, now);
                    codex_ranks.push((
                        profile.clone(),
                        limit_info,
                        expired,
                        effective_used,
                        window.window_minutes,
                        window.resets_at,
                    ));
                }
            }
        }

        if !codex_ranks.is_empty() {
            println!();
            println!("{}", format_section_title("Codex"));
            codex_ranks.sort_by(|a, b| {
                a.2.cmp(&b.2).then_with(|| {
                    let avail_a = 100.0 - a.3;
                    let avail_b = 100.0 - b.3;
                    avail_b
                        .partial_cmp(&avail_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });

            let mut table = new_ui_table(vec![
                "Profile",
                "Limit",
                "Snapshot",
                "Used",
                "Available",
                "Pacing",
                "Resets",
            ]);
            let mut any_expired = false;
            for (profile, limit_info, expired, used_percent, window_minutes, resets_at) in
                codex_ranks
            {
                if expired {
                    any_expired = true;
                }
                let available = (100.0 - used_percent).clamp(0.0, 100.0);
                let pacing = format_rank_pacing(available, window_minutes, resets_at, now);
                let resets_display = if expired {
                    "expired *".to_string()
                } else {
                    format_unix_timestamp_utc(resets_at, utc_offset)
                };
                table.add_row(vec![
                    Cell::new(&profile),
                    Cell::new(limit_info.as_deref().unwrap_or("-")),
                    Cell::new(if expired { "expired" } else { "fresh" }),
                    Cell::new(format_percent(used_percent)).set_alignment(CellAlignment::Right),
                    Cell::new(format_percent(available)).set_alignment(CellAlignment::Right),
                    Cell::new(pacing).set_alignment(CellAlignment::Right),
                    Cell::new(resets_display),
                ]);
            }
            println!("{table}");
            if any_expired {
                println!("  * {}", expired_usage_rank_hint("codex"));
            }
            rendered_any = true;
        }
    }

    if !rendered_any {
        println!();
        println!("No weekly usage data was found for any supported CLI across profiles.");
    }

    Ok(())
}

fn effective_used_percent(used_percent: f64, resets_at: i64, now: i64) -> f64 {
    if resets_at > now {
        used_percent
    } else {
        0.0
    }
}

fn format_rank_pacing(available: f64, window_minutes: u64, resets_at: i64, now: i64) -> String {
    let (actual_remaining, actual_seconds) = if resets_at > now {
        (available, (resets_at - now) as f64)
    } else {
        (100.0, (window_minutes * 60) as f64)
    };
    let days = (actual_seconds / 86400.0).max(0.01);
    format!("{:.1}%/d", actual_remaining / days)
}

fn maybe_provision_claude_statusline(
    cli_name: &str,
    profile: &str,
    cfg: &config::Config,
) -> Result<()> {
    if cli_name != "claude" || !cfg.cli.contains_key("claude") {
        return Ok(());
    }

    let profile_dir = paths::profile_dir(profile)?;
    let _ = provision_default_claude_statusline(&profile_dir, cfg)?;
    Ok(())
}

fn print_claude_limits_snapshot(snapshot: &ClaudeRateLimitSnapshot, utc_offset: i32) {
    let detail = format_limit_subject_detail(
        snapshot.plan_type.as_deref(),
        snapshot.rate_limit_tier.as_deref(),
    );

    match detail {
        Some(detail) => {
            print_detail_line("Status", "usage snapshot available");
            print_detail_line("Details", &detail);
        }
        None => print_detail_line("Status", "usage snapshot available"),
    }

    print_detail_line("Observed", &snapshot.observed_at);
    if !snapshot.windows.is_empty() {
        println!(
            "{}",
            build_usage_windows_table(
                snapshot.windows.iter().map(|window| {
                    (
                        window.label,
                        window.window_minutes,
                        window.used_percent,
                        window.resets_at,
                    )
                }),
                utc_offset,
            )
        );
    }

    if snapshot
        .windows
        .iter()
        .any(|window| usage_window_expired(window.resets_at, unix_now()))
    {
        print_detail_line("Next", &expired_usage_snapshot_hint("claude"));
    }
}

fn print_limit_section_header(subject: &str, rendered_sections: &mut usize) {
    if *rendered_sections > 0 {
        println!();
    }

    let title = format_section_title(&format_cli_label(subject));
    println!("{title}");
    *rendered_sections += 1;
}

fn print_detail_line(label: &str, value: &str) {
    let label = if io::stdout().is_terminal() {
        format!("  {}", label.bold().bright_black())
    } else {
        format!("  {label}")
    };
    println!("{label}: {value}");
}

fn format_limit_subject_detail(
    plan_type: Option<&str>,
    rate_limit_tier: Option<&str>,
) -> Option<String> {
    match (plan_type, rate_limit_tier) {
        (Some(plan_type), Some(rate_limit_tier)) => {
            Some(format!("plan: {plan_type}, tier: {rate_limit_tier}"))
        }
        (Some(plan_type), None) => Some(format!("plan: {plan_type}")),
        (None, Some(rate_limit_tier)) => Some(format!("tier: {rate_limit_tier}")),
        (None, None) => None,
    }
}

fn print_codex_limits_snapshot(snapshot: &CodexRateLimitSnapshot, utc_offset: i32) {
    match snapshot.plan_type.as_deref() {
        Some(plan_type) => {
            print_detail_line("Status", "usage snapshot available");
            print_detail_line("Details", &format!("plan: {plan_type}"));
        }
        None => print_detail_line("Status", "usage snapshot available"),
    }

    print_detail_line("Observed", &snapshot.observed_at);

    if let Some(limit_name) = snapshot.limit_name.as_deref() {
        print_detail_line("Limit", limit_name);
    } else if let Some(limit_id) = snapshot.limit_id.as_deref() {
        print_detail_line("Limit", limit_id);
    }

    if !snapshot.windows.is_empty() {
        println!(
            "{}",
            build_usage_windows_table(
                snapshot.windows.iter().map(|window| {
                    (
                        window.label,
                        window.window_minutes,
                        window.used_percent,
                        window.resets_at,
                    )
                }),
                utc_offset,
            )
        );
    }

    if snapshot
        .windows
        .iter()
        .any(|window| usage_window_expired(window.resets_at, unix_now()))
    {
        print_detail_line("Next", &expired_usage_snapshot_hint("codex"));
    }

    if let Some(credits) = snapshot.credits.as_ref() {
        print_detail_line("Credits", &format_codex_credits(credits, utc_offset));
    }
}

fn print_profile_separator() {
    if io::stdout().is_terminal() {
        println!("\n{}", "──────────────────────────────────".dimmed());
    } else {
        println!("\n──────────────────────────────────");
    }
}

fn format_main_heading(title: &str) -> String {
    if io::stdout().is_terminal() {
        title.bold().underline().to_string()
    } else {
        title.to_string()
    }
}

fn format_section_title(title: &str) -> String {
    if io::stdout().is_terminal() {
        title.bold().cyan().to_string()
    } else {
        title.to_string()
    }
}

fn format_cli_label(cli_name: &str) -> String {
    match cli_name {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "gemini" => "Gemini".to_string(),
        other => capitalize_label(other),
    }
}

fn capitalize_label(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn new_ui_table(header: Vec<&str>) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(header);
    table
}

fn build_usage_windows_table<'a, I>(windows: I, utc_offset: i32) -> String
where
    I: IntoIterator<Item = (&'a str, u64, f64, i64)>,
{
    let now = unix_now();

    let mut table = new_ui_table(vec![
        "Limit",
        "Window",
        "Used",
        "Remaining",
        "Pacing",
        "Resets",
    ]);

    for (label, window_minutes, used_percent, resets_at) in windows {
        let expired = usage_window_expired(resets_at, now);

        let effective_used = if expired { 0.0 } else { used_percent };
        let effective_remaining = (100.0 - effective_used).clamp(0.0, 100.0);

        let actual_seconds = if expired {
            (window_minutes * 60) as f64
        } else {
            (resets_at - now) as f64
        };

        let pacing_str = if window_minutes >= 60 * 24 {
            let days = (actual_seconds / 86400.0).max(0.01);
            format!("{:.1}%/d", effective_remaining / days)
        } else {
            let hours = (actual_seconds / 3600.0).max(0.01);
            format!("{:.1}%/h", effective_remaining / hours)
        };

        let resets_display = if expired {
            "expired *".to_string()
        } else {
            format_unix_timestamp_utc(resets_at, utc_offset)
        };

        table.add_row(vec![
            Cell::new(label),
            Cell::new(format_window_minutes(window_minutes)),
            Cell::new(format_percent(effective_used)).set_alignment(CellAlignment::Right),
            Cell::new(format_percent(effective_remaining)).set_alignment(CellAlignment::Right),
            Cell::new(pacing_str).set_alignment(CellAlignment::Right),
            Cell::new(resets_display),
        ]);
    }

    table.to_string()
}

fn usage_window_expired(resets_at: i64, now: i64) -> bool {
    resets_at <= now
}

fn missing_usage_snapshot_hint(cli_name: &str) -> String {
    match cli_name {
        "claude" => "open or continue a Claude session in this profile; the statusline writes usage-limits.json after Claude receives a response".to_string(),
        "codex" => "open or continue a Codex session in this profile; Codex writes fresh rate-limit data to codex/sessions after token_count events".to_string(),
        other => format!(
            "open or continue a {} session in this profile to populate local usage data",
            format_cli_label(other)
        ),
    }
}

fn expired_usage_snapshot_hint(cli_name: &str) -> String {
    match cli_name {
        "claude" => "some limits have expired since the last snapshot; open or continue a Claude session in this profile and wait for a response to capture a fresh snapshot".to_string(),
        "codex" => "some limits have expired since the last snapshot; open or continue a Codex session in this profile to record a fresh token_count snapshot".to_string(),
        other => format!(
            "some limits have expired since the last snapshot; open or continue a {} session in this profile to refresh local usage data",
            format_cli_label(other)
        ),
    }
}

fn expired_usage_rank_hint(cli_name: &str) -> String {
    match cli_name {
        "claude" => "some Claude rows are based on expired snapshots; open or continue Claude in the affected profile and wait for a response to write a fresh snapshot".to_string(),
        "codex" => "some Codex rows are based on expired snapshots; open or continue Codex in the affected profile to record a fresh token_count snapshot".to_string(),
        other => format!(
            "some {other} rows are based on expired snapshots; open or continue the CLI in the affected profile to refresh local usage data"
        ),
    }
}

fn format_codex_credits(credits: &CodexCreditsSummary, utc_offset: i32) -> String {
    let mut parts = Vec::new();

    if let Some(value) = credits.used.as_deref() {
        parts.push(format!("used {value}"));
    }
    if let Some(value) = credits.remaining.as_deref() {
        parts.push(format!("remaining {value}"));
    }
    if let Some(value) = credits.total.as_deref() {
        parts.push(format!("total {value}"));
    }
    if let Some(value) = credits.resets_at {
        parts.push(format!(
            "resets {}",
            format_unix_timestamp_utc(value, utc_offset)
        ));
    }

    if parts.is_empty() && credits.opaque {
        return "available (details unavailable)".to_string();
    }

    parts.join(", ")
}

fn format_window_minutes(minutes: u64) -> String {
    if minutes.is_multiple_of(60 * 24 * 7) {
        return format!("{}w", minutes / (60 * 24 * 7));
    }

    if minutes.is_multiple_of(60 * 24) {
        return format!("{}d", minutes / (60 * 24));
    }

    if minutes.is_multiple_of(60) {
        return format!("{}h", minutes / 60);
    }

    format!("{minutes}m")
}

fn format_percent(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        return format!("{value:.0}%");
    }

    format!("{value:.1}%")
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn format_unix_timestamp_utc(timestamp: i64, utc_offset: i32) -> String {
    let adjusted = timestamp + (utc_offset as i64) * 3_600;
    let days = adjusted.div_euclid(86_400);
    let seconds = adjusted.rem_euclid(86_400);

    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    let second = seconds % 60;
    let (year, month, day) = civil_from_days(days);

    let tz_label = match utc_offset.cmp(&0) {
        std::cmp::Ordering::Equal => "UTC".to_string(),
        std::cmp::Ordering::Greater => format!("UTC+{utc_offset}"),
        std::cmp::Ordering::Less => format!("UTC{utc_offset}"),
    };

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} {tz_label}")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    (year, month, day)
}

fn list_profiles() -> Result<Vec<String>> {
    let root = paths::profiles_dir()?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut profiles = Vec::new();
    for entry in
        fs::read_dir(&root).wrap_err_with(|| format!("failed reading {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                profiles.push(name.to_string());
            }
        }
    }

    profiles.sort();
    Ok(profiles)
}

fn profile_exists(name: &str) -> Result<bool> {
    Ok(paths::profile_dir(name)?.is_dir())
}

fn current_dir() -> Result<PathBuf> {
    let physical = std::env::current_dir().wrap_err("failed to read current directory")?;

    if let Some(pwd_os) = std::env::var_os("PWD") {
        let pwd = PathBuf::from(pwd_os);
        if pwd.is_absolute() {
            let pwd_real = fs::canonicalize(&pwd).ok();
            let physical_real = fs::canonicalize(&physical).ok();

            if pwd_real.is_some() && pwd_real == physical_real {
                return Ok(pwd);
            }
        }
    }

    Ok(physical)
}

fn confirm(question: &str) -> Result<bool> {
    print!("{} [y/N]: ", question);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let decision = input.trim().to_ascii_lowercase();
    Ok(matches!(decision.as_str(), "y" | "yes"))
}

fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(home) {
            return format!("~/{}", stripped.display());
        }
    }

    path.display().to_string()
}

struct StatuslineProvisionResult {
    script_created: bool,
    script_updated: bool,
    settings_updated: bool,
    script_path: PathBuf,
    settings_path: PathBuf,
}

fn provision_default_claude_statusline(
    profile_dir: &Path,
    cfg: &config::Config,
) -> Result<StatuslineProvisionResult> {
    let claude_cfg_present = cfg.cli.contains_key("claude");
    let claude_dir = profile_dir.join("claude");
    let script_path = claude_dir.join("statusline-command.sh");
    let settings_path = claude_dir.join("settings.json");

    #[cfg(not(unix))]
    {
        return Ok(StatuslineProvisionResult {
            script_created: false,
            script_updated: false,
            settings_updated: false,
            script_path,
            settings_path,
        });
    }

    if !claude_cfg_present || !claude_dir.is_dir() {
        return Ok(StatuslineProvisionResult {
            script_created: false,
            script_updated: false,
            settings_updated: false,
            script_path,
            settings_path,
        });
    }

    let mut script_created = false;
    let mut script_updated = false;
    if !script_path.exists() {
        write_default_claude_statusline_script(&script_path)?;
        script_created = true;
    } else {
        let existing = fs::read_to_string(&script_path)
            .wrap_err_with(|| format!("failed reading {}", script_path.display()))?;
        if should_update_generated_claude_statusline(&existing) {
            write_default_claude_statusline_script(&script_path)?;
            script_updated = true;
        }
    }

    let mut settings: Value = if settings_path.exists() {
        let raw = fs::read_to_string(&settings_path)
            .wrap_err_with(|| format!("failed reading {}", settings_path.display()))?;
        serde_json::from_str(&raw)
            .wrap_err_with(|| format!("invalid JSON at {}", settings_path.display()))?
    } else {
        json!({})
    };

    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| eyre!("{} must contain a JSON object", settings_path.display()))?;

    let needs_statusline = settings_obj
        .get("statusLine")
        .map(|value| value.is_null())
        .unwrap_or(true);

    let mut settings_updated = false;
    if needs_statusline {
        settings_obj.insert(
            "statusLine".to_string(),
            json!({
                "type": "command",
                "command": default_claude_statusline_command(&script_path),
            }),
        );

        let serialized = serde_json::to_string_pretty(&settings)
            .wrap_err("failed serializing Claude settings")?;
        fs::write(&settings_path, format!("{serialized}\n"))
            .wrap_err_with(|| format!("failed writing {}", settings_path.display()))?;
        paths::set_owner_only_file(&settings_path)?;
        settings_updated = true;
    }

    Ok(StatuslineProvisionResult {
        script_created,
        script_updated,
        settings_updated,
        script_path,
        settings_path,
    })
}

fn default_claude_statusline_script() -> &'static str {
    r#"#!/usr/bin/env bash
# cloak-generated-statusline-v2
set -euo pipefail

input="$(cat)"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
snapshot_path="$script_dir/usage-limits.json"

if command -v jq >/dev/null 2>&1; then
  snapshot="$(printf '%s' "$input" | jq -c '
    {
      observed_at: (now | todateiso8601),
      rate_limits: (
        {
          five_hour: .rate_limits.five_hour?,
          seven_day: .rate_limits.seven_day?
        } | with_entries(select(.value != null))
      )
    }
  ')"

  if [ "$(printf '%s' "$snapshot" | jq -r '.rate_limits | length')" -gt 0 ]; then
    printf '%s\n' "$snapshot" > "$snapshot_path"
    chmod 600 "$snapshot_path" 2>/dev/null || true
  fi

  line="$(printf '%s' "$input" | jq -r '
    def firststr($arr): ($arr | map(select(type=="string" and . != "")) | .[0] // "");
    def firstnum($arr): ($arr | map(select(type=="number")) | .[0] // null);

    {
      model: firststr([.model.display_name?, .model.name?, .model?]),
      context: firstnum([.context_window.used_percentage?, .contextWindow.usedPercentage?, .context_usage.percent?]),
      cost: firstnum([.cost.session_usd?, .cost.session?, .session.cost_usd?]),
      cwd: firststr([.workspace.current_dir?, .cwd?, .working_directory?])
    } | "\(.model)\t\(.context // "")\t\(.cost // "")\t\(.cwd)"
  ')"

  IFS=$'\t' read -r model context cost cwd <<< "$line"
  if [ -z "${model:-}" ]; then
    model="Claude"
  fi

  out="$model"
  if [ -n "${context:-}" ]; then
    out="$out | ctx ${context}%"
  fi
  if [ -n "${cost:-}" ]; then
    out="$out | \$$cost"
  fi
  if [ -n "${cwd:-}" ]; then
    out="$out | $(basename "$cwd")"
  fi

  printf '%s\n' "$out"
  exit 0
fi

printf 'Claude\n'
"#
}

fn legacy_claude_statusline_script() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail

input="$(cat)"

if command -v jq >/dev/null 2>&1; then
  line="$(printf '%s' "$input" | jq -r '
    def firststr($arr): ($arr | map(select(type=="string" and . != "")) | .[0] // "");
    def firstnum($arr): ($arr | map(select(type=="number")) | .[0] // null);

    {
      model: firststr([.model.display_name?, .model.name?, .model?]),
      context: firstnum([.context_window.percent_used?, .contextWindow.percentUsed?, .context_usage.percent?]),
      cost: firstnum([.cost.session_usd?, .cost.session?, .session.cost_usd?]),
      cwd: firststr([.workspace.current_dir?, .cwd?, .working_directory?])
    } | "\(.model)\t\(.context // "")\t\(.cost // "")\t\(.cwd)"
  ')"

  IFS=$'\t' read -r model context cost cwd <<< "$line"
  if [ -z "${model:-}" ]; then
    model="Claude"
  fi

  out="$model"
  if [ -n "${context:-}" ]; then
    out="$out | ctx ${context}%"
  fi
  if [ -n "${cost:-}" ]; then
    out="$out | \$$cost"
  fi
  if [ -n "${cwd:-}" ]; then
    out="$out | $(basename "$cwd")"
  fi

  printf '%s\n' "$out"
  exit 0
fi

printf 'Claude\n'
"#
}

fn should_update_generated_claude_statusline(existing: &str) -> bool {
    existing == legacy_claude_statusline_script()
}

fn write_default_claude_statusline_script(script_path: &Path) -> Result<()> {
    fs::write(script_path, default_claude_statusline_script()).wrap_err_with(|| {
        format!(
            "failed to write Claude statusline script at {}",
            script_path.display()
        )
    })?;

    set_script_permissions_owner_only(script_path)
}

fn default_claude_statusline_command(script_path: &Path) -> String {
    #[cfg(unix)]
    {
        format!("bash {}", shell_single_quote(script_path))
    }

    #[cfg(not(unix))]
    {
        let _ = script_path;
        String::new()
    }
}

fn shell_single_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\"'\"'"))
}

fn set_script_permissions_owner_only(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms)
            .wrap_err_with(|| format!("failed setting script permissions on {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::config::{CliConfig, Config, GeneralConfig};

    use super::{
        civil_from_days, expired_usage_rank_hint, expired_usage_snapshot_hint,
        format_codex_credits, format_limit_subject_detail, format_percent,
        format_unix_timestamp_utc, format_window_minutes, legacy_claude_statusline_script,
        missing_usage_snapshot_hint, provision_default_claude_statusline, shell_single_quote,
        should_update_generated_claude_statusline,
    };
    use crate::account::CodexCreditsSummary;

    #[test]
    fn test_provision_statusline_assets_for_claude_profile() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("work");
        fs::create_dir_all(profile_dir.join("claude")).expect("create claude dir");

        let mut cli_map = std::collections::HashMap::new();
        cli_map.insert(
            "claude".to_string(),
            CliConfig {
                binary: "claude".to_string(),
                config_dir_env: Some("CLAUDE_CONFIG_DIR".to_string()),
                remove_env_vars: vec![],
                extra_env: Default::default(),
                launch_args: vec![],
            },
        );

        let cfg = Config {
            general: GeneralConfig {
                default_profile: "personal".to_string(),
            },
            cli: cli_map,
            agents: std::collections::HashMap::new(),
        };

        let result = provision_default_claude_statusline(&profile_dir, &cfg).expect("provision");
        assert!(result.script_created);
        assert!(!result.script_updated);
        assert!(result.settings_updated);

        let script = fs::read_to_string(profile_dir.join("claude/statusline-command.sh"))
            .expect("read script");
        assert!(script.contains("jq -r"));
        assert!(script.contains("usage-limits.json"));

        let settings =
            fs::read_to_string(profile_dir.join("claude/settings.json")).expect("read settings");
        assert!(settings.contains("\"statusLine\""));
        assert!(settings.contains("statusline-command.sh"));
    }

    #[test]
    fn test_provision_does_not_override_existing_statusline() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("work");
        let claude_dir = profile_dir.join("claude");
        fs::create_dir_all(&claude_dir).expect("create claude dir");
        fs::write(
            claude_dir.join("settings.json"),
            r#"{"statusLine":{"type":"command","command":"bash /tmp/custom.sh"}}"#,
        )
        .expect("write settings");

        let mut cli_map = std::collections::HashMap::new();
        cli_map.insert(
            "claude".to_string(),
            CliConfig {
                binary: "claude".to_string(),
                config_dir_env: Some("CLAUDE_CONFIG_DIR".to_string()),
                remove_env_vars: vec![],
                extra_env: Default::default(),
                launch_args: vec![],
            },
        );

        let cfg = Config {
            general: GeneralConfig {
                default_profile: "personal".to_string(),
            },
            cli: cli_map,
            agents: std::collections::HashMap::new(),
        };

        let result = provision_default_claude_statusline(&profile_dir, &cfg).expect("provision");
        assert!(!result.settings_updated);
        assert!(!result.script_updated);

        let settings =
            fs::read_to_string(profile_dir.join("claude/settings.json")).expect("read settings");
        assert!(settings.contains("/tmp/custom.sh"));
    }

    #[test]
    fn test_provision_updates_legacy_generated_statusline_script() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("work");
        let claude_dir = profile_dir.join("claude");
        fs::create_dir_all(&claude_dir).expect("create claude dir");
        fs::write(
            claude_dir.join("statusline-command.sh"),
            legacy_claude_statusline_script(),
        )
        .expect("write statusline script");

        let mut cli_map = std::collections::HashMap::new();
        cli_map.insert(
            "claude".to_string(),
            CliConfig {
                binary: "claude".to_string(),
                config_dir_env: Some("CLAUDE_CONFIG_DIR".to_string()),
                remove_env_vars: vec![],
                extra_env: Default::default(),
                launch_args: vec![],
            },
        );

        let cfg = Config {
            general: GeneralConfig {
                default_profile: "personal".to_string(),
            },
            cli: cli_map,
            agents: std::collections::HashMap::new(),
        };

        let result = provision_default_claude_statusline(&profile_dir, &cfg).expect("provision");
        assert!(!result.script_created);
        assert!(result.script_updated);

        let script = fs::read_to_string(claude_dir.join("statusline-command.sh"))
            .expect("read statusline script");
        assert!(script.contains("usage-limits.json"));
        assert!(script.contains("# cloak-generated-statusline-v2"));
    }

    #[test]
    fn test_format_unix_timestamp_utc_renders_expected_value() {
        assert_eq!(
            format_unix_timestamp_utc(1_774_719_759, 0),
            "2026-03-28 17:42:39 UTC"
        );
    }

    #[test]
    fn test_format_unix_timestamp_utc_renders_epoch_zero() {
        assert_eq!(format_unix_timestamp_utc(0, 0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn test_format_unix_timestamp_utc_with_negative_offset() {
        assert_eq!(
            format_unix_timestamp_utc(1_774_719_759, -3),
            "2026-03-28 14:42:39 UTC-3"
        );
    }

    #[test]
    fn test_format_unix_timestamp_utc_with_positive_offset() {
        assert_eq!(
            format_unix_timestamp_utc(1_774_719_759, 9),
            "2026-03-29 02:42:39 UTC+9"
        );
    }

    #[test]
    fn test_format_unix_timestamp_utc_offset_crosses_day_boundary() {
        // 1970-01-01 23:00:00 UTC → with +2 becomes 1970-01-02 01:00:00 UTC+2
        assert_eq!(
            format_unix_timestamp_utc(82_800, 2),
            "1970-01-02 01:00:00 UTC+2"
        );
    }

    #[test]
    fn test_format_window_minutes_renders_weeks() {
        assert_eq!(format_window_minutes(10_080), "1w");
        assert_eq!(format_window_minutes(20_160), "2w");
    }

    #[test]
    fn test_format_window_minutes_renders_days() {
        assert_eq!(format_window_minutes(1_440), "1d");
        assert_eq!(format_window_minutes(4_320), "3d");
    }

    #[test]
    fn test_format_window_minutes_renders_hours() {
        assert_eq!(format_window_minutes(60), "1h");
        assert_eq!(format_window_minutes(300), "5h");
    }

    #[test]
    fn test_format_window_minutes_renders_raw_minutes() {
        assert_eq!(format_window_minutes(45), "45m");
        assert_eq!(format_window_minutes(90), "90m");
    }

    #[test]
    fn test_format_percent_renders_integer_without_decimal() {
        assert_eq!(format_percent(0.0), "0%");
        assert_eq!(format_percent(100.0), "100%");
        assert_eq!(format_percent(42.0), "42%");
    }

    #[test]
    fn test_format_percent_renders_fractional_with_one_decimal() {
        assert_eq!(format_percent(12.5), "12.5%");
        assert_eq!(format_percent(99.9), "99.9%");
    }

    #[test]
    fn test_format_limit_subject_detail_all_combinations() {
        assert_eq!(
            format_limit_subject_detail(Some("team"), Some("default_raven")),
            Some("plan: team, tier: default_raven".to_string())
        );
        assert_eq!(
            format_limit_subject_detail(Some("team"), None),
            Some("plan: team".to_string())
        );
        assert_eq!(
            format_limit_subject_detail(None, Some("default_raven")),
            Some("tier: default_raven".to_string())
        );
        assert_eq!(format_limit_subject_detail(None, None), None);
    }

    #[test]
    fn test_format_codex_credits_with_all_fields() {
        let credits = CodexCreditsSummary {
            used: Some("12.5".to_string()),
            remaining: Some("87.5".to_string()),
            total: Some("100".to_string()),
            resets_at: Some(1_774_719_759),
            opaque: false,
        };
        let result = format_codex_credits(&credits, 0);
        assert!(result.contains("used 12.5"));
        assert!(result.contains("remaining 87.5"));
        assert!(result.contains("total 100"));
        assert!(result.contains("resets 2026-03-28 17:42:39 UTC"));
    }

    #[test]
    fn test_format_codex_credits_partial_fields() {
        let credits = CodexCreditsSummary {
            used: Some("5.0".to_string()),
            remaining: None,
            total: None,
            resets_at: None,
            opaque: false,
        };
        assert_eq!(format_codex_credits(&credits, 0), "used 5.0");
    }

    #[test]
    fn test_format_codex_credits_opaque() {
        let credits = CodexCreditsSummary {
            used: None,
            remaining: None,
            total: None,
            resets_at: None,
            opaque: true,
        };
        assert_eq!(
            format_codex_credits(&credits, 0),
            "available (details unavailable)"
        );
    }

    #[test]
    fn test_civil_from_days_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn test_civil_from_days_known_dates() {
        assert_eq!(civil_from_days(20_540), (2026, 3, 28));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn test_shell_single_quote_simple_path() {
        let result = shell_single_quote(std::path::Path::new("/tmp/scripts/run.sh"));
        assert_eq!(result, "'/tmp/scripts/run.sh'");
    }

    #[test]
    fn test_shell_single_quote_with_single_quote_in_path() {
        let result = shell_single_quote(std::path::Path::new("/tmp/it's here/run.sh"));
        assert_eq!(result, "'/tmp/it'\"'\"'s here/run.sh'");
    }

    #[test]
    fn test_should_update_generated_claude_statusline_detects_legacy() {
        assert!(should_update_generated_claude_statusline(
            legacy_claude_statusline_script()
        ));
    }

    #[test]
    fn test_should_update_generated_claude_statusline_ignores_custom() {
        assert!(!should_update_generated_claude_statusline(
            "#!/usr/bin/env bash\necho custom\n"
        ));
    }

    #[test]
    fn test_missing_usage_snapshot_hint_is_cli_specific() {
        assert!(
            missing_usage_snapshot_hint("claude").contains("statusline writes usage-limits.json")
        );
        assert!(missing_usage_snapshot_hint("codex").contains("token_count events"));
    }

    #[test]
    fn test_expired_usage_hints_are_cli_specific() {
        assert!(expired_usage_snapshot_hint("claude").contains("wait for a response"));
        assert!(expired_usage_snapshot_hint("codex").contains("token_count snapshot"));
        assert!(expired_usage_rank_hint("claude").contains("affected profile"));
    }
}
