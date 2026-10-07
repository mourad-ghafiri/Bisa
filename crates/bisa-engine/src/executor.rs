//! The work-item executor: resolve a harness *and a model*, place the session
//! in the folder of the thing it works on, launch, drive, settle — and, when
//! the model dies rather than the work, do it again on the next model in the
//! agent's plan without losing the workstream or the clock.
//!
//! A work item exists because an `agent` step asked for it, and every
//! settlement reports back to that step through [`effects::item_settled`]:
//! a result that landed completes the step, anything else fails it.

use crate::events::{EnginePayload, ExecutionOutcome};
use crate::models::{self, ModelLedger};
use crate::registry::{AgentRef, AgentStatus, LiveRunId, SessionKind};
use crate::{
    assign, debug_on_err, effects, ops, projects, scheduler, warn_on_err_for, EngineError, Inner,
};
use bisa_core::event::JournalPayload;
use bisa_core::workitem::{WorkItemSpec, WorkItemState};
use bisa_core::{
    AgentId, Effort, EffortChoice, Home, SessionId, WorkItemId, WorkItemTransition, Workstream,
    WorkstreamKind,
};
use bisa_harness::{
    HarnessError, HarnessSession, LifecycleEvent, McpMount, McpServerConfig, ModelPlan, Outcome,
    ProgressEvent, SessionEvent, SessionSpec, SkillPayload,
};
use bisa_store::{SessionRow, SessionStatus};
use futures::StreamExt;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn new_ulid() -> ulid::Ulid {
    ulid::Ulid::from_datetime(SystemTime::now())
}

/// How a session settled, before its step hears of it.
#[derive(Debug, Clone, PartialEq)]
pub enum RunSettled {
    Completed,
    Aborted,
    Failed(String),
    BudgetExhausted,
    WallClockExceeded,
    /// The **model** hit a wall, not the work. Never a settlement: the caller
    /// either relaunches on the next model in the plan or, once the attempt
    /// budget is spent, converts it into a plain [`RunSettled::Failed`].
    ModelWall(ModelWall),
}

impl From<&RunSettled> for ExecutionOutcome {
    fn from(s: &RunSettled) -> Self {
        match s {
            RunSettled::Completed => ExecutionOutcome::Completed,
            RunSettled::Aborted => ExecutionOutcome::Aborted,
            RunSettled::Failed(e) => ExecutionOutcome::Failed { reason: e.clone() },
            RunSettled::BudgetExhausted => ExecutionOutcome::BudgetExhausted,
            RunSettled::WallClockExceeded => ExecutionOutcome::WallClockExceeded,
            RunSettled::ModelWall(w) => ExecutionOutcome::Failed {
                reason: w.sentence(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// The two-dimensional launch walk: harness candidates × the agent's model plan
// ---------------------------------------------------------------------------

/// One `(harness, model)` pair that said no, and how long it will keep saying
/// it.
///
/// The same struct carries both shapes of the fact, because to the engine they
/// *are* one fact seen at two moments (see `bisa-adapters`'s "Launch
/// versus mid-run"):
///
/// - `after_progress == false` — either a synchronous
///   [`HarnessError::ModelUnavailable`] from `launch()`, or a terminal
///   [`Outcome::ModelUnavailable`] that arrived before the session did any
///   real work. Both mean *the launch failed*; retrying on the next model is
///   transparent, because nothing happened.
/// - `after_progress == true` — the session ran tools or produced text and
///   *then* hit the wall. Still a retry, but not a transparent one: a partial
///   edit may already be sitting in the workstream, so the journal says so.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelWall {
    pub harness: String,
    /// The ledger's name for the model — the pinned id, or "<harness>
    /// default" for an unpinned session.
    pub model: String,
    pub reason: String,
    /// Seconds the harness itself asked us to wait, when it said so at all.
    /// The ledger owns the guess when it did not.
    pub retry_after: Option<u64>,
    /// Unix seconds the cooldown the ledger set expires at.
    pub cooldown_until: u64,
    pub after_progress: bool,
}

impl ModelWall {
    /// The half-sentence every report of this wall starts with.
    pub(crate) fn sentence(&self) -> String {
        let retry_in = self.cooldown_until.saturating_sub(now_secs());
        let when = if self.after_progress {
            "died mid-run"
        } else {
            "unavailable"
        };
        format!(
            "{} {when} on {} ({}, retry in {retry_in}s)",
            self.model, self.harness, self.reason
        )
    }
}

/// Which harnesses and which models one run may try.
pub(crate) struct LaunchPlan<'a> {
    /// Harness fallback chain, in order.
    pub candidates: &'a [String],
    /// The agent's plan. Ignored entirely when `pin` is set.
    pub plan: &'a ModelPlan,
    /// A hard pin (`WorkItemSpec::model`). **Never substituted**: the walk
    /// tries exactly this model, on each harness in turn, and fails loudly
    /// rather than quietly running something the user did not ask for.
    pub pin: Option<&'a str>,
    /// The model an `auto_route` plan leads with — the one the
    /// Decision-Making Agent picked for this task
    /// ([`crate::decider::route`]). A preference, never a pin: the rest of the
    /// plan still follows it, and a plan that is not routed does not read it.
    pub lead: Option<&'a str>,
    /// The step's effort pin (`WorkItemSpec::effort`): the first link of the
    /// chain, over the model's own and the plan's.
    pub effort_pin: Option<EffortChoice>,
    /// `agents.effort` as it resolves for the project the work stands in:
    /// the last link.
    pub effort_setting: EffortChoice,
    /// The level the Decision-Making Agent named for this walk
    /// ([`crate::decider::effort`]), read by every attempt that comes to
    /// `auto` and fitted to its own model. `None` when nobody was asked or
    /// nobody was sure: each attempt's own fallback runs.
    pub judged_effort: Option<Effort>,
    /// Handed to [`ModelPlan::order`]; one value per *run*, so `RoundRobin`
    /// advances across work items while a within-run retry is steered by the
    /// cooldowns this run just wrote.
    pub rotation: u64,
    /// Deliver skills in the first prompt instead of as files in the cwd.
    /// Set when the placement is an adopted root: the platform
    /// never writes its scratch into a folder it did not create.
    pub skills_in_prompt: bool,
}

impl LaunchPlan<'_> {
    /// The models to try on `harness`, best-first. `None` means "leave
    /// [`SessionSpec::model`] unset and let the harness run its default" —
    /// which is a single, real attempt, not the absence of one.
    fn models_for(&self, ledger: &ModelLedger, harness: &str) -> Vec<Option<String>> {
        if let Some(pin) = self.pin {
            return vec![Some(pin.to_string())];
        }
        let ordered: Vec<String> = self
            .plan
            .order(&ledger.view(harness), self.rotation, self.lead)
            .into_iter()
            .map(str::to_string)
            .collect();
        if ordered.is_empty() {
            vec![None]
        } else {
            ordered.into_iter().map(Some).collect()
        }
    }

    /// The effort an attempt on `model` is sent, fitted to what the harness
    /// `takes` for it. `None` when it takes none: nothing is sent.
    fn effort_for(&self, model: Option<&str>, takes: &[Effort]) -> Option<Effort> {
        crate::effort::fitted(
            self.plan,
            model,
            self.effort_pin,
            self.effort_setting,
            self.judged_effort,
            takes,
        )
    }
}

/// The attempt budget for one run, shared by the launch walk and the mid-run
/// relaunch loop so a failover can never reset it.
pub(crate) struct Attempts {
    used: usize,
    max: usize,
}

impl Attempts {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            used: 0,
            max: max.max(1),
        }
    }

    fn take(&mut self) -> bool {
        if self.used >= self.max {
            return false;
        }
        self.used += 1;
        true
    }

    pub(crate) fn spent(&self) -> bool {
        self.used >= self.max
    }
}

/// A live session, and what it cost to get one.
pub(crate) struct Launched {
    pub harness: String,
    pub session: Box<dyn HarnessSession>,
    /// Skills the harness could not host natively, for the first prompt.
    pub skill_appendix: Option<String>,
    /// The ledger's name for what went into [`SessionSpec::model`] — the
    /// model id, or `"<harness> default"` for an unpinned session. What the
    /// ledger, the plan and the journal all agree to call it.
    pub model_key: String,
    /// The effort the session was launched at, after the fit; `None` when
    /// the harness takes none for this model and nothing was sent.
    pub effort: Option<Effort>,
    /// Holds this pair's `LeastBusy` slot for as long as the session lives.
    pub in_flight: models::InFlight,
    /// Model walls walked past on the way here, oldest first.
    pub walls: Vec<ModelWall>,
}

/// No session, and why.
pub(crate) struct LaunchFailure {
    pub message: String,
    pub walls: Vec<ModelWall>,
}

