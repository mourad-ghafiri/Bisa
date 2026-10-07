//! `bisa run`: begin a goal's work and follow it, with a live activity
//! timeline and interactive gate prompts — and the run's other verbs:
//! `stop`, `restart`, `runs`. [`follow_run`] and [`follow_run_via_node`]
//! follow one run by its id instead — what `bisa workflow run` does with a
//! run of the workspace, which has no goal.
//!
//! **One start door.** With no `--start`, `bisa run` is a person's start: a
//! goal whose workflow begins on events *listens* — no run is made, and what
//! it hears is said, with each public hook's secret, once — and any other
//! runs by hand. `--start` names where a run begins: the start by hand is a
//! run now, whatever else the workflow begins on; an event start is a **test
//! run**, begun there as if its event had happened with `--data`.
//!
//! Execution belongs to whichever engine holds the run. With a daemon the
//! CLI asks it to start the run and tails its activity — this process can
//! disconnect at any time without ending the work. Without one, a
//! short-lived embedded engine starts the run and this process *is* the
//! engine until the run settles: done, failed, or waiting on a person.

use crate::activity::{self, Style};
use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{anyhow, Context as _, Result};
use bisa_core::{
    GoalId, GoalStatus, ListenerHost, NoWayIn, RunId, RunStatus, SignalSource, StepId, StepState,
    WayIn, Workflow, WorkflowRun,
};
use bisa_engine::{Begun, Engine, EnginePayload};
use bisa_store::Workspace;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

/// `--data <json>`, as any verb that takes a payload reads it; text that is
/// not JSON is refused by name.
pub fn parse_data(raw: &str) -> Result<Value> {
    serde_json::from_str(raw)
        .with_context(|| bisa_core::text!("cli-run-data-not-json", raw = format!("{raw:?}")))
}

/// Where a run verb was told the run begins: nowhere in particular — a
/// person's start — or at a start step, with the sample its event carries
/// when the run is a test.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Begin {
    pub start: Option<StepId>,
    pub data: Option<Value>,
}

impl Begin {
    /// `--start <step>` and `--data <json>` as they were typed; a step that
    /// is no step id and a sample that is not JSON are refused by name.
    pub fn parse(start: Option<&str>, data: Option<&str>) -> Result<Self> {
        let start = start
            .map(|raw| {
                StepId::new(raw).with_context(|| {
                    bisa_core::text!("cli-step-not-step-id", step = format!("{raw:?}"))
                })
            })
            .transpose()?;
        let data = data.map(parse_data).transpose()?;
        Ok(Self { start, data })
    }

    /// Nothing was named: a person's start.
    pub fn is_plain(&self) -> bool {
        self.start.is_none() && self.data.is_none()
    }

    /// The body the node's two run routes read: `{inputs, start?, event?}`.
    pub fn body(&self, inputs: &BTreeMap<String, Value>) -> Value {
        let mut body = json!({ "inputs": inputs });
        if let Some(start) = &self.start {
            body["start"] = json!(start);
        }
        if let Some(data) = &self.data {
            body["event"] = data.clone();
        }
        body
    }
}

/// Which of the engine's doors a run verb names, when no node reads the
/// body: what `begin` asks of `workflow` — absent when its home has none
/// yet, which the engine refuses in its own words. The reading is the core's
/// ([`WayIn::of`]), the one the node makes of a body, so a run begins the
/// same way with or without a daemon; a refusal is said in the words of the
/// command line.
pub fn way_in(workflow: Option<&Workflow>, begin: Begin) -> Result<WayIn> {
    WayIn::of(workflow, begin.start, begin.data).map_err(|refused| match refused {
        NoWayIn::EventWithoutStart => anyhow!(bisa_core::text!("cli-run-data-needs-start")),
        NoWayIn::ByHandReadsNoEvent { step } => anyhow!(bisa_core::text!(
            "cli-run-start-by-hand-reads-no-data",
            step = step.to_string()
        )),
    })
}

/// Whether a run is a test run: begun at a start as if its event had
/// happened — the one source only a test's sample carries.
pub fn is_test(run: &WorkflowRun) -> bool {
    run.event
        .as_ref()
        .is_some_and(|event| event.source == SignalSource::Test)
}

/// Begin the goal's work (or pick up the live run) and follow it. With
/// `new` — or a start named — a run is made even while one is live, queued
/// behind it, and the live one is followed.
pub async fn run(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    inputs: crate::inputs::Typed,
    begin: Begin,
    watch: bool,
    new: bool,
) -> Result<()> {
    let id = GoalId::from_str(id).context(bisa_core::text!("cli-run-invalid-goal-id"))?;
    let inputs = inputs.read_for_goal(&ctx.workspace()?, id);
    if let Some(client) = ctx.node_client().await {
        return run_via_node(client, out, id, inputs, begin, watch, new).await;
    }
    let (engine, _) = ctx.engine().await?;
    let engine = Arc::new(engine);
    let style = Style::stderr();

    let current = engine.current_run(id)?;
    match current {
        Some(run) if !run.is_finished() && !new && begin.is_plain() => {
            eprintln!(
                "{}",
                style.bold(&bisa_i18n::say(&bisa_core::text!(
                    "cli-run-run-already-following",
                    a0 = (run.id).to_string(),
                    a1 = (run.status().as_str()).to_string()
                )))
            );
        }
        _ => {
            let goal = engine.workspace().get_goal(id)?;
            if goal.workflow.is_none() {
                out.say(&bisa_core::text!(
                    "cli-run-nothing-run-goal-has-no-workflow",
                    id = id.to_string(),
                    a0 = (if goal.mode.designs() {
                        bisa_i18n::say(&bisa_core::text!(
                            "cli-run-workflow-agent-designs-one-see-bisa"
                        ))
                    } else {
                        bisa_i18n::say(&bisa_core::text!(
                            "cli-run-design-workflow-tab-pick-one-with"
                        ))
                    })
                    .to_string()
                ));
                out.json_value(json!({"goal": id.to_string(), "started": false}));
                shutdown(engine).await;
                return Ok(());
            }
            let begun = match begin_embedded(&engine, &goal, inputs, begin) {
                Ok(begun) => begun,
                Err(e) => {
                    shutdown(engine).await;
                    return Err(e);
                }
            };
            let run = match begun {
                Begun::Run(run) => *run,
                // The goal listens: no run was made, and there is none to
                // follow. What it hears is said, its secrets once.
                Begun::Listening(turned) => {
                    let armed = armed_embedded(&engine, id, &turned.secrets);
                    shutdown(engine).await;
                    render_armed(out, &armed?, false);
                    return Ok(());
                }
            };
            if run.is_queued() {
                let position = engine
                    .workspace()
                    .queued_runs(id)?
                    .iter()
                    .position(|r| r.id == run.id)
                    .map_or(1, |i| i + 1);
                eprintln!(
                    "{}",
                    style.bold(&queued_line(
                        &run,
                        engine.current_run(id)?.as_ref(),
                        position
                    ))
                );
            } else if is_test(&run) {
                eprintln!("{}", style.bold(&test_run_line(&run)));
            } else if watch {
                eprintln!(
                    "{}",
                    style.bold(&bisa_i18n::say(&bisa_core::text!(
                        "cli-run-started-run",
                        a0 = (run.id).to_string()
                    )))
                );
            }
        }
    }
    follow(engine, out, id, watch).await
}

