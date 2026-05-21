use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use color_eyre::eyre::{Context, Result};
use serde_json::{json, Value};

use crate::paths;

const STDERR_CAPTURE_LIMIT: usize = 4096;
const LINE_TRUNCATE: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteKind {
    Http,
    Sse,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Remote {
        url: String,
        kind: RemoteKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpEntry {
    pub cli: String,
    pub profile: String,
    pub server_name: String,
    pub transport: Transport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    Ok {
        server_name: Option<String>,
        server_version: Option<String>,
        protocol_version: Option<String>,
        tools: Option<Vec<String>>,
    },
    Skipped {
        reason: String,
    },
    Failed {
        reason: String,
        stderr: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub entry: McpEntry,
    pub outcome: ProbeOutcome,
    pub elapsed_ms: u128,
}

pub fn read_entries_for(cli: &str, profile: &str) -> Result<Vec<McpEntry>> {
    let dir = paths::profile_cli_dir(profile, cli)?;
    match cli {
        "codex" => read_codex_entries(&dir, profile),
        "claude" => read_claude_entries(&dir, profile),
        _ => Ok(Vec::new()),
    }
}

pub fn read_codex_entries(dir: &Path, profile: &str) -> Result<Vec<McpEntry>> {
    let path = dir.join("config.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(&path).wrap_err_with(|| format!("failed reading {}", path.display()))?;
    parse_codex_entries(&text, profile)
        .wrap_err_with(|| format!("failed parsing {}", path.display()))
}

pub fn read_claude_entries(dir: &Path, profile: &str) -> Result<Vec<McpEntry>> {
    let path = dir.join(".claude.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(&path).wrap_err_with(|| format!("failed reading {}", path.display()))?;
    parse_claude_entries(&text, profile)
        .wrap_err_with(|| format!("failed parsing {}", path.display()))
}

pub fn parse_codex_entries(text: &str, profile: &str) -> Result<Vec<McpEntry>> {
    let value: toml::Value = toml::from_str(text).wrap_err("invalid TOML")?;
    let Some(servers) = value.get("mcp_servers").and_then(|v| v.as_table()) else {
        return Ok(Vec::new());
    };

    let mut entries = Vec::new();
    for (name, raw) in servers {
        let Some(table) = raw.as_table() else {
            continue;
        };

        let transport = if let Some(url) = table.get("url").and_then(|v| v.as_str()) {
            Transport::Remote {
                url: url.to_string(),
                kind: RemoteKind::Http,
            }
        } else if let Some(command) = table.get("command").and_then(|v| v.as_str()) {
            let args = table
                .get("args")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let env = table
                .get("env")
                .and_then(|v| v.as_table())
                .map(|et| {
                    et.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            Transport::Stdio {
                command: command.to_string(),
                args,
                env,
            }
        } else {
            continue;
        };

        entries.push(McpEntry {
            cli: "codex".to_string(),
            profile: profile.to_string(),
            server_name: name.clone(),
            transport,
        });
    }
    entries.sort_by(|a, b| a.server_name.cmp(&b.server_name));
    Ok(entries)
}

pub fn parse_claude_entries(text: &str, profile: &str) -> Result<Vec<McpEntry>> {
    let value: Value = serde_json::from_str(text).wrap_err("invalid JSON")?;
    let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) else {
        return Ok(Vec::new());
    };

    let mut entries = Vec::new();
    for (name, raw) in servers {
        let ty = raw.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");

        let transport = match ty {
            "stdio" => {
                let Some(command) = raw.get("command").and_then(|v| v.as_str()) else {
                    continue;
                };
                let args = raw
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let env = raw
                    .get("env")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();
                Transport::Stdio {
                    command: command.to_string(),
                    args,
                    env,
                }
            }
            kind @ ("http" | "sse") => {
                let Some(url) = raw.get("url").and_then(|v| v.as_str()) else {
                    continue;
                };
                Transport::Remote {
                    url: url.to_string(),
                    kind: if kind == "http" {
                        RemoteKind::Http
                    } else {
                        RemoteKind::Sse
                    },
                }
            }
            other => Transport::Remote {
                url: raw
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                kind: RemoteKind::Other(other.to_string()),
            },
        };

        entries.push(McpEntry {
            cli: "claude".to_string(),
            profile: profile.to_string(),
            server_name: name.clone(),
            transport,
        });
    }
    entries.sort_by(|a, b| a.server_name.cmp(&b.server_name));
    Ok(entries)
}

pub fn probe(entry: &McpEntry, timeout: Duration, with_tools: bool) -> ProbeResult {
    let start = Instant::now();
    let outcome = match &entry.transport {
        Transport::Stdio { command, args, env } => {
            probe_stdio(command, args, env, timeout, with_tools)
        }
        Transport::Remote { kind, .. } => {
            let label = match kind {
                RemoteKind::Http => "http",
                RemoteKind::Sse => "sse",
                RemoteKind::Other(s) => s.as_str(),
            };
            ProbeOutcome::Skipped {
                reason: format!("remote transport '{label}' not covered by doctor yet"),
            }
        }
    };
    ProbeResult {
        entry: entry.clone(),
        outcome,
        elapsed_ms: start.elapsed().as_millis(),
    }
}

fn probe_stdio(
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    timeout: Duration,
    with_tools: bool,
) -> ProbeOutcome {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            return ProbeOutcome::Failed {
                reason: format!("failed to spawn `{command}`: {err}"),
                stderr: None,
            };
        }
    };

    let stderr_buf: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let stderr_done = child.stderr.take().map(|stderr| {
        let buf = Arc::clone(&stderr_buf);
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(|l| l.ok()) {
                let mut guard = match buf.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if guard.len() >= STDERR_CAPTURE_LIMIT {
                    return;
                }
                guard.push_str(&line);
                guard.push('\n');
                if guard.len() >= STDERR_CAPTURE_LIMIT {
                    guard.truncate(STDERR_CAPTURE_LIMIT);
                }
            }
            let _ = done_tx.send(());
        });
        done_rx
    });

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            return ProbeOutcome::Failed {
                reason: "no stdout pipe".into(),
                stderr: drain_stderr(&stderr_buf, stderr_done.as_ref()),
            };
        }
    };
    let (stdout_tx, stdout_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(|l| l.ok()) {
            if stdout_tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut stdin = match child.stdin.take() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            return ProbeOutcome::Failed {
                reason: "no stdin pipe".into(),
                stderr: drain_stderr(&stderr_buf, stderr_done.as_ref()),
            };
        }
    };

    if let Err(err) = writeln!(stdin, "{}", initialize_request()) {
        let _ = child.kill();
        return ProbeOutcome::Failed {
            reason: format!("failed writing initialize: {err}"),
            stderr: drain_stderr(&stderr_buf, stderr_done.as_ref()),
        };
    }
    let _ = stdin.flush();

    let init_line = match recv_line(&stdout_rx, timeout, &mut child) {
        Ok(line) => line,
        Err(reason) => {
            let _ = child.kill();
            return ProbeOutcome::Failed {
                reason,
                stderr: drain_stderr(&stderr_buf, stderr_done.as_ref()),
            };
        }
    };

    let parsed: Value = match serde_json::from_str(&init_line) {
        Ok(v) => v,
        Err(_) => {
            let _ = child.kill();
            return ProbeOutcome::Failed {
                reason: format!(
                    "non-JSON initialize response: {}",
                    truncate_char_boundary(&init_line, LINE_TRUNCATE)
                ),
                stderr: drain_stderr(&stderr_buf, stderr_done.as_ref()),
            };
        }
    };

    if let Some(err) = parsed.get("error") {
        let _ = child.kill();
        return ProbeOutcome::Failed {
            reason: format!("initialize returned error: {err}"),
            stderr: drain_stderr(&stderr_buf, stderr_done.as_ref()),
        };
    }

    let server_name = parsed
        .pointer("/result/serverInfo/name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let server_version = parsed
        .pointer("/result/serverInfo/version")
        .and_then(|v| v.as_str())
        .map(String::from);
    let protocol_version = parsed
        .pointer("/result/protocolVersion")
        .and_then(|v| v.as_str())
        .map(String::from);

    let tools = if with_tools {
        let _ = writeln!(stdin, "{}", tools_list_request());
        let _ = stdin.flush();
        match read_json_with_id(&stdout_rx, timeout, 2) {
            Ok(v) => Some(extract_tool_names(&v)),
            Err(_) => None,
        }
    } else {
        None
    };

    let _ = child.kill();
    let _ = child.wait();

    ProbeOutcome::Ok {
        server_name,
        server_version,
        protocol_version,
        tools,
    }
}

fn recv_line(
    rx: &mpsc::Receiver<String>,
    timeout: Duration,
    child: &mut Child,
) -> Result<String, String> {
    match rx.recv_timeout(timeout) {
        Ok(line) => Ok(line),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "timed out waiting for initialize reply after {timeout:?}"
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let status = child.try_wait().ok().flatten();
            match status {
                Some(status) => Err(format!(
                    "process exited before initialize (status: {status})"
                )),
                None => Err("stdout closed before initialize".into()),
            }
        }
    }
}

