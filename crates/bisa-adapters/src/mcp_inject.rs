//! How the MCP servers the engine hands a session reach the harnesses that
//! take them by configuration rather than by a flag of their own (ide/18):
//! Codex CLI through `-c mcp_servers.<name>.…` overrides on `codex exec`,
//! OpenCode through the JSON its `OPENCODE_CONFIG_CONTENT` variable carries.
//! Claude Code has `--mcp-config` and ACP its `mcpServers` field; both are
//! their adapters' own. Nothing here names a program: the servers are the
//! engine's, this only spells them the way each harness reads them.

use bisa_harness::McpServerConfig;
use std::collections::BTreeMap;

/// A field of a server's transport this harness cannot carry: said once,
/// the same way for every adapter, and the field is dropped — never quietly
/// applied elsewhere, never guessed into another key. The values are not
/// logged; the field's name and the server's are.
pub fn warn_dropped(harness: &str, server: &str, field: &str) {
    tracing::warn!(
        target: "bisa_adapters::mcp_inject",
        harness,
        server,
        field,
        "the {harness} launch cannot carry `{field}` for the MCP server `{server}`; it is dropped"
    );
}

/// A TOML basic string: quoted, with the quote, the backslash and the
/// control characters escaped — what a `-c key=value` override takes.
pub fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// An inline TOML table of string pairs — `{ K = "v", … }`.
fn toml_table(map: &BTreeMap<String, String>) -> String {
    let pairs: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{} = {}", toml_key(k), toml_string(v)))
        .collect();
    format!("{{ {} }}", pairs.join(", "))
}

/// The `-c key=value` pairs that put every server into Codex's
/// `mcp_servers` table for one run — the keys its config reference names
/// (learn.chatgpt.com/docs/config-file/config-reference): a stdio server's
/// `command`, `args`, `env` and `cwd`; a Streamable HTTP server's `url` and
/// `http_headers`. Codex has no HTTP+SSE transport, so an `sse` server is
/// dropped with a warning rather than handed over as the other contract.
/// Each pair is two arguments — `-c` and the assignment — in the order Codex
/// reads them.
pub fn codex_overrides(servers: &[McpServerConfig]) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |key: String, value: String| {
        out.push("-c".to_string());
        out.push(format!("mcp_servers.{key}={value}"));
    };
    for server in servers {
        let name = server.name();
        match server {
            McpServerConfig::Stdio {
                command,
                args,
                env,
                cwd,
                ..
            } => {
                push(format!("{name}.command"), toml_string(command));
                let args: Vec<String> = args.iter().map(|a| toml_string(a)).collect();
                push(format!("{name}.args"), format!("[{}]", args.join(", ")));
                if !env.is_empty() {
                    push(format!("{name}.env"), toml_table(env));
                }
                if let Some(dir) = cwd {
                    push(format!("{name}.cwd"), toml_string(dir));
                }
            }
            McpServerConfig::Http { url, headers, .. } => {
                push(format!("{name}.url"), toml_string(url));
                if !headers.is_empty() {
                    push(format!("{name}.http_headers"), toml_table(headers));
                }
            }
            McpServerConfig::Sse { .. } => warn_dropped("codex", name, "transport = sse"),
        }
    }
    out
}

/// A TOML key: bare when it may be, quoted otherwise.
fn toml_key(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_string()
    } else {
        toml_string(k)
    }
}

/// The JSON OpenCode reads from `OPENCODE_CONFIG_CONTENT`
/// (opencode.ai/docs/mcp-servers): an `mcp` table with every server — a
/// stdio one as a `local` command with its `environment` and `cwd`, a
/// remote one — Streamable HTTP or the older HTTP+SSE, which OpenCode's one
/// `remote` kind covers — as `remote` with its `url` and `headers`, each
/// enabled.
pub fn opencode_config(servers: &[McpServerConfig]) -> String {
    let mut mcp = serde_json::Map::new();
    for server in servers {
        let entry = match server {
            McpServerConfig::Stdio {
                command,
                args,
                env,
                cwd,
                ..
            } => {
                let mut argv = vec![command.clone()];
                argv.extend(args.iter().cloned());
                let environment: BTreeMap<&str, &str> =
                    env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                let mut entry = serde_json::json!({
                    "type": "local",
                    "command": argv,
                    "environment": environment,
                    "enabled": true,
                });
                if let Some(dir) = cwd {
                    entry["cwd"] = serde_json::Value::String(dir.clone());
                }
                entry
            }
            McpServerConfig::Http { url, headers, .. }
            | McpServerConfig::Sse { url, headers, .. } => {
                let mut entry = serde_json::json!({
                    "type": "remote",
                    "url": url,
                    "enabled": true,
                });
                if !headers.is_empty() {
                    entry["headers"] = serde_json::to_value(headers).unwrap_or_default();
                }
                entry
            }
        };
        mcp.insert(server.name().to_string(), entry);
    }
    serde_json::json!({ "mcp": mcp }).to_string()
}

/// The name of the variable OpenCode reads its inline configuration from.
pub const OPENCODE_CONFIG_CONTENT: &str = "OPENCODE_CONFIG_CONTENT";

#[cfg(test)]
mod tests {
    use super::*;

