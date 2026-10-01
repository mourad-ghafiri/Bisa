//! Agents (definition + harness + model plan + skill and MCP references),
//! teams, the runtime session roster, and the owner's view of agent Recall.
//!
//! An Agent is a *definition*, not a running thing: a system prompt bound to a
//! harness, a **model plan** (an ordered list of models plus the strategy for
//! choosing among them), and **references** into the workspace's skill library
//! and MCP registry. `bisa sessions` shows what is actually running.
//!
//! Skills and MCP servers are ids, never bodies: one library entry serves
//! every agent that names it, and the agent's public snapshot carries the id
//! rather than a machine's command line. Both lists can be set two ways, and
//! the two mean different things:
//!
//! * `--skill` / `--mcp` on `add` and `edit` **replace** the list, the same
//!   rule `--model` and `--tag` follow;
//! * `agent skill add` / `agent mcp add` adjust one reference without
//!   retyping the rest.
//!
//! Reads go to the workspace directly; a write goes through the running node
//! when there is one — the engine that holds the workspace is the one that
//! says, in every channel, who was stood up or down — and through this
//! process's own store when there is none.
//!
//! `--model` is repeatable and **order matters**: it is the plan, best-first.
//! A model is `id[=weight][@effort][: what it suits]`; `--effort` is the
//! plan's own, for every model that names none. An id with `[1m]` is typed
//! quoted: unquoted, the brackets are a glob to a shell.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::tags::{keeps, TagFilterArgs, TagSetArgs};
use anyhow::{bail, Context as _, Result};
use bisa_core::tags::TagEntity;
use bisa_core::{
    Agent, AgentId, AgentOrigin, Assignee, EffortChoice, McpId, Origin, RespondPolicy, SkillId,
    Team, TeamId,
};
use bisa_harness::{AllHealthy, ModelChoice, ModelPlan, ModelStrategy};
use bisa_store::{NewAgent, UsageKind};
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand)]
pub enum AgentCmd {
    /// List agent definitions
    List {
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Show one agent, including the skills and MCP servers it references
    Show { id: String },
    /// Define a new agent, when the catalog has no ready-made one that fits
    /// (`bisa catalog list --kind agent`)
    Add {
        #[arg(long)]
        name: String,
        /// System prompt text (or use --prompt-file)
        #[arg(long)]
        prompt: Option<String>,
        /// Read the system prompt from a file
        #[arg(long)]
        prompt_file: Option<std::path::PathBuf>,
        /// Harness this agent runs on (see `bisa harness list`)
        #[arg(long, default_value = "claude-code")]
        harness: String,
        /// A model for the plan, best-first (repeatable), as
        /// `id[=weight][@effort][: what it suits]` and quoted, since `[1m]`
        /// is a glob to a shell: `--model "claude-opus-5-5[1m]=3@max: design
        /// work"`. See `bisa agent models <harness>`.
        #[arg(long = "model")]
        models: Vec<String>,
        /// How the plan picks: fallback (default) | weighted | round-robin |
        /// least-busy | auto-route (the Decision-Making Agent picks per task)
        #[arg(long)]
        strategy: Option<String>,
        /// How hard the plan's models work: auto (the Decision-Making Agent
        /// names the level per task) | minimal | low | medium | high | xhigh
        /// | max, fitted to what each model takes. Omit it, or say inherit,
        /// and the `agents.effort` setting decides
        #[arg(long)]
        effort: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Skill library id (repeatable, in the order the harness reads them).
        /// See `bisa skill list`.
        #[arg(long = "skill")]
        skills: Vec<String>,
        /// MCP registry id (repeatable). See `bisa mcp list`.
        #[arg(long = "mcp")]
        mcps: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
        /// Who may direct this agent: owner-only (default) | members
        #[arg(long)]
        respond: Option<String>,
        /// Let the Decision-Making Agent stand in at the decision points this
        /// agent reaches, whatever the workspace's switch says
        #[arg(long)]
        decision_making: bool,
    },
    /// Change an existing agent
    Edit {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long)]
        prompt_file: Option<std::path::PathBuf>,
        #[arg(long)]
        harness: Option<String>,
        /// Replace the model plan, best-first (repeatable), each model as
        /// `id[=weight][@effort][: what it suits]` and quoted: `--model
        /// "claude-opus-5-5[1m]@max"`. Omit to leave the plan alone.
        #[arg(long = "model")]
        models: Vec<String>,
        /// Replace the strategy: fallback | weighted | round-robin | least-busy
        /// | auto-route
        #[arg(long)]
        strategy: Option<String>,
        /// Replace the plan's effort: inherit (the `agents.effort` setting
        /// decides again) | auto | minimal | low | medium | high | xhigh |
        /// max. With --model, omit it to keep the effort the plan had
        #[arg(long)]
        effort: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Replace the skill list with these library ids (repeatable). To add
        /// one without retyping the rest, use `bisa agent skill add`.
        #[arg(long = "skill")]
        skills: Vec<String>,
        /// Replace the MCP list with these registry ids (repeatable). To add
        /// one, use `bisa agent mcp add`.
        #[arg(long = "mcp")]
        mcps: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
        #[arg(long)]
        respond: Option<String>,
        #[arg(long)]
        enabled: Option<bool>,
        /// Switch the Decision-Making Agent on or off for this agent
        #[arg(long)]
        decision_making: Option<bool>,
    },
    /// What still points at this agent: teams, channels, assignments,
    /// unfinished work items, workflows, gate policies
    Usage { id: String },
    /// Delete an agent definition. Refused while anything still points at it
    /// (`bisa agent usage <id>`), and `general-agent` is refused
    /// outright: it is the one agent every workspace can always reach.
    #[command(alias = "remove")]
    Rm { id: String },
    /// The skills one agent carries, one reference at a time
    Skill {
        #[command(subcommand)]
        command: RefCmd,
    },
    /// The MCP servers one agent carries, one reference at a time
    Mcp {
        #[command(subcommand)]
        command: RefCmd,
    },
    /// Models a harness advertises (empty = unknown, free-text is fine)
    Models { harness: String },
}

/// `add` / `rm` / `list` over one agent's reference list. Skills and MCP
/// servers are referenced identically, so they take identical verbs.
#[derive(Subcommand)]
pub enum RefCmd {
    /// Attach one, leaving the rest of the list alone (idempotent)
    Add { agent: String, id: String },
    /// Detach one, leaving the rest alone
    #[command(alias = "remove")]
    Rm { agent: String, id: String },
    /// What this agent references, in the order the harness reads it
    List { agent: String },
}