fn read_json_with_id(
    rx: &mpsc::Receiver<String>,
    timeout: Duration,
    want_id: i64,
) -> Result<Value, ()> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(());
        }
        let line = rx.recv_timeout(remaining).map_err(|_| ())?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(|v| v.as_i64()) == Some(want_id) {
            return Ok(value);
        }
    }
}

fn drain_stderr(
    buf: &Arc<Mutex<String>>,
    stderr_done: Option<&mpsc::Receiver<()>>,
) -> Option<String> {
    if let Some(done) = stderr_done {
        let _ = done.recv_timeout(Duration::from_millis(25));
    }

    let guard = buf.lock().ok()?;
    let trimmed = guard.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn extract_tool_names(value: &Value) -> Vec<String> {
    value
        .pointer("/result/tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn initialize_request() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "cloak-doctor",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }
    })
    .to_string()
}

fn tools_list_request() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    })
    .to_string()
}

fn truncate_char_boundary(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_stdio_entry() {
        let toml = r#"
[mcp_servers.gitnexus]
command = "gitnexus"
args = ["mcp"]

[mcp_servers.secret]
command = "bin"
env = { TOKEN = "abc" }
"#;
        let entries = parse_codex_entries(toml, "work").expect("parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].server_name, "gitnexus");
        assert_eq!(entries[0].profile, "work");
        assert_eq!(entries[0].cli, "codex");
        match &entries[0].transport {
            Transport::Stdio { command, args, env } => {
                assert_eq!(command, "gitnexus");
                assert_eq!(args, &vec!["mcp".to_string()]);
                assert!(env.is_empty());
            }
            other => panic!("expected stdio, got {other:?}"),
        }
        match &entries[1].transport {
            Transport::Stdio { env, .. } => {
                assert_eq!(env.get("TOKEN").map(String::as_str), Some("abc"));
            }
            other => panic!("expected stdio, got {other:?}"),
        }
    }

    #[test]
    fn parses_codex_http_entry() {
        let toml = r#"
[mcp_servers.remote]
url = "https://example.com/mcp"
"#;
        let entries = parse_codex_entries(toml, "work").expect("parse");
        assert_eq!(entries.len(), 1);
        match &entries[0].transport {
            Transport::Remote { url, kind } => {
                assert_eq!(url, "https://example.com/mcp");
                assert_eq!(kind, &RemoteKind::Http);
            }
            other => panic!("expected remote, got {other:?}"),
        }
    }

    #[test]
    fn parses_codex_empty_when_no_servers() {
        let entries = parse_codex_entries("[other]\nkey = 1\n", "work").expect("parse");
        assert!(entries.is_empty());
    }

    #[test]
    fn parses_claude_stdio_and_http() {
        let json = r#"{
          "mcpServers": {
            "gitnexus": {"type": "stdio", "command": "npx", "args": ["-y","gitnexus","mcp"], "env": {"FOO":"bar"}},
            "sentry":   {"type": "http", "url": "https://mcp.sentry.dev/mcp"}
          }
        }"#;
        let entries = parse_claude_entries(json, "personal").expect("parse");
        assert_eq!(entries.len(), 2);
        let gitnexus = entries
            .iter()
            .find(|e| e.server_name == "gitnexus")
            .unwrap();
        let sentry = entries.iter().find(|e| e.server_name == "sentry").unwrap();
        match &gitnexus.transport {
            Transport::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(
                    args,
                    &vec!["-y".to_string(), "gitnexus".into(), "mcp".into()]
                );
                assert_eq!(env.get("FOO").map(String::as_str), Some("bar"));
            }
            other => panic!("expected stdio, got {other:?}"),
        }
        match &sentry.transport {
            Transport::Remote { url, kind } => {
                assert_eq!(url, "https://mcp.sentry.dev/mcp");
                assert_eq!(kind, &RemoteKind::Http);
            }
            other => panic!("expected remote, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_defaults_type_to_stdio() {
        let json = r#"{"mcpServers":{"x":{"command":"bin"}}}"#;
        let entries = parse_claude_entries(json, "p").expect("parse");
        assert_eq!(entries.len(), 1);
        matches!(entries[0].transport, Transport::Stdio { .. });
    }

    #[test]
    fn extract_tool_names_returns_names_only() {
        let value = serde_json::json!({
            "result": {"tools": [{"name":"a","description":"d"},{"name":"b"}]}
        });
        assert_eq!(extract_tool_names(&value), vec!["a", "b"]);
    }

    #[test]
    fn probe_reports_spawn_failure_for_missing_binary() {
        let entry = McpEntry {
            cli: "codex".into(),
            profile: "work".into(),
            server_name: "bogus".into(),
            transport: Transport::Stdio {
                command: "/definitely/not/on/path/cloak-nope".into(),
                args: vec![],
                env: BTreeMap::new(),
            },
        };
        let result = probe(&entry, Duration::from_millis(500), false);
        match result.outcome {
            ProbeOutcome::Failed { reason, .. } => {
                assert!(reason.contains("failed to spawn"), "got: {reason}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn probe_reports_timeout_when_child_does_not_reply() {
        if which::which("sh").is_err() {
            return; // skip when sh is missing
        }
        let entry = McpEntry {
            cli: "codex".into(),
            profile: "work".into(),
            server_name: "silent".into(),
            transport: Transport::Stdio {
                command: "sh".into(),
                args: vec!["-c".into(), "cat >/dev/null".into()],
                env: BTreeMap::new(),
            },
        };
        let result = probe(&entry, Duration::from_millis(250), false);
        match result.outcome {
            ProbeOutcome::Failed { reason, .. } => {
                assert!(reason.contains("timed out"), "got: {reason}");
            }
            other => panic!("expected timeout, got {other:?}"),
        }
    }

    #[test]
    fn probe_captures_stderr_when_process_dies() {
        if which::which("sh").is_err() {
            return;
        }
        let entry = McpEntry {
            cli: "codex".into(),
            profile: "work".into(),
            server_name: "dies".into(),
            transport: Transport::Stdio {
                command: "sh".into(),
                args: vec![
                    "-c".into(),
                    "echo 'boom: something went wrong' 1>&2; exit 1".into(),
                ],
                env: BTreeMap::new(),
            },
        };
        let result = probe(&entry, Duration::from_millis(1000), false);
        match result.outcome {
            ProbeOutcome::Failed { stderr, .. } => {
                let stderr = stderr.expect("stderr captured");
                assert!(stderr.contains("boom"), "got: {stderr}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn probe_succeeds_against_fake_mcp_server() {
        if which::which("sh").is_err() {
            return;
        }
        // Minimal fake MCP: read one line, reply with a valid initialize response.
        let script = r#"
IFS= read -r _req
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fake","version":"9.9.9"}}}'
cat >/dev/null
"#;
        let entry = McpEntry {
            cli: "codex".into(),
            profile: "work".into(),
            server_name: "fake".into(),
            transport: Transport::Stdio {
                command: "sh".into(),
                args: vec!["-c".into(), script.into()],
                env: BTreeMap::new(),
            },
        };
        let result = probe(&entry, Duration::from_millis(1500), false);
        match result.outcome {
            ProbeOutcome::Ok {
                server_name,
                server_version,
                protocol_version,
                ..
            } => {
                assert_eq!(server_name.as_deref(), Some("fake"));
                assert_eq!(server_version.as_deref(), Some("9.9.9"));
                assert_eq!(protocol_version.as_deref(), Some("2024-11-05"));
            }
            other => panic!("expected ok, got {other:?}"),
        }
    }

    #[test]
    fn probe_skips_remote_transports() {
        let entry = McpEntry {
            cli: "claude".into(),
            profile: "work".into(),
            server_name: "remote".into(),
            transport: Transport::Remote {
                url: "https://example.com/mcp".into(),
                kind: RemoteKind::Http,
            },
        };
        let result = probe(&entry, Duration::from_millis(100), false);
        match result.outcome {
            ProbeOutcome::Skipped { reason } => {
                assert!(reason.contains("http"), "got: {reason}");
            }
            other => panic!("expected skipped, got {other:?}"),
        }
    }

    #[test]
    fn truncate_keeps_char_boundary() {
        let s = "áéíóú"; // 10 bytes, 5 chars
        let out = truncate_char_boundary(s, 3);
        assert!(out.ends_with('…'));
        assert!(out.len() < s.len() + 3);
    }

    #[test]
    fn initialize_request_is_valid_jsonrpc() {
        let req = initialize_request();
        let value: Value = serde_json::from_str(&req).expect("json");
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], 1);
        assert_eq!(value["method"], "initialize");
        assert_eq!(value["params"]["protocolVersion"], "2024-11-05");
    }
}
