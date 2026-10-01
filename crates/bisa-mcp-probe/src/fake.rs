//! A probe that answers from a script — what a test hands the engine where
//! it expects a real one (`--features fake`, or this crate's own tests).

use crate::{McpProbe, McpProbeReport, McpProbeStage};
use bisa_core::sync::Locked;
use bisa_core::McpServerConfig;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

/// Answers the canned reports in order, the last one forever, and records
/// every transport it was asked about — so a test can assert what was
/// dialed and, more often, what was not.
#[derive(Debug, Default)]
pub struct FakeProbe {
    answers: Mutex<VecDeque<McpProbeReport>>,
    asked: Mutex<Vec<McpServerConfig>>,
}

impl FakeProbe {
    pub fn answering(reports: Vec<McpProbeReport>) -> Self {
        Self {
            answers: Mutex::new(reports.into_iter().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// A report that says a server answered in the discover era with `tools` tools.
    pub fn ok_report(transport: &str, tools: usize) -> McpProbeReport {
        McpProbeReport {
            ok: true,
            transport: transport.to_string(),
            era: Some(crate::McpEra::Discover),
            protocol_version: Some("2026-07-28".into()),
            server: Some(crate::McpServerInfo {
                name: "scripted".into(),
                version: "0.1.0".into(),
                title: None,
            }),
            capabilities: crate::McpCapabilities {
                tools: true,
                ..Default::default()
            },
            instructions: None,
            tools: (0..tools.min(crate::MAX_TOOLS))
                .map(|i| crate::McpToolSummary {
                    name: format!("tool_{i}"),
                    description: None,
                })
                .collect(),
            tool_count: tools,
            resource_count: None,
            prompt_count: None,
            elapsed_ms: 3,
            stage: McpProbeStage::Done,
            error: None,
        }
    }

    /// Every transport this fake was asked to dial, in order.
    pub fn asked(&self) -> Vec<McpServerConfig> {
        self.asked.locked().clone()
    }
}

#[async_trait::async_trait]
impl McpProbe for FakeProbe {
    async fn probe(&self, config: &McpServerConfig, _budget: Duration) -> McpProbeReport {
        self.asked.locked().push(config.clone());
        let mut answers = self.answers.locked();
        match answers.len() {
            0 => McpProbeReport::failed(
                config,
                McpProbeStage::Spawn,
                "the fake has no answer",
                Duration::ZERO,
            ),
            1 => answers[0].clone(),
            _ => answers.pop_front().expect("one answer"),
        }
    }
}
