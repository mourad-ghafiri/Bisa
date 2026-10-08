//! What the installed MCP servers answered when last checked (06 § MCP
//! servers): in memory, per server, for this engine's lifetime — a health
//! is a fact about a moment, not a record — and a probe on ask alone: no
//! schedule, no dial at start. One probe per server at a time; a second ask
//! while one runs joins it rather than opening a second process against the
//! same command. An edit or a removal forgets the last answer.
//!
//! The dial itself is `bisa-mcp-probe`'s ([`McpProbe`]); this module owns
//! the rules around it: a disabled server is not dialed, the reserved name
//! is not dialed, a draft transport (the editor's *Test connection*) is
//! dialed without a registry entry, and the bus hears every finished probe.
//! A probe whose caller went away before it answered — a closed window, a
//! deadline above the call — leaves no slot behind for the next ask to wait
//! on: a runner gone without an answer is taken over, never joined.

use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::sync::Locked;
use bisa_core::{McpId, McpServerConfig, RESERVED_MCP_NAME};
use bisa_mcp_probe::{budget_of, McpProbe, McpProbeReport, McpProbeStage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

/// What is known about one server: the last report and when it came.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct LastProbe {
    pub report: McpProbeReport,
    pub checked_at: u64,
}

/// Where a server's health stands: nothing yet, fine, or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpHealthState {
    Unknown,
    Ok,
    Failing,
}

impl McpHealthState {
    /// The word the wire and the command line say.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Ok => "ok",
            Self::Failing => "failing",
        }
    }
}

/// A server's health as the registry answers it: nothing yet, fine, or not.
/// Read back as well as written — the command line takes the node's view of
/// a server whole (`dto::McpServerView`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpHealthView {
    pub state: McpHealthState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub era: Option<bisa_mcp_probe::McpEra>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<bisa_mcp_probe::McpServerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<McpProbeStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl McpHealthView {
    pub fn unknown() -> Self {
        Self {
            state: McpHealthState::Unknown,
            checked_at: None,
            era: None,
            protocol_version: None,
            server: None,
            tool_count: None,
            stage: None,
            error: None,
        }
    }

    pub fn of(last: &LastProbe) -> Self {
        let r = &last.report;
        Self {
            state: if r.ok {
                McpHealthState::Ok
            } else {
                McpHealthState::Failing
            },
            checked_at: Some(last.checked_at),
            era: r.era,
            protocol_version: r.protocol_version.clone(),
            server: r.server.clone(),
            tool_count: r.ok.then_some(r.tool_count),
            stage: (!r.ok).then_some(r.stage),
            error: r.error.clone(),
        }
    }
}

/// The sentence a probe of a server that must not be dialed answers.
pub const DISABLED: &str = "the server is disabled — enable it to check it";
pub const RESERVED: &str = "the reserved name is never dialed";

enum Slot {
    /// A probe is running; the receiver resolves when it finishes.
    Running(watch::Receiver<Option<McpProbeReport>>),
    /// Boxed: a report is many times the receiver's size, and a map of
    /// slots should not pay the largest variant's price for every server.
    Done(Box<LastProbe>),
}

pub struct McpHealth {
    slots: Mutex<HashMap<McpId, Slot>>,
    probe: Arc<dyn McpProbe>,
}

/// The runner's hold on its slot: let go without an answer — the future
/// cancelled mid-probe — it takes the `Running` slot away, so the next ask
/// becomes the runner instead of waiting on nobody.
struct Runner<'a> {
    slots: &'a Mutex<HashMap<McpId, Slot>>,
    id: McpId,
    done: bool,
}

impl Drop for Runner<'_> {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        let mut slots = self.slots.locked();
        if matches!(slots.get(&self.id), Some(Slot::Running(_))) {
            slots.remove(&self.id);
        }
    }
}