/// Begin the goal's work on an embedded engine, through the door `begin`
/// names: a person's start (it listens, or it runs), a run now at the start
/// by hand, or a test run of an event start.
fn begin_embedded(
    engine: &Engine,
    goal: &bisa_core::Goal,
    inputs: BTreeMap<String, Value>,
    begin: Begin,
) -> Result<Begun> {
    // The workflow is read only when a start is named: what it is judged
    // against.
    let workflow = match (&begin.start, goal.workflow) {
        (Some(_), Some(id)) => Some(engine.workspace().get_workflow(id)?),
        _ => None,
    };
    Ok(match way_in(workflow.as_ref(), begin)? {
        WayIn::Start => engine.begin_goal(goal.id, inputs)?,
        WayIn::ByHand => Begun::Run(Box::new(engine.start_run(goal.id, inputs)?)),
        WayIn::Test { start, payload } => Begun::Run(Box::new(
            engine.test_run_goal(goal.id, &start, payload, inputs)?,
        )),
    })
}

/// A test run said as one: where it began, and that its event is a sample.
pub fn test_run_line(run: &WorkflowRun) -> String {
    bisa_i18n::say(&bisa_core::text!(
        "cli-run-test-run-started",
        run = run.id.to_string(),
        start = run
            .start
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default()
    ))
}

/// What a start that armed the goal answers, in the one shape both paths
/// render: no run, where the goal stands, what it hears and the secrets the
/// start minted.
fn armed(goal: GoalId, status: Value, listeners: Vec<Value>, secrets: Vec<Value>) -> Value {
    json!({
        "goal": goal.to_string(),
        "status": status,
        "run": null,
        "listeners": listeners,
        "secrets": secrets,
    })
}

/// [`armed`], read from an embedded engine's workspace.
fn armed_embedded(
    engine: &Engine,
    id: GoalId,
    secrets: &[bisa_engine::HookSecret],
) -> Result<Value> {
    let ws = engine.workspace();
    let goal = ws.get_goal(id)?;
    let status = goal.status(ws.get_current_run(id)?.as_ref());
    let listeners = crate::listening::listeners_of(engine, &ListenerHost::Goal { goal: id })?;
    let secrets = secrets
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(armed(id, json!(status), listeners, secrets))
}

/// The goal listens: said with what it hears, and each public hook's secret
/// — once. `heard` is whether a node is there to hear its events; without
/// one they are heard once one runs.
fn render_armed(out: &Out, armed: &Value, heard: bool) {
    out.human(&armed_lines(armed, heard).join("\n"));
    out.json_value(armed.clone());
}

/// [`render_armed`]'s lines.
fn armed_lines(armed: &Value, heard: bool) -> Vec<String> {
    let goal = armed["goal"].as_str().unwrap_or_default();
    let mut lines = vec![bisa_i18n::say(&bisa_core::text!(
        "cli-run-goal-listening",
        id = goal.to_string(),
        status = armed["status"].as_str().unwrap_or_default().to_string()
    ))];
    lines.extend(crate::listening::listener_lines(crate::listening::rows(
        &armed["listeners"],
    )));
    lines.extend(crate::listening::secret_lines(
        crate::listening::HookHost::Goal,
        goal,
        crate::listening::rows(&armed["secrets"]),
    ));
    if !heard {
        lines.push(bisa_i18n::say(&bisa_core::text!("cli-listening-no-node")));
    }
    lines
}

/// The word for a run that waits its turn.
fn queued_line(run: &WorkflowRun, live: Option<&WorkflowRun>, position: usize) -> String {
    match live {
        Some(live) => bisa_i18n::say(&bisa_core::text!(
            "cli-run-run-queued-behind-run-position",
            a0 = (run.id).to_string(),
            a1 = (live.id).to_string(),
            position = position.to_string()
        )),
        None => bisa_i18n::say(&bisa_core::text!(
            "cli-run-run-queued-position",
            a0 = (run.id).to_string(),
            position = position.to_string()
        )),
    }
}

/// Stop the goal: it stops listening, its sessions are ended, its queued
/// runs withdrawn, its live run cancelled. Says what it stopped.
pub async fn stop(ctx: &Ctx, out: &Out, id: &str, rationale: Option<String>) -> Result<()> {
    let id = GoalId::from_str(id).context(bisa_core::text!("cli-run-invalid-goal-id"))?;
    let rationale = rationale.filter(|r| !r.trim().is_empty());
    // Whether it listened, read before the stop: the stop's answer names
    // the runs it ended, and the engine ends the listening with them.
    let listened = ctx
        .workspace()?
        .get_goal(id)
        .is_ok_and(|goal| goal.listening.is_some());
    let stopped: Value = if let Some(client) = ctx.node_client().await {
        client
            .post(
                &format!("/goals/{id}/stop"),
                json!({"rationale": rationale}),
            )
            .await?
    } else {
        let (engine, _) = ctx.engine().await?;
        let stopped = engine.stop_goal(id, rationale).await;
        engine.shutdown().await;
        let stopped = stopped?;
        json!({"stopped": stopped.run, "withdrawn": stopped.withdrawn, "ended": stopped.ended})
    };
    out.human(&stopped_lines(id, &stopped, listened).join("\n"));
    out.json_value(json!({
        "goal": id.to_string(),
        "stopped": stopped["stopped"],
        "withdrawn": stopped["withdrawn"],
        "ended": stopped["ended"],
        "listening_stopped": listened,
    }));
    Ok(())
}

/// What a stop did, in lines: the run it ended and the runs it withdrew —
/// and that the goal no longer listens, when it did.
fn stopped_lines(id: GoalId, stopped: &Value, listened: bool) -> Vec<String> {
    let withdrawn = stopped["withdrawn"].as_array().map_or(0, |w| w.len());
    let mut lines = Vec::new();
    match (stopped["stopped"].as_str(), withdrawn) {
        // Nothing ran and nothing waited: the listening is all there was.
        (None, 0) if listened => {}
        (None, 0) => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-nothing-stop",
            id = id.to_string()
        ))),
        (None, n) => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-queued-run-withdrawn",
            id = id.to_string(),
            n = n.to_string(),
            a0 = (plural(n)).to_string()
        ))),
        (Some(run), 0) => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-run-stopped",
            id = id.to_string(),
            run = run.to_string()
        ))),
        (Some(run), n) => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-run-stopped-queued-run-withdrawn",
            id = id.to_string(),
            run = run.to_string(),
            n = n.to_string(),
            a0 = (plural(n)).to_string()
        ))),
    }
    if listened {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-stopped-listening",
            id = id.to_string()
        )));
    }
    // What the stop ended besides: a harness that had to be terminated, a
    // session that could not be ended, the goals spawned by this one.
    let ended = &stopped["ended"];
    let count = |key: &str| ended[key].as_u64().unwrap_or(0);
    let children = ended["children"].as_array().map_or(0, |c| c.len());
    if count("terminated") > 0 {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-harnesses-terminated",
            n = count("terminated").to_string()
        )));
    }
    if count("still_live") > 0 {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-sessions-not-ended",
            n = count("still_live").to_string()
        )));
    }
    if children > 0 {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-run-goal-spawned-stopped",
            n = children.to_string()
        )));
    }
    lines
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Restart the goal: a new run of its last run's workflow and inputs,
/// started at once, then followed.
pub async fn restart(ctx: &Ctx, out: &Out, id: &str, watch: bool) -> Result<()> {
    let id = GoalId::from_str(id).context(bisa_core::text!("cli-run-invalid-goal-id"))?;
    let style = Style::stderr();
    if let Some(client) = ctx.node_client().await {
        let v = client
            .post(&format!("/goals/{id}/restart"), json!({}))
            .await?;
        eprintln!(
            "{}",
            style.bold(&bisa_i18n::say(&bisa_core::text!(
                "cli-run-restarted-run",
                a0 = (v["run"]["id"].as_str().unwrap_or("?")).to_string()
            )))
        );
        return follow_via_node(client, out, id, watch).await;
    }
    let (engine, _) = ctx.engine().await?;
    let engine = Arc::new(engine);
    let run = match engine.restart_goal(id).await {
        Ok(run) => run,
        Err(e) => {
            shutdown(engine).await;
            return Err(e.into());
        }
    };
    eprintln!(
        "{}",
        style.bold(&bisa_i18n::say(&bisa_core::text!(
            "cli-run-restarted-run",
            a0 = (run.id).to_string()
        )))
    );
    follow(engine, out, id, watch).await
}

