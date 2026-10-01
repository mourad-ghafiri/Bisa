//! MCP bridge between harness sessions and the Bisa engine.
//!
//! The engine injects this server into every harness session:
//! `bisa mcp --socket <path> --work-item <ulid>` for worker sessions,
//! `bisa mcp --socket <path> --goal <ulid>` for guided sessions.
//! Tools bridge to the engine's intake unix socket; the tool set differs by
//! scope (see `server.rs`). The agent a session speaks and remembers as is
//! the scope's (`--agent`); a worker's is resolved engine-side from its
//! work item.
//!
//! The General Agent and the Workflow Agent get more, in whichever scope they run: both read the
//! workspace (`workspace_overview`, `list_catalog`); the General Agent
//! ([`CORE_AGENT_ID`]) staffs it and captures what recurs
//! (`install_catalog_entry`, `assign`, `capture_goal`); the Workflow Agent
//! ([`WORKFLOW_AGENT_ID`]) reads and validates workflows and, in a goal,
//! proposes and amends them. Nothing else does.

pub mod client;
pub mod error_text;
pub mod server;

pub use client::{
    Decision, GuardedWrite, IntakeClient, IntakeError, NewProjectRequest, Proposed, Raised, Saved,
    Scope, StandingGoal, SubmitOutcome,
};
pub use server::{
    is_core_agent, ArtifactParam, AskHumanParams, AskOptionParam, BisaServer, ToolCore,
    CORE_AGENT_ID, CORE_AGENT_IDS, WORKFLOW_AGENT_ID,
};

use std::path::PathBuf;

/// Serve MCP over stdio for one scope (work item or goal). Returns when
/// the client (the harness) closes the transport.
pub async fn run_stdio(socket_path: PathBuf, scope: Scope) -> anyhow::Result<()> {
    use rmcp::ServiceExt;
    let service = BisaServer::new(socket_path, scope)
        .serve(rmcp::transport::io::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
