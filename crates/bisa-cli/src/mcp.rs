//! `bisa mcp`: the registry of MCP servers agents can be given.
//!
//! A server is registered once and referenced by id, so the same server on six
//! agents is one record rather than six copies that drift. It is also
//! deliberately **local**: a stdio transport is a command line plus an
//! environment on *this* machine, so the registry never leaves it — what
//! travels on an agent's public snapshot is the id alone.
//!
//! A server cannot be deleted while an agent carries it — `bisa mcp
//! usage <id>` says who does — for the same reason a skill cannot.
//!
//! `bisa mcp probe` dials a server — a registered one, or a transport given
//! on the command line — and prints what answered (`bisa-mcp-probe`):
//! through the running node when there is one, so the answer becomes the
//! server's health there too, else directly. What this command prints never
//! carries a secret: every `env` and `headers` value reads as the mask.
//!
//! `bisa mcp` with no subcommand is the other personality of this word:
//! the stdio MCP *server* the engine injects into harness sessions
//! (`bisa mcp --socket <path> --goal <ulid>`). The engine spawns that
//! form by name, so the registry verbs live under it rather than replacing it.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::tags::{keeps, TagFilterArgs, TagSetArgs};
use anyhow::{bail, Context as _, Result};
use bisa_core::tags::TagEntity;
use bisa_core::{McpId, McpServer};
use bisa_harness::McpServerConfig;
use bisa_store::{NewMcp, UsageKind};
use clap::{Args, Subcommand};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Subcommand)]
pub enum McpCmd {
    /// List registered servers
    List {
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Show one server, including its transport
    Show { id: String },
    /// Register a server
    Add {
        /// Stable id agents reference: lowercase letters, digits, `-` and `_`
        #[arg(long)]
        id: String,
        /// The name the harness sees. It may not be `bisa`: the engine
        /// injects its own server under that name into every session.
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[command(flatten)]
        transport: TransportArgs,
        #[command(flatten)]
        tags: TagSetArgs,
    },
    /// Change a server. The id is fixed: agents reference it.
    Edit {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[command(flatten)]
        transport: TransportArgs,
        #[command(flatten)]
        tags: TagSetArgs,
    },
    /// Which agents carry this server
    Usage { id: String },
    /// Dial a server and print what answered: who, the negotiated protocol
    /// revision and era, its capabilities and tools — or the stage it stopped
    /// at. A registered server by id, or a transport given here.
    Probe {
        /// A registered server; leave it out to probe --command or --url
        id: Option<String>,
        #[command(flatten)]
        transport: TransportArgs,
        /// How long to wait for the whole conversation, 1–30 seconds
        #[arg(long)]
        timeout_secs: Option<u64>,
    },
    /// Delete a server. Refused while any agent still carries it — detach it
    /// there first (`bisa mcp usage <id>` lists them).
    #[command(alias = "remove")]
    Rm { id: String },
    /// Switch a server on
    Enable { id: String },
    /// Switch a server off. Agents keep the reference; sessions skip it.
    Disable { id: String },
}

/// How the harness reaches the server: a local process, or a URL over
/// Streamable HTTP — or over the older HTTP+SSE transport with `--sse`.
#[derive(Args, Clone, Debug)]
pub struct TransportArgs {
    /// stdio transport: the command that starts the server
    #[arg(long, conflicts_with = "url")]
    pub command: Option<String>,
    /// Argument for --command, in order (repeatable). Hyphens are values here,
    /// not flags: `--arg -y` is how nearly every stdio server is launched.
    #[arg(long = "arg", requires = "command", allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Environment entry for --command, `KEY=VALUE` (repeatable). It is stored
    /// on this machine only, never published with the agent, and never
    /// printed back.
    #[arg(long = "env", requires = "command")]
    pub env: Vec<String>,
    /// Working directory for --command; absent, the session's own
    #[arg(long, requires = "command")]
    pub cwd: Option<String>,
    /// Remote transport: the server's URL (Streamable HTTP)
    #[arg(long)]
    pub url: Option<String>,
    /// The URL speaks the 2024-11-05 HTTP+SSE transport, not Streamable HTTP
    #[arg(long, requires = "url")]
    pub sse: bool,
    /// Header sent with every request to --url, `Name: value` or `Name=value`
    /// (repeatable) — a bearer token, an API key; never printed back
    #[arg(long = "header", requires = "url")]
    pub headers: Vec<String>,
}

/// `KEY=VALUE` pairs, or `Name: value` for a header, into a map.
fn pairs(entries: &[String], flag: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for entry in entries {
        let split = entry
            .split_once('=')
            .or_else(|| entry.split_once(':'))
            .with_context(|| {
                bisa_core::text!(
                    "cli-mcp-must-be-key-value",
                    flag = flag.to_string(),
                    entry = format!("{entry:?}")
                )
            })?;
        out.insert(split.0.trim().to_string(), split.1.trim().to_string());
    }
    Ok(out)
}

impl TransportArgs {
    fn given(&self) -> bool {
        self.command.is_some() || self.url.is_some()
    }