/// Who started a run, as the node's run summary says it (`started_by`): a
/// person, an occurrence one of the workflow's start events heard, or a test
/// run.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum StartedBy {
    /// By hand: no event began it.
    You,
    /// An event — its kind, and what there is to say of it: who wrote the
    /// message, which signal, which run.
    Event {
        event: SignalSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// A test run, begun at a start as if its event — named by its word —
    /// had happened.
    Test { event: String },
}

impl StartedBy {
    /// Who started `run`, as the node says it: read off the run
    /// (`dto::StartedBy::of`) with what there is to say of its event — the
    /// names the node looks up in the workspace among them
    /// (`runs::start_detail`). Each rule has one home.
    pub fn of(ws: &Workspace, run: &WorkflowRun) -> Self {
        Self::said(run, bisa_node::runs::start_detail(ws, run))
    }

    /// Who started `run`, given what there is to say of its event.
    fn said(run: &WorkflowRun, detail: Option<String>) -> Self {
        use bisa_node::dto::StartedBy as Said;
        match Said::of(run, detail) {
            Said::You => StartedBy::You,
            Said::Event { event, detail } => StartedBy::Event {
                event: source_of(event),
                detail,
            },
            Said::Test { event } => StartedBy::Test { event },
        }
    }

    /// Who started a run, from a summary the node answered; a person when
    /// the summary does not say.
    pub fn read(summary: &Value) -> Self {
        serde_json::from_value(summary["started_by"].clone()).unwrap_or(StartedBy::You)
    }

    /// The words a run's line and a run's page say it in: *by you*, *by
    /// schedule*, *by message from Maya*, *by signal report.ready*, *test*.
    pub fn words(&self) -> String {
        match self {
            StartedBy::You => bisa_i18n::say(&bisa_core::text!("cli-run-started-by-you")),
            StartedBy::Event {
                event: SignalSource::Message,
                detail: Some(who),
            } => bisa_i18n::say(&bisa_core::text!(
                "cli-run-started-by-message-from",
                who = who.to_string()
            )),
            StartedBy::Event { event, detail } => bisa_i18n::say(&bisa_core::text!(
                "cli-run-started-by-event",
                event = event.as_str(),
                detail = detail
                    .as_deref()
                    .map(|d| format!(" {d}"))
                    .unwrap_or_default()
            )),
            StartedBy::Test { event } => bisa_i18n::say(&bisa_core::text!(
                "cli-run-started-by-test",
                event = event.to_string()
            )),
        }
    }
}

/// The signal source an event's kind names.
fn source_of(kind: bisa_node::dto::EventKind) -> SignalSource {
    use bisa_node::dto::EventKind as K;
    match kind {
        K::Schedule => SignalSource::Schedule,
        K::Hook => SignalSource::Hook,
        K::Message => SignalSource::Message,
        K::Signal => SignalSource::Signal,
        K::Project => SignalSource::Project,
        K::Run => SignalSource::Run,
        K::Platform => SignalSource::Platform,
        K::Connector => SignalSource::Connector,
        K::Check => SignalSource::Check,
    }
}

/// One run of a goal, as `bisa runs` prints it — the node's run summary
/// shape, so the two paths print the same line.
#[derive(serde::Serialize, serde::Deserialize)]
struct RunLine {
    id: bisa_core::RunId,
    status: bisa_core::RunStatus,
    workflow: bisa_core::WorkflowId,
    revision: u64,
    started_by: StartedBy,
    queued_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<bisa_core::RunOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<bisa_core::CancelCause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<usize>,
}

impl RunLine {
    /// Newest first; a queued run numbered by its place in the queue, each
    /// with who started it (`started_by`).
    fn list(runs: &[WorkflowRun], started_by: impl Fn(&WorkflowRun) -> StartedBy) -> Vec<RunLine> {
        let mut position = 0;
        let mut out: Vec<RunLine> = runs
            .iter()
            .map(|r| {
                let queued = r.is_queued();
                if queued {
                    position += 1;
                }
                RunLine {
                    id: r.id,
                    status: r.status(),
                    workflow: r.workflow.id,
                    revision: r.workflow.revision,
                    started_by: started_by(r),
                    queued_at: r.queued_at,
                    started_at: r.started_at,
                    finished_at: r.finished_at,
                    outcome: r.outcome,
                    cause: r.cancelled.clone(),
                    position: queued.then_some(position),
                }
            })
            .collect();
        out.reverse();
        out
    }

    fn words(&self) -> String {
        let mut words = vec![
            self.id.to_string(),
            self.status.as_str().to_string(),
            format!("rev {}", self.revision),
            self.started_by.words(),
        ];
        match (self.position, self.started_at) {
            (Some(n), _) => words.push(format!("position {n}")),
            (None, Some(at)) => words.push(format!("started {at}")),
            (None, None) => words.push(format!("queued {}", self.queued_at)),
        }
        if let Some(at) = self.finished_at {
            words.push(format!("finished {at}"));
        }
        if let Some(cause) = &self.cause {
            words.push(cause.as_str().to_string());
        }
        words.join("  ")
    }
}

/// Every run of the goal, newest first.
pub async fn runs(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let id = GoalId::from_str(id).context(bisa_core::text!("cli-run-invalid-goal-id"))?;
    let lines: Vec<RunLine> = if let Some(client) = ctx.node_client().await {
        let v = client.get(&format!("/goals/{id}/runs")).await?;
        serde_json::from_value(v["runs"].clone())
            .context(bisa_core::text!("cli-run-bad-runs-payload-from-node"))?
    } else {
        let ws = ctx.workspace()?;
        ws.get_goal(id)?;
        RunLine::list(&ws.list_runs(id)?, |run| StartedBy::of(&ws, run))
    };
    if lines.is_empty() {
        out.say(&bisa_core::text!(
            "cli-run-goal-has-no-run-yet",
            id = id.to_string()
        ));
    }
    for line in &lines {
        out.human(&line.words());
    }
    out.json_value(json!({"goal": id.to_string(), "runs": lines}));
    Ok(())
}

