//! `bisa workflow`: the library of workflows — the catalog's templates
//! and this workspace's own — its runs of the workspace, and the goal ⇄
//! workflow relation.
//!
//! **Records here, runs through an engine.** Listing, showing, defining,
//! editing, validating and removing a workflow is a signed snapshot plus an
//! index row, so those verbs talk to the workspace directly and work with or
//! without a daemon (a running daemon is preferred so the desktop hears
//! `workflow_changed`). Pointing a goal at a workflow (`use`) goes through the
//! engine, because it is refused while a run is live and that rule lives
//! there.
//!
//! A definition is read from a file or stdin: JSON, or TOML in the shape a
//! catalog template is written in — read by the store's own template parser,
//! so a `spawn` step may name another template by slug and a misspelled key
//! is refused the way the installer refuses it. The two land in the same
//! `NewWorkflow`; nothing is converted.
//!
//! `new` and `edit` keep a **draft**: the definition is recorded with its
//! problems and the problems are printed, with or without a daemon, so the
//! two paths answer alike. A start (`bisa workflow run`, `bisa run`) refuses
//! problems. `edit` names the revision it edited (the stored one by default,
//! `--revision` to say otherwise); a workflow that moved since is a
//! conflict, not a silent overwrite.
//!
//! **Runs of the workspace.** `run` starts one at once — no goal is
//! captured, any number may go together — and follows it; `runs` lists the
//! workflow's; `stop` and `restart` act on every one that is going, or on
//! the one `--run` names. A goal's run of the workflow is its goal's
//! (`bisa stop`, `bisa restart`). `run --start` is the one test door: naming
//! an event start makes a **test run**, begun there as if its event had
//! happened with `--data`.
//!
//! **On and Off.** A library workflow hears its start events only once a
//! person turns it On: `on` hands the inputs its events do not supply and an
//! optional per-run budget to the engine's one door, which judges whether it
//! may listen — a refusal is the node's, in its words — and prints what it
//! listens for and each public hook's secret, **once**. `off` stops it;
//! `listeners` says what every armed start is doing; `hook-secret` shows a
//! hook's standing and, with `--rotate`, mints its secret anew. A toggle is
//! never a revision of the definition.

use crate::ctx::Ctx;
use crate::listening::{self, rows, HookHost};
use crate::output::Out;
use crate::run::{way_in, Begin, StartedBy};
use crate::tags::{keeps, TagFilterArgs};
use anyhow::{anyhow, bail, Context as _, Result};
use bisa_core::tags::TagEntity;
use bisa_core::workflow::Problem;
use bisa_core::{
    Budget, ListenerHost, ListenerKey, Listening, StepId, WayIn, Workflow, WorkflowId,
    WorkflowOrigin, WorkflowRun,
};
use bisa_engine::Engine;
use bisa_node::dto::{RunSummary, StartSummary};
use bisa_store::{CatalogKind, NewWorkflow, Workspace};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::str::FromStr;

/// The ceiling each run the workflow's events start is given. None of the
/// three leaves the workspace default (`budget.default.*`) in force; any of
/// them makes the listening's own budget, the others unlimited — and a zero
/// lifts a ceiling, as it does in the settings.
#[derive(Args, Default)]
pub struct BudgetArgs {
    /// Token ceiling for each run its events start; 0 lifts it
    #[arg(long = "budget-tokens")]
    budget_tokens: Option<u64>,
    /// Cost ceiling in US cents for each run its events start; 0 lifts it
    #[arg(long = "budget-usd-cents")]
    budget_usd_cents: Option<u64>,
    /// Wall-clock ceiling in seconds for each run its events start; 0 lifts it
    #[arg(long = "budget-secs")]
    budget_secs: Option<u64>,
}

impl BudgetArgs {
    /// The budget these flags say: none when no flag was given, so the
    /// workspace default applies.
    fn budget(&self) -> Option<Budget> {
        let given = [self.budget_tokens, self.budget_usd_cents, self.budget_secs];
        if given.iter().all(Option::is_none) {
            return None;
        }
        let lifted = |ceiling: Option<u64>| ceiling.filter(|n| *n > 0);
        Some(Budget {
            max_tokens: lifted(self.budget_tokens),
            max_usd_cents: lifted(self.budget_usd_cents),
            max_wall_clock_secs: lifted(self.budget_secs),
        })
    }
}