    fn parse(&self, name: String) -> Result<McpServerConfig> {
        match (&self.command, &self.url) {
            (Some(command), None) => Ok(McpServerConfig::Stdio {
                name,
                command: command.clone(),
                args: self.args.clone(),
                env: pairs(&self.env, "env")?,
                cwd: self.cwd.clone(),
            }),
            (None, Some(url)) if self.sse => Ok(McpServerConfig::Sse {
                name,
                url: url.clone(),
                headers: pairs(&self.headers, "header")?,
            }),
            (None, Some(url)) => Ok(McpServerConfig::Http {
                name,
                url: url.clone(),
                headers: pairs(&self.headers, "header")?,
            }),
            // clap refuses both; only "neither" can reach here.
            _ => bail!(bisa_core::text!(
                "cli-mcp-server-needs-transport-command-cmd-url"
            )),
        }
    }
}

/// The transport in one line — the shape, never a value.
fn transport_label(t: &McpServerConfig) -> String {
    match t {
        McpServerConfig::Stdio {
            command,
            args,
            env,
            cwd,
            ..
        } => {
            let mut s = bisa_i18n::say(&bisa_core::text!(
                "cli-mcp-stdio",
                command = command.to_string()
            ));
            for a in args {
                s.push(' ');
                s.push_str(a);
            }
            s.push('`');
            if !env.is_empty() {
                s.push_str(&bisa_i18n::say(&bisa_core::text!(
                    "cli-mcp-env",
                    a0 = (env.len()).to_string()
                )));
            }
            if let Some(dir) = cwd {
                s.push_str(&format!("  in {dir}"));
            }
            s
        }
        McpServerConfig::Http { url, headers, .. } => {
            format!("http {url}{}", header_count(headers))
        }
        McpServerConfig::Sse { url, headers, .. } => {
            format!("sse {url}{}", header_count(headers))
        }
    }
}

fn header_count(headers: &BTreeMap<String, String>) -> String {
    if headers.is_empty() {
        String::new()
    } else {
        bisa_i18n::say(&bisa_core::text!(
            "cli-mcp-headers",
            a0 = (headers.len()).to_string()
        ))
    }
}

/// A server as this command prints it: every secret value masked.
fn shown(m: &McpServer) -> McpServer {
    McpServer {
        transport: m.transport.masked(),
        ..m.clone()
    }
}

/// One line of a health, for `list`: a glyph and the words.
fn health_glyph(health: &serde_json::Value) -> &'static str {
    match health["state"].as_str() {
        Some("ok") => "✓",
        Some("failing") => "✗",
        _ => "·",
    }
}

/// A report, for a person: the words, then the tools.
fn render_report(out: &Out, report: &bisa_mcp_probe::McpProbeReport) {
    out.human(&format!(
        "{} {} — {}",
        if report.ok { "✓" } else { "✗" },
        report.transport,
        report.words()
    ));
    if let Some(era) = report.era {
        out.say(&bisa_core::text!(
            "cli-mcp-era-elapsed-ms",
            era = format!("{era:?}"),
            a0 = (report.elapsed_ms).to_string()
        ));
    }
    if report.ok {
        let caps = [
            ("tools", report.capabilities.tools),
            ("resources", report.capabilities.resources),
            ("prompts", report.capabilities.prompts),
            ("logging", report.capabilities.logging),
            ("completions", report.capabilities.completions),
        ];
        let on: Vec<&str> = caps.iter().filter(|(_, v)| *v).map(|(k, _)| *k).collect();
        out.human(&format!(
            "  capabilities: {}",
            if on.is_empty() {
                "none".to_string()
            } else {
                on.join(", ")
            }
        ));
        for t in &report.tools {
            match &t.description {
                Some(d) => out.human(&format!("  - {}  {d}", t.name)),
                None => out.human(&format!("  - {}", t.name)),
            }
        }
        if report.tool_count > report.tools.len() {
            out.human(&format!(
                "  … and {} more",
                report.tool_count - report.tools.len()
            ));
        }
    } else {
        out.say(&bisa_core::text!(
            "cli-mcp-stopped",
            a0 = format!("{:?}", report.stage)
        ));
    }
    out.json_value(json!({"report": report}));
}

