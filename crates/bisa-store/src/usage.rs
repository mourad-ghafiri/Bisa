//! What points at an object, asked before anything is deleted.
//!
//! The rule: **nothing is deleted while something points at it, and the
//! refusal names what.** Detaching on the owner's behalf is a data loss they
//! cannot see. Detach first, then delete — two deliberate steps beat one that
//! silently does both.
//!
//! [`Workspace::usage_of`] is readable on its own, so "why can I not delete
//! this" has an answer before the delete is attempted.
//!
//! **Nothing here reaches for the engine.** A live session exists *because* a
//! work item is unsettled with `spec.agent` set, so the durable check covers
//! "this agent is working on something".

use crate::error::StoreError;
use crate::governance::GatePolicy;
use crate::workspace::Workspace;
use bisa_core::{
    AgentId, Assignee, BoundaryAct, BoundaryOn, Gate, StartOn, Step, StepKind, ValueRef, WaitFor,
    Workflow, WorkflowId,
};
use serde::{Deserialize, Serialize};

const NAMED_IN_REFUSAL: usize = 2;
const LABEL_CHARS: usize = 60;

/// One place an object is referenced, named the way a person would say it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub id: String,
    pub label: String,
    /// The reference is in motion: a goal whose unfinished run is of this
    /// workflow. A designer reads it to go read-only while the run goes.
    pub live: bool,
}

impl Reference {
    fn new(kind: ReferenceKind, id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            label: label.into(),
            live: false,
        }
    }

    fn live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }

    fn render(&self) -> String {
        if self.label.contains(char::is_whitespace) {
            format!("{} {:?}", self.kind.noun(), self.label)
        } else {
            format!("{} {}", self.kind.noun(), self.label)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Agent,
    Team,
    Channel,
    Goal,
    Project,
    WorkItem,
    Workflow,
    Governance,
    /// A connector account on this machine.
    Account,
}

impl ReferenceKind {
    pub const ALL: &'static [ReferenceKind] = &[
        ReferenceKind::Agent,
        ReferenceKind::Team,
        ReferenceKind::Channel,
        ReferenceKind::Goal,
        ReferenceKind::Project,
        ReferenceKind::WorkItem,
        ReferenceKind::Workflow,
        ReferenceKind::Governance,
        ReferenceKind::Account,
    ];

    pub fn noun(self) -> &'static str {
        match self {
            ReferenceKind::Agent => "agent",
            ReferenceKind::Team => "team",
            ReferenceKind::Channel => "channel",
            ReferenceKind::Goal => "goal",
            ReferenceKind::Project => "project",
            ReferenceKind::WorkItem => "work item",
            ReferenceKind::Workflow => "workflow",
            ReferenceKind::Governance => "gate policy",
            ReferenceKind::Account => "account",
        }
    }

    pub fn remedy(self) -> &'static str {
        match self {
            ReferenceKind::Agent => "Detach it from the agents that carry it first.",
            ReferenceKind::Team => "Remove it from the team first.",
            ReferenceKind::Channel => "Take it off the channel's roster first.",
            ReferenceKind::Goal => {
                "Unassign it from the goal, or point the goal at another workflow, first."
            }
            ReferenceKind::Project => "Unassign it from the project first.",
            ReferenceKind::WorkItem => "Let that work item settle, or cancel it, first.",
            ReferenceKind::Workflow => "Point the workflow's step or start elsewhere first.",
            ReferenceKind::Governance => "Take it out of the gate policy first.",
            ReferenceKind::Account => "Forget the account first.",
        }
    }
}

impl std::fmt::Display for ReferenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.noun())
    }
}

/// Everything pointing at one object. Empty means it can be removed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Usage(Vec<Reference>);

impl Usage {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn as_slice(&self) -> &[Reference] {
        &self.0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Reference> {
        self.0.iter()
    }