    fn bisa() -> McpServerConfig {
        McpServerConfig::Stdio {
            name: "bisa".into(),
            command: "/usr/local/bin/bisa".into(),
            args: vec![
                "mcp".into(),
                "--socket".into(),
                "/tmp/ws/run/intake.sock".into(),
                "--conversation".into(),
                "general".into(),
            ],
            env: BTreeMap::from([("BISA_LOG_DIR".to_string(), "/tmp/ws/logs".to_string())]),
            cwd: None,
        }
    }

    fn docs(headers: &[(&str, &str)]) -> McpServerConfig {
        McpServerConfig::Http {
            name: "docs".into(),
            url: "https://mcp.example.com/".into(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn a_toml_string_escapes_what_a_shell_would_not() {
        assert_eq!(toml_string("plain"), "\"plain\"");
        assert_eq!(
            toml_string("a \"quoted\" word"),
            "\"a \\\"quoted\\\" word\""
        );
        assert_eq!(toml_string("C:\\path"), "\"C:\\\\path\"");
        assert_eq!(toml_string("two\nlines"), "\"two\\nlines\"");
        assert_eq!(toml_string("bell\u{7}"), "\"bell\\u0007\"");
        assert_eq!(toml_key("BISA_LOG_DIR"), "BISA_LOG_DIR");
        assert_eq!(toml_key("odd key"), "\"odd key\"");
    }

    #[test]
    fn codex_reads_every_server_as_config_overrides_and_none_for_an_empty_list() {
        assert!(codex_overrides(&[]).is_empty());
        let args = codex_overrides(&[bisa()]);
        assert_eq!(
            args,
            vec![
                "-c",
                "mcp_servers.bisa.command=\"/usr/local/bin/bisa\"",
                "-c",
                "mcp_servers.bisa.args=[\"mcp\", \"--socket\", \"/tmp/ws/run/intake.sock\", \"--conversation\", \"general\"]",
                "-c",
                "mcp_servers.bisa.env={ BISA_LOG_DIR = \"/tmp/ws/logs\" }",
            ]
        );
        assert_eq!(
            codex_overrides(&[docs(&[])]),
            vec!["-c", "mcp_servers.docs.url=\"https://mcp.example.com/\""]
        );
        let bare = McpServerConfig::Stdio {
            name: "x".into(),
            command: "x".into(),
            args: vec![],
            env: BTreeMap::new(),
            cwd: None,
        };
        assert_eq!(
            codex_overrides(&[bare]),
            vec![
                "-c",
                "mcp_servers.x.command=\"x\"",
                "-c",
                "mcp_servers.x.args=[]"
            ],
            "no env, no env override"
        );
    }

    #[test]
    fn opencode_reads_every_server_from_its_inline_configuration() {
        let json: serde_json::Value = serde_json::from_str(&opencode_config(&[bisa()])).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"mcp": {"bisa": {
                "type": "local",
                "command": ["/usr/local/bin/bisa", "mcp", "--socket", "/tmp/ws/run/intake.sock", "--conversation", "general"],
                "environment": {"BISA_LOG_DIR": "/tmp/ws/logs"},
                "enabled": true,
            }}})
        );
        let json: serde_json::Value = serde_json::from_str(&opencode_config(&[docs(&[])])).unwrap();
        assert_eq!(
            json["mcp"]["docs"],
            serde_json::json!({"type": "remote", "url": "https://mcp.example.com/", "enabled": true})
        );
        assert_eq!(opencode_config(&[]), "{\"mcp\":{}}");
        assert_eq!(OPENCODE_CONFIG_CONTENT, "OPENCODE_CONFIG_CONTENT");
    }

    #[test]
    fn headers_cwd_and_sse_are_rendered_where_the_harness_reads_them_and_dropped_where_it_cannot() {
        let mut proc = bisa();
        if let McpServerConfig::Stdio { cwd, .. } = &mut proc {
            *cwd = Some("/srv/ws".into());
        }
        let args = codex_overrides(&[proc.clone()]);
        assert!(
            args.contains(&"mcp_servers.bisa.cwd=\"/srv/ws\"".to_string()),
            "{args:?}"
        );
        let with = docs(&[("Authorization", "Bearer t"), ("X-Tenant", "acme")]);
        let args = codex_overrides(std::slice::from_ref(&with));
        assert!(
            args.contains(&"mcp_servers.docs.http_headers={ Authorization = \"Bearer t\", X-Tenant = \"acme\" }".to_string()),
            "{args:?}"
        );
        let sse = McpServerConfig::Sse {
            name: "old".into(),
            url: "https://mcp.example.com/sse".into(),
            headers: BTreeMap::new(),
        };
        assert!(
            codex_overrides(std::slice::from_ref(&sse)).is_empty(),
            "codex has no sse transport: dropped, said in the log"
        );

        let json: serde_json::Value =
            serde_json::from_str(&opencode_config(&[proc, with, sse])).unwrap();
        assert_eq!(json["mcp"]["bisa"]["cwd"], "/srv/ws");
        assert_eq!(json["mcp"]["docs"]["headers"]["Authorization"], "Bearer t");
        assert_eq!(
            json["mcp"]["old"]["type"], "remote",
            "opencode's one remote kind covers sse"
        );
        assert_eq!(json["mcp"]["old"]["url"], "https://mcp.example.com/sse");
    }
}