/// Resolve the first `(harness, model)` pair that probes available AND
/// launches.
///
/// The walk is two-dimensional, and the two dimensions are walked for
/// different reasons — this is the whole point of the taxonomy in
/// [`HarnessError`]:
///
/// | What happened | What the walk does |
/// |---|---|
/// | probe says unavailable, or `launch` returns [`HarnessError::Unavailable`] | abandon this **harness**, move to the next candidate |
/// | `launch` returns [`HarnessError::ModelUnavailable`] | mark the ledger, try the next **model** on the same harness |
/// | any other launch error | fail the run (two-phase discipline: a real error is not a fallback signal) |
///
/// `tried` is per-run and keyed on the pair, so a relaunch after a mid-run
/// wall never comes back to a model this run already buried — even though
/// [`ModelPlan::order`] deliberately keeps yielding cooling models (a cooldown
/// is a guess, and "everything is briefly throttled" must not become "this
/// item failed"). When the untried pairs run out, so does the run.
pub(crate) async fn resolve_and_launch(
    inner: &Inner,
    launch: &LaunchPlan<'_>,
    tried: &mut HashSet<(String, String)>,
    attempts: &mut Attempts,
    session_spec: &SessionSpec,
) -> Result<Launched, LaunchFailure> {
    let mut reasons = Vec::new();
    let mut walls = Vec::new();
    for candidate in launch.candidates {
        if inner.config.disabled_harnesses.contains(candidate) {
            reasons.push(format!("{candidate}: disabled"));
            continue;
        }
        let Some(adapter) = inner.catalog.get(candidate) else {
            reasons.push(format!("{candidate}: no adapter"));
            continue;
        };
        let probe = adapter.probe().await;
        if !probe.available {
            reasons.push(format!("{candidate}: {}", probe.reason.unwrap_or_default()));
            continue;
        }
        // Skills are delivered per harness: native skill files where the
        // harness has a skills dir, otherwise as an instructions appendix
        // the caller appends before the first prompt. An adopted root gets
        // the appendix whatever the harness: no file of ours lands there.
        let mut injection = if launch.skills_in_prompt {
            bisa_harness::skills::as_appendix(&session_spec.skills)
        } else {
            bisa_harness::skills::materialize(&session_spec.skills, &session_spec.cwd, candidate)
        };
        // An installed MCP server rides only where its tools are judged; a
        // harness that is not handed one is told, in the first prompt.
        let (mounts, unmounted) =
            mounts_for_harness(inner, adapter.caps(), &session_spec.mcp_servers);
        if !unmounted.is_empty() {
            tracing::info!(
                harness = %candidate,
                servers = ?unmounted,
                "installed MCP servers kept off an observed harness"
            );
            injection
                .prompt_appendix
                .get_or_insert_with(String::new)
                .push_str(&unmounted_note(&unmounted));
        }
        for model in launch.models_for(&inner.models, candidate) {
            let key = models::model_key(candidate, model.as_deref());
            if !tried.insert((candidate.clone(), key.clone())) {
                continue; // this run already buried this pair
            }
            if !attempts.take() {
                return Err(LaunchFailure {
                    message: format!(
                        "model attempts exhausted after {} launches; last tried {key} on {candidate}",
                        attempts.used
                    ),
                    walls,
                });
            }
            let mut spec = session_spec.clone();
            spec.model = model.clone();
            // Who decides for this model, then what it takes on this harness:
            // each attempt is fitted to its own list, and an empty one sends
            // nothing.
            let effort = launch.effort_for(model.as_deref(), &adapter.efforts(model.as_deref()));
            spec.effort = effort;
            spec.mcp_servers = mounts.clone();
            match adapter.launch(spec).await {
                Ok(session) => {
                    return Ok(Launched {
                        in_flight: inner.models.acquire(candidate, &key),
                        harness: candidate.clone(),
                        // Every driver launches here, so every prompt any of
                        // them sends is redacted by this one line.
                        session: crate::security::wrap_session(inner, session),
                        skill_appendix: injection.prompt_appendix,
                        model_key: key,
                        effort,
                        walls,
                    });
                }
                Err(HarnessError::ModelUnavailable {
                    model: reported,
                    reason,
                    retry_after,
                }) => {
                    let until = inner.models.note_unavailable(candidate, &key, retry_after);
                    reasons.push(format!("{candidate}/{reported}: {reason}"));
                    walls.push(ModelWall {
                        harness: candidate.clone(),
                        model: key,
                        reason,
                        retry_after,
                        cooldown_until: until,
                        after_progress: false,
                    });
                }
                Err(HarnessError::Unavailable(r)) => {
                    reasons.push(format!("{candidate}: {r}"));
                    break; // the harness is out, not just this model
                }
                Err(e) => {
                    return Err(LaunchFailure {
                        message: format!("{candidate}: {e}"),
                        walls,
                    })
                }
            }
        }
    }
    let message = if reasons.is_empty() {
        // Every pair this run could reach, it has already buried.
        "no untried model left on any harness candidate".to_string()
    } else {
        format!("no harness available: [{}]", reasons.join("; "))
    };
    Err(LaunchFailure { message, walls })
}

/// **The harnesses an item is launched on**, in order — one rule, read once
/// the item is taken:
///
/// 1. the ones its step named: the author's word, whoever takes the item;
/// 2. else the harness of the agent that took it — an agent runs on its own
///    harness, which is what made it a candidate (`assign::workers` offers
///    work only to an agent whose harness is there);
/// 3. else the platform's default.
fn harnesses_for(inner: &Inner, spec: &WorkItemSpec) -> Vec<String> {
    if !spec.harness_candidates.is_empty() {
        return spec.harness_candidates.clone();
    }
    let of_its_agent = spec
        .agent
        .as_deref()
        .and_then(|a| AgentId::new(a).ok())
        .and_then(|a| inner.ws.get_agent(&a).ok())
        .map(|def| def.harness);
    vec![of_its_agent.unwrap_or_else(|| bisa_core::DEFAULT_HARNESS.to_string())]
}

/// The model plan a work item runs under: its agent's, or the empty plan
/// ("whatever the harness runs by default") when it has no agent definition.
///
/// A `WorkItemSpec::model` pin is handled by [`LaunchPlan::pin`], not here —
/// a pin bypasses the plan rather than replacing it, so that an unavailable
/// pin fails loudly instead of falling through to a model nobody asked for.
fn model_plan_for(inner: &Inner, spec: &WorkItemSpec) -> ModelPlan {
    spec.agent
        .as_deref()
        .and_then(|a| AgentId::new(a).ok())
        .and_then(|a| inner.ws.get_agent(&a).ok())
        .map(|def| def.models)
        .unwrap_or_default()
}

/// The closing contract appended to every work-item's first prompt.
fn result_protocol(spec: &WorkItemSpec) -> String {
    let schema_line = match &spec.output_schema {
        Some(schema) => format!(
            "\nYour result must conform to this JSON Schema:\n{}\n",
            serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string())
        ),
        None => String::new(),
    };
    format!(
        "\n\n---\n## How this work item completes\n\
         Report progress as you go with the `report_progress` tool.\n\
         When you make something for a person to look at — a page, a chart, a report, a \
         sheet, a deck, an image — post it as an artifact with `post_message` \
         (artifacts: [{{path, title}}], a path in this checkout or your scratch); it \
         renders live in the conversation you post it to.\n\
         When the work is done you MUST call the `yield_result` tool with a structured \
         summary of what you produced — that call is what marks this item complete. \
         Finishing your turn without it fails the item, however good the work was.{schema_line}\n\
         If something blocks you or a decision is genuinely the human's to make, use \
         `ask_human` rather than guessing."
    )
}

/// A work item standing in a project's workstream: committed, patched or
/// left in the primary when the item settles.
struct Checkout {
    project: bisa_core::project::Project,
    workstream: Workstream,
    cwd: PathBuf,
    iso: Option<(bisa_iso::BackendKind, PathBuf, PathBuf)>,
}

/// Where a work item's session stands, decided once before launch and held
/// across every model attempt.
enum Placement {
    /// A project's workstream. Boxed: the records are many times a path, and
    /// the placement lives across the whole attempt loop.
    Checkout(Box<Checkout>),
    /// The home's scratch folder — the goal's, or the run of the
    /// workspace's: no project, no workstream; the result the session yields
    /// is its whole deliverable.
    Scratch { cwd: PathBuf },
}

impl Placement {
    fn cwd(&self) -> &std::path::Path {
        match self {
            Placement::Checkout(c) => &c.cwd,
            Placement::Scratch { cwd } => cwd,
        }
    }

    fn workstream(&self) -> Option<&Workstream> {
        match self {
            Placement::Checkout(c) => Some(&c.workstream),
            Placement::Scratch { .. } => None,
        }
    }

    /// A workstream of an adopted project is still the adopted repository —
    /// a worktree shares its owner's `.git`, and a checkout is theirs — so
    /// skills ride the prompt there rather than files in the tree.
    fn skills_in_prompt(&self) -> bool {
        match self {
            Placement::Checkout(c) => c.project.root.is_external(),
            Placement::Scratch { .. } => false,
        }
    }
}