#[derive(Subcommand)]
pub enum TeamCmd {
    /// Create a team (agents + humans working one goal together), when the
    /// catalog has no ready-made one that fits
    /// (`bisa catalog list --kind team`)
    Create {
        name: String,
        #[arg(long)]
        purpose: Option<String>,
        /// Human member pubkey (repeatable)
        #[arg(long = "human")]
        humans: Vec<String>,
        /// Agent member id (repeatable)
        #[arg(long = "agent")]
        agents: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
    },
    /// List teams
    List {
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Show one team: its members, and the goals it is carrying
    Show { id: String },
    /// Add a member: --human <pubkey> or --agent <id>
    AddMember {
        id: String,
        #[arg(long)]
        human: Option<String>,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Remove a member: --human <pubkey> or --agent <id>
    RemoveMember {
        id: String,
        #[arg(long)]
        human: Option<String>,
        #[arg(long)]
        agent: Option<String>,
    },
    /// What still points at this team: assignments, workflows, gate policies
    Usage { id: String },
    /// Delete a team. Its members are untouched — a team groups agents, it
    /// does not own them — and it is refused while anything still names the
    /// team itself (`bisa team usage <id>`).
    #[command(alias = "remove")]
    Rm { id: String },
}

#[derive(Subcommand)]
pub enum SessionCmd {
    /// List running/parked sessions (the runtime roster)
    List,
    /// Terminate a session at once — `aborted` is terminal
    Abort { id: String },
}

#[derive(Subcommand)]
pub enum RecallCmd {
    /// List an agent's memories (owner view)
    List { agent: String },
    /// Read one memory by slug
    Get { agent: String, slug: String },
}

fn model_strategy(s: &str) -> Result<ModelStrategy> {
    match s {
        "fallback" => Ok(ModelStrategy::Fallback),
        "weighted" => Ok(ModelStrategy::Weighted),
        "round-robin" | "round_robin" => Ok(ModelStrategy::RoundRobin),
        "least-busy" | "least_busy" => Ok(ModelStrategy::LeastBusy),
        "auto-route" | "auto_route" => Ok(ModelStrategy::AutoRoute),
        other => bail!(bisa_core::text!(
            "cli-agents-unknown-model-strategy-fallback-weighted-round",
            other = format!("{other:?}")
        )),
    }
}

/// The word `--effort` takes for *say nothing*: the plan names no effort
/// and the `agents.effort` setting decides. The CLI's own word — a plan on
/// the wire simply carries no effort.
const INHERIT: &str = "inherit";

/// What `--effort` asks for: `None` for `inherit`, else a level or `auto`.
fn plan_effort(word: &str) -> Result<Option<EffortChoice>> {
    if word == INHERIT {
        return Ok(None);
    }
    match word.parse::<EffortChoice>() {
        Ok(effort) => Ok(Some(effort)),
        Err(_) => bail!(bisa_core::text!(
            "cli-agents-unknown-effort",
            other = format!("{word:?}")
        )),
    }
}

/// The effort a model spec ends with, split from what stands before it: the
/// word after the last `@`, when it is an effort. An id may carry an `@` of
/// its own (`claude-opus-5-5@20260101`), so a tail that is no effort is
/// part of the id.
fn split_effort(head: &str) -> (&str, Option<EffortChoice>) {
    match head.rsplit_once('@') {
        Some((rest, word)) => match word.trim().parse::<EffortChoice>() {
            Ok(effort) => (rest, Some(effort)),
            Err(_) => (head, None),
        },
        None => (head, None),
    }
}

/// `id[=weight][@effort][: what the model is suited for]`. Order is the
/// plan order; the weight only matters to the `weighted` strategy, the
/// sentence only to `auto-route`, and the effort is that model's own, over
/// the plan's.
fn model_choice(spec: &str) -> Result<ModelChoice> {
    let (head, suited_for) = match spec.split_once(": ") {
        Some((head, suited)) => (head, Some(suited.trim()).filter(|s| !s.is_empty())),
        None => (spec, None),
    };
    let (head, effort) = split_effort(head);
    let (model, weight) = match head.split_once('=') {
        Some((m, w)) => (
            m.trim(),
            w.trim().parse::<u32>().with_context(|| {
                bisa_core::text!(
                    "cli-agents-model-weight-must-be-number",
                    spec = format!("{spec:?}")
                )
            })?,
        ),
        None => (head.trim(), 1),
    };
    if model.is_empty() {
        bail!(bisa_core::text!(
            "cli-agents-model-has-no-model-id",
            spec = format!("{spec:?}")
        ));
    }
    Ok(ModelChoice {
        model: model.to_string(),
        weight,
        enabled: true,
        suited_for: suited_for.map(str::to_string),
        effort,
    })
}

fn model_plan(
    models: &[String],
    strategy: Option<&str>,
    effort: Option<EffortChoice>,
) -> Result<ModelPlan> {
    Ok(ModelPlan {
        strategy: match strategy {
            Some(s) => model_strategy(s)?,
            None => ModelStrategy::default(),
        },
        effort,
        models: models
            .iter()
            .map(|m| model_choice(m))
            .collect::<Result<Vec<_>>>()?,
    })
}

/// The plan an edit leaves: `--model` replaces the models and the strategy
/// and keeps the effort the plan had unless `--effort` says otherwise;
/// `--strategy` and `--effort` on their own each edit their own word and
/// leave the models alone.
fn edited_plan(
    current: ModelPlan,
    models: &[String],
    strategy: Option<&str>,
    effort: Option<&str>,
) -> Result<ModelPlan> {
    let effort = effort.map(plan_effort).transpose()?;
    if !models.is_empty() {
        return model_plan(models, strategy, effort.unwrap_or(current.effort));
    }
    let mut plan = current;
    if let Some(s) = strategy {
        plan.strategy = model_strategy(s)?;
    }
    if let Some(effort) = effort {
        plan.effort = effort;
    }
    Ok(plan)
}

/// A plan on one line: each model as its spec reads — `id=weight@effort` —
/// then the strategy, then the plan's own effort as `@effort`.
fn render_plan(plan: &ModelPlan) -> String {
    let effort = plan
        .effort
        .map(|effort| format!(" @{effort}"))
        .unwrap_or_default();
    if plan.models.is_empty() {
        let default = bisa_i18n::say(&bisa_core::text!("cli-agents-harness-default"));
        return format!("{default}{effort}");
    }
    let entries: Vec<String> = plan
        .models
        .iter()
        .map(|c| {
            let mut s = c.model.clone();
            if c.weight != 1 {
                s.push_str(&format!("={}", c.weight));
            }
            if let Some(effort) = c.effort {
                s.push_str(&format!("@{effort}"));
            }
            if !c.enabled {
                s.push_str(" (off)");
            }
            if let Some(suited) = &c.suited_for {
                s.push_str(&format!(" ({suited})"));
            }
            s
        })
        .collect();
    format!("{} [{:?}]{effort}", entries.join(" → "), plan.strategy)
}

/// The efforts a model takes, as one column: the wire words, lowest first.
/// Empty for a model the harness has no effort control for.
fn effort_words(efforts: &[bisa_core::Effort]) -> String {
    efforts
        .iter()
        .map(|effort| effort.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn respond_policy(s: &str) -> Result<RespondPolicy> {
    match s {
        "owner-only" | "owner_only" => Ok(RespondPolicy::OwnerOnly),
        "members" => Ok(RespondPolicy::Members),
        other => bail!(bisa_core::text!(
            "cli-agents-unknown-respond-policy-owner-only-members",
            other = format!("{other:?}")
        )),
    }
}

fn prompt_text(
    prompt: Option<String>,
    prompt_file: Option<std::path::PathBuf>,
) -> Result<Option<String>> {
    match (prompt, prompt_file) {
        (Some(_), Some(_)) => bail!(bisa_core::text!(
            "cli-agents-pass-prompt-prompt-file-not-both"
        )),
        (Some(p), None) => Ok(Some(p)),
        (None, Some(f)) => Ok(Some(
            std::fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?,
        )),
        (None, None) => Ok(None),
    }
}

fn id_list(ids: &[String]) -> String {
    if ids.is_empty() {
        "—".to_string()
    } else {
        ids.join(", ")
    }
}

/// How an agent's provenance reads in a detail view.
///
/// The core agent gets a sentence rather than a word because its provenance is
/// the only one that changes what you can do with the agent: `agent rm` and
/// `agent edit --enabled false` refuse it, and finding that out from an error
/// is worse than reading it here first.
fn agent_provenance(origin: &AgentOrigin) -> String {
    match origin {
        AgentOrigin::Core => "  (the platform's own agent: always here, and it cannot be \
                              removed or switched off)"
            .to_string(),
        AgentOrigin::Catalog { slug } => bisa_i18n::say(&bisa_core::text!(
            "cli-agents-from-catalog",
            slug = slug.to_string()
        )),
        AgentOrigin::Local => String::new(),
    }
}

/// The same fact in one column, for a list where every row must stay one line.
fn origin_word(origin: &AgentOrigin) -> &'static str {
    match origin {
        AgentOrigin::Core => "built-in",
        AgentOrigin::Catalog { .. } => "catalog",
        AgentOrigin::Local => "",
    }
}

fn agent_id(s: &str) -> Result<AgentId> {
    AgentId::new(s).map_err(|e| anyhow::anyhow!("{e}"))
}

fn team_id(s: &str) -> Result<TeamId> {
    TeamId::new(s).map_err(|e| anyhow::anyhow!("{e}"))
}

fn skill_ids(ids: &[String]) -> Result<Vec<SkillId>> {
    ids.iter()
        .map(|s| SkillId::new(s).map_err(|e| anyhow::anyhow!("{e}")))
        .collect()
}

fn mcp_ids(ids: &[String]) -> Result<Vec<McpId>> {
    ids.iter()
        .map(|s| McpId::new(s).map_err(|e| anyhow::anyhow!("{e}")))
        .collect()
}

/// An agent as the node answered it.
fn agent_said(answer: &serde_json::Value) -> Result<Agent> {
    Ok(serde_json::from_value(answer["agent"].clone())?)
}

/// A team as the node answered it.
fn team_said(answer: &serde_json::Value) -> Result<Team> {
    Ok(serde_json::from_value(answer["team"].clone())?)
}

/// A new agent as `POST /agents` reads it.
fn new_agent_body(new: &NewAgent) -> serde_json::Value {
    json!({
        "name": new.name,
        "description": new.description,
        "system_prompt": new.system_prompt,
        "harness": new.harness,
        "models": new.models,
        "skills": new.skills,
        "mcps": new.mcps,
        "tags": new.tags,
        "respond": new.respond,
        "decision_making": new.decision_making,
    })
}

/// An agent's record as edited, as `PATCH /agents/{id}` reads it: every
/// field the command line can change, so what it left alone goes back as it
/// was. The picture is no word of the command line's and is left out, which
/// keeps it.
fn edited_agent_body(def: &Agent) -> serde_json::Value {
    json!({
        "name": def.name,
        "description": def.description,
        "system_prompt": def.system_prompt,
        "harness": def.harness,
        "models": def.models,
        "skills": def.skills,
        "mcps": def.mcps,
        "tags": def.tags,
        "respond": def.respond,
        "enabled": def.enabled,
        "decision_making": def.decision_making,
    })
}

/// A team's member as the node's bodies read one.
fn member_said(member: &Assignee) -> serde_json::Value {
    match member {
        Assignee::Human(pubkey) => json!({"human": pubkey}),
        Assignee::Agent(id) => json!({"agent": id}),
        Assignee::Team(id) => json!({"team": id}),
    }
}

/// An agent's record as edited, written — by the node when one runs. With
/// none, the engine of this one command writes an edit that stands the agent
/// up or down, since that is said in every channel the agent is in; any
/// other edit is the store's.
async fn agent_written(ctx: &Ctx, def: Agent, stands: bool) -> Result<Agent> {
    if let Some(node) = ctx.node_client().await {
        let answer = node
            .patch(&format!("/agents/{}", def.id), edited_agent_body(&def))
            .await?;
        return agent_said(&answer);
    }
    if !stands {
        return Ok(ctx.workspace()?.update_agent(def)?);
    }
    let (engine, _) = ctx.engine().await?;
    let written = bisa_engine::directory::update_agent(engine.inner(), def);
    engine.shutdown().await;
    Ok(written?)
}

/// A team's roster as edited, written: by the node when one runs.
async fn roster_written(ctx: &Ctx, team: Team) -> Result<Team> {
    match ctx.node_client().await {
        Some(node) => {
            let members: Vec<serde_json::Value> = team.members.iter().map(member_said).collect();
            let answer = node
                .patch(&format!("/teams/{}", team.id), json!({"members": members}))
                .await?;
            team_said(&answer)
        }
        None => Ok(ctx.workspace()?.update_team(team)?),
    }
}

fn render_agent(out: &Out, a: &Agent) {
    let provenance = agent_provenance(&a.origin);
    out.say(&bisa_core::text!(
        "cli-agents-harness-models-responds-skills-mcp-tags",
        a0 = (a.id).to_string(),
        a1 = (a.name).to_string(),
        provenance = provenance.to_string(),
        a2 = (a.harness).to_string(),
        a3 = (render_plan(&a.models)).to_string(),
        a4 = format!("{:?}", a.respond),
        a5 = (id_list(&a.skills.iter().map(|s| s.to_string()).collect::<Vec<_>>())).to_string(),
        a6 = (id_list(&a.mcps.iter().map(|m| m.to_string()).collect::<Vec<_>>())).to_string(),
        a7 = (crate::tags::label(&a.tags)).to_string(),
        a8 = (a.pubkey).to_string()
    ));
}

/// The node's route for one reference of one agent. The id is read as the
/// kind it names first, so what goes into the path is an id and nothing else.
fn reference_route(agent: &AgentId, id: &str, skills: bool) -> Result<String> {
    Ok(if skills {
        let skill = SkillId::new(id).map_err(|e| anyhow::anyhow!("{e}"))?;
        format!("/agents/{agent}/skills/{skill}")
    } else {
        let server = McpId::new(id).map_err(|e| anyhow::anyhow!("{e}"))?;
        format!("/agents/{agent}/mcps/{server}")
    })
}

/// One agent's skill or MCP reference list. Both are `Vec<String>` of ids on
/// `AgentDef`, so the verbs differ only in which store call they make.
async fn agent_refs(ctx: &Ctx, out: &Out, cmd: RefCmd, skills: bool) -> Result<()> {
    let ws = ctx.workspace()?;
    let field = if skills { "skills" } else { "mcps" };
    match cmd {
        RefCmd::Add { agent, id } => {
            let agent = agent_id(&agent)?;
            let a = if let Some(node) = ctx.node_client().await {
                let reference = reference_route(&agent, &id, skills)?;
                agent_said(&node.post(&reference, json!({})).await?)?
            } else if skills {
                ws.attach_skill(&agent, &skill_ids(std::slice::from_ref(&id))?[0])?
            } else {
                ws.attach_mcp(&agent, &mcp_ids(std::slice::from_ref(&id))?[0])?
            };
            render_agent(out, &a);
            out.json_value(json!({"agent": a}));
        }
        RefCmd::Rm { agent, id } => {
            let agent = agent_id(&agent)?;
            let a = if let Some(node) = ctx.node_client().await {
                let reference = reference_route(&agent, &id, skills)?;
                agent_said(&node.delete(&reference).await?)?
            } else if skills {
                ws.detach_skill(&agent, &skill_ids(std::slice::from_ref(&id))?[0])?
            } else {
                ws.detach_mcp(&agent, &mcp_ids(std::slice::from_ref(&id))?[0])?
            };
            render_agent(out, &a);
            out.json_value(json!({"agent": a}));
        }
        RefCmd::List { agent } => {
            let a = ws.get_agent(&agent_id(&agent)?)?;
            let ids: Vec<String> = if skills {
                a.skills.iter().map(|s| s.to_string()).collect()
            } else {
                a.mcps.iter().map(|m| m.to_string()).collect()
            };
            if ids.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-agents-agent-references-no",
                    a0 = (a.id).to_string(),
                    field = field.to_string()
                ));
            }
            for id in &ids {
                // A reference that no longer resolves costs the agent that
                // skill or server and nothing else, so say so here rather
                // than at launch, where it is only a log line.
                let name = if skills {
                    SkillId::new(id)
                        .ok()
                        .and_then(|s| ws.get_skill(&s).map(|s| s.name).ok())
                } else {
                    McpId::new(id)
                        .ok()
                        .and_then(|m| ws.get_mcp(&m).map(|m| m.name).ok())
                };
                out.human(&format!(
                    "{:<34} {}",
                    id,
                    name.unwrap_or_else(|| "(unresolved)".into())
                ));
            }
            out.json_value(json!({"agent": a.id, field: ids}));
        }
    }
    Ok(())
}

