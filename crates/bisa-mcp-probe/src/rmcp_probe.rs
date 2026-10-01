//! The real probe: `rmcp`'s client over stdio and Streamable HTTP, in
//! whichever era the server speaks (`ClientLifecycleMode::Auto`: try
//! `server/discover`, fall back to `initialize`), and the legacy HTTP+SSE
//! transport through [`crate::sse`]. One budget for the whole conversation;
//! the stage the conversation reached is the report's when it stops.

use crate::{
    bounded, sanitize, McpCapabilities, McpEra, McpProbe, McpProbeReport, McpProbeStage,
    McpServerInfo, McpToolSummary, MAX_INSTRUCTIONS_CHARS, MAX_STDERR_BYTES, MAX_TOOLS,
};
use bisa_core::sync::Locked;
use bisa_core::McpServerConfig;
use rmcp::model::{
    ClientRequest, Implementation, InitializeRequestParams, PingRequest, ProtocolVersion,
};
use rmcp::service::{ClientLifecycleMode, ClientServiceExt, RoleClient, RunningService};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;

/// Dials a server with `rmcp` (stdio, Streamable HTTP) or the legacy SSE
/// client, reads what it says, and hangs up.
#[derive(Debug, Default, Clone)]
pub struct RmcpProbe;

/// The client this probe introduces itself as.
fn client_info() -> Implementation {
    let mut info = Implementation::from_build_env();
    info.name = "bisa".to_string();
    info.version = env!("CARGO_PKG_VERSION").to_string();
    info.title = Some("Bisa MCP probe".to_string());
    info
}

/// The revisions offered: the discover era first, the newest handshake
/// revision as the fallback — the server answers with what it speaks.
fn lifecycle() -> ClientLifecycleMode {
    ClientLifecycleMode::Auto {
        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        legacy_version: Some(ProtocolVersion::V_2025_11_25),
    }
}

/// Where the conversation stands, shared with the timeout so an elapsed
/// budget names the stage it ran out in.
type StageCell = Arc<Mutex<McpProbeStage>>;

fn set_stage(cell: &StageCell, stage: McpProbeStage) {
    *cell.locked() = stage;
}

/// The last [`MAX_STDERR_BYTES`] of what a child wrote to stderr.
#[derive(Clone, Default)]
struct StderrTail(Arc<Mutex<Vec<u8>>>);

impl StderrTail {
    fn follow(&self, mut stderr: tokio::process::ChildStderr) {
        let tail = Arc::clone(&self.0);
        tokio::spawn(async move {
            let mut chunk = [0u8; 1024];
            loop {
                match stderr.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        let mut buf = tail.locked();
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.len() > MAX_STDERR_BYTES {
                            let cut = buf.len() - MAX_STDERR_BYTES;
                            buf.drain(..cut);
                        }
                    }
                }
            }
        });
    }

    fn words(&self) -> Option<String> {
        let buf = self.0.locked();
        let text = String::from_utf8_lossy(&buf).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

type Client = RunningService<RoleClient, InitializeRequestParams>;

#[async_trait::async_trait]
impl McpProbe for RmcpProbe {
    async fn probe(&self, config: &McpServerConfig, budget: Duration) -> McpProbeReport {
        let started = Instant::now();
        let stage: StageCell = Arc::new(Mutex::new(McpProbeStage::Spawn));
        let outcome = tokio::time::timeout(budget, run(config, Arc::clone(&stage), started)).await;
        let report = match outcome {
            Ok(Ok(report)) => report,
            Ok(Err((stage, error))) => {
                McpProbeReport::failed(config, stage, error, started.elapsed())
            }
            Err(_) => {
                let at = *stage.locked();
                McpProbeReport::failed(
                    config,
                    at,
                    format!("no answer within {} s", budget.as_secs()),
                    started.elapsed(),
                )
            }
        };
        tracing::info!(
            target: "bisa_mcp_probe",
            transport = %report.transport,
            era = ?report.era,
            stage = ?report.stage,
            ok = report.ok,
            elapsed_ms = report.elapsed_ms,
            "mcp probe finished"
        );
        report
    }
}

/// The conversation, step by step; an error names the stage it happened in.
async fn run(
    config: &McpServerConfig,
    stage: StageCell,
    started: Instant,
) -> Result<McpProbeReport, (McpProbeStage, String)> {
    match config {
        McpServerConfig::Sse { url, headers, .. } => {
            let http = bisa_http::Clients::shared().outbound();
            crate::sse::probe(&http, url, headers, config, Arc::clone(&stage), started).await
        }
        McpServerConfig::Stdio {
            command,
            args,
            env,
            cwd,
            ..
        } => {
            let mut cmd = tokio::process::Command::new(command);
            cmd.args(args).envs(env);
            if let Some(dir) = cwd {
                cmd.current_dir(dir);
            }
            let tail = StderrTail::default();
            let (process, stderr) = TokioChildProcess::builder(cmd)
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| {
                    (
                        McpProbeStage::Spawn,
                        format!("could not start `{command}`: {e}"),
                    )
                })?;
            if let Some(stderr) = stderr {
                tail.follow(stderr);
            }
            set_stage(&stage, McpProbeStage::Initialize);
            let client = connect(process).await.map_err(|e| {
                let why = match tail.words() {
                    Some(said) => format!("{e} — the server wrote: {said}"),
                    None => e,
                };
                (McpProbeStage::Initialize, why)
            })?;
            converse(client, config, stage, started).await
        }
        McpServerConfig::Http { url, headers, .. } => {
            let mut custom = HashMap::new();
            for (k, v) in headers {
                let name = reqwest::header::HeaderName::from_bytes(k.as_bytes())
                    .map_err(|e| (McpProbeStage::Spawn, format!("header `{k}`: {e}")))?;
                let value = reqwest::header::HeaderValue::from_str(v)
                    .map_err(|e| (McpProbeStage::Spawn, format!("header `{k}`: {e}")))?;
                custom.insert(name, value);
            }
            let mut transport_config =
                StreamableHttpClientTransportConfig::with_uri(url.as_str()).custom_headers(custom);
            // A discover-era server has no session; a handshake-era one may.
            transport_config.allow_stateless = true;
            let http = bisa_http::Clients::shared().outbound();
            let transport =
                StreamableHttpClientTransport::with_client((*http).clone(), transport_config);
            set_stage(&stage, McpProbeStage::Initialize);
            let client = connect(transport)
                .await
                .map_err(|e| (McpProbeStage::Initialize, e))?;
            converse(client, config, stage, started).await
        }
    }
}