/// Place a work item: in a workstream of its project when it has one, in its
/// home's scratch folder when it has none. The two sentences a failure can
/// say name what failed — the project's placement, or the workstream.
async fn place(inner: &Inner, spec: &mut WorkItemSpec) -> Result<Placement, String> {
    let project = projects::project_for_step(inner, spec)
        .await
        .map_err(|e| format!("project placement failed: {e}"))?;
    let Some(project) = project else {
        return Ok(Placement::Scratch {
            cwd: projects::scratch_dir(inner, &spec.home),
        });
    };
    let place = projects::open_workstream(inner, spec, &project)
        .await
        .map_err(|e| format!("workstream failed: {e}"))?;
    Ok(Placement::Checkout(Box::new(Checkout {
        project,
        workstream: place.workstream,
        cwd: place.cwd,
        iso: place.iso,
    })))
}

/// Where this session is standing, and what that place is for.
///
/// Four placements, and they do not make the same promise. A worktree
/// workstream is a branch that gets committed when the item settles; a copy
/// workstream stays while its run may read it and survives it only as a
/// patch; the project's primary
/// workstream — the repository itself, where a run lands while HEAD is still
/// unborn — has nothing committing for it; and the goal's scratch folder —
/// an item that names no project on a goal with none — is not a repository
/// at all, where the result the session yields is the whole deliverable.
/// Nothing used to say which applied — the first prompt was the plan
/// author's instructions and the result protocol, both silent on placement
/// — and an agent told nothing about where it is standing assumes the
/// version control it needs is there, then runs `git init` when it finds
/// it is not.
/// What an auto goal's step is told after its instructions: the run is
/// unattended, so a question costs the goal its watcher.
pub const UNATTENDED_APPENDIX: &str =
    "\n\nThis goal runs unattended: nobody is watching for a question. \
Decide with sensible defaults and say your assumptions in your result; ask a person only for a \
fact only they hold or an act only they may take.";

/// Whether the work item's home runs unattended — an auto goal. A run of the
/// workspace is attended: whoever started it, a person reads its questions.
fn runs_unattended(inner: &Inner, home: &Home) -> bool {
    home.goal()
        .and_then(|goal| inner.ws.get_goal(goal).ok())
        .is_some_and(|g| g.mode.unattended())
}

/// What the first prompt says when a restart cut an earlier session on this
/// item short: the checkout is where that session got to.
fn interruption_note(interruptions: u8) -> String {
    let (cut_short, whose) = match interruptions {
        1 => ("A previous session on this work was".to_string(), "Its"),
        n => (format!("{n} previous sessions on this work were"), "Their"),
    };
    format!(
        "\n\n{cut_short} interrupted by a restart of the platform before finishing. {whose} work so far is in this folder as it was left: read what is there before you act, continue from there rather than starting over, and yield the result the same way."
    )
}

fn placement_note(placement: &Placement) -> String {
    let cwd = placement.cwd().display();
    let body = match placement {
        Placement::Scratch { .. } => format!(
            "`{cwd}` is this work's scratch folder. This item names no project and nothing \
             it runs for has one, so nothing here is a repository and nothing commits it: \
             read and analyse here freely, and your deliverable is the result you yield. If \
             you were asked for files that must be kept, call create_project first — it makes \
             a repository (attached to the goal, when the work is a goal's) — and work inside \
             the path it returns."
        ),
        Placement::Checkout(checkout) => match &checkout.workstream.kind {
            WorkstreamKind::Worktree { branch, .. } => format!(
                "`{cwd}` is a checkout of the project `{}` on the branch `{branch}`. What you \
                 leave in the tree is committed to that branch when this item settles, so leave \
                 it in the state you want committed. Do not commit, push or open a pull request \
                 yourself. Files belong here, not outside it.",
                checkout.project.slug
            ),
            WorkstreamKind::Copy => format!(
                "`{cwd}` is a working copy of the project `{}`, which is not under version \
                 control. The copy is taken down when this run ends and what survives is \
                 the diff, so anything you want kept must be a change to files in this tree — \
                 not a note somewhere else on disk.",
                checkout.project.slug
            ),
            // The primary workstream: a repository with no commit to branch
            // from yet — the session runs in the project itself.
            WorkstreamKind::Primary => format!(
                "`{cwd}` is the project `{}` itself — its primary workstream, so this run works \
                 in it directly rather than on a branch of its own. Files belong here. Nothing \
                 commits for you at this placement: leave the tree in the state you want a \
                 person to commit, and do not write outside it.",
                checkout.project.slug
            ),
        },
    };
    format!("\n\n---\n## Where you are\n{body}")
}

/// The MCP servers an agent definition names, mounted as installed: the
/// registry's transports (a disabled or unresolvable id costs the session
/// that one server, warned about inside the store), each wearing the
/// provenance that puts its tools through the guard.
pub(crate) fn installed_mounts(
    inner: &Inner,
    agent: &AgentId,
    ids: &[bisa_core::McpId],
) -> Vec<McpMount> {
    inner
        .ws
        .mcp_configs(agent, ids)
        .into_iter()
        .map(McpMount::installed)
        .collect()
}

/// `security.mcp.observed`: whether a harness the guard cannot judge is
/// handed the MCP servers a person installed on an agent.
pub const OBSERVED_MCP_KEY: &str = "security.mcp.observed";

/// The mounts a candidate harness is handed, and the names it is not: every
/// platform mount always; an installed one where the harness stops before a
/// tool runs and obeys the guard (`TOOL_GUARD`), or where the person set
/// `security.mcp.observed` to `allow`. An observed harness asks nobody the
/// platform can hear, so a server that reaches the machine or the outside
/// world is kept off it by default.
pub(crate) fn mounts_for_harness(
    inner: &Inner,
    caps: bisa_core::HarnessCaps,
    mounts: &[McpMount],
) -> (Vec<McpMount>, Vec<String>) {
    let judged = caps.contains(bisa_core::HarnessCaps::TOOL_GUARD);
    let allowed = inner
        .ws
        .setting(OBSERVED_MCP_KEY, None)
        .ok()
        .and_then(|r| r.value.as_str().map(|s| s == "allow"))
        .unwrap_or(false);
    if judged || allowed {
        return (mounts.to_vec(), Vec::new());
    }
    let (kept, dropped): (Vec<_>, Vec<_>) = mounts.iter().cloned().partition(McpMount::is_platform);
    (kept, dropped.iter().map(|m| m.name().to_string()).collect())
}

/// What a session on an observed harness is told about the servers it was
/// not handed.
pub(crate) fn unmounted_note(dropped: &[String]) -> String {
    let list = dropped
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\n\nThe MCP server{} {list} {} installed on this agent but not mounted in this session: \
         this harness runs under its own approval prompt, which the platform's guard cannot judge \
         (`{OBSERVED_MCP_KEY}`). Work with the tools you have, and say so in your result if the \
         work needed {}.",
        if dropped.len() == 1 { "" } else { "s" },
        if dropped.len() == 1 { "is" } else { "are" },
        if dropped.len() == 1 { "it" } else { "them" },
    )
}

/// Build the SessionSpec for a work-item. Every session gets the Bisa
/// MCP server — the goal tools (result, progress, questions, notes, spawn)
/// are the platform's hands, not an option.
fn build_session_spec(
    inner: &Inner,
    spec: &WorkItemSpec,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
) -> SessionSpec {
    // The proxy the platform follows, handed to the harness as its tools read it.
    let (env, env_remove) = crate::network::session_env(inner, env);
    let command = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "bisa".into());
    let mut mcp_servers = vec![McpMount::platform(McpServerConfig::Stdio {
        name: "bisa".into(),
        command,
        args: vec![
            "mcp".into(),
            "--socket".into(),
            inner.socket_path.display().to_string(),
            "--work-item".into(),
            spec.id.to_string(),
        ],
        // Where the MCP server writes its own log: the parent's folder,
        // named — it is spawned without `--data-dir` and must not guess.
        env: crate::logging::child_env(inner),
        cwd: None,
    })];
    // Skills and extra MCP servers are *references* on the item's Agent
    // definition, resolved here against the workspace library and registry.
    // The skill markdown rides the SessionSpec and is materialized per
    // candidate harness in `resolve_and_launch`; an installed server is
    // mounted with its provenance, so a judged harness puts its tools to the
    // guard and an observed one is handed it only where the person allows.
    let mut skills: Vec<SkillPayload> = Vec::new();
    if let Some(agent) = spec.agent.as_deref().and_then(|a| AgentId::new(a).ok()) {
        if let Ok(def) = inner.ws.get_agent(&agent) {
            mcp_servers.extend(installed_mounts(inner, &def.id, &def.mcps));
            skills = inner.ws.skill_payloads(&def.id, &def.skills);
        }
    }
    SessionSpec {
        work_item: Some(spec.id),
        cwd,
        // Contract: leave empty — the executor drives the first turn via
        // `session.prompt()` below; a non-empty launch prompt would make the
        // adapter auto-send and the explicit prompt() would then hit Busy.
        prompt: String::new(),
        // The model is chosen per attempt by `resolve_and_launch`, from the
        // item's pin or the agent's plan; this is the template.
        model: None,
        // And the effort with it, fitted to that model.
        effort: None,
        mcp_servers,
        env,
        env_remove,
        tier_ceiling: spec.tier_ceiling,
        output_schema: spec.output_schema.clone(),
        skills,
    }
}