fn mcp_id(s: &str) -> Result<McpId> {
    McpId::new(s).map_err(|e| anyhow::anyhow!("{e}"))
}

fn render(out: &Out, m: &McpServer) {
    out.human(&format!(
        "{}  {}{}\n  {}\n  tags: {}",
        m.id,
        m.name,
        if m.enabled { "" } else { "  (disabled)" },
        transport_label(&m.transport),
        crate::tags::label(&m.tags)
    ));
    if !m.description.is_empty() {
        out.human(&format!("  {}", m.description));
    }
}

/// The saved server, printed: a person reads the shape, JSON reads the mask.
fn say(out: &Out, m: &McpServer) {
    render(out, m);
    out.json_value(json!({"mcp": shown(m)}));
}

async fn set_enabled(ctx: &Ctx, out: &Out, id: &str, enabled: bool) -> Result<()> {
    let id = mcp_id(id)?;
    let m = if let Some(node) = ctx.node_client().await {
        let v = node
            .patch(&format!("/mcp/{id}"), json!({"enabled": enabled}))
            .await?;
        serde_json::from_value::<bisa_node::dto::McpServerView>(v["mcp"].clone())?.server
    } else {
        let ws = ctx.workspace()?;
        let mut def = ws.get_mcp(&id)?;
        def.enabled = enabled;
        ws.update_mcp(def)?
    };
    say(out, &m);
    Ok(())
}