/// Follow an embedded engine's run until it settles, then shut the engine
/// down. Shared with `new --workflow` and `approve`, which start a run inside
/// a process that would otherwise exit and take the engine with it.
pub async fn follow(engine: Arc<Engine>, out: &Out, id: GoalId, watch: bool) -> Result<()> {
    let style = Style::stderr();
    let mut rx = engine.events();
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    let status = loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                eprintln!("{}", style.bold(&bisa_i18n::say(&bisa_core::text!("cli-run-interrupted-shutting-down"))));
                shutdown(engine).await;
                if let Some(log) = bisa_log::current() {
                    log.goodbye("interrupted");
                }
                std::process::exit(130);
            }
            event = rx.recv() => {
                if let Ok(event) = event {
                    if watch || is_spine(&event.payload) {
                        if let Some(line) = activity::engine_line(&style, &event) {
                            eprintln!("{line}");
                        }
                    }
                    if let EnginePayload::GateOpened { gate_id, question, .. } = &event.payload {
                        prompt_gate(&engine, &style, gate_id, question, &id.to_string()).await;
                    }
                }
            }
            _ = poll.tick() => {
                let goal = engine.workspace().get_goal(id)?;
                let run = engine.workspace().get_current_run(id)?;
                let status = goal.status(run.as_ref());
                match status {
                    GoalStatus::Running => {}
                    GoalStatus::Waiting => {
                        // Somebody has to act, and headless has nobody to
                        // prompt: say what waits, then hand off.
                        if let Some(run) = &run {
                            for line in waiting_lines(run, &id.to_string()) {
                                eprintln!("{}", style.bold(&line));
                            }
                        }
                        // A live gate in this process is still answerable
                        // from a TTY; give it one chance before leaving.
                        if !engine.inbox().is_empty() && std::io::stdin().is_terminal() {
                            continue;
                        }
                        break status;
                    }
                    _ => break status,
                }
            }
        }
    };

    let run = engine.workspace().get_current_run(id)?;
    out.say(&bisa_core::text!(
        "cli-run-goal",
        id = id.to_string(),
        a0 = (status.as_str()).to_string(),
        a1 = (run
            .as_ref()
            .and_then(|r| r.outcome)
            .map(|o| bisa_i18n::say(&bisa_core::text!(
                "cli-run-run",
                a0 = (o.as_str()).to_string()
            )))
            .unwrap_or_default())
        .to_string()
    ));
    out.json_value(json!({"goal": id.to_string(), "status": status, "run": run}));
    shutdown(engine).await;
    Ok(())
}

/// One line per step that waits, with the verb that moves it — naming `id`,
/// the goal's or the run's, as the verb takes it — or what it holds for; and
/// under it, one line per boundary event still armed on the step: what would
/// divert it, and what acts beside it.
fn waiting_lines(run: &WorkflowRun, id: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for step in &run.workflow.steps {
        let Some(record) = run.steps.get(&step.id) else {
            continue;
        };
        if record.state != StepState::Waiting {
            continue;
        }
        let verb = match &step.kind {
            bisa_core::StepKind::Human { .. } => bisa_i18n::say(&bisa_core::text!(
                "cli-run-bisa-step-answer-step-done",
                id = id.to_string(),
                a0 = (step.id).to_string(),
                a1 = (step.id).to_string()
            )),
            bisa_core::StepKind::Approval { .. } => bisa_i18n::say(&bisa_core::text!(
                "cli-run-bisa-approve-no",
                id = id.to_string()
            )),
            bisa_core::StepKind::Wait { until } => match until {
                bisa_core::WaitFor::Release => bisa_i18n::say(&bisa_core::text!(
                    "cli-run-bisa-step-release",
                    id = id.to_string(),
                    a0 = (step.id).to_string()
                )),
                other => bisa_i18n::say(&bisa_core::text!(
                    "cli-run-waiting",
                    a0 = (activity::wait_label(other)).to_string()
                )),
            },
            _ => bisa_i18n::say(&bisa_core::text!(
                "cli-run-waiting-step",
                a0 = (step.id).to_string()
            )),
        };
        lines.push(format!(
            "⏸ {} ({}) — {verb}",
            step.name,
            activity::step_kind_label(&step.kind)
        ));
        for boundary in &step.boundaries {
            let fired = record.fired.get(&boundary.name).map_or(0, |f| f.count);
            // A timeout that fired and a reminder that reached its `max`
            // are spent: nothing of them is armed any more.
            if boundary.on.may_fire_again(fired) {
                lines.push(format!(
                    "   ↳ {}",
                    activity::boundary_label(boundary, fired)
                ));
            }
        }
    }
    lines
}

/// A gate needs a decision. On a TTY, ask inline; otherwise explain how to
/// decide it from another terminal (every gate has a durable path via
/// `bisa approve`).
async fn prompt_gate(engine: &Arc<Engine>, style: &Style, gate_id: &str, question: &str, id: &str) {
    if std::io::stdin().is_terminal() {
        let q = format!("{} [y/N] ", style.bold(question));
        let answer = tokio::task::spawn_blocking(move || crate::output::confirm_on_stderr(&q))
            .await
            .unwrap_or(false);
        match engine.decide(gate_id, answer, None, None, None) {
            Ok(_) => {}
            Err(e) => eprintln!(
                "{}",
                style.red(&bisa_i18n::say(&bisa_core::text!(
                    "cli-run-gate-decision-failed",
                    e = e.to_string()
                )))
            ),
        }
    } else {
        eprintln!(
            "{}",
            style.bold(&bisa_i18n::say(&bisa_core::text!(
                "cli-run-gate-pending-no-tty-decide-with",
                id = id.to_string()
            )))
        );
    }
}

fn is_spine(payload: &EnginePayload) -> bool {
    !matches!(payload, EnginePayload::Session { .. })
}

async fn shutdown(engine: Arc<Engine>) {
    if let Ok(engine) = Arc::try_unwrap(engine) {
        engine.shutdown().await;
    }
}