/// Execute one work-item to settlement. Never panics; every failure path
/// settles the item and emits events.
pub async fn run_work_item(inner: Arc<Inner>, mut spec: WorkItemSpec) {
    let home = spec.home;
    let item_id = spec.id;
    // What everything this item's sessions emit is about, read once.
    let scope = inner.item_scope(&spec);
    inner.active_items.insert(item_id, home);
    inner.pause.wait_running().await;

    // Assignment resolves in one place (`assign::workers`): the item's
    // project, its goal and every ancestor, teams expanded, humans dropped.
    // Persisted on the item so the journal, the board and the transcript all
    // show who actually took it.
    if spec.agent.is_none() {
        let pool = assign::workers(&inner, &spec).await;
        if let Some(agent) = assign::choose(&inner, &pool, &spec).await {
            let who = AgentId::new(&agent)
                .ok()
                .and_then(|id| inner.ws.get_agent(&id).ok())
                .map(|d| d.name)
                .unwrap_or_else(|| agent.clone());
            spec.agent = Some(agent);
            warn_on_err_for(
                home,
                Some(item_id),
                inner.ws.put_work_item(&spec),
                "recording the routed agent",
            );
            // Attribution belongs in the journal, not just the row: the
            // assignment is why this agent has the work.
            let owner = inner.ws.owner_keys().clone();
            warn_on_err_for(
                home,
                Some(item_id),
                inner.ws.append_journal(
                    &home,
                    JournalPayload::Note {
                        text: format!("routed to {who}"),
                    },
                    &owner,
                    None,
                ),
                "journaling the routing",
            );
        }
    }

    // Where it runs, now that who takes it is known — said on the item, so a
    // session resumed after a restart runs where the first one did.
    let harnesses = harnesses_for(&inner, &spec);
    if harnesses != spec.harness_candidates {
        spec.harness_candidates = harnesses;
        warn_on_err_for(
            home,
            Some(item_id),
            inner.ws.put_work_item(&spec),
            "recording the harness the item runs on",
        );
    }

    // Placement, before launch: the session's cwd is decided here, once, and
    // held across every model attempt.
    //
    // An item with a project runs in a **workstream** — for a git project that
    // is a worktree on its own branch, and the worktree *is* the isolation.
    // Layering a copy or an overlay on top of one would buy nothing (the tree
    // is already separate from every other checkout) and would cost the run
    // its one real result, the commit: the session's writes would land in a
    // merged view that gets thrown away instead of in the branch.
    //
    // An item with none — the step named no project and nothing it runs for
    // has one — runs in its home's own `scratch/`, the goal's or the run of
    // the workspace's: not a repository, never committed, and its result is
    // its deliverable. Nothing is created here: no project
    // is born of a step (an agent asked for files that must be kept makes one
    // with `create_project`), and no workstream is opened for a session that
    // has nowhere to commit.
    //
    // There used to be a third answer — a caller-supplied `cwd`, defaulting
    // to the workspace root, optionally copied into `run/iso/<item>` when the
    // item asked to be isolated. Every part of that was wrong: the default
    // handed an agent the journal, the index and the key files. The goal's
    // folder answers all of it: it is the goal's, it is not truth, and it is
    // still there when the run ends.
    // `TMPDIR` keeps a tool's scratch inside the home instead of in the
    // system temp directory, where it outlives the run and belongs to nobody.
    // The home's scratch folder is exactly that — scratch. Well-behaved tools
    // only: this is placement, not a sandbox, and a process that opens
    // `/tmp` by name is unaffected.
    let tmp = inner.ws.paths().home(&spec.home).tmp();
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        spec.state = settle_failed(&inner, &spec, format!("scratch directory failed: {e}")).await;
        return;
    }
    let env = BTreeMap::from([("TMPDIR".to_string(), tmp.display().to_string())]);
    let placement = match place(&inner, &mut spec).await {
        Ok(placement) => placement,
        Err(reason) => {
            spec.state = settle_failed(&inner, &spec, reason).await;
            return;
        }
    };

    // Acquire concurrency permits.
    //
    // Everything from here is *inside the attempt loop*, except the three
    // things that must survive a model failover:
    //
    //  * the placement — the workstream, opened once above. A relaunch re-enters
    //    the same directory rather than calling `open_workstream` again, because a
    //    second `git worktree add` on the same item's branch would fail
    //    outright, and a second copy workstream would silently abandon whatever
    //    the dead session had already written.
    //  * `started_at` — the wall clock. A failover does not buy the item a
    //    fresh budget; each attempt's timeout is what is *left*.
    //  * `attempts` / `tried` — the bound. Both the launch walk and the
    //    mid-run relaunch draw on the same budget, so a run can never loop.
    let base_spec = build_session_spec(&inner, &spec, placement.cwd().to_path_buf(), env);
    let first_candidate = spec.harness_candidates.first().cloned().unwrap_or_default();
    let _permits = match inner.caps.acquire(&first_candidate).await {
        Ok(permits) => permits,
        Err(e) => {
            spec.state = settle_failed(&inner, &spec, e.to_string()).await;
            return;
        }
    };

    // Signing as the item's Agent when one is set (per-agent attribution;
    // owner otherwise).
    let (signer, attestation) = ops::signer_for(&inner.ws, spec.agent.as_deref());
    let plan = model_plan_for(&inner, &spec);
    // The setting is read for the project the work stands in.
    let effort_setting = crate::effort::setting(
        &inner,
        placement.workstream().map(|w| w.project).or(spec.project),
    );
    let rotation = inner.models.next_rotation();
    // What the Decision-Making Agent names for the walk, asked once: the
    // model that leads — a pinned step is never routed, the pin is the
    // author's word — and the level of every attempt that comes to `auto`.
    let judged = crate::decider::walk(
        &inner,
        &crate::decider::WalkAsk {
            plan: &plan,
            candidates: &spec.harness_candidates,
            model_pin: spec.model.as_deref(),
            effort_pin: spec.effort,
            effort_setting,
            rotation,
            task: &spec.instructions,
            agent: spec.agent.as_deref(),
        },
        &crate::decider::Standing {
            home: Some(spec.home),
            run: spec.run,
            step: spec.step.clone(),
            project: spec.project,
            agent: spec.agent.clone(),
            ..Default::default()
        },
    )
    .await;
    let launch_plan = LaunchPlan {
        candidates: &spec.harness_candidates,
        plan: &plan,
        pin: spec.model.as_deref(),
        lead: judged.lead.as_deref(),
        effort_pin: spec.effort,
        effort_setting,
        judged_effort: judged.effort,
        rotation,
        // A workstream of an adopted project is still the adopted repository:
        // a worktree shares its owner's `.git`, and a checkout is theirs.
        skills_in_prompt: placement.skills_in_prompt(),
    };
    let mut tried: HashSet<(String, String)> = HashSet::new();
    let mut attempts = Attempts::new(inner.config.max_model_attempts);
    let wall_clock = Duration::from_secs(
        spec.budget
            .max_wall_clock_secs
            .unwrap_or(inner.config.default_wall_clock_secs),
    );
    let started_at = SystemTime::now();
    // Walls seen but not yet reported: a mid-run wall only knows what it is
    // switching *to* once the next attempt has resolved a model, so reporting
    // waits for the relaunch.
    let mut pending_walls: Vec<ModelWall> = Vec::new();
    // Every wall of the whole run, kept for the one sentence that ends up on
    // the work item. The journal has them one at a time; the item's blocked
    // reason has to name all of them at once, or a human reading the board
    // sees only the last model that died.
    let mut all_walls: Vec<ModelWall> = Vec::new();

    let mut last_run: Option<LiveRunId> = None;
    let settled = loop {
        let launched =
            match resolve_and_launch(&inner, &launch_plan, &mut tried, &mut attempts, &base_spec)
                .await
            {
                Ok(ok) => ok,
                // No session this time round. Break rather than return: the
                // teardown below is the *only* place the clock is banked and
                // the workstream is settled, and a failover that already burned
                // minutes and wrote half an edit must not skip either.
                Err(failure) => {
                    pending_walls.extend(failure.walls.iter().cloned());
                    all_walls.extend(failure.walls);
                    let reason = give_up_reason(&all_walls, &failure.message);
                    report_switches(
                        &inner,
                        (home, scope),
                        Some(item_id),
                        &pending_walls,
                        None,
                        &signer,
                        attestation.clone(),
                    );
                    tracing::warn!(work_item = %item_id, "work item failed: {reason}");
                    break RunSettled::Failed(reason);
                }
            };
        pending_walls.extend(launched.walls.iter().cloned());
        all_walls.extend(launched.walls.iter().cloned());
        report_switches(
            &inner,
            (home, scope),
            Some(item_id),
            &pending_walls,
            Some(&launched.model_key),
            &signer,
            attestation.clone(),
        );
        pending_walls.clear();

        let harness_id = launched.harness.clone();
        let model_key = launched.model_key.clone();
        let session = launched.session;
        inner.emit(scope.event(
            Some(item_id),
            EnginePayload::Scheduled {
                harness: harness_id.clone(),
            },
        ));

        // Register the agent + session (engine owns identity). Each attempt is
        // a distinct session and gets its own pair of ids.
        let agent_id = LiveRunId::from_ulid(new_ulid());
        let session_id = SessionId::from_ulid(new_ulid());
        let transcript = session.resume_token().and_then(|t| t.transcript_path);
        debug_on_err(
            inner.registry.register_if(
                AgentRef {
                    id: agent_id,
                    kind: SessionKind::Worker,
                    status: AgentStatus::Running,
                    generation: 1,
                    session_id: Some(session_id),
                    work_item: Some(item_id),
                    conversation: None,
                    goal: None,
                    workstream: placement.workstream().map(|w| w.id),
                    transcript_path: transcript.clone(),
                    last_activity: now_secs(),
                },
                None,
            ),
            "registering a worker run",
        );
        last_run = Some(agent_id);
        inner.presence.register(
            &inner,
            agent_id,
            crate::presence::SessionMeta {
                kind: SessionKind::Worker,
                harness: harness_id.clone(),
                model: Some(model_key.clone()),
                effort: launched.effort,
                agent: spec.agent.as_deref().and_then(|a| AgentId::new(a).ok()),
                session_id: Some(session_id),
                work_item: Some(item_id),
                conversation: None,
                goal: home.goal(),
                run: spec.run,
                workstream: placement.workstream().map(|w| w.id),
                project: placement.workstream().map(|w| w.project),
                transcript_path: transcript.as_ref().map(|p| p.display().to_string()),
            },
        );
        warn_on_err_for(
            home,
            Some(item_id),
            inner.ws.record_session(&SessionRow {
                id: session_id.to_string(),
                adapter: harness_id.clone(),
                kind: SessionKind::Worker,
                conversation: None,
                work_item: Some(item_id.to_string()),
                workstream: placement.workstream().map(|w| w.id.to_string()),
                agent_id: spec.agent.clone(),
                transcript_path: transcript.map(|p| p.display().to_string()),
                resume_token_json: session
                    .resume_token()
                    .and_then(|t| serde_json::to_string(&t).ok()),
                status: SessionStatus::Live,
                parked_at: None,
                pid: None,
                pid_seen_at: None,
                ended_at: None,
            }),
            "recording a worker session",
        );

        // Claim + mark in progress, through the item's transitions.
        match mark_in_progress(
            &inner,
            &spec,
            &harness_id,
            session_id,
            &signer,
            attestation.clone(),
        ) {
            Ok(state) => spec.state = state,
            Err(e) => {
                warn_on_err_for(
                    home,
                    Some(item_id),
                    session.dispose().await,
                    "disposing an unclaimable session",
                );
                drop(launched.in_flight);
                spec.state =
                    settle_failed(&inner, &spec, format!("cannot claim the work item: {e}")).await;
                return;
            }
        }

        let events = session.subscribe();
        // Skills the harness cannot host natively ride the first prompt.
        let mut first_prompt = match &launched.skill_appendix {
            Some(appendix) => format!("{}\n\n{appendix}", spec.instructions),
            None => spec.instructions.clone(),
        };
        // A goal's run is told its goal: what the work is for, whatever the
        // step's own words say. A run of the workspace has none to tell.
        if let Some(note) = home
            .goal()
            .and_then(|g| crate::framing::goal_note(&inner, g))
        {
            first_prompt.push_str(&note);
        }
        // An auto goal runs with nobody present: the agent is told, so it
        // decides with defaults and asks only what a person alone can give.
        let unattended = runs_unattended(&inner, &home);
        if unattended {
            first_prompt.push_str(UNATTENDED_APPENDIX);
        }
        // A restart cut a previous session on this item short: the new one
        // is told, because the checkout already holds that session's work
        // and a fresh start over it would be a second attempt at the same
        // thing.
        if spec.interruptions > 0 {
            first_prompt.push_str(&interruption_note(spec.interruptions));
        }
        // The result protocol is the platform's guarantee, not the step
        // author's job: a work item is only complete when the session yields a
        // structured result, so every first prompt states that contract
        // explicitly. Placement is the same kind of fact: the step author
        // chose the project, but only the executor knows what that turned into
        // on disk.
        first_prompt.push_str(&placement_note(&placement));
        first_prompt.push_str(&crate::framing::browser_note(unattended));
        // Mobile development, when this machine does it: the tools and the
        // run line, said only where they would answer.
        first_prompt.push_str(&crate::framing::mobile_development_note(
            &inner,
            match &placement {
                Placement::Checkout(c) => Some(c.project.id),
                Placement::Scratch { .. } => None,
            },
        ));
        // The person's context, when there is any: the documents given to
        // the goal, by their folder. Read, never guessed at.
        if let Some(note) = home.goal().and_then(|g| crate::documents::note(&inner, g)) {
            first_prompt.push_str("\n\n");
            first_prompt.push_str(&note);
        }
        first_prompt.push_str(&result_protocol(&spec));

        let outcome = if let Err(e) = session.prompt(first_prompt.as_str().into()).await {
            RunSettled::Failed(format!("prompt failed: {e}"))
        } else {
            drive_session(
                &inner,
                scope,
                agent_id,
                session_id,
                &spec,
                session.as_ref(),
                events,
                &harness_id,
                &model_key,
                started_at,
                wall_clock,
                &signer,
                attestation.clone(),
            )
            .await
        };

        // Tear this attempt down before deciding whether there is another.
        if let Some(agent) = inner.registry.get(agent_id) {
            debug_on_err(
                inner
                    .registry
                    .mutate(agent_id, agent.generation, |a| a.status = AgentStatus::Idle),
                "idling a worker run",
            );
        }
        warn_on_err_for(
            home,
            Some(item_id),
            session.dispose().await,
            "disposing a worker session",
        );
        crate::sessions::ended(&inner, &session_id.to_string());
        drop(launched.in_flight);

        match outcome {
            RunSettled::ModelWall(mut wall) => {
                // Not a failure: the model died, the work did not. Bury the
                // pair and go round again — same workstream, same clock.
                wall.cooldown_until =
                    inner
                        .models
                        .note_unavailable(&wall.harness, &wall.model, wall.retry_after);
                pending_walls.push(wall.clone());
                all_walls.push(wall);
                // This attempt never really ran: its row leaves at once.
                inner.presence.forget(&inner, agent_id);
                last_run = None;
                if attempts.spent() {
                    let reason = give_up_reason(&all_walls, "the attempt budget is spent");
                    report_switches(
                        &inner,
                        (home, scope),
                        Some(item_id),
                        &pending_walls,
                        None,
                        &signer,
                        attestation.clone(),
                    );
                    break RunSettled::Failed(reason);
                }
            }
            other => {
                // The model ran, whatever the work did. It is healthy now.
                inner.models.note_success(&harness_id, &model_key);
                break other;
            }
        }
    };

    // Wall-clock spend for the run — measured across every attempt, because
    // the item only ever had one clock.
    let secs = started_at.elapsed().unwrap_or_default().as_secs();
    if secs > 0 {
        warn_on_err_for(
            home,
            Some(item_id),
            inner.ws.add_spend(&home, 0, 0, secs),
            "banking the wall clock",
        );
    }

    // Capture the result and tear down.
    if let Placement::Checkout(checkout) = placement {
        let Checkout {
            workstream, iso, ..
        } = *checkout;
        let result = inner.ws.paths().home(&home).result(item_id);
        capture_iso_result(&spec, result, iso).await;
        settle_workstream(&inner, &spec, &workstream).await;
    }

    // The engine is stopping: nothing more is written. The item stays as a
    // restart's sweep finds it — interrupted — never failed by its own
    // engine's ending, and never touched under an engine that took over.
    if inner.stopping.load(std::sync::atomic::Ordering::SeqCst) {
        tracing::debug!(target: "bisa_engine", work_item = %item_id, "engine stopping; the item is left to the restart");
        return;
    }
    // The item as it stands now: a result may have landed through the intake
    // socket while the session was still winding down.
    if let Ok(current) = inner.ws.get_work_item(&home, item_id) {
        spec = current;
    }
    // An item already settled — a result the intake accepted, blocked when
    // its last result missed the schema, or cancelled with its step — keeps
    // that verdict: the session's own ending is not a second reason.
    let failure: Option<String> = if already_settled(&spec.state) {
        None
    } else {
        match &settled {
            RunSettled::Completed => Some("the session ended without yielding a result".into()),
            RunSettled::Aborted => Some("session aborted".into()),
            RunSettled::BudgetExhausted => Some("budget exhausted".into()),
            RunSettled::Failed(e) => Some(e.clone()),
            RunSettled::WallClockExceeded => Some("wall clock exceeded".into()),
            // The attempt loop converts a wall into a `Failed` before
            // breaking; this arm exists so a future path that forgets to
            // cannot lose the item silently.
            RunSettled::ModelWall(wall) => Some(wall.sentence()),
        }
    };
    if let Some(reason) = &failure {
        spec.state = block_item(&inner, &spec, reason.clone());
        effects::item_settled(&inner, &spec, Err(reason.clone()));
    }

    // The bus hears how the session ended; the roster says what became of
    // the work: a session that ended politely without yielding a result is
    // a failed item, and its row reads so.
    let outcome = ExecutionOutcome::from(&settled);
    if let Some(run) = last_run {
        let on_roster = match &failure {
            Some(reason) => ExecutionOutcome::Failed {
                reason: reason.clone(),
            },
            None => outcome.clone(),
        };
        inner.presence.ended(&inner, run, &on_roster);
    }
    inner.emit(scope.event(Some(item_id), EnginePayload::ExecutionEnded { outcome }));
}

