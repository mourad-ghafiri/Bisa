//! An MCP server from a script, for the probe's tests: JSON-RPC over stdio,
//! one message per line. The mode is `argv[1]`:
//!
//! - `discover` — the discover era (2026-07-28): `server/discover` answers,
//!   `initialize` is refused, `tools/list` answers two tools.
//! - `handshake:<revision>` — the handshake era: `server/discover` is
//!   *method not found* so a client falls back, `initialize` answers with
//!   `<revision>`, `ping` answers, `tools/list` answers two tools.
//! - `hang` — reads and never answers.
//! - `exit` — writes a line to stderr and exits with status 3 at once.
//!
//! Nothing here reads a file or a network; only stdin and stdout.

use std::io::{BufRead, Write};

fn write(out: &mut impl Write, v: &serde_json::Value) {
    writeln!(out, "{v}").expect("the probe reads stdout");
    out.flush().expect("the probe reads stdout");
}

fn tools() -> serde_json::Value {
    serde_json::json!({"tools": [
        {"name": "search_docs", "description": "Search the documentation", "inputSchema": {"type": "object"}},
        {"name": "read_page", "description": "Read one page", "inputSchema": {"type": "object"}}
    ]})
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "handshake:2025-06-18".to_string());
    if mode == "exit" {
        eprintln!("scripted-mcp: refusing to start: no API key");
        std::process::exit(3);
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let discover = mode == "discover";
    let revision = mode
        .strip_prefix("handshake:")
        .unwrap_or("2025-06-18")
        .to_string();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        if mode == "hang" {
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let method = msg["method"].as_str().unwrap_or_default().to_string();
        let id = msg.get("id").cloned();
        let Some(id) = id else {
            // A notification — `notifications/initialized`, a cancel — takes no answer.
            continue;
        };
        let error = |code: i64, text: &str| serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": text}});
        match method.as_str() {
            "server/discover" if discover => write(
                &mut out,
                // The discover result as the 2026-07-28 revision spells it
                // (rmcp's `DiscoverResult`): a `ttlMs` and a `cacheScope` are
                // required fields — without them a client reads the answer
                // as some other result and the handshake fails.
                &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {
                    "resultType": "complete",
                    "supportedVersions": ["2026-07-28"],
                    "capabilities": {"tools": {}},
                    "instructions": "A scripted server for the probe.",
                    "ttlMs": 0,
                    "cacheScope": "private",
                    "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "scripted-mcp", "version": "0.1.0", "title": "Scripted"}}
                }}),
            ),
            "server/discover" => write(&mut out, &error(-32601, "Method not found")),
            "initialize" if !discover => write(
                &mut out,
                &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": revision,
                    "capabilities": {"tools": {}, "logging": {}},
                    "serverInfo": {"name": "scripted-mcp", "version": "0.1.0"},
                    "instructions": "A scripted server for the probe."
                }}),
            ),
            "initialize" => write(&mut out, &error(-32601, "Method not found")),
            "ping" => write(
                &mut out,
                &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": {}}),
            ),
            "tools/list" => write(
                &mut out,
                &serde_json::json!({"jsonrpc": "2.0", "id": id, "result": tools()}),
            ),
            _ => write(&mut out, &error(-32601, "Method not found")),
        }
    }
}
