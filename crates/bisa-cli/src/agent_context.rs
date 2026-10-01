//! `bisa agent-context`: what an agent session can rely on — the tools
//! each kind of session is handed, and the framing a chat session reads —
//! from the same source the MCP server serves, so a prompt cannot drift from
//! the truth and an agent at a terminal can ask rather than be told.

use crate::output::Out;
use anyhow::Result;
use bisa_mcp::client::Scope;
use bisa_mcp::BisaServer;
use serde_json::json;
use std::path::PathBuf;

fn manifest(scope: Scope) -> Vec<(String, String)> {
    // The server opens no socket until a tool is called; a path that does not
    // exist is enough to build the router.
    BisaServer::new(PathBuf::from("/nonexistent/agent-context.sock"), scope).tool_manifest()
}

pub fn run(out: &Out) -> Result<()> {
    let sessions: Vec<(&str, Scope)> = vec![
        ("work_item", Scope::WorkItem("01WORKITEM".to_string())),
        (
            "goal",
            Scope::Goal {
                goal: "01GOAL".to_string(),
                agent: Some("developer".to_string()),
            },
        ),
        (
            "conversation",
            Scope::Conversation {
                scope: "01SCOPE".to_string(),
                agent: "developer".to_string(),
                goal: None,
            },
        ),
    ];
    let mut sets = serde_json::Map::new();
    for (name, scope) in sessions {
        let tools = manifest(scope);
        out.human(&format!("\n== {name} session: {} tools ==", tools.len()));
        for (n, d) in &tools {
            let first = d.split(['.', '\n']).next().unwrap_or("").trim();
            out.human(&format!("  {n:<26} {first}"));
        }
        sets.insert(
            name.to_string(),
            json!(tools
                .into_iter()
                .map(|(n, d)| json!({"name": n, "description": d}))
                .collect::<Vec<_>>()),
        );
    }
    out.say(&bisa_core::text!("cli-agent-context-chat-framing"));
    out.human(&bisa_engine::conversation::conversation_framing());
    out.say(&bisa_core::text!("cli-agent-context-project-thread-frame"));
    out.human(&bisa_engine::framing::project_frame(
        "web-app",
        &[bisa_i18n::say(&bisa_core::text!(
            "cli-agent-context-dark-mode"
        ))],
        None,
    ));
    out.human(&bisa_engine::framing::project_frame("web-app", &[], None));
    out.json_value(json!({
        "tools": sets,
        "chat_framing": bisa_engine::conversation::conversation_framing(),
        "project_frame": {
            "attached": bisa_engine::framing::project_frame("web-app", &[bisa_i18n::say(&bisa_core::text!("cli-agent-context-dark-mode"))], None),
            "standalone": bisa_engine::framing::project_frame("web-app", &[], None),
        },
    }));
    Ok(())
}