/// Whether the item was settled before its session ended. The intake got
/// there first — `Review` is the moment it wrote a result, `Accepted` the
/// step having taken it, `Blocked` a last result that missed the schema
/// (only the executor and that path block an item, so `Blocked` under a live
/// session is the intake's) — or the item was `Cancelled` with its step:
/// diverted, stopped or amended away, which is what ended the session.
fn already_settled(state: &WorkItemState) -> bool {
    matches!(
        state,
        WorkItemState::Review { .. }
            | WorkItemState::Accepted
            | WorkItemState::Blocked { .. }
            | WorkItemState::Cancelled
    )
}

/// Drive one attempt's session to settlement.
///
/// Returns [`RunSettled::ModelWall`] — never a `Failed` — when the session's
/// terminal outcome is [`Outcome::ModelUnavailable`], because that is the
/// model dying, not the work. Whether real progress landed first is recorded
/// on the wall: for every subprocess adapter a failed *launch* also arrives
/// here (see `bisa-adapters`'s "Launch versus mid-run"), and "terminal
/// wall, no progress yet" is exactly its signature.
#[allow(clippy::too_many_arguments)]
async fn drive_session(
    inner: &Inner,
    scope: crate::events::EventScope,
    live_run: LiveRunId,
    session_id: SessionId,
    spec: &WorkItemSpec,
    session: &dyn HarnessSession,
    mut events: bisa_harness::BoxEventStream,
    harness: &str,
    model_key: &str,
    started_at: SystemTime,
    wall_clock: Duration,
    signer: &nostr::key::Keys,
    attestation: Option<nostr::event::Tag>,
) -> RunSettled {
    let home = spec.home;
    let item_id = spec.id;
    // The item's stop signal: a cancel, a close or a retirement stops the
    // mark, and this loop aborts the harness at once. A mark already gone
    // means the reason to run went before the session was driven.
    let mark = match inner.inflight.get(&item_id) {
        Some(mark) => mark.clone(),
        None => {
            warn_on_err_for(
                home,
                Some(item_id),
                session.abort().await,
                "aborting an item stopped before it was driven",
            );
            return RunSettled::Aborted;
        }
    };
    // "Real progress" is work the session actually did — a tool, some text, a
    // token spend. Turn boundaries and `Started` are plumbing, and a harness
    // that emits them before dying at launch has still done nothing.
    let mut progressed = false;
    let mut settled = RunSettled::Failed("event stream closed without terminal end".into());

    loop {
        let remaining = wall_clock
            .checked_sub(started_at.elapsed().unwrap_or_default())
            .unwrap_or(Duration::ZERO);
        let next = tokio::select! {
            biased;
            () = mark.stopped() => {
                warn_on_err_for(home, Some(item_id), session.abort().await, "aborting a stopped item's session");
                return RunSettled::Aborted;
            }
            next = tokio::time::timeout(remaining, events.next()) => next,
        };
        let event = match next {
            Err(_) => {
                warn_on_err_for(
                    home,
                    Some(item_id),
                    session.abort().await,
                    "aborting on the wall clock",
                );
                return RunSettled::WallClockExceeded;
            }
            Ok(None) => return settled, // stream closed; keep default settled
            Ok(Some(ev)) => ev,
        };

        inner.presence.apply(inner, live_run, &event);
        inner.emit(scope.event(
            Some(item_id),
            EnginePayload::Session {
                event: event.clone(),
            },
        ));
        // The step this item runs for was cancelled or amended away: the
        // reservation is gone, and so is the reason to keep the session.
        if !inner.inflight.contains_key(&item_id) {
            warn_on_err_for(
                home,
                Some(item_id),
                session.abort().await,
                "aborting a cancelled item's session",
            );
            return RunSettled::Aborted;
        }

        match &event {
            SessionEvent::Progress(ProgressEvent::ToolStarted {
                name, args_summary, ..
            }) => {
                progressed = true;
                warn_on_err_for(
                    home,
                    Some(item_id),
                    inner.ws.append_journal(
                        &home,
                        JournalPayload::Progress {
                            work_item: item_id,
                            verb: "ran".into(),
                            object: format!("{name} {args_summary}").trim().to_string(),
                            outcome: None,
                        },
                        signer,
                        attestation.clone(),
                    ),
                    "journaling a tool run",
                );
            }
            SessionEvent::Progress(ProgressEvent::ToolEnded { .. })
            | SessionEvent::Progress(ProgressEvent::TextDelta { .. })
            | SessionEvent::Progress(ProgressEvent::ThinkingDelta { .. }) => progressed = true,
            // The driver owns a child now: its pid goes on the session row,
            // with the moment, so a boot after a crash can end it.
            SessionEvent::Lifecycle(LifecycleEvent::ProcessStarted { pid }) => {
                crate::sessions::process_started(inner, &session_id.to_string(), *pid);
            }
            SessionEvent::Progress(ProgressEvent::CostDelta {
                input_tokens,
                output_tokens,
                usd_cents,
            }) => {
                progressed = true;
                warn_on_err_for(
                    home,
                    Some(item_id),
                    inner
                        .ws
                        .add_spend(&home, input_tokens + output_tokens, *usd_cents, 0),
                    "recording spend",
                );
                if !inner.ws.budget_allows(&home).unwrap_or(true) {
                    warn_on_err_for(
                        home,
                        Some(item_id),
                        session.abort().await,
                        "aborting on budget",
                    );
                    return RunSettled::BudgetExhausted;
                }
            }
            // The harness stopped for an answer: one answerer for every driver.
            SessionEvent::Lifecycle(LifecycleEvent::InputRequested { request }) => {
                // How far this step goes on its own: its ceiling — an auto
                // goal's `write` step runs commands too — and what happens
                // above it, read together so the two can never disagree.
                let reach = crate::inputs::step_reach(inner, home.goal(), spec.tier_ceiling);
                crate::inputs::answer_request(
                    inner,
                    crate::inputs::InputContext {
                        live_run,
                        home: Some(home),
                        work_item: Some(item_id),
                        tier_ceiling: reach.ceiling,
                        agent: spec.agent.clone(),
                        // The workstream the session runs in, as the harness
                        // itself names it: what a relative path in a command
                        // is resolved against before the guard's path rules.
                        cwd: session.resume_token().map(|t| t.cwd),
                        classifier: true,
                        on_behalf_of: None,
                        above: reach.above,
                        conversation: None,
                    },
                    session,
                    request,
                )
                .await;
            }
            SessionEvent::Lifecycle(LifecycleEvent::Ended {
                outcome,
                is_terminal,
            }) => {
                let mapped = match outcome {
                    Outcome::Completed => RunSettled::Completed,
                    Outcome::Aborted => RunSettled::Aborted,
                    Outcome::Failed { error } => RunSettled::Failed(error.clone()),
                    Outcome::ModelUnavailable {
                        reason,
                        retry_after,
                        ..
                    } => RunSettled::ModelWall(ModelWall {
                        harness: harness.to_string(),
                        // Key on what we *asked for*, not on the name the
                        // harness reported back: the ledger, the plan and the
                        // journal all have to agree on one name, and an
                        // adapter is free to normalise an alias.
                        model: model_key.to_string(),
                        reason: reason.clone(),
                        retry_after: *retry_after,
                        cooldown_until: 0, // the ledger fills this in
                        after_progress: progressed,
                    }),
                    Outcome::Suspended { reason } => {
                        RunSettled::Failed(format!("suspended: {reason}"))
                    }
                };
                if *is_terminal {
                    return mapped;
                }
                // Non-terminal turn end. Streaming harnesses keep the process
                // alive between turns, so a terminal end may never come: the
                // session is over once its result has landed (the intake sets
                // Review synchronously before acking the tool call — a short
                // grace poll covers slow paths). A turn that ended without a
                // result is a failed item, however good the work was: the
                // step needs an output to move on.
                if matches!(mapped, RunSettled::Completed) {
                    let mut landed = false;
                    for _ in 0..30 {
                        match inner.ws.get_work_item(&home, item_id).map(|w| w.state) {
                            // Accepted, or blocked by the intake's last
                            // refusal: either way the item is settled.
                            Ok(state) if already_settled(&state) => {
                                landed = true;
                                break;
                            }
                            _ => tokio::time::sleep(Duration::from_millis(500)).await,
                        }
                    }
                    if landed {
                        return RunSettled::Completed;
                    }
                    return RunSettled::Failed(
                        "session ended its turn without yielding a result".into(),
                    );
                }
                settled = mapped;
                return settled;
            }
            _ => {}
        }
    }
}