#[derive(Subcommand)]
pub enum WorkflowCmd {
    /// List the library; `--templates` the catalog's, `--goal` one goal's designs
    List {
        /// The catalog's templates instead of installed workflows
        #[arg(long)]
        templates: bool,
        /// One goal's own designs
        #[arg(long)]
        goal: Option<String>,
        /// Everything — the library and every goal's designs
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Show one workflow — by id, or by catalog slug — with its problems
    Show { workflow: String },
    /// Record a local workflow from a definition file (`-` reads stdin)
    New {
        /// JSON or TOML: `{name, description?, inputs?, steps, tags?}`
        #[arg(long)]
        from: String,
    },
    /// Replace a workflow's definition as its next revision
    Edit {
        id: String,
        #[arg(long)]
        from: String,
        /// The revision the definition was edited from (defaults to the
        /// stored one). A workflow that moved past it is a conflict.
        #[arg(long)]
        revision: Option<u64>,
    },
    /// Every problem a definition has, without recording it
    Validate {
        #[arg(long)]
        from: String,
    },
    /// Forget a workflow. Refused while a goal or another workflow uses it.
    Rm { id: String },
    /// Copy a goal's design into the library
    Promote { id: String },
    /// Run a workflow in the workspace — no goal — and follow the run; with
    /// `--start` naming an event start, a test run
    Run {
        id: String,
        /// Run input, `name=value` (repeatable)
        #[arg(long = "input")]
        inputs: Vec<String>,
        /// The start step the run begins at: the start by hand, or — for a
        /// test run — an event start
        #[arg(long)]
        start: Option<String>,
        /// A test run's sample event as JSON: what the start's mapping reads,
        /// as if it had happened
        #[arg(long, requires = "start")]
        data: Option<String>,
        /// Render a live activity timeline
        #[arg(long)]
        watch: bool,
    },
    /// Turn a workflow On: its start events are heard from now on, each
    /// occurrence a run of the workspace. Prints what it listens for, and
    /// each public hook's secret once
    On {
        id: String,
        /// An input its event runs bind, `name=value` (repeatable): what its
        /// start events read and do not map
        #[arg(long = "input")]
        inputs: Vec<String>,
        #[command(flatten)]
        budget: BudgetArgs,
    },
    /// Turn a workflow Off: its start events are no longer heard; a run
    /// already going goes on
    Off { id: String },
    /// The listeners — every start event armed for a workflow that is On or
    /// a goal that listens: one workflow's, one goal's, or all
    Listeners {
        /// A workflow's id; without it and without `--goal`, every listener
        id: Option<String>,
        /// A goal's listeners instead
        #[arg(long, conflicts_with = "id")]
        goal: Option<String>,
    },
    /// A hook start's standing — its paths, whether its secret is minted —
    /// or, with `--rotate`, a new secret, shown once
    HookSecret {
        /// Whose hook it is: `workflow` or `goal`
        #[arg(value_enum)]
        host: HookHost,
        /// The workflow's id, or the goal's
        id: String,
        /// The hook start's step id
        step: String,
        /// Mint a new secret: the old one stops verifying at once
        #[arg(long)]
        rotate: bool,
    },
    /// The workflow's runs of the workspace, newest first
    Runs { id: String },
    /// Stop every run of the workspace of the workflow that is going
    Stop {
        id: String,
        /// Stop this one run of it instead
        #[arg(long)]
        run: Option<String>,
        /// Why, in a sentence — kept on the run
        #[arg(long)]
        rationale: Option<String>,
    },
    /// Restart every run of the workspace of the workflow that is going
    Restart {
        id: String,
        /// Restart this one run of it instead, and follow the new run
        #[arg(long)]
        run: Option<String>,
        /// Render a live activity timeline while following
        #[arg(long)]
        watch: bool,
    },
    /// Point a goal at the workflow its next run will use
    Use {
        goal: String,
        /// Workflow id or catalog slug (a slug is installed on the way);
        /// with `--from`, a name for the design being recorded instead
        #[arg(default_value = "")]
        workflow: String,
        /// A definition file (`-` reads stdin): recorded as this goal's own
        /// design and pointed at
        #[arg(long)]
        from: Option<String>,
    },
}

pub async fn workflow(ctx: &Ctx, out: &Out, cmd: WorkflowCmd) -> Result<()> {
    match cmd {
        WorkflowCmd::List {
            templates,
            goal,
            all,
            filter,
        } => list(ctx, out, templates, goal.as_deref(), all, &filter).await,
        WorkflowCmd::Show { workflow } => show(ctx, out, &workflow).await,
        WorkflowCmd::New { from } => new(ctx, out, &from).await,
        WorkflowCmd::Edit { id, from, revision } => edit(ctx, out, &id, &from, revision).await,
        WorkflowCmd::Validate { from } => validate(ctx, out, &from).await,
        WorkflowCmd::Rm { id } => rm(ctx, out, &id).await,
        WorkflowCmd::Promote { id } => promote(ctx, out, &id).await,
        WorkflowCmd::Run {
            id,
            inputs,
            start,
            data,
            watch,
        } => {
            let begin = Begin::parse(start.as_deref(), data.as_deref())?;
            let inputs = crate::inputs::Typed::parse(&inputs)?;
            run(ctx, out, &id, inputs, begin, watch).await
        }
        WorkflowCmd::On { id, inputs, budget } => {
            let inputs = crate::inputs::Typed::parse(&inputs)?;
            on(ctx, out, &id, inputs, budget.budget()).await
        }
        WorkflowCmd::Off { id } => off(ctx, out, &id).await,
        WorkflowCmd::Listeners { id, goal } => {
            listeners(ctx, out, id.as_deref(), goal.as_deref()).await
        }
        WorkflowCmd::HookSecret {
            host,
            id,
            step,
            rotate,
        } => hook_secret(ctx, out, host, &id, &step, rotate).await,
        WorkflowCmd::Runs { id } => runs(ctx, out, &id).await,
        WorkflowCmd::Stop { id, run, rationale } => {
            stop(ctx, out, &id, run.as_deref(), rationale).await
        }
        WorkflowCmd::Restart { id, run, watch } => {
            restart(ctx, out, &id, run.as_deref(), watch).await
        }
        WorkflowCmd::Use {
            goal,
            workflow,
            from,
        } => use_workflow(ctx, out, &goal, &workflow, from.as_deref()).await,
    }
}

// ---------------------------------------------------------------------------
// Reading a definition
// ---------------------------------------------------------------------------

/// A definition from a file or stdin. `.toml` is read as a catalog-shaped
/// template — the definition under `[workflow]`, or bare — through the
/// store's parser, which resolves a `spawn` step's slug against what this
/// workspace has installed; anything else is read as JSON. Both are the one
/// `NewWorkflow` shape, and a key it does not have is refused.
pub fn read_definition(ws: &Workspace, src: &str) -> Result<NewWorkflow> {
    let text = if src == "-" {
        std::io::read_to_string(std::io::stdin())
            .context(bisa_core::text!("cli-workflow-reading-workflow-from-stdin"))?
    } else {
        std::fs::read_to_string(src).with_context(|| format!("reading {src}"))?
    };
    if src.ends_with(".toml") {
        Ok(ws.parse_workflow_toml(src, &text)?)
    } else {
        serde_json::from_str(&text).with_context(|| {
            bisa_i18n::say(&bisa_core::text!(
                "cli-workflow-not-workflow-definition",
                src = src.to_string()
            ))
        })
    }
}

/// The revision of the goal's own design, when the goal points at one — what
/// a `use --from` on a goal that already has a design must name.
fn design_revision(ws: &Workspace, goal: bisa_core::GoalId) -> Result<Option<u64>> {
    let g = ws.get_goal(goal)?;
    Ok(g.workflow
        .and_then(|id| ws.get_workflow(id).ok())
        .filter(|wf| wf.origin == WorkflowOrigin::Goal { goal })
        .map(|wf| wf.revision))
}

/// A workflow id, or a catalog slug. A slug that is not installed yet is
/// installed on the way when `install` says so; otherwise it is an error
/// naming both things it could have been.
pub fn resolve_workflow(ws: &Workspace, raw: &str, install: bool) -> Result<Workflow> {
    let raw = raw.trim();
    if let Ok(id) = WorkflowId::from_str(raw) {
        return Ok(ws.get_workflow(id)?);
    }
    if let Some(wf) = ws.workflow_for_slug(raw)? {
        return Ok(wf);
    }
    if !install {
        bail!(bisa_core::text!(
            "cli-workflow-neither-installed-workflow-s-id-nor",
            raw = format!("{raw:?}")
        ));
    }
    let installed = ws.install(CatalogKind::Workflow, raw)?;
    match installed.workflows.iter().find(|(slug, _)| slug == raw) {
        Some((_, id)) => Ok(ws.get_workflow(*id)?),
        None => ws.workflow_for_slug(raw)?.ok_or_else(|| {
            anyhow!(bisa_core::text!(
                "cli-workflow-no-workflow-catalog-template-named",
                raw = format!("{raw:?}")
            ))
        }),
    }
}

/// A definition as a workflow that exists only to be validated or read: a
/// fresh id, the owner as author, local, revision zero.
fn draft_as_workflow(ws: &Workspace, draft: NewWorkflow) -> Workflow {
    Workflow {
        id: WorkflowId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now())),
        name: draft.name,
        description: draft.description,
        inputs: draft.inputs,
        steps: draft.steps,
        origin: WorkflowOrigin::Workspace,
        author: ws.owner_principal(),
        tags: draft.tags,
        revision: 0,
        archived: None,
        decision_making: draft.decision_making,
        created_at: 0,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn origin_label(o: &WorkflowOrigin) -> String {
    match o {
        WorkflowOrigin::Workspace => "workspace".to_string(),
        WorkflowOrigin::Catalog { slug } => format!("catalog:{slug}"),
        WorkflowOrigin::Goal { goal } => format!("goal:{goal}"),
    }
}

/// A workflow as `list` and `show` print it: the definition with its
/// problems, and — in the words the node's row says them — where it stands as
/// a listener.
#[derive(serde::Serialize)]
struct Row {
    workflow: Workflow,
    problems: Vec<Problem>,
    /// Its standing while it is On; `null` while it is Off.
    listening: Option<Listening>,
    /// One per `start` step, the manual one included: which event, in words.
    starts: Vec<StartSummary>,
    /// Only events begin it: `run` is a test run.
    event_only: bool,
    /// What turning it On asks.
    listening_needs: Vec<String>,
}

impl Row {
    /// The row of `workflow` as this workspace holds it. A start whose event
    /// reads an input is summarised as it is armed while the workflow is On.
    fn of(ws: &Workspace, workflow: Workflow, problems: Vec<Problem>) -> Result<Self> {
        let listening = ws.listening(&ListenerHost::Workspace {
            workflow: workflow.id,
        })?;
        Ok(Row {
            starts: StartSummary::list(&workflow, listening.as_ref()),
            event_only: workflow.is_event_only(),
            listening_needs: bisa_node::dto::listening_needs(&workflow),
            listening,
            problems,
            workflow,
        })
    }