/// Reads go to the workspace directly; a write goes through the running node
/// when there is one — its registry and its bus hear of it — and through the
/// store when there is none, as a settings write does.
pub async fn mcp(ctx: &Ctx, out: &Out, cmd: McpCmd) -> Result<()> {
    let ws = ctx.workspace()?;
    match cmd {
        McpCmd::List { filter } => {
            let admitted = filter.admitted(&ws, TagEntity::Mcp)?;
            let servers: Vec<McpServer> = ws
                .list_mcps()?
                .into_iter()
                .filter(|m| keeps(&admitted, m.id.as_str()))
                .collect();
            // The health is the node's: it lives where the probes ran.
            let health: BTreeMap<String, serde_json::Value> = match ctx.node_client().await {
                Some(node) => node
                    .get("/mcp")
                    .await
                    .ok()
                    .and_then(|v| v["mcp"].as_array().cloned())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|m| Some((m["id"].as_str()?.to_string(), m["health"].clone())))
                    .collect(),
                None => BTreeMap::new(),
            };
            if servers.is_empty() {
                out.say(&bisa_core::text!("cli-mcp-no-mcp-servers-match-bisa-mcp"));
            }
            for m in &servers {
                let glyph = health.get(m.id.as_str()).map(health_glyph).unwrap_or(" ");
                out.human(&format!(
                    "{}{} {:<24} {:<20} {}",
                    if m.enabled { " " } else { "·" },
                    glyph,
                    m.id,
                    m.name,
                    transport_label(&m.transport)
                ));
            }
            let listed: Vec<serde_json::Value> = servers
                .iter()
                .map(|m| {
                    let mut v = json!(shown(m));
                    if let Some(h) = health.get(m.id.as_str()) {
                        v["health"] = h.clone();
                    }
                    v
                })
                .collect();
            out.json_value(json!({"mcps": listed}));
        }
        McpCmd::Show { id } => {
            let m = ws.get_mcp(&mcp_id(&id)?)?;
            say(out, &m);
        }
        McpCmd::Probe {
            id,
            transport,
            timeout_secs,
        } => {
            let budget = timeout_secs.map(|s| s.clamp(1, bisa_mcp_probe::MAX_BUDGET.as_secs()));
            let report = match (id, transport.given()) {
                (Some(id), false) => {
                    let mcp = mcp_id(&id)?;
                    if let Some(node) = ctx.node_client().await {
                        let v = node
                            .post(
                                &format!("/mcp/{mcp}/probe"),
                                json!({"timeout_secs": budget}),
                            )
                            .await?;
                        serde_json::from_value(v["report"].clone())?
                    } else {
                        let def = ws.get_mcp(&mcp)?;
                        if !def.enabled {
                            bail!(bisa_core::text!(
                                "cli-mcp-disabled-bisa-mcp-enable-first",
                                a0 = (def.id).to_string(),
                                id = id.to_string()
                            ));
                        }
                        bisa_mcp_probe::McpProbe::probe(
                            &bisa_mcp_probe::RmcpProbe,
                            &def.transport,
                            bisa_mcp_probe::budget_of(budget),
                        )
                        .await
                    }
                }
                (None, true) => {
                    let config = transport.parse("probe".to_string())?;
                    config.validate().map_err(|e| anyhow::anyhow!("{e}"))?;
                    if let Some(node) = ctx.node_client().await {
                        let v = node
                            .post(
                                "/mcp/probe",
                                json!({"transport": config, "timeout_secs": budget}),
                            )
                            .await?;
                        serde_json::from_value(v["report"].clone())?
                    } else {
                        bisa_mcp_probe::McpProbe::probe(
                            &bisa_mcp_probe::RmcpProbe,
                            &config,
                            bisa_mcp_probe::budget_of(budget),
                        )
                        .await
                    }
                }
                _ => bail!(bisa_core::text!(
                    "cli-mcp-probe-one-thing-registered-id-transport"
                )),
            };
            render_report(out, &report);
        }
        McpCmd::Add {
            id,
            name,
            description,
            transport,
            tags,
        } => {
            let new = NewMcp {
                id: mcp_id(&id)?,
                description,
                tags: tags.parse()?,
                transport: transport.parse(name)?,
            };
            let m = if let Some(node) = ctx.node_client().await {
                let v = node
                    .post(
                        "/mcp",
                        json!({"id": new.id, "description": new.description, "tags": new.tags, "transport": new.transport}),
                    )
                    .await?;
                serde_json::from_value::<bisa_node::dto::McpServerView>(v["mcp"].clone())?.server
            } else {
                ws.create_mcp(new)?
            };
            say(out, &m);
        }
        McpCmd::Edit {
            id,
            name,
            description,
            transport,
            tags,
        } => {
            let mut def = ws.get_mcp(&mcp_id(&id)?)?;
            // The transport carries the name, so a rename rebuilds it from
            // whichever half the caller left alone.
            let name = name.unwrap_or_else(|| def.name.clone());
            if transport.given() {
                def.transport = transport.parse(name)?;
            } else {
                def.transport = def.transport.renamed(name);
            }
            if let Some(d) = description {
                def.description = d;
            }
            def.tags = tags.apply(def.tags.clone())?;
            let m = if let Some(node) = ctx.node_client().await {
                let v = node
                    .patch(
                        &format!("/mcp/{}", def.id),
                        json!({"description": def.description, "tags": def.tags, "transport": def.transport}),
                    )
                    .await?;
                serde_json::from_value::<bisa_node::dto::McpServerView>(v["mcp"].clone())?.server
            } else {
                ws.update_mcp(def)?
            };
            say(out, &m);
        }
        McpCmd::Usage { id } => {
            let usage = ws.usage_of(UsageKind::Mcp, &id)?;
            crate::usage::render(out, UsageKind::Mcp, &id, &usage);
        }
        McpCmd::Rm { id } => {
            // The same correction the skill library took: detaching on the
            // owner's behalf hid a delete inside a delete.
            let mcp = mcp_id(&id)?;
            if let Some(node) = ctx.node_client().await {
                node.delete(&format!("/mcp/{mcp}")).await?;
            } else {
                ws.remove_mcp(&mcp)?;
            }
            out.say(&bisa_core::text!(
                "cli-mcp-mcp-server-removed",
                id = id.to_string()
            ));
            out.json_value(json!({"removed": id}));
        }
        McpCmd::Enable { id } => return set_enabled(ctx, out, &id, true).await,
        McpCmd::Disable { id } => return set_enabled(ctx, out, &id, false).await,
    }
    Ok(())
}