pub async fn agent(ctx: &Ctx, out: &Out, cmd: AgentCmd) -> Result<()> {
    let ws = ctx.workspace()?;
    match cmd {
        AgentCmd::List { filter } => {
            let admitted = filter.admitted(&ws, TagEntity::Agent)?;
            let agents: Vec<Agent> = ws
                .list_agents()?
                .into_iter()
                .filter(|a| keeps(&admitted, a.id.as_str()))
                .collect();
            if agents.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-agents-no-agents-match-bisa-catalog-list"
                ));
            }
            for a in &agents {
                let mark = if a.enabled { " " } else { "·" };
                let head = a
                    .models
                    .first(&AllHealthy, 0)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        bisa_i18n::say(&bisa_core::text!("cli-agents-harness-default"))
                    });
                out.human(&format!(
                    "{mark} {:<26} {:<20} {:<14} {:<14} {:<9} {}",
                    a.id,
                    a.name,
                    a.harness,
                    head,
                    origin_word(&a.origin),
                    crate::tags::label(&a.tags)
                ));
            }
            out.json_value(json!({"agents": agents}));
        }
        AgentCmd::Show { id } => {
            let a = ws.get_agent(&agent_id(&id)?)?;
            render_agent(out, &a);
            out.human(&format!("\n{}", a.system_prompt));
            out.json_value(json!({"agent": a}));
        }
        AgentCmd::Add {
            name,
            prompt,
            prompt_file,
            harness,
            models,
            strategy,
            effort,
            description,
            skills,
            mcps,
            tags,
            respond,
            decision_making,
        } => {
            let system_prompt = prompt_text(prompt, prompt_file)?.context(bisa_core::text!(
                "cli-agents-agent-needs-prompt-prompt-file"
            ))?;
            // `inherit` on a new agent is the same as saying nothing.
            let effort = effort.as_deref().map(plan_effort).transpose()?.flatten();
            let respond = match respond.as_deref() {
                Some(r) => respond_policy(r)?,
                None => RespondPolicy::OwnerOnly,
            };
            let new = NewAgent {
                name,
                photo: None,
                description,
                system_prompt,
                harness,
                models: model_plan(&models, strategy.as_deref(), effort)?,
                skills: skill_ids(&skills)?,
                mcps: mcp_ids(&mcps)?,
                tags: tags.parse()?,
                respond,
                decision_making,
            };
            let agent = match ctx.node_client().await {
                Some(node) => agent_said(&node.post("/agents", new_agent_body(&new)).await?)?,
                None => ws.add_agent(new)?,
            };
            render_agent(out, &agent);
            out.json_value(json!({"agent": agent}));
        }
        AgentCmd::Edit {
            id,
            name,
            prompt,
            prompt_file,
            harness,
            models,
            strategy,
            effort,
            description,
            skills,
            mcps,
            tags,
            respond,
            enabled,
            decision_making,
        } => {
            // Every word is read before anything is written, and the record
            // is written once: an edit that is refused has stood nobody down.
            let mut def = ws.get_agent(&agent_id(&id)?)?;
            if let Some(on) = enabled {
                def.enabled = on;
            }
            if let Some(n) = name {
                def.name = n;
            }
            if let Some(p) = prompt_text(prompt, prompt_file)? {
                def.system_prompt = p;
            }
            if let Some(h) = harness {
                def.harness = h;
            }
            // The list is ordered, so `--model` replaces it wholesale; a
            // strategy or an effort on its own edits that word and leaves
            // the list alone.
            def.models = edited_plan(def.models, &models, strategy.as_deref(), effort.as_deref())?;
            // Same rule for the reference lists: they are ordered, so passing
            // any `--skill` or `--mcp` means "these are all of them".
            if !skills.is_empty() {
                def.skills = skill_ids(&skills)?;
            }
            if !mcps.is_empty() {
                def.mcps = mcp_ids(&mcps)?;
            }
            def.tags = tags.apply(def.tags.clone())?;
            if let Some(d) = description {
                def.description = Some(d);
            }
            if let Some(r) = respond {
                def.respond = respond_policy(&r)?;
            }
            if let Some(on) = decision_making {
                def.decision_making = on;
            }
            let agent = agent_written(ctx, def, enabled.is_some()).await?;
            render_agent(out, &agent);
            out.json_value(json!({"agent": agent}));
        }
        AgentCmd::Usage { id } => {
            let usage = ws.usage_of(UsageKind::Agent, &id)?;
            crate::usage::render(out, UsageKind::Agent, &id, &usage);
        }
        AgentCmd::Rm { id } => {
            // The store's refusal names the holders and the way out, so it
            // travels to the user unwrapped: rephrasing it here would be a
            // second copy of the same sentence to keep in step.
            let agent = agent_id(&id)?;
            match ctx.node_client().await {
                Some(node) => {
                    node.delete(&format!("/agents/{agent}")).await?;
                }
                None => ws.remove_agent(&agent)?,
            }
            out.say(&bisa_core::text!(
                "cli-agents-agent-removed",
                id = id.to_string()
            ));
            out.json_value(json!({"removed": id}));
        }
        AgentCmd::Skill { command } => return agent_refs(ctx, out, command, true).await,
        AgentCmd::Mcp { command } => return agent_refs(ctx, out, command, false).await,
        AgentCmd::Models { harness } => {
            // A one-shot CLI process: the default TTL is fine — the static cache
            // does not outlive the command anyway.
            let ttl = bisa_core::CacheSettings::default().harness_models_ttl();
            let catalog = ctx.catalog();
            let models = catalog.models_for(&harness, ttl).await;
            // What the harness takes for a model it does not list.
            let efforts = catalog
                .get(&harness)
                .map(|adapter| adapter.efforts(None))
                .unwrap_or_default();
            if models.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-agents-no-model-list",
                    harness = harness.to_string()
                ));
            }
            for m in &models {
                out.human(&format!(
                    "{:<34} {:<28} {}",
                    m.id,
                    m.label.as_deref().unwrap_or(""),
                    effort_words(&m.efforts)
                ));
            }
            out.json_value(json!({"harness": harness, "efforts": efforts, "models": models}));
        }
    }
    Ok(())
}