/// Settle the workstream a run happened in.
///
/// A git workstream that changed something settles into a **commit** and is
/// then left exactly where it is: the branch persists, closing it is an
/// explicit act, and pushing it is the `Publish` gate — a separate, deliberate
/// decision, never a side effect of a work item finishing. One that settles
/// clean — the session read, judged or answered and left the tree as it found
/// it — is closed and its branch deleted (`projects::close_clean_worktree`),
/// so a read-only step leaves nothing behind. A copy workstream settles into
/// a `.patch` result and stays, tree and record, until its run ends: what the
/// run's later steps read is where the work landed.
///
/// Called **once**, after the attempt loop: a model failover reuses the
/// workstream rather than settling and reopening it, so a half-finished edit
/// from the dead session is still there for its replacement to build on.
async fn settle_workstream(inner: &Inner, spec: &WorkItemSpec, w: &Workstream) {
    match &w.kind {
        WorkstreamKind::Worktree { .. } => {
            match projects::commit_on_settle(inner, spec, w.id).await {
                projects::Settlement::Clean => projects::close_clean_worktree(inner, w).await,
                projects::Settlement::Committed | projects::Settlement::Refused => {}
            }
        }
        // The root checkout is the person's own tree: nothing is ever
        // auto-committed there.
        WorkstreamKind::Primary => {
            tracing::debug!(workstream = %w.id, "settled in the primary; nothing is committed");
        }
        // The copy stays open with its tree: the patch is captured, and the
        // run's end closes it (`effects::release_workstreams`), so a `check`
        // that follows this step still finds the file the agent wrote.
        WorkstreamKind::Copy => {
            tracing::debug!(workstream = %w.id, "settled in a copy; its patch is captured and the tree stays until the run ends");
        }
    }
}

