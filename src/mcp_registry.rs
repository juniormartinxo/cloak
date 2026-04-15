use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{eyre, Context, Result};
use serde::Deserialize;

use crate::{cli::McpTransport, paths};

const BUILTIN_REGISTRY_TOML: &str = include_str!("../resources/mcp_registry.toml");

#[derive(Debug, Deserialize)]
struct RawEntry {
    description: String,
    transport: String,
    #[serde(default)]
    supported: Option<Vec<String>>,
    #[serde(default)]
    command: Option<Vec<String>>,
    #[serde(default)]
    command_per_cli: Option<BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: Vec<String>,
    #[serde(default)]
    bearer_token_env_var: Option<String>,
    #[serde(default)]
    raw: bool,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub name: String,
    pub description: String,
    pub notes: Option<String>,
    pub transport: McpTransport,
    pub raw: bool,
    pub supported: Vec<String>,
    command: CommandShape,
    pub env: Vec<String>,
    pub url: Option<String>,
    pub headers: Vec<String>,
    pub bearer_token_env_var: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandShape {
    Shared(Vec<String>),
    PerCli(BTreeMap<String, Vec<String>>),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEntry {
    pub name: String,
    pub description: String,
    pub notes: Option<String>,
    pub transport: McpTransport,
    pub raw: bool,
    pub command: Vec<String>,
    pub env: Vec<String>,
    pub url: Option<String>,
    pub headers: Vec<String>,
    pub bearer_token_env_var: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Registry {
    entries: BTreeMap<String, RegistryEntry>,
}

impl Registry {
    pub fn load() -> Result<Self> {
        let user_path = user_registry_path().ok();
        Self::load_from_sources(BUILTIN_REGISTRY_TOML, user_path.as_deref())
    }

    pub fn load_from_sources(builtin_toml: &str, user_path: Option<&Path>) -> Result<Self> {
        let mut raw_map: BTreeMap<String, RawEntry> = toml::from_str(builtin_toml)
            .wrap_err("failed parsing built-in MCP registry (resources/mcp_registry.toml)")?;

        if let Some(path) = user_path {
            if path.exists() {
                let body = fs::read_to_string(path)
                    .wrap_err_with(|| format!("failed reading user registry {}", path.display()))?;
                let user_map: BTreeMap<String, RawEntry> = toml::from_str(&body)
                    .wrap_err_with(|| format!("failed parsing user registry {}", path.display()))?;
                for (name, raw) in user_map {
                    raw_map.insert(name, raw);
                }
            }
        }

        let mut entries: BTreeMap<String, RegistryEntry> = BTreeMap::new();
        for (name, raw) in raw_map {
            let entry = build_entry(&name, raw)
                .wrap_err_with(|| format!("invalid MCP registry entry '{}'", name))?;
            entries.insert(name, entry);
        }

        Ok(Self { entries })
    }

    pub fn get(&self, name: &str) -> Option<&RegistryEntry> {
        self.entries.get(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RegistryEntry> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl RegistryEntry {
    /// Builds a concrete, environment-expanded entry for the given target CLI.
    pub fn resolve(&self, cli_name: &str) -> Result<ResolvedEntry> {
        self.resolve_with(cli_name, &DefaultExpansion)
    }

    /// Resolution with a pluggable variable expander (used by tests).
    pub fn resolve_with(
        &self,
        cli_name: &str,
        expander: &dyn VariableExpander,
    ) -> Result<ResolvedEntry> {
        if !self.supported.iter().any(|s| s == cli_name) {
            return Err(eyre!(
                "MCP '{}' does not declare support for CLI '{}' (supported: {})",
                self.name,
                cli_name,
                self.supported.join(", ")
            ));
        }

        let raw_command: Vec<String> = match &self.command {
            CommandShape::Shared(items) => items.clone(),
            CommandShape::PerCli(map) => map.get(cli_name).cloned().ok_or_else(|| {
                eyre!(
                    "MCP '{}' has no command_per_cli entry for '{}'",
                    self.name,
                    cli_name
                )
            })?,
            CommandShape::None => Vec::new(),
        };

        let command = expand_list(&raw_command, expander)?;
        let env = expand_list(&self.env, expander)?;
        let headers = expand_list(&self.headers, expander)?;
        let url = match &self.url {
            Some(u) => Some(expand_string(u, expander)?),
            None => None,
        };

        Ok(ResolvedEntry {
            name: self.name.clone(),
            description: self.description.clone(),
            notes: self.notes.clone(),
            transport: self.transport,
            raw: self.raw,
            command,
            env,
            url,
            headers,
            bearer_token_env_var: self.bearer_token_env_var.clone(),
        })
    }
}

fn build_entry(name: &str, raw: RawEntry) -> Result<RegistryEntry> {
    if raw.description.trim().is_empty() {
        return Err(eyre!("description cannot be empty"));
    }

    let transport = parse_transport(&raw.transport)?;

    let command_shape = match (raw.command, raw.command_per_cli) {
        (Some(_), Some(_)) => {
            return Err(eyre!(
                "command and command_per_cli cannot both be set — pick one"
            ));
        }
        (Some(items), None) => {
            if items.is_empty() {
                return Err(eyre!("command cannot be empty"));
            }
            CommandShape::Shared(items)
        }
        (None, Some(map)) => {
            if map.is_empty() {
                return Err(eyre!("command_per_cli cannot be empty"));
            }
            for (cli, items) in &map {
                paths::validate_cli_name(cli)
                    .wrap_err_with(|| format!("invalid CLI name '{}' in command_per_cli", cli))?;
                if items.is_empty() {
                    return Err(eyre!("command_per_cli['{}'] cannot be empty", cli));
                }
            }
            CommandShape::PerCli(map)
        }
        (None, None) => CommandShape::None,
    };

    let supported = match (&command_shape, raw.supported) {
        (CommandShape::PerCli(map), Some(list)) => {
            let expected: Vec<String> = {
                let mut keys: Vec<String> = map.keys().cloned().collect();
                keys.sort();
                keys
            };
            let mut actual = list;
            actual.sort();
            actual.dedup();
            if actual != expected {
                return Err(eyre!(
                    "supported {:?} must match command_per_cli keys {:?}",
                    actual,
                    expected
                ));
            }
            expected
        }
        (CommandShape::PerCli(map), None) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            keys
        }
        (_, Some(list)) => {
            let mut out = list;
            out.sort();
            out.dedup();
            if out.is_empty() {
                return Err(eyre!("supported cannot be empty"));
            }
            for cli in &out {
                paths::validate_cli_name(cli)
                    .wrap_err_with(|| format!("invalid CLI name '{}' in supported", cli))?;
            }
            out
        }
        (_, None) => {
            return Err(eyre!(
                "supported is required when command is shared or absent"
            ));
        }
    };

    match transport {
        McpTransport::Stdio => {
            if matches!(command_shape, CommandShape::None) {
                return Err(eyre!(
                    "stdio MCP entries require `command` or `command_per_cli`"
                ));
            }
            if raw.url.is_some() {
                return Err(eyre!("url is only valid for http/sse transports"));
            }
            if !raw.headers.is_empty() {
                return Err(eyre!("headers are only valid for http/sse transports"));
            }
            if raw.bearer_token_env_var.is_some() {
                return Err(eyre!(
                    "bearer_token_env_var is only valid for http transport"
                ));
            }
            for item in &raw.env {
                if item.split_once('=').is_none() {
                    return Err(eyre!("env entries must use KEY=VALUE format: '{}'", item));
                }
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            if raw.url.is_none() {
                return Err(eyre!("http/sse MCP entries require `url`"));
            }
            if !matches!(command_shape, CommandShape::None) {
                return Err(eyre!("http/sse MCP entries cannot define stdio commands"));
            }
            if !raw.env.is_empty() {
                return Err(eyre!("env is only valid for stdio transport"));
            }
            for item in &raw.headers {
                if item.split_once(':').is_none() {
                    return Err(eyre!(
                        "header entries must use 'Name: value' format: '{}'",
                        item
                    ));
                }
            }
            if raw.raw {
                return Err(eyre!("raw mode is only valid for stdio transport"));
            }
            if matches!(transport, McpTransport::Sse) && raw.bearer_token_env_var.is_some() {
                return Err(eyre!(
                    "bearer_token_env_var is only valid for http transport"
                ));
            }
        }
    }

    Ok(RegistryEntry {
        name: name.to_string(),
        description: raw.description.trim().to_string(),
        notes: raw
            .notes
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty()),
        transport,
        raw: raw.raw,
        supported,
        command: command_shape,
        env: raw.env,
        url: raw.url,
        headers: raw.headers,
        bearer_token_env_var: raw.bearer_token_env_var,
    })
}

fn parse_transport(value: &str) -> Result<McpTransport> {
    match value {
        "stdio" => Ok(McpTransport::Stdio),
        "http" => Ok(McpTransport::Http),
        "sse" => Ok(McpTransport::Sse),
        other => Err(eyre!(
            "unknown transport '{}' (expected stdio, http or sse)",
            other
        )),
    }
}

fn user_registry_path() -> Result<PathBuf> {
    Ok(paths::cloak_config_dir()?.join("mcp_registry.toml"))
}

pub trait VariableExpander {
    fn resolve(&self, name: &str) -> Option<String>;
}

struct DefaultExpansion;

impl VariableExpander for DefaultExpansion {
    fn resolve(&self, name: &str) -> Option<String> {
        match name {
            "CWD" => std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
            "HOME" => dirs::home_dir()
                .map(|p| p.display().to_string())
                .or_else(|| std::env::var("HOME").ok()),
            other => std::env::var(other).ok(),
        }
    }
}

fn expand_list(items: &[String], expander: &dyn VariableExpander) -> Result<Vec<String>> {
    items.iter().map(|s| expand_string(s, expander)).collect()
}

fn expand_string(input: &str, expander: &dyn VariableExpander) -> Result<String> {
    let mut out = String::with_capacity(input.len());
    let mut missing: Vec<String> = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end_rel) = input[i + 2..].find('}') {
                let end = i + 2 + end_rel;
                let name = &input[i + 2..end];
                if name.is_empty() {
                    return Err(eyre!("empty ${{}} placeholder in '{}'", input));
                }
                match expander.resolve(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        if !missing.contains(&name.to_string()) {
                            missing.push(name.to_string());
                        }
                        // keep literal so the error preview stays readable
                        out.push_str(&input[i..=end]);
                    }
                }
                i = end + 1;
                continue;
            } else {
                return Err(eyre!("unterminated ${{...}} placeholder in '{}'", input));
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }

    if !missing.is_empty() {
        return Err(eyre!(
            "missing environment variable{} while expanding '{}': {}",
            if missing.len() > 1 { "s" } else { "" },
            input,
            missing.join(", ")
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeEnv(HashMap<String, String>);

    impl VariableExpander for FakeEnv {
        fn resolve(&self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
    }

    fn env(pairs: &[(&str, &str)]) -> FakeEnv {
        FakeEnv(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        )
    }

    #[test]
    fn builtin_registry_parses_and_covers_expected_entries() {
        let registry =
            Registry::load_from_sources(BUILTIN_REGISTRY_TOML, None).expect("builtin should parse");
        for name in [
            "everything",
            "fetch",
            "filesystem",
            "git",
            "memory",
            "sequential-thinking",
            "time",
            "playwright",
            "context7",
            "gitnexus",
            "shadcn",
            "github",
            "sentry",
        ] {
            assert!(
                registry.get(name).is_some(),
                "missing built-in entry '{}'",
                name
            );
        }
    }

    #[test]
    fn resolve_expands_cwd_in_shared_command() {
        let registry = Registry::load_from_sources(BUILTIN_REGISTRY_TOML, None).unwrap();
        let entry = registry.get("filesystem").unwrap();
        let expander = env(&[("CWD", "/tmp/project")]);

        let resolved = entry.resolve_with("codex", &expander).expect("resolve");
        assert_eq!(
            resolved.command,
            vec![
                "npx".to_string(),
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "/tmp/project".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_errors_on_missing_env_var() {
        let registry = Registry::load_from_sources(BUILTIN_REGISTRY_TOML, None).unwrap();
        let entry = registry.get("github").unwrap();
        let expander = env(&[]); // GITHUB_PERSONAL_ACCESS_TOKEN missing

        let err = entry
            .resolve_with("codex", &expander)
            .expect_err("must fail without token");
        let msg = err.to_string();
        assert!(msg.contains("GITHUB_PERSONAL_ACCESS_TOKEN"), "got: {}", msg);
    }

    #[test]
    fn resolve_rejects_unsupported_cli() {
        let registry = Registry::load_from_sources(BUILTIN_REGISTRY_TOML, None).unwrap();
        let entry = registry.get("gitnexus").unwrap();

        let err = entry
            .resolve_with("gemini", &DefaultExpansion)
            .expect_err("must fail");
        assert!(err.to_string().contains("does not declare support"));
    }

    #[test]
    fn resolve_picks_per_cli_command() {
        let registry = Registry::load_from_sources(BUILTIN_REGISTRY_TOML, None).unwrap();
        let entry = registry.get("shadcn").unwrap();

        let codex = entry.resolve_with("codex", &DefaultExpansion).unwrap();
        assert!(codex.command.ends_with(&["codex".to_string()]));

        let claude = entry.resolve_with("claude", &DefaultExpansion).unwrap();
        assert!(claude.command.ends_with(&["claude".to_string()]));
    }

    #[test]
    fn http_entry_does_not_carry_stdio_fields() {
        let registry = Registry::load_from_sources(BUILTIN_REGISTRY_TOML, None).unwrap();
        let entry = registry.get("sentry").unwrap();

        let resolved = entry.resolve_with("claude", &DefaultExpansion).unwrap();
        assert_eq!(resolved.transport, McpTransport::Http);
        assert_eq!(resolved.url.as_deref(), Some("https://mcp.sentry.dev/mcp"));
        assert!(resolved.command.is_empty());
    }

    #[test]
    fn user_override_replaces_builtin_entry() {
        let user = r#"
[gitnexus]
description = "overridden"
transport = "stdio"
command = ["echo", "overridden"]
supported = ["codex"]
"#;
        let tmp = tempfile::tempdir().expect("tmp");
        let path = tmp.path().join("mcp_registry.toml");
        std::fs::write(&path, user).expect("write user registry");

        let registry =
            Registry::load_from_sources(BUILTIN_REGISTRY_TOML, Some(&path)).expect("merge");
        let entry = registry.get("gitnexus").unwrap();
        assert_eq!(entry.description, "overridden");
        assert_eq!(entry.supported, vec!["codex".to_string()]);
    }

    #[test]
    fn shared_command_requires_supported() {
        let toml = r#"
[foo]
description = "demo"
transport = "stdio"
command = ["echo"]
"#;
        let err = Registry::load_from_sources(toml, None).expect_err("missing supported must fail");
        let full = format!("{:#}", err);
        assert!(full.contains("supported"), "got: {}", full);
    }

    #[test]
    fn stdio_entry_rejects_url() {
        let toml = r#"
[foo]
description = "demo"
transport = "stdio"
command = ["echo"]
supported = ["codex"]
url = "https://example.com"
"#;
        let err = Registry::load_from_sources(toml, None).expect_err("must fail");
        let full = format!("{:#}", err);
        assert!(full.contains("url"), "got: {}", full);
    }

    #[test]
    fn expand_string_handles_multiple_placeholders() {
        let expander = env(&[("A", "alpha"), ("B", "beta")]);
        let out = expand_string("${A}/${B}/literal", &expander).unwrap();
        assert_eq!(out, "alpha/beta/literal");
    }

    #[test]
    fn expand_string_reports_all_missing_vars() {
        let expander = env(&[]);
        let err = expand_string("${FOO}-${BAR}", &expander).expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("FOO"));
        assert!(msg.contains("BAR"));
    }
}