fn one_member(human: Option<String>, agent: Option<String>) -> Result<Assignee> {
    match (human, agent) {
        (Some(h), None) => Ok(Assignee::Human(
            bisa_core::PrincipalId::new(h).map_err(|e| anyhow::anyhow!("{e}"))?,
        )),
        (None, Some(a)) => Ok(Assignee::Agent(a)),
        _ => bail!(bisa_core::text!(
            "cli-agents-pass-exactly-one-human-pubkey-agent"
        )),
    }
}

fn render_team(out: &Out, t: &Team) {
    let origin = match &t.origin {
        Origin::Catalog { slug } => bisa_i18n::say(&bisa_core::text!(
            "cli-agents-from-catalog",
            slug = slug.to_string()
        )),
        Origin::Local => String::new(),
    };
    out.human(&format!(
        "{}  {}{}{origin}\n  tags: {}",
        t.id,
        t.name,
        t.purpose
            .as_deref()
            .map(|p| format!(" — {p}"))
            .unwrap_or_default(),
        crate::tags::label(&t.tags)
    ));
    for m in &t.members {
        match m {
            Assignee::Human(pk) => out.human(&format!("  human  {pk}")),
            Assignee::Agent(id) => out.human(&format!("  agent  {id}")),
            // The store refuses a nested team; render it rather than panic if
            // a hand-edited truth file carries one.
            Assignee::Team(id) => out.human(&format!("  team   {id}")),
        }
    }
}