    /// What it hears, in words: one summary per start that begins on an event.
    fn hears(&self) -> Vec<String> {
        self.starts
            .iter()
            .filter(|start| start.event != bisa_core::StartOn::Manual.as_str())
            .map(|start| bisa_i18n::say(&start.summary))
            .collect()
    }

    /// The mark a row that is On wears in the list, and nothing while Off.
    fn on_words(&self) -> String {
        match &self.listening {
            None => String::new(),
            Some(listening) if listening.is_paused() => bisa_i18n::say(&bisa_core::text!(
                "cli-workflow-on-paused",
                hears = self.hears().join(" · ")
            )),
            Some(_) => bisa_i18n::say(&bisa_core::text!(
                "cli-workflow-on",
                hears = self.hears().join(" · ")
            )),
        }
    }
}

fn line(row: &Row) -> String {
    let w = &row.workflow;
    let problems = row.problems.len();
    bisa_i18n::say(&bisa_core::text!(
        "cli-workflow-step-s-rev",
        a0 = (w.id).to_string(),
        a1 = format!("{:<28}", w.name.chars().take(28).collect::<String>()),
        a2 = format!("{:>2}", w.steps.len()),
        a3 = format!("{:<3}", w.revision),
        a4 = format!("{:<22}", origin_label(&w.origin)),
        a5 = (crate::tags::label(&w.tags)).to_string(),
        a6 = (if problems == 0 {
            String::new()
        } else {
            format!(" ⚠ {problems} problem(s)")
        })
        .to_string(),
        on = row.on_words()
    ))
}

pub fn render_problems(out: &Out, problems: &[Problem]) {
    if problems.is_empty() {
        out.say(&bisa_core::text!("cli-workflow-no-problems"));
        return;
    }
    out.human(&format!("{} problem(s):", problems.len()));
    for p in problems {
        let step = p
            .step
            .as_ref()
            .map(|s| bisa_i18n::say(&bisa_core::text!("cli-workflow-step", s = s.to_string())))
            .unwrap_or_default();
        out.human(&format!("  {step}{:?} — {}", p.kind, out.text(&p.text)));
    }
}

/// How a workflow begins and whether it listens, in lines: one per start,
/// then its standing while it is On — or, while it is Off and has a start on
/// an event, the verb that turns it On and what that asks.
fn listening_lines(row: &Row) -> Vec<String> {
    if row.starts.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![bisa_i18n::say(&bisa_core::text!("cli-workflow-starts"))];
    for start in &row.starts {
        // The start as its row says it, in the line every listing shares.
        let said = serde_json::to_value(start).unwrap_or_default();
        lines.push(format!("    {}", listening::start_line(&said)));
    }
    match &row.listening {
        Some(_) => lines.extend(
            listening::standing_lines(row.listening.as_ref())
                .into_iter()
                .map(|line| format!("  {line}")),
        ),
        None if row.hears().is_empty() => {}
        None if row.listening_needs.is_empty() => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-workflow-off",
            id = row.workflow.id.to_string()
        ))),
        None => lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-workflow-off-needs",
            id = row.workflow.id.to_string(),
            needs = row.listening_needs.join(", ")
        ))),
    }
    lines
}

