//! Dialing an installed MCP server to see what answers (06 § MCP servers).
//!
//! The platform's own server (`bisa-mcp`) is the *server* every session is
//! handed; this crate is the *client* that checks a server a person
//! registered — over its own transport, in whichever protocol era it speaks
//! — and reports back: who answered, which revision was negotiated, what it
//! can do, which tools it offers, and, when it did not answer, how far the
//! conversation got. It never calls a tool and never keeps a connection.
//!
//! Two eras (modelcontextprotocol.io/specification/versioning): the
//! **handshake era** — `2024-11-05` through `2025-11-25`, `initialize` →
//! `notifications/initialized` → `ping` — and the **discover era** —
//! `2026-07-28`, stateless, one `server/discover` call. `rmcp`'s client
//! negotiates both (`ClientLifecycleMode::Auto`) over stdio and Streamable
//! HTTP; the 2024-11-05 HTTP+SSE transport, which `rmcp` deliberately does
//! not ship, is [`sse::LegacySseClient`]'s, small and ours.
//!
//! What a report may carry is bounded — the instructions, the tool list —
//! and what an error may carry is sanitised: never a header or an
//! environment value, never a URL's userinfo.

use bisa_core::McpServerConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod fake;
mod rmcp_probe;
mod sanitize;
pub mod sse;

pub use rmcp_probe::RmcpProbe;

/// How long a probe waits for the whole conversation when the caller names
/// no budget.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(10);
/// The most a caller may ask a probe to wait.
pub const MAX_BUDGET: Duration = Duration::from_secs(30);
/// How many tools a report carries by name; the count is the true count.
pub const MAX_TOOLS: usize = 50;
/// How much of a server's `instructions` a report keeps.
pub const MAX_INSTRUCTIONS_CHARS: usize = 2_000;
/// How much of a stdio server's stderr a spawn failure quotes.
pub const MAX_STDERR_BYTES: usize = 4 * 1024;

/// A budget clamped to what a probe may wait: at least a second, at most
/// [`MAX_BUDGET`]; `None` is [`DEFAULT_BUDGET`].
pub fn budget_of(secs: Option<u64>) -> Duration {
    match secs {
        None => DEFAULT_BUDGET,
        Some(s) => Duration::from_secs(s.clamp(1, MAX_BUDGET.as_secs())),
    }
}

/// Which lifecycle the server answered in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpEra {
    /// `initialize` / `notifications/initialized` / `ping` — `2024-11-05` through `2025-11-25`.
    Handshake,
    /// `server/discover`, stateless — `2026-07-28` onwards.
    Discover,
}

impl McpEra {
    /// The era a negotiated revision belongs to.
    pub fn of_version(version: &str) -> Self {
        if version >= "2026-07-28" {
            McpEra::Discover
        } else {
            McpEra::Handshake
        }
    }
}

/// How far the conversation got before it stopped — what a failure is *of*.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum McpProbeStage {
    /// The process would not start, or the URL would not connect.
    Spawn,
    /// The transport is up; the lifecycle — `server/discover` or `initialize` — did not complete.
    Initialize,
    /// The handshake era's `ping` went unanswered.
    Ping,
    /// `tools/list` (and the other lists) did not answer.
    Tools,
    /// Every step answered.
    Done,
}

impl McpProbeStage {
    /// The stage's word for a person, said about a failure there.
    pub fn failure_words(self) -> &'static str {
        match self {
            McpProbeStage::Spawn => "could not start the command or reach the URL",
            McpProbeStage::Initialize => {
                "reached the server, but the MCP handshake did not complete"
            }
            McpProbeStage::Ping => "answered the handshake, but not a ping",
            McpProbeStage::Tools => "answered the handshake, but not the tool list",
            McpProbeStage::Done => "answered",
        }
    }
}

/// Who answered.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// What the server said it can do.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct McpCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
    pub logging: bool,
    pub completions: bool,
}