impl McpHealth {
    pub fn new(probe: Arc<dyn McpProbe>) -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            probe,
        }
    }

    /// The last answer for a server, if one was ever asked for.
    pub fn health_of(&self, id: &McpId) -> Option<LastProbe> {
        match self.slots.locked().get(id) {
            Some(Slot::Done(last)) => Some(last.as_ref().clone()),
            _ => None,
        }
    }

    /// As the registry answers it.
    pub fn view_of(&self, id: &McpId) -> McpHealthView {
        self.health_of(id)
            .map(|l| McpHealthView::of(&l))
            .unwrap_or_else(McpHealthView::unknown)
    }

    /// Forget what a server answered — its shape moved, or it is gone.
    pub fn invalidate(&self, id: &McpId) {
        self.slots.locked().remove(id);
    }

    /// Dial a transport that is nobody's registry entry yet — the editor's
    /// *Test connection* before *Save* — and answer the report; nothing is kept.
    pub async fn probe_transport(
        &self,
        config: &McpServerConfig,
        budget_secs: Option<u64>,
    ) -> McpProbeReport {
        if config.name() == RESERVED_MCP_NAME {
            return McpProbeReport::failed(config, McpProbeStage::Spawn, RESERVED, Duration::ZERO);
        }
        self.probe.probe(config, budget_of(budget_secs)).await
    }

    /// Dial a registered server, keep the answer, and tell the bus. A probe
    /// already running for this id is joined, not doubled; a disabled server
    /// is answered in words and never dialed.
    pub async fn probe_mcp(
        &self,
        inner: &Inner,
        id: &McpId,
        budget_secs: Option<u64>,
    ) -> Result<McpProbeReport, EngineError> {
        let def = inner.ws.get_mcp(id)?;
        if !def.enabled {
            return Ok(McpProbeReport::failed(
                &def.transport,
                McpProbeStage::Spawn,
                DISABLED,
                Duration::ZERO,
            ));
        }
        if def.transport.name() == RESERVED_MCP_NAME {
            return Ok(McpProbeReport::failed(
                &def.transport,
                McpProbeStage::Spawn,
                RESERVED,
                Duration::ZERO,
            ));
        }
        // Join a probe already running for this id, or become its runner. A
        // runner that went away without an answer — its sender gone — is
        // taken over, never joined. The decision is taken under the lock and
        // the lock is let go before any await: a guard held across the probe
        // would keep every other id's probe waiting and make the future
        // `!Send`.
        let role = {
            let mut slots = self.slots.locked();
            match slots.get(id) {
                Some(Slot::Running(rx)) if rx.has_changed().is_ok() => Err(rx.clone()),
                _ => {
                    let (tx, rx) = watch::channel(None);
                    slots.insert(id.clone(), Slot::Running(rx));
                    Ok(tx)
                }
            }
        };
        let mut rx = match role {
            Ok(tx) => {
                let mut runner = Runner {
                    slots: &self.slots,
                    id: id.clone(),
                    done: false,
                };
                let report = self
                    .probe
                    .probe(&def.transport, budget_of(budget_secs))
                    .await;
                let last = LastProbe {
                    report: report.clone(),
                    checked_at: now_secs(),
                };
                self.slots
                    .locked()
                    .insert(id.clone(), Slot::Done(Box::new(last)));
                runner.done = true;
                // A joiner that stopped listening is not a fault.
                if tx.send(Some(report.clone())).is_err() {
                    tracing::debug!(target: "bisa_engine::mcp_health", %id, "nobody waited for the probe");
                }
                tracing::info!(
                    target: "bisa_engine::mcp_health",
                    %id,
                    transport = %report.transport,
                    era = ?report.era,
                    stage = ?report.stage,
                    ok = report.ok,
                    elapsed_ms = report.elapsed_ms,
                    "mcp server probed"
                );
                inner.emit(EngineEvent::global(EnginePayload::McpProbed {
                    id: id.clone(),
                    ok: report.ok,
                }));
                return Ok(report);
            }
            Err(rx) => rx,
        };
        loop {
            if let Some(r) = rx.borrow().clone() {
                return Ok(r);
            }
            if rx.changed().await.is_err() {
                // The runner went away without an answer: read what it kept.
                return Ok(self.health_of(id).map(|l| l.report).unwrap_or_else(|| {
                    McpProbeReport::failed(
                        &def.transport,
                        McpProbeStage::Spawn,
                        "the probe was dropped",
                        Duration::ZERO,
                    )
                }));
            }
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