/// The lifecycle — discover, or the handshake — over any transport.
async fn connect<T, E, A>(transport: T) -> Result<Client, String>
where
    T: rmcp::transport::IntoTransport<RoleClient, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut params = InitializeRequestParams::default();
    params.client_info = client_info();
    params.protocol_version = ProtocolVersion::V_2025_11_25;
    params
        .serve_with_lifecycle(transport, lifecycle())
        .await
        .map_err(|e| e.to_string())
}

/// What the server said, then the lists it advertised, then goodbye.
async fn converse(
    client: Client,
    config: &McpServerConfig,
    stage: StageCell,
    started: Instant,
) -> Result<McpProbeReport, (McpProbeStage, String)> {
    let info = client.peer_info().ok_or((
        McpProbeStage::Initialize,
        "the server answered no capabilities".to_string(),
    ))?;
    let protocol_version = info.protocol_version.as_str().to_string();
    let era = McpEra::of_version(&protocol_version);
    let capabilities = McpCapabilities {
        tools: info.capabilities.tools.is_some(),
        resources: info.capabilities.resources.is_some(),
        prompts: info.capabilities.prompts.is_some(),
        logging: info.capabilities.logging.is_some(),
        completions: info.capabilities.completions.is_some(),
    };
    let server = info.server_info.as_ref().map(|s| McpServerInfo {
        name: s.name.clone(),
        version: s.version.clone(),
        title: s.title.clone(),
    });
    let instructions = info
        .instructions
        .as_deref()
        .map(|i| bounded(i, MAX_INSTRUCTIONS_CHARS));

    // The handshake era has a ping; the discover era removed it.
    if era == McpEra::Handshake {
        set_stage(&stage, McpProbeStage::Ping);
        client
            .send_request(ClientRequest::PingRequest(PingRequest::default()))
            .await
            .map_err(|e| (McpProbeStage::Ping, e.to_string()))?;
    }

    set_stage(&stage, McpProbeStage::Tools);
    let (tools, tool_count) = if capabilities.tools {
        let all = client
            .list_all_tools()
            .await
            .map_err(|e| (McpProbeStage::Tools, e.to_string()))?;
        let count = all.len();
        let named = all
            .into_iter()
            .take(MAX_TOOLS)
            .map(|t| McpToolSummary {
                name: t.name.to_string(),
                description: t.description.as_ref().map(|d| bounded(d, 200)),
            })
            .collect();
        (named, count)
    } else {
        (Vec::new(), 0)
    };
    let resource_count = if capabilities.resources {
        Some(
            client
                .list_all_resources()
                .await
                .map_err(|e| (McpProbeStage::Tools, e.to_string()))?
                .len(),
        )
    } else {
        None
    };
    let prompt_count = if capabilities.prompts {
        Some(
            client
                .list_all_prompts()
                .await
                .map_err(|e| (McpProbeStage::Tools, e.to_string()))?
                .len(),
        )
    } else {
        None
    };

    // Goodbye: the transport's own end — the child dropped, the session deleted.
    if let Err(e) = client.cancel().await {
        tracing::debug!(target: "bisa_mcp_probe", "the probe's connection did not close cleanly: {e}");
    }
    set_stage(&stage, McpProbeStage::Done);
    Ok(McpProbeReport {
        ok: true,
        transport: config.kind().to_string(),
        era: Some(era),
        protocol_version: Some(protocol_version),
        server,
        capabilities,
        instructions: instructions.map(|i| sanitize::sanitize(&i, config)),
        tools,
        tool_count,
        resource_count,
        prompt_count,
        elapsed_ms: started.elapsed().as_millis() as u64,
        stage: McpProbeStage::Done,
        error: None,
    })
}
