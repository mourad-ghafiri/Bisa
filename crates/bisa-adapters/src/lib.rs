//! Concrete harness adapters for Bisa.
//!
//! One module + one cargo feature per adapter (all on by default):
//! `claude-code`, `codex`, `acp` (the Agent Client Protocol, and the generic
//! ACP agents), `pi-rpc`, `omp`, `opencode`, `copilot` (GitHub Copilot CLI)
//! and `grok` (Grok Build) — two harnesses that speak ACP, each its own
//! facts over [`acp::open`] — and `custom-json`. Shared subprocess/session
//! machinery lives in [`util`] and [`oneshot`]; the pi wire mapping shared by
//! pi/omp in [`pi_wire`].

pub mod hooks;
pub mod mcp_inject;
pub mod oneshot;
pub mod usage;
pub mod util;

#[cfg(feature = "a2a")]
pub mod a2a;
#[cfg(feature = "acp")]
pub mod acp;
#[cfg(feature = "claude-code")]
pub mod claude_code;
#[cfg(feature = "codex")]
pub mod codex;
#[cfg(feature = "copilot")]
pub mod copilot;
#[cfg(feature = "custom-json")]
pub mod custom_json;
#[cfg(feature = "grok")]
pub mod grok;
#[cfg(feature = "omp")]
pub mod omp;
#[cfg(feature = "opencode")]
pub mod opencode;
#[cfg(feature = "pi-rpc")]
pub mod pi_rpc;
#[cfg(feature = "pi-rpc")]
pub mod pi_wire;

use std::sync::Arc;

use bisa_harness::HarnessCatalog;

/// Register every enabled compiled-in adapter, plus one [`acp::AcpAdapter`]
/// per known ACP-speaking target, plus a [`custom_json::CustomJsonAdapter`]
/// for each descriptor already loaded into the catalog's custom tier.
///
/// `http` is the engine's clients: every adapter that reaches the network
/// itself — a harness account's usage endpoint, an A2A peer — goes through
/// it, under the proxy and the HTTP version the `network.*` settings name.
pub fn register_all(catalog: &mut HarnessCatalog, http: Arc<bisa_http::Clients>) {
    #[cfg(feature = "claude-code")]
    catalog.register(Arc::new(claude_code::ClaudeCodeAdapter {
        http: Arc::clone(&http),
        ..Default::default()
    }));
    #[cfg(feature = "codex")]
    catalog.register(Arc::new(codex::CodexAdapter::default()));
    #[cfg(feature = "pi-rpc")]
    catalog.register(Arc::new(pi_rpc::PiRpcAdapter::default()));
    #[cfg(feature = "omp")]
    catalog.register(Arc::new(omp::OmpAdapter::default()));
    #[cfg(feature = "opencode")]
    catalog.register(Arc::new(opencode::OpencodeAdapter {
        http: Arc::clone(&http),
        ..Default::default()
    }));
    #[cfg(feature = "copilot")]
    catalog.register(Arc::new(copilot::CopilotAdapter::default()));
    #[cfg(feature = "grok")]
    catalog.register(Arc::new(grok::GrokAdapter::default()));
    #[cfg(feature = "acp")]
    {
        // Known ACP targets, PATH-probed like any adapter. `omp acp` gives a
        // second, ACP-shaped route into omp.
        catalog.register(Arc::new(acp::AcpAdapter::new(
            "acp:goose",
            "Goose (ACP)",
            "goose",
            vec!["acp".into()],
        )));
        catalog.register(Arc::new(acp::AcpAdapter::new(
            "acp:cursor-agent",
            "Cursor Agent (ACP)",
            "cursor-agent",
            vec!["acp".into()],
        )));
        catalog.register(Arc::new(acp::AcpAdapter::new(
            "acp:omp",
            "Oh My Pi (ACP)",
            "omp",
            vec!["acp".into()],
        )));
        // `opencode acp` — its ACP server over stdio, a second route into
        // OpenCode beside `opencode run`.
        catalog.register(Arc::new(acp::AcpAdapter::new(
            "acp:opencode",
            "OpenCode (ACP)",
            "opencode",
            vec!["acp".into()],
        )));
    }
    #[cfg(feature = "a2a")]
    catalog.register_resolver(a2a::A2aAdapter::resolver(http));
    #[cfg(feature = "custom-json")]
    {
        let specs: Vec<_> = catalog.custom_specs().to_vec();
        for spec in specs {
            catalog.register(Arc::new(custom_json::CustomJsonAdapter::new(spec)));
        }
    }
}