/// Journal and broadcast every model switch. Failover has to be **visible**:
/// a run that quietly changed models under a human is a run whose journal
/// lies about what produced the work. The notes go on the item's home's
/// journal; the frames carry the item's scope.
///
/// `next` is the model the run continued on, or `None` when it gave up.
pub(crate) fn report_switches(
    inner: &Inner,
    (home, scope): (Home, crate::events::EventScope),
    work_item: Option<WorkItemId>,
    walls: &[ModelWall],
    next: Option<&str>,
    signer: &nostr::key::Keys,
    attestation: Option<nostr::event::Tag>,
) {
    for (i, wall) in walls.iter().enumerate() {
        let to = walls.get(i + 1).map(|w| w.model.as_str()).or(next);
        let tail = match to {
            Some(to) => format!("; retrying on {to}"),
            None => "; no model left in the plan".to_string(),
        };
        // A wall that arrived after real work is not a transparent retry: the
        // dead session may already have written into the workstream, and whoever
        // reads this later needs to know the change has two authors.
        let caveat = if wall.after_progress {
            " — work had already started, so a partial change may be in the workstream"
        } else {
            ""
        };
        let text = format!("{}{tail}{caveat}", wall.sentence());
        tracing::info!(%home, "{text}");
        warn_on_err_for(
            home,
            work_item,
            inner.ws.append_journal(
                &home,
                JournalPayload::Note { text },
                signer,
                attestation.clone(),
            ),
            "journaling a model switch",
        );
        if let Some(to) = to {
            inner.emit(scope.event(
                work_item,
                EnginePayload::ModelSwitched {
                    work_item,
                    from: wall.model.clone(),
                    to: to.to_string(),
                    reason: wall.reason.clone(),
                    retry_in_secs: wall.cooldown_until.saturating_sub(now_secs()),
                    after_progress: wall.after_progress,
                },
            ));
        }
    }
}

/// Why the run gave up, in one sentence that names the models rather than the
/// machinery.
///
/// When a model wall was involved the walls *are* the explanation, and each
/// one already carries the harness's own words — so the machinery's summary
/// ("no harness available: []") is dropped rather than tacked on. `fallback`
/// survives only when nothing walled at all: no adapter, nothing probed
/// available, a launch error that was never about a model.
fn give_up_reason(walls: &[ModelWall], fallback: &str) -> String {
    if walls.is_empty() {
        return fallback.to_string();
    }
    let tried: Vec<String> = walls.iter().map(|w| w.sentence()).collect();
    format!("every model tried is unavailable: {}", tried.join("; "))
}

/// Block an item that never ran, and say so on the bus. Returns the state the
/// item ended in.
async fn settle_failed(inner: &Arc<Inner>, spec: &WorkItemSpec, reason: String) -> WorkItemState {
    tracing::warn!(work_item = %spec.id, "work item failed: {reason}");
    let state = block_item(inner, spec, reason.clone());
    inner.emit(inner.item_scope(spec).event(
        Some(spec.id),
        EnginePayload::ExecutionEnded {
            outcome: ExecutionOutcome::Failed {
                reason: reason.clone(),
            },
        },
    ));
    effects::item_settled(inner, spec, Err(reason));
    state
}

/// Capture a copy workstream's diff as its `.patch` result. The tree stays:
/// a `check` step after this one runs **where the run's work landed**
/// (`projects::check_cwd`), and a copy torn down the moment its item settled
/// made that a race — the check saw the file the agent wrote, or a project
/// root it was never written to, by which of two tasks ran first. The copy
/// goes with its run (`effects::settle`, through `release_workstreams`),
/// the way a worktree stays until somebody closes it.
///
/// Only ever reached for [`WorkstreamKind::Copy`] — a non-git project's
/// workstream, which is the one placement whose tree is thrown away and whose
/// changes therefore have nowhere else to go. A git workstream settles into a
/// commit on its own branch and passes `None`, and a project-less item runs
/// in its home's `scratch/`, where what it wrote is simply still there.
async fn capture_iso_result(
    spec: &WorkItemSpec,
    result: PathBuf,
    iso: Option<(bisa_iso::BackendKind, PathBuf, PathBuf)>,
) {
    let Some((kind, lower, merged)) = iso else {
        return;
    };
    let captured = tokio::task::spawn_blocking(move || {
        let backend = bisa_iso::backend(kind);
        match backend.diff(&lower, &merged) {
            Ok(diff) if !diff.is_empty() => {
                if let Err(e) = bisa_store::write_atomic(&result, diff.unified_text().as_bytes()) {
                    tracing::warn!("failed writing the patch result: {e}");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("diff capture failed: {e}"),
        }
    })
    .await;
    if let Err(e) = captured {
        tracing::warn!(work_item = %spec.id, "the patch capture task did not finish: {e}");
    }
}

// ---------------------------------------------------------------------------
// Work-item state, through its transitions only
// ---------------------------------------------------------------------------

/// Walk an item to `InProgress` from wherever it stands: claim it first when
/// nobody has, unblock it when a previous attempt left it blocked. A relaunch
/// after a model wall finds it already in progress and does nothing.
fn mark_in_progress(
    inner: &Inner,
    spec: &WorkItemSpec,
    harness: &str,
    session: SessionId,
    signer: &nostr::key::Keys,
    attestation: Option<nostr::event::Tag>,
) -> Result<WorkItemState, EngineError> {
    let (home, id) = (&spec.home, spec.id);
    let updated = match &spec.state {
        WorkItemState::Open | WorkItemState::Rejected { .. } => {
            inner
                .ws
                .claim_work_item(home, id, harness, session, signer, attestation)?;
            inner
                .ws
                .transition_work_item(home, id, &WorkItemTransition::Start)?
        }
        WorkItemState::Claimed { .. } => {
            inner
                .ws
                .transition_work_item(home, id, &WorkItemTransition::Start)?
        }
        WorkItemState::Blocked { .. } => {
            inner
                .ws
                .transition_work_item(home, id, &WorkItemTransition::Unblock)?
        }
        WorkItemState::InProgress { .. } => return Ok(spec.state.clone()),
        other => {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-work-item-cannot-be-started",
                id = id.to_string(),
                a0 = (other.as_str()).to_string()
            )))
        }
    };
    Ok(updated.state)
}