    /// The references as one phrase: `team Engineering, goal "Ship the
    /// landing page" and 3 others`.
    pub fn describe(&self) -> String {
        let named: Vec<String> = self
            .0
            .iter()
            .take(NAMED_IN_REFUSAL)
            .map(Reference::render)
            .collect();
        let rest = self.0.len() - named.len();
        match (named.as_slice(), rest) {
            ([], _) => "nothing".to_string(),
            ([one], 0) => one.clone(),
            (all, 0) => all.join(" and "),
            (all, 1) => format!("{} and 1 other", all.join(", ")),
            (all, n) => format!("{} and {n} others", all.join(", ")),
        }
    }
}

impl<'a> IntoIterator for &'a Usage {
    type Item = &'a Reference;
    type IntoIter = std::slice::Iter<'a, Reference>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for Usage {
    type Item = Reference;
    type IntoIter = std::vec::IntoIter<Reference>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// The kinds of object a delete has to ask about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    Agent,
    Team,
    Skill,
    Mcp,
    Channel,
    Workflow,
    Connector,
}

impl UsageKind {
    pub const ALL: &'static [UsageKind] = &[
        UsageKind::Agent,
        UsageKind::Team,
        UsageKind::Skill,
        UsageKind::Mcp,
        UsageKind::Channel,
        UsageKind::Workflow,
        UsageKind::Connector,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UsageKind::Agent => "agent",
            UsageKind::Team => "team",
            UsageKind::Skill => "skill",
            UsageKind::Mcp => "mcp",
            UsageKind::Channel => "channel",
            UsageKind::Workflow => "workflow",
            UsageKind::Connector => "connector",
        }
    }
}

impl std::fmt::Display for UsageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for UsageKind {
    type Err = StoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        UsageKind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-unknown-usage-kind",
                    s = format!("{s:?}")
                ))
            })
    }
}