pub async fn team(ctx: &Ctx, out: &Out, cmd: TeamCmd) -> Result<()> {
    let ws = ctx.workspace()?;
    match cmd {
        TeamCmd::Create {
            name,
            purpose,
            humans,
            agents,
            tags,
        } => {
            let mut members = Vec::new();
            for h in humans {
                members.push(Assignee::Human(
                    bisa_core::PrincipalId::new(h).map_err(|e| anyhow::anyhow!("{e}"))?,
                ));
            }
            for a in agents {
                members.push(Assignee::Agent(a));
            }
            let tags = tags.parse()?;
            let team = match ctx.node_client().await {
                Some(node) => {
                    let members: Vec<serde_json::Value> = members.iter().map(member_said).collect();
                    let body = json!({
                        "name": name,
                        "purpose": purpose,
                        "members": members,
                        "tags": tags,
                    });
                    team_said(&node.post("/teams", body).await?)?
                }
                None => ws.create_team(&name, purpose.as_deref(), members, tags)?,
            };
            render_team(out, &team);
            out.json_value(json!({"team": team}));
        }
        TeamCmd::List { filter } => {
            let admitted = filter.admitted(&ws, TagEntity::Team)?;
            let teams: Vec<Team> = ws
                .list_teams()?
                .into_iter()
                .filter(|t| keeps(&admitted, t.id.as_str()))
                .collect();
            if teams.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-agents-no-teams-match-bisa-catalog-list"
                ));
            }
            for t in &teams {
                out.human(&format!(
                    "{:<28} {:<20} {} member(s)  {}",
                    t.id,
                    t.name,
                    t.members.len(),
                    crate::tags::label(&t.tags)
                ));
            }
            out.json_value(json!({"teams": teams}));
        }
        TeamCmd::Show { id } => {
            let t = ws.get_team(&team_id(&id)?)?;
            render_team(out, &t);
            let goals = ws
                .goals_for_assignee(&Assignee::Team(id.clone()))
                .unwrap_or_default();
            if goals.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-agents-carrying-nothing-yet-bisa-assign-goal"
                ));
            } else {
                out.say(&bisa_core::text!("cli-agents-carrying"));
                for i in &goals {
                    out.human(&format!(
                        "    {}  {}  {}",
                        i.id,
                        status_of(&ws, i).as_str(),
                        i.title
                            .clone()
                            .unwrap_or_else(|| i.statement.chars().take(60).collect())
                    ));
                }
            }
            out.json_value(json!({
                "team": t,
                "goals": goals.iter().map(|i| json!({
                    "id": i.id.to_string(), "title": i.title, "status": status_of(&ws, i)
                })).collect::<Vec<_>>(),
            }));
        }
        TeamCmd::AddMember { id, human, agent } => {
            let member = one_member(human, agent)?;
            let mut t = ws.get_team(&team_id(&id)?)?;
            if !t.members.contains(&member) {
                t.members.push(member);
            }
            let t = roster_written(ctx, t).await?;
            render_team(out, &t);
            out.json_value(json!({"team": t}));
        }
        TeamCmd::RemoveMember { id, human, agent } => {
            let member = one_member(human, agent)?;
            let mut t = ws.get_team(&team_id(&id)?)?;
            t.members.retain(|m| *m != member);
            let t = roster_written(ctx, t).await?;
            render_team(out, &t);
            out.json_value(json!({"team": t}));
        }
        TeamCmd::Usage { id } => {
            let usage = ws.usage_of(UsageKind::Team, &id)?;
            crate::usage::render(out, UsageKind::Team, &id, &usage);
        }
        TeamCmd::Rm { id } => {
            // The counterpart to `catalog install team <slug>`. Installing a
            // team creates several agents, and an install with no uninstall is
            // a one-way door — this reverses the team itself. The agents it
            // brought stay: they may already be carrying work, and removing
            // them here would be a delete the owner never asked for.
            //
            // Anything still naming the team is the store's refusal, and it
            // names what and what to do about it.
            let team = team_id(&id)?;
            match ctx.node_client().await {
                Some(node) => {
                    node.delete(&format!("/teams/{team}")).await?;
                }
                None => ws.remove_team(&team)?,
            }
            out.say(&bisa_core::text!(
                "cli-agents-team-removed-members-untouched-remove-any",
                id = id.to_string()
            ));
            out.json_value(json!({"removed": id}));
        }
    }
    Ok(())
}