/// Block an item with a reason, walking it through whatever transitions its
/// current state needs. Nothing here is fatal: the run is already over, and
/// the reason is the one thing worth keeping.
pub(crate) fn block_item(inner: &Inner, spec: &WorkItemSpec, reason: String) -> WorkItemState {
    let by = inner.ws.owner_principal();
    let block = WorkItemTransition::Block {
        reason: reason.clone(),
    };
    let steps: Vec<WorkItemTransition> = match &spec.state {
        WorkItemState::Open | WorkItemState::Rejected { .. } => vec![
            WorkItemTransition::Claim { by },
            WorkItemTransition::Start,
            block,
        ],
        WorkItemState::Claimed { .. } => vec![WorkItemTransition::Start, block],
        WorkItemState::InProgress { .. } => vec![block],
        WorkItemState::Blocked { .. } => vec![WorkItemTransition::Unblock, block],
        WorkItemState::Review { .. } | WorkItemState::Accepted | WorkItemState::Cancelled => {
            tracing::debug!(work_item = %spec.id, "not blocking a {} item: {reason}", spec.state.as_str());
            return spec.state.clone();
        }
    };
    let mut state = spec.state.clone();
    for step in &steps {
        match inner.ws.transition_work_item(&spec.home, spec.id, step) {
            Ok(updated) => state = updated.state,
            Err(e) => {
                tracing::warn!(work_item = %spec.id, "cannot block the item ({}): {e}", step.name());
                break;
            }
        }
    }
    state
}

/// What `inner.inflight` holds for a reserved item: its home, and the
/// signal that stops its session at once. A cancel, a close, a retirement
/// — anything that ends the item's reason to run — `stop()`s the mark
/// before removing it, and the session driver aborts the harness on the
/// spot rather than on its next event (a quiet harness has none).
#[derive(Clone)]
pub struct InFlightMark {
    pub home: Home,
    stop: Arc<tokio::sync::Notify>,
}

impl InFlightMark {
    fn new(home: Home) -> Self {
        Self {
            home,
            stop: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Tell the session driver to abort now. Idempotent; a stop before the
    /// driver listens is kept for it (`Notify` holds one permit).
    pub fn stop(&self) {
        self.stop.notify_one();
    }

    /// Resolves once `stop` was called — the driver's other arm.
    async fn stopped(&self) {
        self.stop.notified().await;
    }
}

/// Stop the mark of one item, if it is reserved. Answers whether it was.
pub(crate) fn stop_item(inner: &Inner, item: WorkItemId) -> bool {
    match inner.inflight.get(&item) {
        Some(mark) => {
            mark.stop();
            true
        }
        None => false,
    }
}

/// A work item's reservation for execution: taken before the executor's
/// task is spawned, released when that task ends — by settling, by a panic,
/// or by being dropped. `inner.inflight` is what the preflight and the
/// session driver read as "this item still has a reason to run"; a mark
/// that outlived its task (a panic used to leave one) refused every
/// relaunch of the step for good.
pub(crate) struct InFlight {
    inner: Arc<Inner>,
    item: WorkItemId,
}

impl InFlight {
    /// Reserve synchronously — a second launch in the window must skip it.
    pub(crate) fn reserve(inner: &Arc<Inner>, item: WorkItemId, home: Home) -> Self {
        inner.inflight.insert(item, InFlightMark::new(home));
        Self {
            inner: Arc::clone(inner),
            item,
        }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.inner.inflight.remove(&self.item);
    }
}

/// Hand one step's work item to the executor: preflight, reserve, spawn.
///
/// The preflight's rejection comes back typed; the caller settles the step
/// with it. Nothing is allocated before the preflight passes. A panic in the
/// executor is caught here and settles the step as a failure with the
/// panic's sentence — `retries` and `on_fail` then decide, as for any other
/// failure — and the reservation is released either way.
pub(crate) fn launch_step_item(
    inner: &Arc<Inner>,
    spec: WorkItemSpec,
    launch: scheduler::Launch,
) -> Result<(), scheduler::ScheduleRejection> {
    scheduler::preflight(&inner.ws, &inner.config, &spec, launch)?;
    // The list is held from the stopping check to the push, so `shutdown` —
    // which sets the flag and then takes the list — sees every task there is.
    let mut executors = inner.executors.lock().unwrap_or_else(|e| e.into_inner());
    if inner.stopping.load(std::sync::atomic::Ordering::SeqCst) {
        tracing::debug!(target: "bisa_engine", work_item = %spec.id, "engine stopping; the item is left to the restart");
        return Ok(());
    }
    let reservation = InFlight::reserve(inner, spec.id, spec.home);
    let inner = Arc::clone(inner);
    let task = tokio::spawn({
        let inner = Arc::clone(&inner);
        async move {
            let _reservation = reservation;
            let run = run_work_item(Arc::clone(&inner), spec.clone());
            if let Err(panic) =
                futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(run)).await
            {
                let reason = format!(
                    "the executor panicked: {}",
                    bisa_log::panic_message(&*panic)
                );
                tracing::error!(target: "bisa_engine", work_item = %spec.id, home = %spec.home, "{reason}");
                // The item as the store holds it now, so the block reads its
                // real state; the spec as launched when even that cannot be read.
                let mut current = inner.ws.get_work_item(&spec.home, spec.id).unwrap_or(spec);
                current.state = block_item(&inner, &current, reason.clone());
                crate::effects::item_settled(&inner, &current, Err(reason));
            }
        }
    });
    // Remembered so `shutdown` can end it; the finished ones are let go here
    // so the list is the live tasks, never the engine's whole history.
    executors.retain(|h| !h.is_finished());
    executors.push(task);
    Ok(())
}

/// Cancel a step's work item: its record moves to `Cancelled`, its
/// reservation is dropped (a live session sees that and aborts), and the
/// bus hears why. Nothing is deleted.
pub(crate) fn cancel_item(inner: &Arc<Inner>, home: &Home, item: WorkItemId, why: &str) {
    let Ok(spec) = inner.ws.get_work_item(home, item) else {
        return;
    };
    // The record first, then the session: the executor reads the item again
    // once its session ended, and what it finds there is what it settles on.
    // A session stopped before the record said *cancelled* read as one that
    // failed, and its item was blocked for it.
    let cancelled = (!spec.state.is_terminal()).then(|| {
        inner
            .ws
            .transition_work_item(home, item, &WorkItemTransition::Cancel)
    });
    // The session hears it now — an abort at once, not on the harness's next
    // event — and the mark goes.
    stop_item(inner, item);
    inner.inflight.remove(&item);
    inner.active_items.remove(&item);
    match cancelled {
        None => {}
        Some(Ok(_)) => inner.emit(inner.item_scope(&spec).event(
            Some(item),
            EnginePayload::ExecutionEnded {
                outcome: ExecutionOutcome::Cancelled {
                    reason: why.to_string(),
                },
            },
        )),
        Some(Err(e)) => tracing::warn!(work_item = %item, "cannot cancel the item: {e}"),
    }
}

/// Locate a work item by id: the active map first, the index otherwise.
pub fn find_work_item(inner: &Inner, item: WorkItemId) -> Option<WorkItemSpec> {
    if let Some(home) = inner.active_items.get(&item).map(|h| *h) {
        if let Ok(spec) = inner.ws.get_work_item(&home, item) {
            return Some(spec);
        }
    }
    let home = inner.ws.home_of_work_item(item).ok()?;
    inner.ws.get_work_item(&home, item).ok()
}

#[cfg(test)]
mod prompts {
    use super::*;

    #[test]
    fn an_interruption_is_said_in_a_whole_sentence_of_one_session_and_of_several() {
        let one = interruption_note(1);
        assert!(
            one.contains(
                "A previous session on this work was interrupted by a restart of the platform"
            ),
            "{one}"
        );
        assert!(one.contains("Its work so far is in this folder"), "{one}");
        let three = interruption_note(3);
        assert!(
            three.contains(
                "3 previous sessions on this work were interrupted by a restart of the platform"
            ),
            "{three}"
        );
        assert!(
            three.contains("Their work so far is in this folder"),
            "{three}"
        );
        for said in [one, three] {
            assert!(said.contains(bisa_core::run::INTERRUPTED), "{said}");
            assert!(said.contains("continue from there rather than starting over"));
        }
    }
}

#[cfg(test)]
mod in_flight {
    use super::*;

    /// The reservation is the mark: when the task holding it ends by a
    /// panic, the mark goes with it and the item can be launched again.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_dropped_reservation_leaves_no_mark() {
        let dir = tempfile::tempdir().unwrap();
        let ws = bisa_store::Workspace::open_with_keystore(
            dir.path(),
            Box::new(bisa_store::MemoryKeyStore::default()),
        )
        .unwrap();
        let engine = crate::Engine::start(
            ws,
            bisa_harness::HarnessCatalog::new(),
            crate::EngineConfig {
                design_enabled: false,
                events_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
        let inner = Arc::clone(engine.inner());
        let item = WorkItemId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()));
        let goal = bisa_core::GoalId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()));
        let reservation = InFlight::reserve(&inner, item, Home::Goal { goal });
        assert!(inner.inflight.contains_key(&item));
        let task = tokio::spawn(async move {
            let _held = reservation;
            panic!("the executor died");
        });
        assert!(task.await.is_err(), "the task panicked");
        assert!(!inner.inflight.contains_key(&item), "the mark went with it");
        engine.shutdown().await;
    }
}