fn one_line(text: &str, fallback: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if line.is_empty() {
        return fallback.to_string();
    }
    if line.chars().count() <= LABEL_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(LABEL_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Does a step of this workflow name `who` by a fixed reference — its kind,
/// a start's or a wait's message filter, a boundary event? An input
/// reference names nobody until a run starts. Exhaustive on purpose.
fn workflow_names(wf: &Workflow, who: &Assignee) -> bool {
    wf.steps.iter().any(|s| {
        s.assignee_refs()
            .iter()
            .any(|r| matches!(r, ValueRef::Fixed(a) if a == who))
    })
}

/// The conversations a step names by a fixed id: where a `notify` step or a
/// boundary posts, where a message start, wait or boundary listens. A
/// template names none until a run renders it.
fn step_conversations(step: &Step) -> Vec<&str> {
    let mut out = Vec::new();
    match &step.kind {
        StepKind::Notify {
            scope: Some(target),
            ..
        } => out.push(target.as_str()),
        StepKind::Start {
            on: StartOn::Message { filter },
            ..
        }
        | StepKind::Wait {
            until: WaitFor::Message { filter },
        } => out.extend(filter.r#in.as_deref()),
        _ => {}
    }
    for b in &step.boundaries {
        if let BoundaryOn::Message { filter } = &b.on {
            out.extend(filter.r#in.as_deref());
        }
        if let BoundaryAct::Notify {
            scope: Some(target),
            ..
        } = &b.act
        {
            out.push(target.as_str());
        }
    }
    out
}

/// Does a step of this workflow post into, or listen in, `scope`?
fn workflow_uses_conversation(wf: &Workflow, scope: &str) -> bool {
    wf.steps
        .iter()
        .any(|s| step_conversations(s).contains(&scope))
}

/// Does a `spawn` step of this workflow start `target`?
fn workflow_spawns(wf: &Workflow, target: WorkflowId) -> bool {
    wf.steps
        .iter()
        .any(|s| matches!(&s.kind, StepKind::Spawn { workflow: Some(w), .. } if *w == target))
}

/// Does a start or a wait of this workflow hear the runs of `target` end?
fn workflow_hears_runs_of(wf: &Workflow, target: WorkflowId) -> bool {
    wf.steps.iter().any(|s| match &s.kind {
        StepKind::Start {
            on: StartOn::Run { filter },
            ..
        }
        | StepKind::Wait {
            until: WaitFor::Run { filter },
        } => filter.workflow == Some(target),
        _ => false,
    })
}

/// Does a `connector` step or a connector start of this workflow call
/// `connector`? One whose connector is not chosen yet calls nothing.
fn workflow_calls(wf: &Workflow, connector: &str) -> bool {
    wf.steps.iter().any(|s| match &s.kind {
        StepKind::Connector {
            connector: Some(c), ..
        }
        | StepKind::Start {
            on: StartOn::Connector {
                connector: Some(c), ..
            },
            ..
        } => c.as_str() == connector,
        _ => false,
    })
}

impl Workspace {
    /// Everything pointing at one object. An id nothing knows about is an
    /// error rather than an empty answer.
    pub fn usage_of(&self, kind: UsageKind, id: &str) -> Result<Usage, StoreError> {
        let mut refs = Vec::new();
        match kind {
            UsageKind::Skill => {
                let sid = bisa_core::SkillId::new(id)?;
                self.get_skill(&sid)?;
                self.push_carrying_agents(&mut refs, |a| a.skills.contains(&sid))?;
            }
            UsageKind::Mcp => {
                let mid = bisa_core::McpId::new(id)?;
                self.get_mcp(&mid)?;
                self.push_carrying_agents(&mut refs, |a| a.mcps.contains(&mid))?;
            }
            UsageKind::Agent => {
                let aid = AgentId::new(id)?;
                let def = self.get_agent(&aid)?;
                let who = Assignee::Agent(id.to_string());
                for t in self.list_teams()? {
                    if t.members.iter().any(|m| m.as_agent() == Some(id)) {
                        refs.push(Reference::new(ReferenceKind::Team, t.id.as_str(), &t.name));
                    }
                }
                let channels = self.idx().channels_rostering("agent", id)?;
                for c in channels {
                    let name = bisa_core::ChannelId::new(&c)
                        .ok()
                        .and_then(|cid| self.get_channel(&cid).ok())
                        .map(|ch| ch.name)
                        .unwrap_or_else(|| c.clone());
                    refs.push(Reference::new(ReferenceKind::Channel, &c, name));
                }
                self.push_assignments(&mut refs, &who)?;
                self.push_work_items(&mut refs, &who)?;
                self.push_workflows(&mut refs, &who)?;
                self.push_gate_policies(&mut refs, def.pubkey.as_hex())?;
            }
            UsageKind::Team => {
                let tid = bisa_core::TeamId::new(id)?;
                self.get_team(&tid)?;
                let who = Assignee::Team(id.to_string());
                let channels = self.idx().channels_rostering("team", id)?;
                for c in channels {
                    let name = bisa_core::ChannelId::new(&c)
                        .ok()
                        .and_then(|cid| self.get_channel(&cid).ok())
                        .map(|ch| ch.name)
                        .unwrap_or_else(|| c.clone());
                    refs.push(Reference::new(ReferenceKind::Channel, &c, name));
                }
                self.push_assignments(&mut refs, &who)?;
                self.push_work_items(&mut refs, &who)?;
                self.push_workflows(&mut refs, &who)?;
                self.push_gate_policies(&mut refs, &format!("team:{id}"))?;
            }
            UsageKind::Connector => {
                let cid = bisa_core::ConnectorId::new(id)?;
                self.get_connector(&cid)?;
                for account in self.list_connector_accounts(&cid)? {
                    refs.push(Reference::new(
                        ReferenceKind::Account,
                        account.id.to_string(),
                        &account.label,
                    ));
                }
                for wf in self.list_workflows()? {
                    if workflow_calls(&wf, id) {
                        refs.push(Reference::new(
                            ReferenceKind::Workflow,
                            wf.id.to_string(),
                            &wf.name,
                        ));
                    }
                }
            }
            UsageKind::Channel => {
                let cid = bisa_core::ChannelId::new(id)?;
                self.get_channel(&cid)?;
                for wf in self.list_workflows()? {
                    if workflow_uses_conversation(&wf, id) {
                        refs.push(Reference::new(
                            ReferenceKind::Workflow,
                            wf.id.to_string(),
                            &wf.name,
                        ));
                    }
                }
            }
            UsageKind::Workflow => {
                let wid: WorkflowId = id
                    .parse()
                    .map_err(|_| StoreError::WorkflowNotFound(id.to_string()))?;
                self.get_workflow(wid)?;
                let goals = self.idx().goals_using_workflow(id)?;
                for goal_id in goals {
                    let goal = goal_id.parse().ok().and_then(|g| self.get_goal(g).ok());
                    let label = goal
                        .as_ref()
                        .map(|g| match &g.title {
                            Some(t) if !t.trim().is_empty() => one_line(t, &goal_id),
                            _ => one_line(&g.statement, &goal_id),
                        })
                        .unwrap_or_else(|| goal_id.clone());
                    // The goal's run of this workflow is live, or waits its
                    // turn in the goal's queue.
                    let live = goal
                        .as_ref()
                        .is_some_and(|g| self.goal_is_busy(g).unwrap_or(false));
                    refs.push(Reference::new(ReferenceKind::Goal, &goal_id, label).live(live));
                }
                for wf in self.list_workflows()? {
                    if wf.id != wid
                        && (workflow_spawns(&wf, wid) || workflow_hears_runs_of(&wf, wid))
                    {
                        refs.push(Reference::new(
                            ReferenceKind::Workflow,
                            wf.id.to_string(),
                            &wf.name,
                        ));
                    }
                }
            }
        }
        Ok(Usage(refs))
    }

    fn push_carrying_agents(
        &self,
        refs: &mut Vec<Reference>,
        held_by: impl Fn(&bisa_core::Agent) -> bool,
    ) -> Result<(), StoreError> {
        for a in self.list_agents()? {
            if held_by(&a) {
                refs.push(Reference::new(ReferenceKind::Agent, a.id.as_str(), &a.name));
            }
        }
        Ok(())
    }

    fn push_assignments(
        &self,
        refs: &mut Vec<Reference>,
        who: &Assignee,
    ) -> Result<(), StoreError> {
        for g in self.goals_for_assignee(who)? {
            let label = match &g.title {
                Some(t) if !t.trim().is_empty() => one_line(t, &g.id.to_string()),
                _ => one_line(&g.statement, &g.id.to_string()),
            };
            refs.push(Reference::new(ReferenceKind::Goal, g.id.to_string(), label));
        }
        for p in self.list_projects()? {
            if p.assignees.contains(who) {
                refs.push(Reference::new(
                    ReferenceKind::Project,
                    p.id.to_string(),
                    &p.name,
                ));
            }
        }
        Ok(())
    }

    fn push_work_items(&self, refs: &mut Vec<Reference>, who: &Assignee) -> Result<(), StoreError> {
        let rows = self.idx().work_items_naming_assignee(&who.to_string())?;
        for (item_id, filed) in rows {
            let (Some(home), Ok(item)) = (filed.home(), item_id.parse()) else {
                continue;
            };
            match self.get_work_item(&home, item) {
                Ok(spec) if spec.state.is_terminal() => {}
                Ok(spec) => refs.push(Reference::new(
                    ReferenceKind::WorkItem,
                    &item_id,
                    one_line(&spec.instructions, &item_id),
                )),
                Err(e) => tracing::warn!("work item {item_id}: unreadable, not counted: {e}"),
            }
        }
        Ok(())
    }

    fn push_workflows(&self, refs: &mut Vec<Reference>, who: &Assignee) -> Result<(), StoreError> {
        for wf in self.list_workflows()? {
            if workflow_names(&wf, who) {
                refs.push(Reference::new(
                    ReferenceKind::Workflow,
                    wf.id.to_string(),
                    &wf.name,
                ));
            }
        }
        Ok(())
    }

    fn push_gate_policies(&self, refs: &mut Vec<Reference>, entry: &str) -> Result<(), StoreError> {
        let gov = self.governance()?;
        for gate in Gate::ALL {
            if let GatePolicy::Listed(list) = gov.for_gate(gate) {
                if list.iter().any(|e| e == entry) {
                    refs.push(Reference::new(
                        ReferenceKind::Governance,
                        gate.as_str(),
                        gate.as_str(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// The refusal a `remove_*` returns, or `Ok(())` when nothing points here.
    pub(crate) fn refuse_if_used(&self, kind: UsageKind, id: &str) -> Result<(), StoreError> {
        let usage = self.usage_of(kind, id)?;
        let Some(nearest) = usage.as_slice().first() else {
            return Ok(());
        };
        Err(StoreError::StillUsed(bisa_core::text!(
            "error-store-still-used-cannot-remove-still-used",
            kind = kind.to_string(),
            id = id.to_string(),
            a0 = (usage.describe()).to_string(),
            a1 = (nearest.kind.remedy()).to_string()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(kind: ReferenceKind, label: &str) -> Reference {
        Reference::new(kind, "id", label)
    }

    #[test]
    fn a_refusal_names_the_first_few_and_counts_the_rest() {
        assert_eq!(Usage(vec![]).describe(), "nothing");
        assert_eq!(
            Usage(vec![r(ReferenceKind::Team, "Engineering")]).describe(),
            "team Engineering"
        );
        assert_eq!(
            Usage(vec![
                r(ReferenceKind::Team, "Engineering"),
                r(ReferenceKind::Goal, "Ship the landing page"),
                r(ReferenceKind::Workflow, "Bug fix"),
            ])
            .describe(),
            "team Engineering, goal \"Ship the landing page\" and 1 other",
        );
    }

    #[test]
    fn usage_kinds_round_trip_and_include_channel_and_workflow() {
        for kind in UsageKind::ALL.iter().copied() {
            assert_eq!(kind.as_str().parse::<UsageKind>().unwrap(), kind);
        }
        assert_eq!("channel".parse::<UsageKind>().unwrap(), UsageKind::Channel);
        assert_eq!(
            "workflow".parse::<UsageKind>().unwrap(),
            UsageKind::Workflow
        );
        assert!("goal".parse::<UsageKind>().is_err());
        assert_eq!(one_line("  ", "01ID"), "01ID");
        assert_eq!(one_line(&"a".repeat(200), "x").chars().count(), LABEL_CHARS);
    }

    /// A start, a wait and a boundary event hold what they name as surely
    /// as a task step does: a channel they listen in or post to, a workflow
    /// whose runs they hear, a connector they poll, an agent they hear from.
    #[test]
    fn starts_waits_and_boundaries_hold_what_they_name() {
        use crate::workflows::tests::step;
        use bisa_core::{
            Boundary, Branch, ConnectorId, MessageFilter, MessageFrom, RunFilter, Schedule,
            WorkflowOrigin,
        };
        let other = WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let start = |on: StartOn, then: &[&str]| {
            step(
                "start",
                StepKind::Start {
                    on,
                    inputs: Default::default(),
                    guard: Default::default(),
                },
                then,
            )
        };
        let workflow = |steps: Vec<Step>| Workflow {
            id: WorkflowId::from_ulid(ulid::Ulid::from_parts(2, 2)),
            name: "Listens".into(),
            description: String::new(),
            inputs: vec![],
            steps,
            origin: WorkflowOrigin::Workspace,
            author: bisa_core::PrincipalId::new("a".repeat(64)).unwrap(),
            tags: Default::default(),
            revision: 1,
            archived: None,
            decision_making: false,
            created_at: 0,
        };
        let end = step(
            "end",
            StepKind::End {
                finish: bisa_core::Finish::Path,
            },
            &[],
        );

        let hears = workflow(vec![
            start(
                StartOn::Run {
                    filter: RunFilter {
                        workflow: Some(other),
                        outcome: None,
                    },
                },
                &["end"],
            ),
            end.clone(),
        ]);
        assert!(workflow_hears_runs_of(&hears, other));
        assert!(!workflow_spawns(&hears, other));

        let listens = workflow(vec![
            start(
                StartOn::Message {
                    filter: MessageFilter {
                        r#in: Some("support".into()),
                        from: MessageFrom::Someone(ValueRef::Fixed(Assignee::Agent(
                            "triager".into(),
                        ))),
                        ..MessageFilter::default()
                    },
                },
                &["end"],
            ),
            end.clone(),
        ]);
        assert!(workflow_uses_conversation(&listens, "support"));
        assert!(!workflow_uses_conversation(&listens, "general"));
        assert!(workflow_names(&listens, &Assignee::Agent("triager".into())));

        let mut waits = step(
            "hold",
            StepKind::Wait {
                until: WaitFor::Release,
            },
            &["end"],
        );
        waits.boundaries = vec![Boundary {
            name: Branch::new("nudge").unwrap(),
            on: BoundaryOn::Every {
                secs: ValueRef::Fixed(60),
                max: 2,
            },
            act: BoundaryAct::Notify {
                scope: Some("ops".into()),
                template: "still waiting".into(),
                mentions: vec![],
                author: None,
            },
        }];
        let posts = workflow(vec![waits, end.clone()]);
        assert!(workflow_uses_conversation(&posts, "ops"));

        let polls = workflow(vec![
            start(
                StartOn::Connector {
                    connector: Some(ConnectorId::new("github").unwrap()),
                    operation: None,
                    account: None,
                    params: Default::default(),
                    key: None,
                    schedule: Schedule::default(),
                },
                &["end"],
            ),
            end,
        ]);
        assert!(workflow_calls(&polls, "github"));
        assert!(!workflow_calls(&polls, "gitlab"));
    }
}