/// One row of the roster as a line: its id, the state it is in, what kind of
/// session it is, and what it is at work on — its item, else the
/// conversation it is a turn of, else the goal it is about.
fn session_line(row: &serde_json::Value) -> String {
    let word = |value: &serde_json::Value| value.as_str().unwrap_or("").to_string();
    let about = ["work_item", "conversation", "goal"]
        .iter()
        .map(|key| word(&row[*key]))
        .find(|id| !id.is_empty())
        .unwrap_or_default();
    format!(
        "{:<27} {:<9} {:<13} {}",
        word(&row["id"]),
        word(&row["state"]["state"]),
        word(&row["kind"]),
        about
    )
    .trim_end()
    .to_string()
}

pub async fn sessions(ctx: &Ctx, out: &Out, cmd: SessionCmd) -> Result<()> {
    match cmd {
        SessionCmd::List => {
            let rows = match ctx.node_client().await {
                Some(client) => client.get("/sessions").await?["sessions"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
                // Sessions live inside a running engine; without a daemon
                // there is nothing running to list.
                None => Vec::new(),
            };
            if rows.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-agents-no-live-sessions-start-daemon-with"
                ));
            }
            for r in &rows {
                out.human(&session_line(r));
            }
            out.json_value(json!({"sessions": rows}));
        }
        SessionCmd::Abort { id } => {
            let client = ctx.node_client().await.context(bisa_core::text!(
                "cli-agents-aborting-needs-daemon-owns-session-start"
            ))?;
            client
                .post(&format!("/sessions/{id}/abort"), json!({}))
                .await?;
            out.say(&bisa_core::text!(
                "cli-agents-session-aborted-terminal-cannot-revive",
                id = id.to_string()
            ));
            out.json_value(json!({"aborted": id}));
        }
    }
    Ok(())
}

pub fn recall(ctx: &Ctx, out: &Out, cmd: RecallCmd) -> Result<()> {
    let ws = ctx.workspace()?;
    match cmd {
        RecallCmd::List { agent } => {
            let records = ws.recall_list(&agent_id(&agent)?)?;
            if records.is_empty() {
                out.say(&bisa_core::text!("cli-agents-agent-has-no-memories-yet"));
            }
            for r in &records {
                let orphan = if r.orphan { "  (orphan)" } else { "" };
                out.human(&format!(
                    "{:<28} {}{orphan}",
                    r.slug,
                    r.value
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(60)
                        .collect::<String>()
                ));
            }
            out.json_value(json!({"agent": agent, "records": records}));
        }
        RecallCmd::Get { agent, slug } => match ws.recall_get(&agent_id(&agent)?, &slug)? {
            Some(r) => {
                out.human(&r.value);
                out.json_value(json!({"record": r}));
            }
            None => bail!(bisa_core::text!(
                "cli-agents-no-memory-agent",
                slug = format!("{slug:?}"),
                agent = agent.to_string()
            )),
        },
    }
    Ok(())
}