/// `bisa run` against a running daemon: begin the goal's work there, tail
/// the SSE activity for display, poll status for settlement. Execution stays
/// inside the daemon (its intake socket is the one injected into sessions),
/// so this process can disconnect at any time without ending the work.
async fn run_via_node(
    client: crate::client::NodeClient,
    out: &Out,
    id: GoalId,
    inputs: BTreeMap<String, Value>,
    begin: Begin,
    watch: bool,
    new: bool,
) -> Result<()> {
    let style = Style::stderr();

    let status = client.get(&format!("/goals/{id}")).await?;
    let current: Option<WorkflowRun> =
        serde_json::from_value(status["run"].clone()).unwrap_or(None);
    match current {
        Some(run) if !run.is_finished() && !new && begin.is_plain() => {
            eprintln!(
                "{}",
                style.bold(&bisa_i18n::say(&bisa_core::text!(
                    "cli-run-run-already-following",
                    a0 = (run.id).to_string(),
                    a1 = (run.status().as_str()).to_string()
                )))
            );
        }
        _ => {
            if status["goal"]["workflow"].is_null() {
                out.say(&bisa_core::text!(
                    "cli-run-nothing-run-via-node-goal-has",
                    id = id.to_string()
                ));
                out.json_value(json!({"goal": id.to_string(), "started": false}));
                return Ok(());
            }
            let v = client
                .post(&format!("/goals/{id}/run"), begin.body(&inputs))
                .await?;
            // The goal listens: the node made no run — `{status, secrets?}`
            // — and there is none to follow.
            if v["run"].is_null() {
                let listeners = client.get(&format!("/goals/{id}/listeners")).await?;
                render_armed(
                    out,
                    &armed(
                        id,
                        v["status"].clone(),
                        crate::listening::rows(&listeners).to_vec(),
                        crate::listening::rows(&v["secrets"]).to_vec(),
                    ),
                    true,
                );
                return Ok(());
            }
            let run_id = v["run"]["id"].as_str().unwrap_or("?");
            if v["status"] == json!("queued") {
                let live = status["run"]["id"].as_str().unwrap_or("?");
                eprintln!(
                    "{}",
                    style.bold(&bisa_i18n::say(&bisa_core::text!(
                        "cli-run-run-queued-behind-run",
                        run_id = run_id.to_string(),
                        live = live.to_string()
                    )))
                );
            } else if v["run"]["event"]["source"] == json!("test") {
                eprintln!(
                    "{}",
                    style.bold(&bisa_i18n::say(&bisa_core::text!(
                        "cli-run-test-run-started",
                        run = run_id.to_string(),
                        start = v["run"]["start"].as_str().unwrap_or_default().to_string()
                    )))
                );
            } else if watch {
                eprintln!(
                    "{}",
                    style.bold(&bisa_i18n::say(&bisa_core::text!(
                        "cli-run-started-run-2",
                        run_id = run_id.to_string()
                    )))
                );
            }
        }
    }
    follow_via_node(client, out, id, watch).await
}

/// Whether a frame of the daemon's stream (`{stream, payload}`) belongs on a
/// goal's timeline: an engine event about the goal — or about nothing in
/// particular — and a message in the goal's own thread. A session's chatter
/// only when watching.
fn on_goal_timeline(frame: &Value, goal: &str, watch: bool) -> bool {
    match frame["stream"].as_str() {
        Some("engine") => {
            let event = &frame["payload"];
            let spine = event["payload"]["type"].as_str() != Some("session");
            let ours = event["goal"].as_str() == Some(goal) || event["goal"].is_null();
            ours && (watch || spine)
        }
        Some("conversation") => frame["payload"]["scope"].as_str() == Some(goal),
        _ => false,
    }
}

/// Whether a frame belongs on one run's timeline: an engine event the run is
/// behind — named on the event's envelope, or in the payloads that are about
/// a run.
fn on_run_timeline(frame: &Value, run: &str, watch: bool) -> bool {
    let Some(event) = activity::engine_event(frame) else {
        return false;
    };
    let spine = event["payload"]["type"].as_str() != Some("session");
    let ours = event["run"].as_str() == Some(run) || event["payload"]["run"].as_str() == Some(run);
    ours && (watch || spine)
}

/// Tail the daemon's activity for the goal and poll its status until the
/// run settles: done, failed, or waiting on a person.
async fn follow_via_node(
    client: crate::client::NodeClient,
    out: &Out,
    id: GoalId,
    watch: bool,
) -> Result<()> {
    let style = Style::stderr();
    let mut events = client.events().await?;
    // The stream may end before the run does — the daemon restarted, the
    // connection dropped: the status is still polled, the activity no longer
    // read, and an ended stream is never asked again.
    let mut streaming = true;
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    let (status, run) = loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                eprintln!("{}", style.bold(&bisa_i18n::say(&bisa_core::text!("cli-run-detached-daemon-keeps-running-work"))));
                if let Some(log) = bisa_log::current() {
                    log.goodbye("detached");
                }
                std::process::exit(130);
            }
            event = events.recv(), if streaming => {
                match event {
                    Some(frame) => {
                        if on_goal_timeline(&frame, &id.to_string(), watch) {
                            if let Some(line) = activity::sse_line(&style, &frame) {
                                eprintln!("{line}");
                            }
                        }
                    }
                    None => streaming = false,
                }
            }
            _ = poll.tick() => {
                let v = client.get(&format!("/goals/{id}")).await?;
                let status = GoalStatus::from_str(v["status"].as_str().unwrap_or(""))
                    .map_err(|_| anyhow::anyhow!(bisa_core::text!("cli-run-bad-status-from-node", a0 = (v["status"]).to_string())))?;
                let run: Option<WorkflowRun> =
                    serde_json::from_value(v["run"].clone()).unwrap_or(None);
                match status {
                    GoalStatus::Running => {}
                    GoalStatus::Waiting => {
                        if let Some(run) = &run {
                            for line in waiting_lines(run, &id.to_string()) {
                                eprintln!("{}", style.bold(&line));
                            }
                        }
                        break (status, run);
                    }
                    _ => break (status, run),
                }
            }
        }
    };

    out.say(&bisa_core::text!(
        "cli-run-goal",
        id = id.to_string(),
        a0 = (status.as_str()).to_string(),
        a1 = (run
            .as_ref()
            .and_then(|r| r.outcome)
            .map(|o| bisa_i18n::say(&bisa_core::text!(
                "cli-run-run",
                a0 = (o.as_str()).to_string()
            )))
            .unwrap_or_default())
        .to_string()
    ));
    out.json_value(json!({"goal": id.to_string(), "status": status, "run": run}));
    Ok(())
}

/// Follow one run by its id — a goal's or the workspace's — on an embedded
/// engine until it settles: finished, or waiting on a person. Then shut the
/// engine down: the work would die with this process otherwise.
pub async fn follow_run(engine: Arc<Engine>, out: &Out, id: RunId, watch: bool) -> Result<()> {
    let style = Style::stderr();
    let mut rx = engine.events();
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    let run = loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                eprintln!("{}", style.bold(&bisa_i18n::say(&bisa_core::text!("cli-run-interrupted-shutting-down"))));
                shutdown(engine).await;
                if let Some(log) = bisa_log::current() {
                    log.goodbye("interrupted");
                }
                std::process::exit(130);
            }
            event = rx.recv() => {
                if let Ok(event) = event {
                    if watch || is_spine(&event.payload) {
                        if let Some(line) = activity::engine_line(&style, &event) {
                            eprintln!("{line}");
                        }
                    }
                    if let EnginePayload::GateOpened { gate_id, question, .. } = &event.payload {
                        prompt_gate(&engine, &style, gate_id, question, &id.to_string()).await;
                    }
                }
            }
            _ = poll.tick() => {
                let run = engine.workspace().get_run(id)?;
                match run.status() {
                    RunStatus::Queued | RunStatus::Running => {}
                    RunStatus::Waiting => {
                        for line in waiting_lines(&run, &id.to_string()) {
                            eprintln!("{}", style.bold(&line));
                        }
                        // A live gate in this process is still answerable
                        // from a TTY; give it one chance before leaving.
                        if !engine.inbox().is_empty() && std::io::stdin().is_terminal() {
                            continue;
                        }
                        break run;
                    }
                    RunStatus::Done | RunStatus::Failed | RunStatus::Cancelled => break run,
                }
            }
        }
    };
    settled(out, &run);
    shutdown(engine).await;
    Ok(())
}