fn render_workflow(out: &Out, row: &Row, used_by: &Value) {
    let (w, problems) = (&row.workflow, &row.problems);
    out.say(&bisa_core::text!(
        "cli-workflow-name-description-origin-revision-tags",
        a0 = (w.id).to_string(),
        a1 = (w.name).to_string(),
        a2 = (w.description).to_string(),
        a3 = (origin_label(&w.origin)).to_string(),
        a4 = (w.revision).to_string(),
        a5 = (crate::tags::label(&w.tags)).to_string()
    ));
    if !w.inputs.is_empty() {
        out.say(&bisa_core::text!("cli-workflow-inputs"));
        for i in &w.inputs {
            out.human(&format!(
                "    {:<20} {:?}{}",
                i.name,
                i.kind,
                if i.required { "  (required)" } else { "" }
            ));
        }
    }
    out.say(&bisa_core::text!("cli-workflow-steps"));
    for s in &w.steps {
        let then: Vec<String> = s
            .then
            .iter()
            .map(|f| match &f.branch {
                Some(b) => format!("{}={}", b, f.to),
                None => f.to.to_string(),
            })
            .collect();
        out.human(&format!(
            "    {:<20} {:<9} {}{}",
            s.id,
            crate::activity::step_kind_label(&s.kind),
            s.name,
            if then.is_empty() {
                String::new()
            } else {
                format!("  → {}", then.join(", "))
            }
        ));
    }
    for line in listening_lines(row) {
        out.human(&line);
    }
    if let Some(refs) = used_by.as_array().filter(|r| !r.is_empty()) {
        out.say(&bisa_core::text!(
            "cli-workflow-used-holder-s",
            a0 = (refs.len()).to_string()
        ));
    }
    render_problems(out, problems);
    out.json_value(json!({
        "workflow": w,
        "problems": problems,
        "used_by": used_by,
        "listening": row.listening,
        "starts": row.starts,
        "event_only": row.event_only,
        "listening_needs": row.listening_needs,
    }));
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn list(
    ctx: &Ctx,
    out: &Out,
    templates: bool,
    goal: Option<&str>,
    all: bool,
    filter: &TagFilterArgs,
) -> Result<()> {
    let ws = ctx.workspace()?;
    if templates {
        let entries = ws.catalog_entries(Some(CatalogKind::Workflow))?;
        for e in &entries {
            out.human(&format!(
                "{} {:<26} {}",
                if e.installed { "✓" } else { "·" },
                e.slug,
                e.description
            ));
        }
        if entries.is_empty() {
            out.say(&bisa_core::text!(
                "cli-workflow-catalog-has-no-workflow-templates"
            ));
        }
        out.json_value(json!({"templates": entries}));
        return Ok(());
    }
    let scope = match (goal, all) {
        (Some(raw), _) => bisa_store::WorkflowScope::Goal(crate::parse_goal_id(raw)?),
        (None, true) => bisa_store::WorkflowScope::All,
        (None, false) => bisa_store::WorkflowScope::Library,
    };
    let admitted = filter.admitted(&ws, TagEntity::Workflow)?;
    let mut listed = Vec::new();
    for w in ws.list_workflows_in(scope)? {
        if !keeps(&admitted, &w.id.to_string()) {
            continue;
        }
        let problems = ws.validate_workflow(&w).unwrap_or_default();
        listed.push(Row::of(&ws, w, problems)?);
    }
    for row in &listed {
        out.human(&line(row));
    }
    if listed.is_empty() {
        out.say(&bisa_core::text!(
            "cli-workflow-no-workflows-yet-bisa-workflow-list"
        ));
    }
    out.json_value(json!({ "workflows": listed }));
    Ok(())
}

async fn show(ctx: &Ctx, out: &Out, raw: &str) -> Result<()> {
    let ws = ctx.workspace()?;
    // An uninstalled template answers as the catalog reads it, under a fresh
    // id nothing holds — a look, not an install.
    let (w, installed) = match resolve_workflow(&ws, raw, false) {
        Ok(w) => (w, true),
        Err(e) => match ws.catalog_workflow(raw.trim())? {
            Some(draft) => (draft_as_workflow(&ws, draft), false),
            None => return Err(e),
        },
    };
    let problems = ws.validate_workflow(&w)?;
    let used_by = if installed {
        let usage = ws.usage_of(bisa_store::UsageKind::Workflow, &w.id.to_string())?;
        json!(usage.as_slice())
    } else {
        out.say(&bisa_core::text!(
            "cli-workflow-catalog-template-not-installed"
        ));
        json!([])
    };
    render_workflow(out, &Row::of(&ws, w, problems)?, &used_by);
    Ok(())
}

/// What a save answers: the workflow and its problems, rendered the same way
/// whether a daemon or the store did the saving.
fn report_saved(out: &Out, verb: &str, w: &Workflow, problems: &[Problem]) {
    out.say(&bisa_core::text!(
        "cli-workflow-workflow-step-revision",
        a0 = (w.id).to_string(),
        verb = verb.to_string(),
        a1 = (w.steps.len()).to_string(),
        a2 = (if w.steps.len() == 1 { "" } else { "s" }).to_string(),
        a3 = (w.revision).to_string()
    ));
    render_problems(out, problems);
    out.json_value(json!({"workflow": w, "problems": problems}));
}

fn saved_payload(v: &Value) -> Result<(Workflow, Vec<Problem>)> {
    let w: Workflow = serde_json::from_value(v["workflow"].clone()).context(bisa_core::text!(
        "cli-workflow-unexpected-workflow-payload-from-node"
    ))?;
    let problems: Vec<Problem> = serde_json::from_value(v["problems"].clone()).unwrap_or_default();
    Ok((w, problems))
}

async fn new(ctx: &Ctx, out: &Out, from: &str) -> Result<()> {
    let ws = ctx.workspace()?;
    let draft = read_definition(&ws, from)?;
    if let Some(client) = ctx.node_client().await {
        let v = client
            .post("/workflows", serde_json::to_value(&draft)?)
            .await?;
        let (w, problems) = saved_payload(&v)?;
        report_saved(out, "recorded", &w, &problems);
        return Ok(());
    }
    let (w, problems) = ws.create_workflow_draft(draft, bisa_core::WorkflowOrigin::Workspace)?;
    report_saved(out, "recorded", &w, &problems);
    Ok(())
}

async fn edit(ctx: &Ctx, out: &Out, id: &str, from: &str, revision: Option<u64>) -> Result<()> {
    let id = WorkflowId::from_str(id).with_context(|| {
        bisa_core::text!("cli-workflow-not-workflow-id", id = format!("{id:?}"))
    })?;
    let ws = ctx.workspace()?;
    let draft = read_definition(&ws, from)?;
    let expected = match revision {
        Some(r) => r,
        None => ws.get_workflow(id)?.revision,
    };
    if let Some(client) = ctx.node_client().await {
        let mut body = serde_json::to_value(&draft)?;
        body["revision"] = json!(expected);
        let v = client.put(&format!("/workflows/{id}"), body).await?;
        let (w, problems) = saved_payload(&v)?;
        report_saved(out, "saved", &w, &problems);
        return Ok(());
    }
    let (w, problems) = ws.save_workflow_draft(id, draft, expected)?;
    report_saved(out, "saved", &w, &problems);
    Ok(())
}

async fn validate(ctx: &Ctx, out: &Out, from: &str) -> Result<()> {
    let ws = ctx.workspace()?;
    let draft = read_definition(&ws, from)?;
    let problems: Vec<Problem> = if let Some(client) = ctx.node_client().await {
        let v = client
            .post("/workflows/validate", serde_json::to_value(&draft)?)
            .await?;
        serde_json::from_value(v["problems"].clone()).context(bisa_core::text!(
            "cli-workflow-unexpected-problems-payload-from-node"
        ))?
    } else {
        let w = draft_as_workflow(&ws, draft);
        ws.validate_workflow(&w)?
    };
    render_problems(out, &problems);
    out.json_value(json!({"problems": problems, "valid": problems.is_empty()}));
    if problems.is_empty() {
        Ok(())
    } else {
        // A non-zero exit is what a script checks; the problems are already
        // on stdout.
        bail!("{} problem(s)", problems.len())
    }
}

async fn rm(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let id = WorkflowId::from_str(id).with_context(|| {
        bisa_core::text!("cli-workflow-not-workflow-id", id = format!("{id:?}"))
    })?;
    if let Some(client) = ctx.node_client().await {
        client.delete(&format!("/workflows/{id}")).await?;
    } else {
        // Through the engine, so the bus hears the deletion on every path.
        let (engine, _) = ctx.engine().await?;
        let result = engine.delete_workflow(id);
        engine.shutdown().await;
        result?;
    }
    out.say(&bisa_core::text!(
        "cli-workflow-removed-workflow",
        id = id.to_string()
    ));
    out.json_value(json!({"ok": true, "workflow": id.to_string()}));
    Ok(())
}

async fn promote(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let id: WorkflowId = id.parse().map_err(|_| {
        anyhow!(bisa_core::text!(
            "cli-workflow-not-workflow-id",
            id = format!("{id:?}")
        ))
    })?;
    // Through the node when one runs — one engine holds a workspace — and by
    // an engine of this process's own when none does.
    let copy: Workflow = if let Some(client) = ctx.node_client().await {
        let v = client
            .post(&format!("/workflows/{id}/promote"), json!({}))
            .await?;
        serde_json::from_value(v["workflow"].clone()).context(bisa_core::text!(
            "cli-workflow-unexpected-workflow-payload-from-node"
        ))?
    } else {
        let (engine, _) = ctx.engine().await?;
        let made = engine.promote_workflow(id);
        engine.shutdown().await;
        made?
    };
    out.say(&bisa_core::text!(
        "cli-workflow-promoted-into-library-as",
        a0 = (copy.id).to_string(),
        a1 = (copy.name).to_string()
    ));
    out.json_value(json!({"workflow": copy}));
    Ok(())
}

fn workflow_id(id: &str) -> Result<WorkflowId> {
    id.parse().map_err(|_| {
        anyhow!(bisa_core::text!(
            "cli-workflow-not-workflow-id",
            id = format!("{id:?}")
        ))
    })
}

/// A run of this workflow in the workspace, by its id: refused, by name,
/// when it names no run, a run of another workflow, or a goal's run.
fn run_of_workflow(ctx: &Ctx, workflow: WorkflowId, raw: &str) -> Result<bisa_core::RunId> {
    let run: bisa_core::RunId = raw.parse().map_err(|_| {
        anyhow!(bisa_core::text!(
            "cli-workflow-not-run-id",
            id = format!("{raw:?}")
        ))
    })?;
    let found = ctx.workspace()?.get_run(run)?;
    if found.workflow.id != workflow || !found.scope.is_workspace() {
        bail!(bisa_core::text!(
            "cli-workflow-run-not-workflow-s",
            run = run.to_string(),
            workflow = workflow.to_string()
        ));
    }
    Ok(run)
}

/// Run the workflow in the workspace and follow the run: started at once,
/// beside any other run of it, with no goal captured — by hand, or, when
/// `begin` names an event start, as a test run, which is said.
async fn run(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    inputs: crate::inputs::Typed,
    begin: Begin,
    watch: bool,
) -> Result<()> {
    let inputs = inputs.read_for(&ctx.workspace()?, id);
    let id = workflow_id(id)?;
    let style = crate::activity::Style::stderr();
    if let Some(client) = ctx.node_client().await {
        let v = client
            .post(&format!("/workflows/{id}/runs"), begin.body(&inputs))
            .await?;
        let run: bisa_core::RunId = serde_json::from_value(v["run"]["id"].clone()).context(
            bisa_core::text!("cli-step-unexpected-run-payload-from-node"),
        )?;
        let test = v["run"]["event"]["source"] == json!("test");
        let start = v["run"]["start"].as_str().unwrap_or_default();
        eprintln!("{}", style.bold(&started_line(id, run, test, start)));
        return crate::run::follow_run_via_node(client, out, run, watch).await;
    }
    let (engine, _) = ctx.engine().await?;
    let engine = std::sync::Arc::new(engine);
    let run = match start_embedded(&engine, id, inputs, begin) {
        Ok(run) => run,
        Err(e) => {
            if let Ok(engine) = std::sync::Arc::try_unwrap(engine) {
                engine.shutdown().await;
            }
            return Err(e);
        }
    };
    let start = run.start.as_ref().map(StepId::as_str).unwrap_or_default();
    let line = started_line(id, run.id, crate::run::is_test(&run), start);
    eprintln!("{}", style.bold(&line));
    crate::run::follow_run(engine, out, run.id, watch).await
}

/// A run of the workspace begun on an embedded engine, through the door
/// `begin` names. The workspace listens through its switch, never through a
/// run's start: a person's start and the start by hand are both a run by hand.
fn start_embedded(
    engine: &Engine,
    id: WorkflowId,
    inputs: BTreeMap<String, Value>,
    begin: Begin,
) -> Result<WorkflowRun> {
    let workflow = match &begin.start {
        Some(_) => Some(engine.workspace().get_workflow(id)?),
        None => None,
    };
    Ok(match way_in(workflow.as_ref(), begin)? {
        WayIn::Start | WayIn::ByHand => engine.start_workspace_run(id, inputs)?,
        WayIn::Test { start, payload } => engine.test_run_workflow(id, &start, payload, inputs)?,
    })
}

/// A run of the workspace that began, said: a run by hand, or — begun at a
/// start as if its event had happened — a test run.
fn started_line(workflow: WorkflowId, run: bisa_core::RunId, test: bool, start: &str) -> String {
    if test {
        return bisa_i18n::say(&bisa_core::text!(
            "cli-workflow-test-run-started-workspace",
            run = run.to_string(),
            workflow = workflow.to_string(),
            start = start.to_string()
        ));
    }
    bisa_i18n::say(&bisa_core::text!(
        "cli-workflow-run-started-workspace",
        run = run.to_string(),
        workflow = workflow.to_string()
    ))
}

/// The workflow's runs of the workspace, newest first — the node's
/// `RunSummary` shape on both paths.
async fn runs(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let id = workflow_id(id)?;
    let rows: Vec<Value> = if let Some(client) = ctx.node_client().await {
        let v = client.get(&format!("/workflows/{id}/runs")).await?;
        serde_json::from_value(v["runs"].clone()).unwrap_or_default()
    } else {
        let ws = ctx.workspace()?;
        ws.get_workflow(id)?;
        let all = ws.list_workflow_runs(id)?;
        RunSummary::list(&all, |run| bisa_node::runs::start_detail(&ws, run))
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
    };
    if rows.is_empty() {
        out.say(&bisa_core::text!(
            "cli-workflow-workflow-has-no-run-yet",
            id = id.to_string()
        ));
    }
    for r in &rows {
        let mut words = vec![
            format!("#{}", r["number"]),
            r["id"].as_str().unwrap_or("?").to_string(),
            r["status"].as_str().unwrap_or("?").to_string(),
            format!("rev {}", r["revision"]),
            StartedBy::read(r).words(),
        ];
        if let Some(at) = r["started_at"].as_u64() {
            words.push(format!("started {at}"));
        }
        if let Some(at) = r["finished_at"].as_u64() {
            words.push(format!("finished {at}"));
        }
        if let Some(cause) = r["cause"]["cause"].as_str() {
            words.push(cause.to_string());
        }
        out.human(&words.join("  "));
    }
    out.json_value(json!({"workflow": id.to_string(), "runs": rows}));
    Ok(())
}

/// Stop every run of the workspace of the workflow that is going — or the
/// one `--run` names.
async fn stop(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    run: Option<&str>,
    rationale: Option<String>,
) -> Result<()> {
    let id = workflow_id(id)?;
    let rationale = rationale.filter(|r| !r.trim().is_empty());
    let one = run.map(|r| run_of_workflow(ctx, id, r)).transpose()?;
    let runs: Vec<String> = if let Some(client) = ctx.node_client().await {
        match one {
            Some(run) => {
                client
                    .post(
                        &format!("/runs/{run}/stop"),
                        json!({"rationale": rationale}),
                    )
                    .await?;
                vec![run.to_string()]
            }
            None => {
                let v = client
                    .post(&format!("/workflows/{id}/stop"), json!({}))
                    .await?;
                serde_json::from_value(v["runs"].clone()).unwrap_or_default()
            }
        }
    } else {
        let (engine, _) = ctx.engine().await?;
        let stopped = match one {
            Some(run) => engine.stop_run(run, rationale).await.map(|r| vec![r.id]),
            None => engine.stop_workflow(id).await,
        };
        engine.shutdown().await;
        stopped?.iter().map(|r| r.to_string()).collect()
    };
    out.human(&match runs.len() {
        0 => bisa_i18n::say(&bisa_core::text!(
            "cli-workflow-workflow-no-run-going",
            id = id.to_string()
        )),
        n => bisa_i18n::say(&bisa_core::text!(
            "cli-workflow-workflow-stopped-run",
            id = id.to_string(),
            n = n.to_string(),
            a0 = (plural(n)).to_string(),
            a1 = (runs.join(", ")).to_string()
        )),
    });
    out.json_value(json!({"workflow": id.to_string(), "runs": runs}));
    Ok(())
}

/// Restart every run of the workspace of the workflow that is going — or
/// the one `--run` names, and follow the new run.
async fn restart(ctx: &Ctx, out: &Out, id: &str, run: Option<&str>, watch: bool) -> Result<()> {
    let id = workflow_id(id)?;
    if let Some(raw) = run {
        let run = run_of_workflow(ctx, id, raw)?;
        let style = crate::activity::Style::stderr();
        let restarted = |new: &str| {
            eprintln!(
                "{}",
                style.bold(&bisa_i18n::say(&bisa_core::text!(
                    "cli-run-restarted-run",
                    a0 = new.to_string()
                )))
            )
        };
        if let Some(client) = ctx.node_client().await {
            let v = client
                .post(&format!("/runs/{run}/restart"), json!({}))
                .await?;
            let new: bisa_core::RunId = serde_json::from_value(v["run"]["id"].clone()).context(
                bisa_core::text!("cli-step-unexpected-run-payload-from-node"),
            )?;
            restarted(&new.to_string());
            return crate::run::follow_run_via_node(client, out, new, watch).await;
        }
        let (engine, _) = ctx.engine().await?;
        let engine = std::sync::Arc::new(engine);
        let new = match engine.restart_run(run).await {
            Ok(new) => new,
            Err(e) => {
                if let Ok(engine) = std::sync::Arc::try_unwrap(engine) {
                    engine.shutdown().await;
                }
                return Err(e.into());
            }
        };
        restarted(&new.id.to_string());
        return crate::run::follow_run(engine, out, new.id, watch).await;
    }
    let runs: Vec<String> = if let Some(client) = ctx.node_client().await {
        let v = client
            .post(&format!("/workflows/{id}/restart"), json!({}))
            .await?;
        serde_json::from_value(v["runs"].clone()).unwrap_or_default()
    } else {
        let (engine, _) = ctx.engine().await?;
        let restarted = engine.restart_workflow(id).await;
        engine.shutdown().await;
        restarted?.iter().map(|r| r.to_string()).collect()
    };
    out.human(&match runs.len() {
        0 => bisa_i18n::say(&bisa_core::text!(
            "cli-workflow-workflow-no-run-going",
            id = id.to_string()
        )),
        n => bisa_i18n::say(&bisa_core::text!(
            "cli-workflow-workflow-restarted-run",
            id = id.to_string(),
            n = n.to_string(),
            a0 = (plural(n)).to_string(),
            a1 = (runs.join(", ")).to_string()
        )),
    });
    out.json_value(json!({"workflow": id.to_string(), "runs": runs}));
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

async fn use_workflow(
    ctx: &Ctx,
    out: &Out,
    goal: &str,
    raw: &str,
    from: Option<&str>,
) -> Result<()> {
    let goal = crate::parse_goal_id(goal)?;
    if let Some(src) = from {
        // A definition of the goal's own: recorded as its design — a draft,
        // kept with its problems — and pointed at; no adoption gate for your
        // own hand.
        let ws = ctx.workspace()?;
        let draft = read_definition(&ws, src)?;
        let revision = design_revision(&ws, goal)?;
        if let Some(client) = ctx.node_client().await {
            let v = client
                .put(
                    &format!("/goals/{goal}/workflow"),
                    json!({"definition": draft, "revision": revision}),
                )
                .await?;
            out.say(&bisa_core::text!(
                "cli-workflow-goal-will-run-own-design",
                goal = goal.to_string(),
                a0 = (v["workflow"]["id"].as_str().unwrap_or("?")).to_string()
            ));
            let problems: Vec<Problem> =
                serde_json::from_value(v["problems"].clone()).unwrap_or_default();
            render_problems(out, &problems);
            out.json_value(v);
            return Ok(());
        }
        let (engine, _) = ctx.engine().await?;
        let made = engine.design_workflow(goal, draft, revision);
        engine.shutdown().await;
        let (wf, problems) = made?;
        out.say(&bisa_core::text!(
            "cli-workflow-goal-will-run-own-design-start",
            goal = goal.to_string(),
            a0 = (wf.id).to_string(),
            a1 = (wf.name).to_string()
        ));
        render_problems(out, &problems);
        out.json_value(json!({"goal": goal.to_string(), "workflow": wf}));
        return Ok(());
    }
    if raw.is_empty() {
        return Err(anyhow!(bisa_core::text!(
            "cli-workflow-name-workflow-id-slug-pass-from"
        )));
    }
    if let Some(client) = ctx.node_client().await {
        let v = client
            .put(&format!("/goals/{goal}/workflow"), json!({"workflow": raw}))
            .await?;
        out.say(&bisa_core::text!(
            "cli-workflow-goal-will-run-workflow",
            goal = goal.to_string(),
            a0 = (v["goal"]["workflow"].as_str().unwrap_or(raw)).to_string()
        ));
        out.json_value(json!({"goal": goal.to_string(), "workflow": v["goal"]["workflow"]}));
        return Ok(());
    }
    let ws = ctx.workspace()?;
    let w = resolve_workflow(&ws, raw, true)?;
    let (engine, _) = ctx.engine().await?;
    let set = engine.set_workflow(goal, Some(w.id));
    engine.shutdown().await;
    let g = set?;
    out.say(&bisa_core::text!(
        "cli-workflow-goal-will-run-workflow-start-with",
        goal = goal.to_string(),
        a0 = (w.id).to_string(),
        a1 = (w.name).to_string()
    ));
    out.json_value(json!({"goal": goal.to_string(), "workflow": g.workflow}));
    Ok(())
}

// ---------------------------------------------------------------------------
// On and Off, listeners, hook secrets
// ---------------------------------------------------------------------------

/// The row of the workflow `id` as the node answers a toggle with it
/// (`WorkflowRow`) — built by the node's own reader, so a toggle answers one
/// shape with or without a daemon.
fn row_of(ws: &Workspace, id: WorkflowId) -> Result<bisa_node::dto::WorkflowRow> {
    Ok(bisa_node::workflows::row_of(ws, ws.get_workflow(id)?)?)
}

/// Turn the workflow On, listening with `inputs` under `budget`, and print
/// what it listens for — and each public hook's secret, once. Whether it may
/// listen is the engine's judgement; a refusal is said in its words.
async fn on(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    inputs: crate::inputs::Typed,
    budget: Option<Budget>,
) -> Result<()> {
    let inputs = inputs.read_for(&ctx.workspace()?, id);
    let id = workflow_id(id)?;
    let client = ctx.node_client().await;
    let heard = client.is_some();
    let answer = match client {
        Some(client) => {
            let mut body = json!({ "inputs": inputs });
            if let Some(budget) = &budget {
                body["budget"] = serde_json::to_value(budget)?;
            }
            client
                .put(&format!("/workflows/{id}/listening"), body)
                .await?
        }
        None => {
            let engine = ctx.quiet_engine().await?;
            let answer = turned_on(&engine, id, inputs, budget);
            engine.shutdown().await;
            answer?
        }
    };
    out.human(&on_lines(id, &answer, heard).join("\n"));
    out.json_value(answer);
    Ok(())
}

/// What the node answers a turn-on with — `{workflow, listeners, secrets}` —
/// from an embedded engine.
fn turned_on(
    engine: &Engine,
    id: WorkflowId,
    inputs: BTreeMap<String, Value>,
    budget: Option<Budget>,
) -> Result<Value> {
    let host = ListenerHost::Workspace { workflow: id };
    let turned = engine.set_listening(host, inputs, budget)?;
    let row = row_of(engine.workspace(), id)?;
    let listeners = listening::listeners_of(engine, &host)?;
    Ok(json!({
        "workflow": row,
        "listeners": listeners,
        "secrets": turned.secrets,
    }))
}

/// A turn-on, in lines: where the workflow stands, what it listens for, and
/// the secrets the turn minted. `heard` is whether a node is there to hear
/// its events; without one they are heard once one runs.
fn on_lines(id: WorkflowId, answer: &Value, heard: bool) -> Vec<String> {
    let standing =
        serde_json::from_value::<Option<Listening>>(answer["workflow"]["listening"].clone())
            .ok()
            .flatten();
    let mut lines = vec![bisa_i18n::say(&bisa_core::text!(
        "cli-workflow-turned-on",
        id = id.to_string()
    ))];
    lines.extend(
        listening::standing_lines(standing.as_ref())
            .into_iter()
            .map(|line| format!("  {line}")),
    );
    lines.extend(listening::listener_lines(rows(&answer["listeners"])));
    lines.extend(listening::secret_lines(
        HookHost::Workflow,
        &id.to_string(),
        rows(&answer["secrets"]),
    ));
    if !heard {
        lines.push(bisa_i18n::say(&bisa_core::text!("cli-listening-no-node")));
    }
    lines
}

/// Turn the workflow Off: its start events are no longer heard. Off already
/// is nothing to do.
async fn off(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let id = workflow_id(id)?;
    let answer = match ctx.node_client().await {
        Some(client) => client.delete(&format!("/workflows/{id}/listening")).await?,
        None => {
            let engine = ctx.quiet_engine().await?;
            let answer = turned_off(&engine, id);
            engine.shutdown().await;
            answer?
        }
    };
    out.say(&bisa_core::text!(
        "cli-workflow-turned-off",
        id = id.to_string()
    ));
    out.json_value(answer);
    Ok(())
}

/// What the node answers a turn-off with — `{workflow}` — from an embedded
/// engine. A workflow nobody has is refused in the store's words.
fn turned_off(engine: &Engine, id: WorkflowId) -> Result<Value> {
    engine.workspace().get_workflow(id)?;
    engine.stop_listening(ListenerHost::Workspace { workflow: id })?;
    let row = row_of(engine.workspace(), id)?;
    Ok(json!({ "workflow": row }))
}

/// The listeners of one workflow, of one goal, or — with neither named — of
/// every host that listens.
async fn listeners(ctx: &Ctx, out: &Out, id: Option<&str>, goal: Option<&str>) -> Result<()> {
    let host = match (id, goal) {
        (Some(id), _) => Some(ListenerHost::Workspace {
            workflow: workflow_id(id)?,
        }),
        (None, Some(goal)) => Some(ListenerHost::Goal {
            goal: crate::parse_goal_id(goal)?,
        }),
        (None, None) => None,
    };
    let views = listening::fetch(ctx, host.as_ref()).await?;
    if views.is_empty() {
        out.say(&bisa_core::text!("cli-listening-none"));
    }
    for line in listening::listener_lines(&views) {
        out.human(&line);
    }
    out.json_value(json!({ "listeners": views }));
    Ok(())
}

/// The host a `hook-secret` names: a library workflow, or a goal.
fn host_of(host: HookHost, id: &str) -> Result<ListenerHost> {
    Ok(match host {
        HookHost::Workflow => ListenerHost::Workspace {
            workflow: workflow_id(id)?,
        },
        HookHost::Goal => ListenerHost::Goal {
            goal: crate::parse_goal_id(id)?,
        },
    })
}

/// A hook start's standing — or, with `rotate`, its secret minted anew and
/// shown this once. No read ever shows a secret.
async fn hook_secret(
    ctx: &Ctx,
    out: &Out,
    host: HookHost,
    id: &str,
    step: &str,
    rotate: bool,
) -> Result<()> {
    let key = ListenerKey {
        host: host_of(host, id)?,
        step: StepId::new(step).with_context(|| {
            bisa_core::text!("cli-step-not-step-id", step = format!("{step:?}"))
        })?,
    };
    if rotate {
        let secret = rotated(ctx, host, &key).await?;
        let lines = listening::secret_lines(host, &key.host.id(), std::slice::from_ref(&secret));
        out.human(&lines.join("\n"));
        out.json_value(secret);
        return Ok(());
    }
    let views = listening::fetch(ctx, Some(&key.host)).await?;
    let hook = views
        .iter()
        .find(|view| view["step"].as_str() == Some(key.step.as_str()))
        .filter(|view| !view["local_hook"].is_null());
    let Some(view) = hook else {
        bail!(bisa_core::text!(
            "cli-workflow-no-hook-listener",
            step = key.step.to_string(),
            host = key.host.to_string()
        ));
    };
    out.human(&hook_lines(view).join("\n"));
    out.json_value(json!({ "listener": view }));
    Ok(())
}

/// The hook's secret minted anew (`HookSecret`): by the node, or by an
/// embedded engine that hears nothing.
async fn rotated(ctx: &Ctx, host: HookHost, key: &ListenerKey) -> Result<Value> {
    if let Some(client) = ctx.node_client().await {
        let route = format!("{}/hooks/{}/secret", host.route(&key.host.id()), key.step);
        return client.post(&route, json!({})).await;
    }
    let engine = ctx.quiet_engine().await?;
    let secret = engine.rotate_hook_secret(key);
    engine.shutdown().await;
    Ok(serde_json::to_value(secret?)?)
}

/// A hook start's standing, in lines: its listener, how it is called — and
/// when its secret is shown, or that it has none.
fn hook_lines(view: &Value) -> Vec<String> {
    let mut lines = listening::listener_block(view);
    lines.push(if view["public_hook"].is_null() {
        bisa_i18n::say(&bisa_core::text!("cli-workflow-hook-not-public"))
    } else {
        bisa_i18n::say(&bisa_core::text!("cli-workflow-hook-secret-shown-once"))
    });
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line the catalog has: a miss renders as the message's id.
    fn sentence(line: &str) {
        assert!(
            !line.is_empty() && !line.contains("cli-workflow") && !line.contains("cli-listening"),
            "a line the catalog does not say: {line:?}"
        );
    }

    #[test]
    fn a_budget_is_none_the_workspace_s_or_the_flags() {
        assert_eq!(BudgetArgs::default().budget(), None);
        let tokens = BudgetArgs {
            budget_tokens: Some(10_000),
            ..BudgetArgs::default()
        };
        assert_eq!(
            tokens.budget(),
            Some(Budget {
                max_tokens: Some(10_000),
                ..Budget::default()
            })
        );
        // A zero lifts the ceiling: alone, it is a listening with none at all.
        let lifted = BudgetArgs {
            budget_usd_cents: Some(0),
            ..BudgetArgs::default()
        };
        assert_eq!(lifted.budget(), Some(Budget::default()));
        assert_eq!(
            serde_json::to_value(lifted.budget()).ok(),
            Some(json!({})),
            "`{{}}` is no ceiling, whatever the default says"
        );
    }

    fn turned_on_answer() -> Value {
        json!({
            "workflow": {
                "workflow": {"id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"},
                "listening": {"inputs": {"who": "the team"}, "since": 7},
            },
            "listeners": [{
                "listener": "workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket",
                "host": "workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV",
                "step": "ticket",
                "event": "hook",
                "summary": {"id": "step-summary-start-hook-public"},
                "backlog": 0,
                "live_runs": 0,
                "local_hook": "/workflows/01ARZ3NDEKTSV4RRFFQ69G5FAV/hooks/ticket",
                "public_hook": {
                    "path": "/hooks/workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket",
                    "has_secret": true
                },
            }],
            "secrets": [{
                "step": "ticket",
                "path": "/hooks/workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket",
                "secret": "ab".repeat(32),
            }],
        })
    }

    #[test]
    fn a_turn_on_says_what_it_listens_for_and_its_secrets_once() {
        let id: WorkflowId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().expect("a workflow id");
        let lines = on_lines(id, &turned_on_answer(), true);
        lines.iter().for_each(|line| sentence(line));
        let page = lines.join("\n");
        for word in [
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "who=the team",
            "begins when called from outside",
            "/workflows/01ARZ3NDEKTSV4RRFFQ69G5FAV/hooks/ticket",
            &"ab".repeat(32),
            "will not be shown again",
        ] {
            assert!(page.contains(word), "{word}: {page}");
        }
        assert_eq!(
            page.matches(&"ab".repeat(32)).count(),
            2,
            "the secret, and the header it is sent as: {page}"
        );
        // With no node to hear them, its events are heard once one runs.
        let unheard = on_lines(id, &turned_on_answer(), false);
        assert_eq!(unheard.len(), lines.len() + 1);
        assert!(
            unheard.last().is_some_and(|l| l.contains("bisa node")),
            "{unheard:?}"
        );
    }

    #[test]
    fn a_hook_s_standing_never_shows_a_secret() {
        let answer = turned_on_answer();
        let public = hook_lines(&answer["listeners"][0]);
        public.iter().for_each(|line| sentence(line));
        let page = public.join("\n");
        assert!(page.contains("minted or rotated"), "{page}");
        assert!(!page.contains(&"ab".repeat(32)), "{page}");

        let mut local = answer["listeners"][0].clone();
        local["public_hook"] = Value::Null;
        let page = hook_lines(&local).join("\n");
        assert!(page.contains("not public"), "{page}");
    }

    #[test]
    fn a_host_is_a_workflow_or_a_goal_by_its_id() {
        let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        assert_eq!(
            host_of(HookHost::Workflow, id).ok().map(|h| h.to_string()),
            Some(format!("workspace:{id}"))
        );
        assert_eq!(
            host_of(HookHost::Goal, id).ok().map(|h| h.to_string()),
            Some(format!("goal:{id}"))
        );
        assert!(host_of(HookHost::Workflow, "not-an-id").is_err());
        assert!(host_of(HookHost::Goal, "not-an-id").is_err());
    }

    #[test]
    fn a_run_of_the_workspace_says_when_it_is_a_test() {
        let workflow: WorkflowId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().expect("a workflow id");
        let run = bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(4, 1));
        let by_hand = started_line(workflow, run, false, "");
        let test = started_line(workflow, run, true, "ticket");
        sentence(&by_hand);
        sentence(&test);
        assert!(!by_hand.contains("test"), "{by_hand}");
        assert!(
            test.contains("test run") && test.contains("ticket") && test.contains(&run.to_string()),
            "{test}"
        );
    }
}