/// A goal's status is a projection over its current run, so a listing that
/// shows one has to read the run.
fn status_of(ws: &bisa_store::Workspace, goal: &bisa_core::Goal) -> bisa_core::GoalStatus {
    let run = ws.get_current_run(goal.id).ok().flatten();
    goal.status(run.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::Effort;

    fn specs(models: &[&str]) -> Vec<String> {
        models.iter().map(|m| m.to_string()).collect()
    }

    /// A roster row says its state under `state.state` — the wire's tagged
    /// word — and the line reads it there: read from a `status` the row
    /// never had, the column was blank for every session.
    #[test]
    fn a_session_line_says_the_rows_state_and_what_it_is_at_work_on() {
        let worker = json!({
            "id": "01K0000000000000000000RUN1",
            "kind": "worker",
            "state": {"state": "running", "tool": "Bash"},
            "work_item": "01K000000000000000000ITEM1",
            "goal": "01K000000000000000000GOAL1",
        });
        assert_eq!(
            session_line(&worker),
            "01K0000000000000000000RUN1  running   worker        01K000000000000000000ITEM1"
        );
        let turn = json!({
            "id": "01K0000000000000000000RUN2",
            "kind": "conversation",
            "state": {"state": "thinking"},
            "conversation": "01K000000000000000000CONV1",
        });
        assert_eq!(
            session_line(&turn),
            "01K0000000000000000000RUN2  thinking  conversation  01K000000000000000000CONV1"
        );
        let wake = json!({
            "id": "01K0000000000000000000RUN3",
            "kind": "guided",
            "state": {"state": "idle"},
            "goal": "01K000000000000000000GOAL1",
        });
        assert!(session_line(&wake).ends_with("idle      guided        01K000000000000000000GOAL1"));
        let bare = json!({"id": "01K0000000000000000000RUN4", "kind": "terminal", "state": {"state": "done"}});
        assert_eq!(
            session_line(&bare),
            "01K0000000000000000000RUN4  done      terminal",
            "nothing trails a row that is about nothing"
        );
    }

    #[test]
    fn the_effort_flag_takes_every_word_of_the_vocabulary_and_inherit() {
        for choice in EffortChoice::ALL {
            assert_eq!(plan_effort(choice.as_str()).unwrap(), Some(choice));
        }
        assert_eq!(plan_effort("auto").unwrap(), Some(EffortChoice::Auto));
        assert_eq!(plan_effort("xhigh").unwrap(), Some(EffortChoice::Xhigh));
        // The CLI's own word: say nothing, and the setting decides.
        assert_eq!(plan_effort("inherit").unwrap(), None);
    }

    #[test]
    fn a_word_that_is_no_effort_is_refused_naming_the_ones_that_are() {
        for odd in ["ultra", "High", "x_high", "off", "", "Inherit"] {
            let refused = plan_effort(odd).unwrap_err();
            let text = refused
                .downcast_ref::<bisa_core::Text>()
                .unwrap_or_else(|| panic!("{odd:?} is refused in the catalog's words"));
            assert_eq!(text.id, "cli-agents-unknown-effort");
            let said = bisa_i18n::say(text);
            assert!(said.contains(&format!("{odd:?}")), "{said}");
            for word in [
                "inherit", "auto", "minimal", "low", "medium", "high", "xhigh", "max",
            ] {
                assert!(said.contains(word), "{word} is missing from: {said}");
            }
        }
    }

    #[test]
    fn a_model_spec_names_its_effort_after_an_at() {
        let choice = model_choice("claude-opus-5-5@max").unwrap();
        assert_eq!(
            choice,
            ModelChoice::new("claude-opus-5-5").at(EffortChoice::Max)
        );
        assert_eq!(
            model_choice("sonnet@auto").unwrap().effort,
            Some(EffortChoice::Auto)
        );
        // No effort named: the plan's.
        let plain = model_choice("claude-opus-5-5").unwrap();
        assert_eq!(plain, ModelChoice::new("claude-opus-5-5"));
        assert_eq!(plain.effort, None);
    }

    #[test]
    fn a_model_spec_takes_a_weight_an_effort_and_a_sentence_together() {
        let choice = model_choice("claude-opus-5-5[1m]=3@auto: design work").unwrap();
        assert_eq!(
            choice,
            ModelChoice {
                model: "claude-opus-5-5[1m]".into(),
                weight: 3,
                enabled: true,
                suited_for: Some("design work".into()),
                effort: Some(EffortChoice::Auto),
            }
        );
        // An `@` in the sentence is the sentence's.
        let mailed = model_choice("opus: write to ops@max").unwrap();
        assert_eq!(mailed.model, "opus");
        assert_eq!(mailed.effort, None);
        assert_eq!(mailed.suited_for.as_deref(), Some("write to ops@max"));
        // A weight that is no number is still refused.
        assert!(model_choice("opus=heavy@max").is_err());
        assert!(model_choice("@max").is_err(), "an effort is no model id");
    }

    #[test]
    fn an_id_keeps_an_at_of_its_own() {
        // A Vertex-style id: the tail is a date, not an effort.
        let dated = model_choice("claude-opus-5-5@20260101").unwrap();
        assert_eq!(dated.model, "claude-opus-5-5@20260101");
        assert_eq!(dated.effort, None);
        // Both: the last `@` is the effort's.
        let both = model_choice("claude-opus-5-5@20260101@max").unwrap();
        assert_eq!(both.model, "claude-opus-5-5@20260101");
        assert_eq!(both.effort, Some(EffortChoice::Max));
        let weighted = model_choice("claude-opus-5-5@20260101=2@low").unwrap();
        assert_eq!(weighted.model, "claude-opus-5-5@20260101");
        assert_eq!(weighted.weight, 2);
        assert_eq!(weighted.effort, Some(EffortChoice::Low));
        // A word after `@` that is nearly an effort is part of the id.
        assert_eq!(
            model_choice("opus@ultra").unwrap(),
            ModelChoice::new("opus@ultra")
        );
    }

    #[test]
    fn an_id_with_the_window_suffix_is_read_whole() {
        let wide = model_choice("claude-sonnet-5-5[1m]").unwrap();
        assert_eq!(wide, ModelChoice::new("claude-sonnet-5-5[1m]"));
        let asked = model_choice("claude-sonnet-5-5[1m]@xhigh").unwrap();
        assert_eq!(asked.model, "claude-sonnet-5-5[1m]");
        assert_eq!(asked.effort, Some(EffortChoice::Xhigh));
        assert_eq!(asked.weight, 1);
    }

    #[test]
    fn a_new_plan_takes_the_efforts_it_was_given() {
        let plan = model_plan(
            &specs(&["claude-opus-5-5[1m]@max", "claude-sonnet-5-5[1m]"]),
            None,
            Some(EffortChoice::High),
        )
        .unwrap();
        assert_eq!(plan.strategy, ModelStrategy::Fallback);
        assert_eq!(plan.effort, Some(EffortChoice::High));
        assert_eq!(plan.models[0].effort, Some(EffortChoice::Max));
        assert_eq!(plan.models[1].effort, None);
        assert_eq!(model_plan(&[], None, None).unwrap(), ModelPlan::default());
    }

    #[test]
    fn an_effort_alone_edits_the_plans_effort_and_leaves_the_models() {
        let mut current = ModelPlan::fallback(["opus", "sonnet"]).at(EffortChoice::Low);
        current.strategy = ModelStrategy::RoundRobin;
        current.models[1].effort = Some(EffortChoice::Max);

        let edited = edited_plan(current.clone(), &[], None, Some("xhigh")).unwrap();
        assert_eq!(edited.effort, Some(EffortChoice::Xhigh));
        assert_eq!(edited.models, current.models);
        assert_eq!(edited.strategy, ModelStrategy::RoundRobin);

        // `inherit` says nothing again.
        let cleared = edited_plan(current.clone(), &[], None, Some("inherit")).unwrap();
        assert_eq!(cleared.effort, None);
        assert_eq!(cleared.models, current.models);

        // A strategy alone leaves the effort where it was.
        let moved = edited_plan(current.clone(), &[], Some("weighted"), None).unwrap();
        assert_eq!(moved.strategy, ModelStrategy::Weighted);
        assert_eq!(moved.effort, Some(EffortChoice::Low));

        // Nothing said, nothing moved; a word that is no effort moves nothing.
        assert_eq!(
            edited_plan(current.clone(), &[], None, None).unwrap(),
            current
        );
        assert!(edited_plan(current, &[], None, Some("ultra")).is_err());
    }

    #[test]
    fn new_models_keep_the_effort_the_plan_had_unless_one_is_given() {
        let current = ModelPlan::fallback(["opus"]).at(EffortChoice::Max);
        let models = specs(&["claude-opus-5-5[1m]@max", "claude-sonnet-5-5[1m]"]);

        let kept = edited_plan(current.clone(), &models, None, None).unwrap();
        assert_eq!(kept.effort, Some(EffortChoice::Max));
        assert_eq!(kept.models.len(), 2);
        assert_eq!(kept.models[0].model, "claude-opus-5-5[1m]");

        let given = edited_plan(current.clone(), &models, None, Some("auto")).unwrap();
        assert_eq!(given.effort, Some(EffortChoice::Auto));

        let cleared = edited_plan(current, &models, Some("weighted"), Some("inherit")).unwrap();
        assert_eq!(cleared.effort, None);
        assert_eq!(cleared.strategy, ModelStrategy::Weighted);
    }

    #[test]
    fn a_plan_is_rendered_as_its_specs_read() {
        let plan = ModelPlan {
            strategy: ModelStrategy::Weighted,
            effort: Some(EffortChoice::High),
            models: vec![
                ModelChoice::weighted("claude-opus-5-5[1m]", 3).at(EffortChoice::Max),
                ModelChoice::new("claude-sonnet-5-5[1m]"),
                ModelChoice::suited("haiku", "quick edits").at(EffortChoice::Auto),
            ],
        };
        assert_eq!(
            render_plan(&plan),
            "claude-opus-5-5[1m]=3@max → claude-sonnet-5-5[1m] → haiku@auto (quick edits) \
             [Weighted] @high"
        );
        // Nobody named an effort: the line is what it was.
        assert_eq!(
            render_plan(&ModelPlan::fallback(["opus", "sonnet"])),
            "opus → sonnet [Fallback]"
        );
        // An empty plan may still say how hard the harness's default works.
        assert_eq!(render_plan(&ModelPlan::default()), "harness default");
        assert_eq!(
            render_plan(&ModelPlan::default().at(EffortChoice::Low)),
            "harness default @low"
        );
    }

    #[test]
    fn a_models_efforts_are_one_column_of_wire_words() {
        assert_eq!(
            effort_words(&[Effort::Low, Effort::High, Effort::Xhigh]),
            "low high xhigh"
        );
        assert_eq!(effort_words(&[]), "");
    }
    fn an_agent() -> Agent {
        serde_json::from_value(json!({
            "id": "scout",
            "name": "Scout",
            "description": "looks ahead",
            "system_prompt": "You look a site over.",
            "harness": "claude-code",
            "models": {"strategy": "fallback", "models": [
                {"model": "claude-opus-5-5[1m]", "weight": 1, "enabled": true}
            ]},
            "skills": ["survey"],
            "mcps": ["platform"],
            "tags": ["research"],
            "respond": "members",
            "decision_making": true,
            "pubkey": "ab".repeat(32),
            "origin": {"catalog": {"slug": "scout"}},
            "enabled": false,
            "created_at": 1,
        }))
        .expect("an agent's record")
    }

    /// What the command line sends a node is a body the node reads: every
    /// key one it knows, every word in its vocabulary.
    #[test]
    fn what_is_sent_to_the_node_is_a_body_the_node_reads() {
        let agent = an_agent();
        let edit: bisa_node::dto::PatchAgentBody =
            serde_json::from_value(edited_agent_body(&agent)).expect("the edit fits");
        assert_eq!(edit.name.as_deref(), Some("Scout"));
        assert_eq!(edit.description, Some(Some("looks ahead".to_string())));
        assert_eq!(edit.system_prompt.as_deref(), Some("You look a site over."));
        assert_eq!(edit.harness.as_deref(), Some("claude-code"));
        assert_eq!(edit.models.as_ref(), Some(&agent.models));
        assert_eq!(edit.skills, Some(vec!["survey".to_string()]));
        assert_eq!(edit.mcps, Some(vec!["platform".to_string()]));
        assert_eq!(edit.tags, Some(vec!["research".to_string()]));
        assert_eq!(edit.respond.as_deref(), Some("members"));
        assert_eq!(edit.enabled, Some(false));
        assert_eq!(edit.decision_making, Some(true));
        assert!(
            edit.photo.is_none(),
            "the picture is no word of the command line's"
        );

        let new = NewAgent {
            name: agent.name.clone(),
            photo: None,
            description: None,
            system_prompt: agent.system_prompt.clone(),
            harness: agent.harness.clone(),
            models: agent.models.clone(),
            skills: agent.skills.clone(),
            mcps: agent.mcps.clone(),
            tags: agent.tags.clone(),
            respond: RespondPolicy::OwnerOnly,
            decision_making: false,
        };
        let made: bisa_node::dto::NewAgentBody =
            serde_json::from_value(new_agent_body(&new)).expect("the new agent fits");
        assert_eq!(made.name, "Scout");
        assert_eq!(made.description, None);
        assert_eq!(made.respond.as_deref(), Some("owner_only"));
        assert_eq!(made.models.as_ref(), Some(&agent.models));
        assert_eq!(made.skills, vec!["survey".to_string()]);
        assert!(!made.decision_making);
    }

    #[test]
    fn a_member_is_said_as_the_node_reads_one() {
        let human = bisa_core::PrincipalId::new("cd".repeat(32)).expect("a key");
        assert_eq!(
            member_said(&Assignee::Human(human)),
            json!({"human": "cd".repeat(32)})
        );
        assert_eq!(
            member_said(&Assignee::Agent("scout".into())),
            json!({"agent": "scout"})
        );
        // A team in a team is the node's to refuse, in its own words.
        assert_eq!(
            member_said(&Assignee::Team("survey".into())),
            json!({"team": "survey"})
        );
    }

    #[test]
    fn a_reference_goes_into_a_route_as_an_id_and_nothing_else() {
        let scout = AgentId::new("scout").unwrap();
        assert_eq!(
            reference_route(&scout, "survey", true).unwrap(),
            "/agents/scout/skills/survey"
        );
        assert_eq!(
            reference_route(&scout, "platform", false).unwrap(),
            "/agents/scout/mcps/platform"
        );
        for odd in ["../mcp", "a/b", "a b", "", "survey?x=1"] {
            assert!(reference_route(&scout, odd, true).is_err(), "{odd:?}");
            assert!(reference_route(&scout, odd, false).is_err(), "{odd:?}");
        }
    }
}