/// Follow one run by its id on the daemon — tail its activity, poll the run
/// until it settles. The work stays the daemon's; this process may leave.
pub async fn follow_run_via_node(
    client: crate::client::NodeClient,
    out: &Out,
    id: RunId,
    watch: bool,
) -> Result<()> {
    let style = Style::stderr();
    let mut events = client.events().await?;
    // As in `follow_via_node`: an ended stream is never asked again.
    let mut streaming = true;
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    let run = loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                eprintln!("{}", style.bold(&bisa_i18n::say(&bisa_core::text!("cli-run-detached-daemon-keeps-running-work"))));
                if let Some(log) = bisa_log::current() {
                    log.goodbye("detached");
                }
                std::process::exit(130);
            }
            event = events.recv(), if streaming => {
                match event {
                    Some(frame) => {
                        if on_run_timeline(&frame, &id.to_string(), watch) {
                            if let Some(line) = activity::sse_line(&style, &frame) {
                                eprintln!("{line}");
                            }
                        }
                    }
                    None => streaming = false,
                }
            }
            _ = poll.tick() => {
                let v = client.get(&format!("/runs/{id}")).await?;
                let run: WorkflowRun = serde_json::from_value(v["run"].clone())
                    .context(bisa_core::text!("cli-main-bad-run-payload-from-node"))?;
                match run.status() {
                    RunStatus::Queued | RunStatus::Running => {}
                    RunStatus::Waiting => {
                        for line in waiting_lines(&run, &id.to_string()) {
                            eprintln!("{}", style.bold(&line));
                        }
                        break run;
                    }
                    RunStatus::Done | RunStatus::Failed | RunStatus::Cancelled => break run,
                }
            }
        }
    };
    settled(out, &run);
    Ok(())
}