/// One tool, by name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct McpToolSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// What a probe came back with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct McpProbeReport {
    pub ok: bool,
    /// `stdio` · `http` · `sse`.
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub era: Option<McpEra>,
    /// The revision the server negotiated — its word, not ours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<McpServerInfo>,
    #[serde(default)]
    pub capabilities: McpCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// The first [`MAX_TOOLS`] tools; `tool_count` is every one.
    #[serde(default)]
    pub tools: Vec<McpToolSummary>,
    pub tool_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_count: Option<usize>,
    pub elapsed_ms: u64,
    pub stage: McpProbeStage,
    /// Why it stopped — sanitised: no header or environment value, no URL userinfo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl McpProbeReport {
    /// A report that never got anywhere — the shape every failure starts from.
    pub fn failed(
        config: &McpServerConfig,
        stage: McpProbeStage,
        error: impl Into<String>,
        elapsed: Duration,
    ) -> Self {
        Self {
            ok: false,
            transport: config.kind().to_string(),
            era: None,
            protocol_version: None,
            server: None,
            capabilities: McpCapabilities::default(),
            instructions: None,
            tools: Vec::new(),
            tool_count: 0,
            resource_count: None,
            prompt_count: None,
            elapsed_ms: elapsed.as_millis() as u64,
            stage,
            error: Some(sanitize::sanitize(&error.into(), config)),
        }
    }

    /// The words a person reads for a report: *server 1.4.2 · 2026-07-28 · 12 tools*, or the stage's failure.
    pub fn words(&self) -> String {
        if !self.ok {
            return match &self.error {
                Some(e) => format!("{}: {e}", self.stage.failure_words()),
                None => self.stage.failure_words().to_string(),
            };
        }
        let mut parts = Vec::new();
        if let Some(s) = &self.server {
            parts.push(if s.version.is_empty() {
                s.name.clone()
            } else {
                format!("{} {}", s.name, s.version)
            });
        }
        if let Some(v) = &self.protocol_version {
            parts.push(v.clone());
        }
        parts.push(format!(
            "{} {}",
            self.tool_count,
            if self.tool_count == 1 {
                "tool"
            } else {
                "tools"
            }
        ));
        parts.join(" · ")
    }
}

/// Dial a server and report — the port the engine and the CLI speak to; the
/// real one is [`RmcpProbe`], a test's is [`fake::FakeProbe`].
#[async_trait::async_trait]
pub trait McpProbe: Send + Sync + std::fmt::Debug {
    async fn probe(&self, config: &McpServerConfig, budget: Duration) -> McpProbeReport;
}

/// A bounded copy of a text: the first `max` characters and a mark.
pub(crate) fn bounded(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}… (cut)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_is_clamped_and_defaults() {
        assert_eq!(budget_of(None), DEFAULT_BUDGET);
        assert_eq!(budget_of(Some(0)), Duration::from_secs(1));
        assert_eq!(budget_of(Some(5)), Duration::from_secs(5));
        assert_eq!(budget_of(Some(900)), MAX_BUDGET);
    }

    #[test]
    fn the_era_follows_the_revision_and_the_words_follow_the_report() {
        assert_eq!(McpEra::of_version("2025-06-18"), McpEra::Handshake);
        assert_eq!(McpEra::of_version("2026-07-28"), McpEra::Discover);
        assert_eq!(McpEra::of_version("2027-01-01"), McpEra::Discover);
        let config = McpServerConfig::Http {
            name: "docs".into(),
            url: "https://x.test/mcp".into(),
            headers: Default::default(),
        };
        let mut r = McpProbeReport::failed(
            &config,
            McpProbeStage::Spawn,
            "connection refused",
            Duration::from_millis(12),
        );
        assert_eq!(
            r.words(),
            "could not start the command or reach the URL: connection refused"
        );
        assert_eq!(r.transport, "http");
        r.ok = true;
        r.error = None;
        r.server = Some(McpServerInfo {
            name: "docs".into(),
            version: "1.4.2".into(),
            title: None,
        });
        r.protocol_version = Some("2026-07-28".into());
        r.tool_count = 12;
        assert_eq!(r.words(), "docs 1.4.2 · 2026-07-28 · 12 tools");
        r.tool_count = 1;
        assert!(r.words().ends_with("1 tool"));
        assert_eq!(bounded("abcdef", 3), "abc… (cut)");
        assert_eq!(bounded("ab", 3), "ab");
    }
}