/// Where a followed run settled, said and handed to `--json`.
fn settled(out: &Out, run: &WorkflowRun) {
    out.say(&bisa_core::text!(
        "cli-run-run-is",
        run = run.id.to_string(),
        status = run.status().as_str().to_string()
    ));
    out.json_value(json!({
        "run": run.id.to_string(),
        "scope": run.scope.as_str(),
        "goal": run.scope.goal().map(|g| g.to_string()),
        "status": run.status(),
        "outcome": run.outcome,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{Fired, RunEntry, RunScope, Signal, SignalScope};

    /// A line the catalog has: a miss renders as the message's id.
    fn sentence(line: &str) {
        assert!(
            !line.is_empty() && !line.contains("cli-"),
            "a line the catalog does not say: {line:?}"
        );
    }

    /// The message a refusal was made with, by its id.
    fn refusal<T>(result: Result<T>) -> String {
        match result {
            Ok(_) => panic!("taken, where a refusal was due"),
            Err(e) => match e.downcast_ref::<bisa_core::Text>() {
                Some(text) => text.id.to_string(),
                None => panic!("a refusal the catalog does not say: {e:#}"),
            },
        }
    }

    fn step(id: &str) -> StepId {
        StepId::new(id).expect("a step id")
    }

    fn workflow_of(steps: Value) -> Workflow {
        serde_json::from_value(json!({
            "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "name": "Review",
            "origin": {"origin": "workspace"},
            "author": "cd".repeat(32),
            "revision": 1,
            "created_at": 0,
            "steps": steps,
        }))
        .expect("a workflow")
    }

    /// A review a person starts by hand and a hook starts: an approval with a
    /// timeout that diverts it and a reminder beside it, then a wait on a
    /// signal and a wait on a person.
    fn review() -> Workflow {
        workflow_of(json!([
            {"id": "by-hand", "name": "By hand", "kind": "start",
             "on": {"event": "manual"}, "then": ["review"]},
            {"id": "ticket", "name": "Ticket", "kind": "start",
             "on": {"event": "hook"}, "then": ["review"]},
            {"id": "review", "name": "Review it", "kind": "approval", "prompt": "ship?",
             "boundaries": [
                 {"name": "late", "on": {"event": "after", "secs": 172800},
                  "act": "divert"},
                 {"name": "nudge", "on": {"event": "every", "secs": 86400, "max": 2},
                  "act": "notify", "template": "still waiting"}
             ],
             "then": ["ready", {"to": "gave-up", "branch": "late"}]},
            {"id": "ready", "name": "Ready", "kind": "wait",
             "until": {"until": "signal", "name": "report.ready"}, "then": ["hold"]},
            {"id": "hold", "name": "Hold", "kind": "wait", "until": {"until": "release"}},
            {"id": "gave-up", "name": "Gave up", "kind": "end", "finish": "failed"}
        ]))
    }

    fn run_of(workflow: Workflow, entry: RunEntry) -> WorkflowRun {
        WorkflowRun::new(
            RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            RunScope::Workspace {
                budget: Default::default(),
            },
            workflow,
            BTreeMap::new(),
            entry,
            7,
        )
    }

    /// An occurrence, as the run it began carries it.
    fn occurrence(source: SignalSource, name: Option<&str>) -> Signal {
        Signal {
            id: "01SIGNAL".into(),
            listener: None,
            source,
            name: name.map(str::to_string),
            at: 7,
            payload: json!({}),
            scope: SignalScope::Workspace,
            chain: Default::default(),
            dedupe_key: None,
        }
    }

    fn waits(run: &mut WorkflowRun, id: &str) {
        let record = run.steps.get_mut(&step(id)).expect("a step of the run");
        record.state = StepState::Waiting;
    }

    fn fired(run: &mut WorkflowRun, id: &str, boundary: &str, count: u32) {
        let record = run.steps.get_mut(&step(id)).expect("a step of the run");
        record.fired.insert(
            bisa_core::Branch::new(boundary).expect("a boundary's name"),
            Fired {
                count,
                seq: 1,
                at: 9,
            },
        );
    }

    #[test]
    fn a_begin_is_what_was_typed_and_the_body_the_node_reads() {
        let plain = Begin::parse(None, None).expect("nothing named");
        assert!(plain.is_plain());
        let inputs = BTreeMap::from([("who".to_string(), json!("the team"))]);
        assert_eq!(plain.body(&inputs), json!({"inputs": {"who": "the team"}}));

        let test = Begin::parse(Some("ticket"), Some(r#"{"subject": "help"}"#))
            .expect("a start and its sample");
        assert!(!test.is_plain());
        assert_eq!(test.start, Some(step("ticket")));
        assert_eq!(
            test.body(&BTreeMap::new()),
            json!({"inputs": {}, "start": "ticket", "event": {"subject": "help"}})
        );
        // A start alone is a start: no event travels that nobody gave.
        let now = Begin::parse(Some("by-hand"), None).expect("a start");
        assert!(!now.is_plain());
        assert_eq!(
            now.body(&BTreeMap::new()),
            json!({"inputs": {}, "start": "by-hand"})
        );

        assert_eq!(
            refusal(Begin::parse(Some("Not A Step"), None)),
            "cli-step-not-step-id"
        );
        assert_eq!(
            refusal(Begin::parse(Some("ticket"), Some("{subject"))),
            "cli-run-data-not-json"
        );
        assert_eq!(parse_data("[1, 2]").ok(), Some(json!([1, 2])));
    }

    #[test]
    fn an_entry_is_the_door_a_begin_names() {
        let workflow = review();
        let begin =
            |start: Option<&str>, data: Option<&str>| Begin::parse(start, data).expect("a begin");
        let entry =
            |start: Option<&str>, data: Option<&str>| way_in(Some(&workflow), begin(start, data));
        assert_eq!(entry(None, None).ok(), Some(WayIn::Start));
        assert_eq!(entry(Some("by-hand"), None).ok(), Some(WayIn::ByHand));
        // An event start is a test run: of the sample, or of an empty one.
        assert_eq!(
            entry(Some("ticket"), Some(r#"{"subject": "help"}"#)).ok(),
            Some(WayIn::Test {
                start: step("ticket"),
                payload: json!({"subject": "help"}),
            })
        );
        assert_eq!(
            entry(Some("ticket"), None).ok(),
            Some(WayIn::Test {
                start: step("ticket"),
                payload: json!({}),
            })
        );
        assert_eq!(
            refusal(entry(Some("by-hand"), Some("{}"))),
            "cli-run-start-by-hand-reads-no-data"
        );
        assert_eq!(refusal(entry(None, Some("{}"))), "cli-run-data-needs-start");
        // A step that is no start is the engine's to refuse, in its words.
        assert_eq!(
            entry(Some("review"), None).ok(),
            Some(WayIn::Test {
                start: step("review"),
                payload: json!({}),
            })
        );

        // A workflow that names no start begins by hand at its root.
        let plain = workflow_of(json!([
            {"id": "build", "name": "Build", "kind": "agent", "instructions": "build it"}
        ]));
        assert_eq!(
            way_in(Some(&plain), begin(Some("build"), None)).ok(),
            Some(WayIn::ByHand)
        );
        // A home with no workflow yet is judged by nothing here.
        assert_eq!(way_in(None, begin(None, None)).ok(), Some(WayIn::Start));
        assert_eq!(
            way_in(None, begin(Some("ticket"), None)).ok(),
            Some(WayIn::Test {
                start: step("ticket"),
                payload: json!({}),
            })
        );
    }

    #[test]
    fn a_run_says_who_started_it() {
        let by_hand = run_of(review(), RunEntry::by_hand());
        assert_eq!(StartedBy::said(&by_hand, None), StartedBy::You);
        assert_eq!(StartedBy::said(&by_hand, None).words(), "by you");
        assert!(!is_test(&by_hand));

        let on_event = |source: SignalSource, name: Option<&str>| {
            run_of(
                review(),
                RunEntry::at(step("ticket"), Some(occurrence(source, name))),
            )
        };
        let called = on_event(SignalSource::Hook, None);
        assert_eq!(
            StartedBy::said(&called, None),
            StartedBy::Event {
                event: SignalSource::Hook,
                detail: None,
            }
        );
        assert_eq!(StartedBy::said(&called, None).words(), "by hook");
        assert_eq!(
            StartedBy::said(&on_event(SignalSource::Schedule, None), None).words(),
            "by schedule"
        );
        // What the node says of an event is said after its kind: a signal's
        // name, a platform's topic, a run's title.
        let named = |source: SignalSource, name: &str| {
            StartedBy::said(&on_event(source, Some(name)), Some(name.to_string())).words()
        };
        assert_eq!(
            named(SignalSource::Signal, "report.ready"),
            "by signal report.ready"
        );
        assert_eq!(
            named(SignalSource::Platform, "goal.closed"),
            "by platform goal.closed"
        );

        // A test run is said as one, with the word of the event it stood for.
        let test = on_event(SignalSource::Test, None);
        assert!(is_test(&test));
        assert_eq!(
            StartedBy::said(&test, None),
            StartedBy::Test {
                event: "hook".into(),
            }
        );
        assert_eq!(
            StartedBy::said(&test, None).words(),
            "test, as if hook had happened"
        );
        let line = test_run_line(&test);
        sentence(&line);
        assert!(
            line.contains("test run")
                && line.contains("ticket")
                && line.contains(&test.id.to_string()),
            "{line}"
        );
    }

    #[test]
    fn who_started_a_run_is_read_as_the_node_says_it() {
        let read = |started_by: Value| StartedBy::read(&json!({ "started_by": started_by }));
        assert_eq!(read(json!({"by": "you"})), StartedBy::You);
        let maya = read(json!({"by": "event", "event": "message", "detail": "Maya"}));
        assert_eq!(maya.words(), "by message from Maya");
        // A message nobody is named for is said by its kind alone.
        assert_eq!(
            read(json!({"by": "event", "event": "message"})).words(),
            "by message"
        );
        assert_eq!(
            read(json!({"by": "event", "event": "run", "detail": "01RUN"})).words(),
            "by run 01RUN"
        );
        assert_eq!(
            read(json!({"by": "test", "event": "schedule"})).words(),
            "test, as if schedule had happened"
        );
        // A summary that does not say is a person's.
        assert_eq!(StartedBy::read(&json!({"id": "01RUN"})), StartedBy::You);

        // Every kind of event the node names is one the CLI reads, and the
        // CLI writes it as the node does.
        use bisa_node::dto::EventKind as K;
        for kind in [
            K::Schedule,
            K::Hook,
            K::Message,
            K::Signal,
            K::Project,
            K::Run,
            K::Platform,
            K::Connector,
            K::Check,
        ] {
            let said = bisa_node::dto::StartedBy::Event {
                event: kind,
                detail: Some("x".into()),
            };
            let wire = serde_json::to_value(&said).expect("a summary's word");
            let read = StartedBy::read(&json!({ "started_by": wire }));
            assert_eq!(
                read,
                StartedBy::Event {
                    event: source_of(kind),
                    detail: Some("x".into()),
                }
            );
            assert_eq!(serde_json::to_value(&read).ok(), Some(wire));
            sentence(&read.words());
        }
    }

    #[test]
    fn a_goal_s_runs_are_listed_with_who_started_each() {
        let mut first = run_of(review(), RunEntry::by_hand());
        first.started_at = Some(8);
        let second = run_of(
            review(),
            RunEntry::at(
                step("ticket"),
                Some(occurrence(SignalSource::Signal, Some("report.ready"))),
            ),
        );
        // Who started each is the caller's to say: here, what its event names.
        let lines = RunLine::list(&[first, second], |run| {
            let named = run.event.as_ref().and_then(|event| event.name.clone());
            StartedBy::said(run, named)
        });
        assert_eq!(lines.len(), 2);
        // Newest first; the queued one says its place.
        assert_eq!(lines[0].position, Some(1));
        assert!(
            lines[0].words().contains("by signal report.ready")
                && lines[0].words().contains("position 1"),
            "{}",
            lines[0].words()
        );
        assert!(
            lines[1].words().contains("by you") && lines[1].words().contains("started 8"),
            "{}",
            lines[1].words()
        );
        let wire = serde_json::to_value(&lines[0]).expect("a run's line");
        assert_eq!(
            wire["started_by"],
            json!({"by": "event", "event": "signal", "detail": "report.ready"})
        );
    }

    #[test]
    fn a_waiting_step_says_its_verb_and_the_boundary_events_still_armed() {
        let mut run = run_of(review(), RunEntry::by_hand());
        let id = run.id.to_string();
        assert!(waiting_lines(&run, &id).is_empty(), "nothing waits yet");

        waits(&mut run, "review");
        fired(&mut run, "review", "nudge", 1);
        let lines = waiting_lines(&run, &id);
        assert_eq!(lines.len(), 3, "{lines:?}");
        lines.iter().for_each(|line| sentence(line));
        assert!(
            lines[0].starts_with("⏸ Review it (approval)")
                && lines[0].contains(&format!("bisa approve {id}")),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].contains("↳ late diverts after 172800s"),
            "{}",
            lines[1]
        );
        assert!(
            lines[2].contains("↳ nudge posts every 86400s") && lines[2].contains("1 of 2"),
            "{}",
            lines[2]
        );

        // A timeout that fired and a reminder at its `max` are spent.
        fired(&mut run, "review", "late", 1);
        fired(&mut run, "review", "nudge", 2);
        let lines = waiting_lines(&run, &id);
        assert_eq!(lines.len(), 1, "{lines:?}");

        // A wait says what it holds for — or the verb that lets it go.
        let mut run = run_of(review(), RunEntry::by_hand());
        waits(&mut run, "ready");
        waits(&mut run, "hold");
        let lines = waiting_lines(&run, &id);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0].starts_with("⏸ Ready (wait)") && lines[0].contains("report.ready"),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].contains(&format!("bisa step release {id} hold")),
            "{}",
            lines[1]
        );
    }

    #[test]
    fn a_stop_says_what_it_ended_and_that_the_goal_no_longer_listens() {
        let id = GoalId::from_ulid(ulid::Ulid::from_parts(5, 1));
        let nothing = json!({"stopped": null, "withdrawn": []});
        let lines = stopped_lines(id, &nothing, false);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("nothing to stop"), "{}", lines[0]);

        // A goal that only listened: the listening is all there was to stop.
        let lines = stopped_lines(id, &nothing, true);
        assert_eq!(lines.len(), 1, "{lines:?}");
        sentence(&lines[0]);
        assert!(
            lines[0].contains(&id.to_string()) && lines[0].contains("no longer listens"),
            "{}",
            lines[0]
        );

        let both = json!({"stopped": "01RUN", "withdrawn": ["01A", "01B"]});
        let lines = stopped_lines(id, &both, true);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0].contains("run 01RUN stopped") && lines[0].contains("2 queued runs"),
            "{}",
            lines[0]
        );
        assert!(lines[1].contains("no longer listens"), "{}", lines[1]);
        let lines = stopped_lines(id, &both, false);
        assert_eq!(lines.len(), 1, "{lines:?}");
    }

    #[test]
    fn an_armed_goal_answers_no_run_and_says_what_it_hears() {
        let id = GoalId::from_ulid(ulid::Ulid::from_parts(5, 1));
        let listener = json!({
            "listener": format!("goal:{id}/ticket"),
            "host": format!("goal:{id}"),
            "step": "ticket",
            "event": "hook",
            "summary": {"id": "step-summary-start-hook-public"},
            "backlog": 0,
            "live_runs": 0,
            "local_hook": format!("/goals/{id}/hooks/ticket"),
            "public_hook": {"path": format!("/hooks/goal:{id}/ticket"), "has_secret": true},
        });
        let secret = json!({
            "step": "ticket",
            "path": format!("/hooks/goal:{id}/ticket"),
            "secret": "ab".repeat(32),
        });
        let said = armed(
            id,
            json!("waiting"),
            vec![listener.clone()],
            vec![secret.clone()],
        );
        assert_eq!(
            said,
            json!({
                "goal": id.to_string(),
                "status": "waiting",
                "run": null,
                "listeners": [listener],
                "secrets": [secret],
            })
        );

        let lines = armed_lines(&said, true);
        lines.iter().for_each(|line| sentence(line));
        let page = lines.join("\n");
        for word in [
            format!("goal {id} is listening (waiting)"),
            format!("goal:{id}/ticket"),
            format!("/goals/{id}/hooks/ticket"),
            "ab".repeat(32),
            "will not be shown again".to_string(),
            format!("bisa workflow hook-secret goal {id} ticket --rotate"),
        ] {
            assert!(page.contains(&word), "{word}: {page}");
        }
        assert!(!page.contains("bisa node"), "a node hears it: {page}");
        // With no node to hear them, its events are heard once one runs.
        let unheard = armed_lines(&said, false);
        assert_eq!(unheard.len(), lines.len() + 1);
        assert!(
            unheard
                .last()
                .is_some_and(|line| line.contains("bisa node")),
            "{unheard:?}"
        );
    }

    /// A frame of the daemon's stream about `goal` and `run`.
    fn engine_frame(goal: Option<&str>, run: Option<&str>, payload: Value) -> Value {
        json!({
            "stream": "engine",
            "payload": {"goal": goal, "run": run, "payload": payload},
        })
    }

    #[test]
    fn a_frame_is_on_a_goal_s_timeline_when_it_is_about_the_goal() {
        let step_changed = json!({"type": "step_changed", "run": "01RUN", "step": "review"});
        let ours = engine_frame(Some("01GOAL"), Some("01RUN"), step_changed.clone());
        assert!(on_goal_timeline(&ours, "01GOAL", false));
        let theirs = engine_frame(Some("01OTHER"), Some("01RUN"), step_changed);
        assert!(!on_goal_timeline(&theirs, "01GOAL", true));
        // An event about no goal in particular is everybody's.
        let global = engine_frame(None, None, json!({"type": "settings_changed"}));
        assert!(on_goal_timeline(&global, "01GOAL", false));
        // A session's chatter is shown only to who watches.
        let chatter = engine_frame(Some("01GOAL"), None, json!({"type": "session"}));
        assert!(!on_goal_timeline(&chatter, "01GOAL", false));
        assert!(on_goal_timeline(&chatter, "01GOAL", true));

        let message = |scope: &str| json!({"stream": "conversation", "payload": {"scope": scope}});
        assert!(on_goal_timeline(&message("01GOAL"), "01GOAL", false));
        assert!(!on_goal_timeline(&message("01OTHER"), "01GOAL", true));
        let inbox = json!({"stream": "inbox", "payload": {"key": "01GOAL"}});
        assert!(!on_goal_timeline(&inbox, "01GOAL", true));
        // An event outside its envelope is no frame of the stream.
        assert!(!on_goal_timeline(
            &json!({"goal": "01GOAL"}),
            "01GOAL",
            true
        ));
    }

    #[test]
    fn a_frame_is_on_a_run_s_timeline_when_the_run_is_behind_it() {
        let step_changed = json!({"type": "step_changed", "run": "01RUN", "step": "review"});
        // Named on the envelope, or in a payload that is about a run.
        let enveloped = engine_frame(None, Some("01RUN"), json!({"type": "scheduled"}));
        assert!(on_run_timeline(&enveloped, "01RUN", false));
        let in_payload = engine_frame(None, None, step_changed);
        assert!(on_run_timeline(&in_payload, "01RUN", false));
        assert!(!on_run_timeline(&in_payload, "01OTHER", true));
        // An event about no run is not a run's, whoever watches.
        let global = engine_frame(None, None, json!({"type": "settings_changed"}));
        assert!(!on_run_timeline(&global, "01RUN", true));
        let chatter = engine_frame(None, Some("01RUN"), json!({"type": "session"}));
        assert!(!on_run_timeline(&chatter, "01RUN", false));
        assert!(on_run_timeline(&chatter, "01RUN", true));
        let message = json!({"stream": "conversation", "payload": {"scope": "01RUN"}});
        assert!(!on_run_timeline(&message, "01RUN", true));
    }
}
