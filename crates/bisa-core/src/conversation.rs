//! `Conversation`: a saved exchange a person starts with one or more agents.
//!
//! A conversation is a record with an identity of its own — unlike a channel
//! or a goal's thread, which are addressed by the thing they belong to and
//! exist once per thing. It has an **origin**: what it is about, which is
//! also where its turns run and who can be reached in it. Many conversations
//! may share one origin; each is listed, searched, resumed and archived on
//! its own. Its messages are kind-3407 events under the scope
//! [`crate::message::ScopeKind::Conversation`], and its memory *is* those
//! messages — a fresh session reads the newest of them; the harness holds
//! the rest as its own context and compacts it itself.
//!
//! A conversation is never a session: a session is one live run of a harness,
//! and a conversation's turn is one kind of session, reached only through its
//! conversation.

use crate::caps::ToolTier;
use crate::id::{ConversationId, DrawingId, GoalId, NoteId, ProjectId, WorkflowId, WorkstreamId};
use serde::{Deserialize, Serialize};

/// Longest title, in characters — a line in a list, never a paragraph.
pub const MAX_TITLE_CHARS: usize = 120;

/// What a conversation is about. `Node` is this machine's own — the
/// harnesses, the setup, the settings that are true here and nowhere else;
/// `Workspace` is a real choice, not a fallback. The wire (`?origin=`) and the
/// index (`origin_kind`) share the words of [`ConversationOrigin::KINDS`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum ConversationOrigin {
    Node,
    Workspace,
    Goal {
        id: GoalId,
    },
    Workflow {
        id: WorkflowId,
    },
    Project {
        id: ProjectId,
    },
    /// A checkout of a project: the turn runs in it. The project is carried
    /// so a peer with no workstream records files the conversation under
    /// the project it does know.
    Workstream {
        id: WorkstreamId,
        project: ProjectId,
    },
    /// A drawing on the canvas (19 — Drawings): the agents draw into it with
    /// the drawing tools, and the engine chooses this drawing for a tool call
    /// that names none.
    Drawing {
        id: DrawingId,
    },
    /// A note in the notes overlay: the conversation stands beside the
    /// document, the agents read it with `note_read` and write into it only
    /// when asked, with `note_append` — and the engine chooses this note for
    /// a note tool call that names none.
    Note {
        id: NoteId,
    },
}

impl ConversationOrigin {
    /// Every kind, in the order a list draws them.
    pub const KINDS: [&'static str; 8] = [
        "node",
        "workspace",
        "goal",
        "workflow",
        "project",
        "workstream",
        "drawing",
        "note",
    ];

    pub fn kind(&self) -> &'static str {
        match self {
            ConversationOrigin::Node => "node",
            ConversationOrigin::Workspace => "workspace",
            ConversationOrigin::Goal { .. } => "goal",
            ConversationOrigin::Workflow { .. } => "workflow",
            ConversationOrigin::Project { .. } => "project",
            ConversationOrigin::Workstream { .. } => "workstream",
            ConversationOrigin::Drawing { .. } => "drawing",
            ConversationOrigin::Note { .. } => "note",
        }
    }

    /// The id of the record the origin names; none for the node and the
    /// workspace, which stand alone.
    pub fn id(&self) -> Option<String> {
        match self {
            ConversationOrigin::Node | ConversationOrigin::Workspace => None,
            ConversationOrigin::Goal { id } => Some(id.to_string()),
            ConversationOrigin::Workflow { id } => Some(id.to_string()),
            ConversationOrigin::Project { id } => Some(id.to_string()),
            ConversationOrigin::Workstream { id, .. } => Some(id.to_string()),
            ConversationOrigin::Drawing { id } => Some(id.to_string()),
            ConversationOrigin::Note { id } => Some(id.to_string()),
        }
    }

    /// Whether a kind names one record or stands alone.
    pub fn kind_takes_id(kind: &str) -> bool {
        !matches!(kind, "node" | "workspace")
    }

    /// The origin a kind, an id and — for a workstream — a project name, as
    /// the index carries them. `None` for a kind outside [`Self::KINDS`], a
    /// standalone kind with an id, a record kind without one, a workstream
    /// without its project, or an id that does not parse.
    pub fn from_parts(kind: &str, id: Option<&str>, project: Option<&str>) -> Option<Self> {
        match (kind, id) {
            ("node", None) => Some(ConversationOrigin::Node),
            ("workspace", None) => Some(ConversationOrigin::Workspace),
            ("goal", Some(id)) => id.parse().ok().map(|id| ConversationOrigin::Goal { id }),
            ("workflow", Some(id)) => id
                .parse()
                .ok()
                .map(|id| ConversationOrigin::Workflow { id }),
            ("project", Some(id)) => id.parse().ok().map(|id| ConversationOrigin::Project { id }),
            ("workstream", Some(id)) => Some(ConversationOrigin::Workstream {
                id: id.parse().ok()?,
                project: project?.parse().ok()?,
            }),
            ("drawing", Some(id)) => id.parse().ok().map(|id| ConversationOrigin::Drawing { id }),
            ("note", Some(id)) => id.parse().ok().map(|id| ConversationOrigin::Note { id }),
            _ => None,
        }
    }

    /// The project a conversation stands in: a project's, or a workstream's.
    pub fn project(&self) -> Option<ProjectId> {
        match self {
            ConversationOrigin::Project { id } => Some(*id),
            ConversationOrigin::Workstream { project, .. } => Some(*project),
            _ => None,
        }
    }

    /// The goal a conversation is about, when its origin is one.
    pub fn goal(&self) -> Option<GoalId> {
        match self {
            ConversationOrigin::Goal { id } => Some(*id),
            _ => None,
        }
    }

    /// The workflow a conversation is about, when its origin is one — the
    /// one `save_workflow` writes, and the only one it may.
    pub fn workflow(&self) -> Option<WorkflowId> {
        match self {
            ConversationOrigin::Workflow { id } => Some(*id),
            _ => None,
        }
    }

    /// The workstream a conversation's turns run in, when its origin is one.
    pub fn workstream(&self) -> Option<WorkstreamId> {
        match self {
            ConversationOrigin::Workstream { id, .. } => Some(*id),
            _ => None,
        }
    }

    /// The drawing a conversation is about, when its origin is one — the
    /// one the drawing tools take when a call names none.
    pub fn drawing(&self) -> Option<DrawingId> {
        match self {
            ConversationOrigin::Drawing { id } => Some(*id),
            _ => None,
        }
    }

    /// The note a conversation is about, when its origin is one — the one
    /// the note tools take when a call names none.
    pub fn note(&self) -> Option<NoteId> {
        match self {
            ConversationOrigin::Note { id } => Some(*id),
            _ => None,
        }
    }

    /// Whether the Workflow Agent takes part: it designs workflows and shapes
    /// goals; a checkout has neither, a drawing is a picture and a note is
    /// somebody's scratchpad, so a conversation about a project, a workstream,
    /// a drawing or a note never offers, resolves or triages to it.
    pub fn reaches_workflow_agent(&self) -> bool {
        !matches!(
            self,
            ConversationOrigin::Project { .. }
                | ConversationOrigin::Workstream { .. }
                | ConversationOrigin::Drawing { .. }
                | ConversationOrigin::Note { .. }
        )
    }

    /// Whether a turn runs in a checkout — a project's tree or a
    /// workstream's. Only there does a [`ConversationMode`] mean anything:
    /// elsewhere a turn runs in the agent's own folder and changes nothing
    /// a person reviews.
    pub fn is_checkout(&self) -> bool {
        self.project().is_some()
    }

    /// Whether the default agent for an unaddressed message is resolved at
    /// the project layer (a project's or a workstream's conversation) rather
    /// than the workspace's.
    pub fn resolves_at_project(&self) -> bool {
        self.project().is_some()
    }
}

/// How far an agent goes on its own in a conversation about a checkout.
/// The mode never loosens the Tool & Commands Guard, the Redactor or the
/// classifier: the rules come first in every mode, and the mode only says
/// what happens to a call no rule decided.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConversationMode {
    /// Edits land and are owed a review; a command no rule decided is asked.
    #[default]
    Manual,
    /// Edits land and commands run; the changes stay reviewable and are
    /// kept when the person sends their next message.
    Auto,
    /// The agent reads and replies with a plan; an edit is refused and a
    /// command is asked.
    Plan,
}

impl ConversationMode {
    /// Every mode, in the order the picker draws and cycles them.
    pub const ALL: [ConversationMode; 3] = [
        ConversationMode::Manual,
        ConversationMode::Auto,
        ConversationMode::Plan,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ConversationMode::Manual => "manual",
            ConversationMode::Auto => "auto",
            ConversationMode::Plan => "plan",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == word)
    }

    /// The highest tier a call no rule decided runs at without a person.
    pub fn ceiling(&self) -> ToolTier {
        match self {
            ConversationMode::Manual => ToolTier::Write,
            ConversationMode::Auto => ToolTier::Exec,
            ConversationMode::Plan => ToolTier::Read,
        }
    }

    /// Whether a file edit may land at all. A plan changes nothing.
    pub fn writes(&self) -> bool {
        !matches!(self, ConversationMode::Plan)
    }

    /// Whether pending changes wait for the person's word.
    pub fn owes_review(&self) -> bool {
        matches!(self, ConversationMode::Manual)
    }

    /// Whether the person's next message keeps what earlier turns changed.
    pub fn keeps_on_next_message(&self) -> bool {
        matches!(self, ConversationMode::Auto)
    }

    /// Whether the harness must be one the guard can stop: a plan that
    /// cannot be held to reading is no plan.
    pub fn needs_tool_guard(&self) -> bool {
        matches!(self, ConversationMode::Plan)
    }
}

/// The record. The messages are not in it: they are the conversation's own
/// log, read by scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Conversation {
    pub id: ConversationId,
    pub origin: ConversationOrigin,
    /// A person's name for it. `None` until one is given; a list then shows
    /// the first message's first line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub created_at: u64,
    #[serde(default)]
    pub archived: bool,
    /// How far an agent goes on its own here. Read only where the origin
    /// [`ConversationOrigin::is_checkout`].
    #[serde(default)]
    pub mode: ConversationMode,
    /// The mode the conversation was in before it went to `plan` — where
    /// *Build this plan* returns to. `None` outside a plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_before_plan: Option<ConversationMode>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConversationError {
    #[error("a conversation's title is a line of words: not blank")]
    BlankTitle,
    #[error("a conversation's title is at most {max} characters, not {0}", max = MAX_TITLE_CHARS)]
    TitleTooLong(usize),
}

/// A title as the record keeps it: trimmed, one line, within the bound.
/// `None` stays `None`; a blank or an over-long one is refused rather than
/// cut, because a title a person typed is theirs to shorten.
pub fn validate_title(title: Option<&str>) -> Result<Option<String>, ConversationError> {
    let Some(raw) = title else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ConversationError::BlankTitle);
    }
    let one_line = trimmed
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let chars = one_line.chars().count();
    if chars > MAX_TITLE_CHARS {
        return Err(ConversationError::TitleTooLong(chars));
    }
    Ok(Some(one_line))
}

impl Conversation {
    /// Move to `mode`, remembering where a plan came from so building it
    /// goes back there. Leaving a plan forgets it.
    pub fn set_mode(&mut self, mode: ConversationMode) {
        if mode == self.mode {
            return;
        }
        self.mode_before_plan = match mode {
            ConversationMode::Plan => Some(self.mode),
            _ => None,
        };
        self.mode = mode;
    }

    /// The mode *Build this plan* switches to: the one before the plan, or
    /// the default when the conversation started in one.
    pub fn mode_after_plan(&self) -> ConversationMode {
        self.mode_before_plan.unwrap_or_default()
    }

    pub fn validate(&self) -> Result<(), ConversationError> {
        validate_title(self.title.as_deref()).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::AgentId;

    fn every_origin() -> Vec<ConversationOrigin> {
        vec![
            ConversationOrigin::Node,
            ConversationOrigin::Workspace,
            ConversationOrigin::Goal {
                id: GoalId::from_ulid(ulid::Ulid::from_parts(2, 1)),
            },
            ConversationOrigin::Workflow {
                id: WorkflowId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            },
            ConversationOrigin::Project {
                id: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            },
            ConversationOrigin::Workstream {
                id: WorkstreamId::from_ulid(ulid::Ulid::from_parts(6, 1)),
                project: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            },
            ConversationOrigin::Drawing {
                id: DrawingId::from_ulid(ulid::Ulid::from_parts(7, 1)),
            },
            ConversationOrigin::Note {
                id: NoteId::from_ulid(ulid::Ulid::from_parts(8, 1)),
            },
        ]
    }

    #[test]
    fn every_origin_carries_its_kind_on_the_wire_and_round_trips() {
        for origin in every_origin() {
            let json = serde_json::to_value(&origin).unwrap();
            assert_eq!(json["kind"], origin.kind(), "{origin:?}");
            assert!(ConversationOrigin::KINDS.contains(&origin.kind()));
            assert_eq!(
                serde_json::from_value::<ConversationOrigin>(json).unwrap(),
                origin
            );
            assert_eq!(
                ConversationOrigin::kind_takes_id(origin.kind()),
                origin.id().is_some(),
                "{origin:?}"
            );
        }
        assert_eq!(
            every_origin().iter().map(|o| o.kind()).collect::<Vec<_>>(),
            ConversationOrigin::KINDS,
            "the list order is the kinds' order"
        );
    }

    #[test]
    fn an_origin_is_rebuilt_from_its_parts_and_only_from_a_pairing_that_makes_sense() {
        for origin in every_origin() {
            let project = origin.project().map(|p| p.to_string());
            assert_eq!(
                ConversationOrigin::from_parts(
                    origin.kind(),
                    origin.id().as_deref(),
                    project.as_deref()
                ),
                Some(origin.clone()),
                "{origin:?} round-trips through its parts"
            );
        }
        assert_eq!(
            ConversationOrigin::from_parts("node", Some("x"), None),
            None
        );
        assert_eq!(ConversationOrigin::from_parts("goal", None, None), None);
        assert_eq!(
            ConversationOrigin::from_parts("goal", Some("not an id"), None),
            None
        );
        let wid = WorkstreamId::from_ulid(ulid::Ulid::from_parts(6, 1)).to_string();
        assert_eq!(
            ConversationOrigin::from_parts("workstream", Some(&wid), None),
            None,
            "a workstream origin carries its project"
        );
        assert_eq!(
            ConversationOrigin::from_parts("team", Some("x"), None),
            None
        );
    }

    #[test]
    fn the_project_the_goal_and_the_workstream_are_read_off_the_origin_that_has_them() {
        let project = ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let workstream = WorkstreamId::from_ulid(ulid::Ulid::from_parts(6, 1));
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(2, 1));
        assert_eq!(
            ConversationOrigin::Workstream {
                id: workstream,
                project
            }
            .project(),
            Some(project)
        );
        assert_eq!(
            ConversationOrigin::Workstream {
                id: workstream,
                project
            }
            .workstream(),
            Some(workstream)
        );
        assert_eq!(
            ConversationOrigin::Project { id: project }.project(),
            Some(project)
        );
        assert_eq!(
            ConversationOrigin::Project { id: project }.workstream(),
            None
        );
        assert_eq!(ConversationOrigin::Goal { id: goal }.goal(), Some(goal));
        assert_eq!(ConversationOrigin::Goal { id: goal }.project(), None);
        assert_eq!(ConversationOrigin::Node.project(), None);
        assert_eq!(ConversationOrigin::Workspace.goal(), None);
    }

    #[test]
    fn the_workflow_agent_is_reachable_everywhere_but_a_checkout_a_drawing_or_a_note() {
        let _ = AgentId::workflow();
        for origin in every_origin() {
            let expected = !matches!(origin.kind(), "project" | "workstream" | "drawing" | "note");
            assert_eq!(origin.reaches_workflow_agent(), expected, "{origin:?}");
            assert_eq!(
                origin.resolves_at_project(),
                origin.project().is_some(),
                "{origin:?}"
            );
            assert_eq!(origin.drawing().is_some(), origin.kind() == "drawing");
            assert_eq!(origin.note().is_some(), origin.kind() == "note");
        }
    }

    #[test]
    fn a_title_is_one_trimmed_line_within_the_bound_or_refused() {
        assert_eq!(validate_title(None), Ok(None));
        assert_eq!(
            validate_title(Some("  Dark mode  ")),
            Ok(Some("Dark mode".into()))
        );
        assert_eq!(
            validate_title(Some("first\n  second\r\n")),
            Ok(Some("first second".into()))
        );
        assert_eq!(
            validate_title(Some("   ")),
            Err(ConversationError::BlankTitle)
        );
        assert_eq!(
            validate_title(Some("\n")),
            Err(ConversationError::BlankTitle)
        );
        let unicode = "é".repeat(MAX_TITLE_CHARS);
        assert_eq!(validate_title(Some(&unicode)), Ok(Some(unicode.clone())));
        let long = "é".repeat(MAX_TITLE_CHARS + 1);
        assert_eq!(
            validate_title(Some(&long)),
            Err(ConversationError::TitleTooLong(MAX_TITLE_CHARS + 1))
        );
    }

    #[test]
    fn the_record_round_trips_and_a_missing_title_is_absent_on_the_wire() {
        let c = Conversation {
            id: ConversationId::from_ulid(ulid::Ulid::from_parts(9, 1)),
            origin: ConversationOrigin::Workspace,
            title: None,
            created_at: 7,
            archived: false,
            mode: ConversationMode::default(),
            mode_before_plan: None,
        };
        assert!(c.validate().is_ok());
        let json = serde_json::to_value(&c).unwrap();
        assert!(json.get("title").is_none());
        assert_eq!(json["mode"], "manual");
        assert!(json.get("mode_before_plan").is_none());
        assert_eq!(json["origin"]["kind"], "workspace");
        assert_eq!(serde_json::from_value::<Conversation>(json).unwrap(), c);
        let blank = Conversation {
            title: Some(" ".into()),
            ..c
        };
        assert_eq!(blank.validate(), Err(ConversationError::BlankTitle));
    }

    #[test]
    fn a_mode_says_how_far_a_call_no_rule_decided_goes() {
        assert_eq!(ConversationMode::default(), ConversationMode::Manual);
        assert_eq!(ConversationMode::Manual.ceiling(), ToolTier::Write);
        assert_eq!(ConversationMode::Auto.ceiling(), ToolTier::Exec);
        assert_eq!(ConversationMode::Plan.ceiling(), ToolTier::Read);
        for mode in ConversationMode::ALL {
            assert_eq!(ConversationMode::parse(mode.as_str()), Some(mode));
            assert_eq!(serde_json::to_value(mode).unwrap(), mode.as_str());
            assert_eq!(mode.writes(), mode != ConversationMode::Plan);
            assert_eq!(mode.owes_review(), mode == ConversationMode::Manual);
            assert_eq!(mode.keeps_on_next_message(), mode == ConversationMode::Auto);
            assert_eq!(mode.needs_tool_guard(), mode == ConversationMode::Plan);
        }
        assert_eq!(ConversationMode::parse("guided"), None);
    }

    #[test]
    fn building_a_plan_goes_back_to_the_mode_before_it() {
        let mut c = Conversation {
            id: ConversationId::from_ulid(ulid::Ulid::from_parts(9, 2)),
            origin: ConversationOrigin::Project {
                id: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            },
            title: None,
            created_at: 7,
            archived: false,
            mode: ConversationMode::Auto,
            mode_before_plan: None,
        };
        assert!(c.origin.is_checkout());
        c.set_mode(ConversationMode::Plan);
        assert_eq!(c.mode_before_plan, Some(ConversationMode::Auto));
        // Setting the mode it is already in forgets nothing.
        c.set_mode(ConversationMode::Plan);
        assert_eq!(c.mode_after_plan(), ConversationMode::Auto);
        c.set_mode(c.mode_after_plan());
        assert_eq!((c.mode, c.mode_before_plan), (ConversationMode::Auto, None));
        // A conversation that started in a plan builds in the default.
        c.mode = ConversationMode::Plan;
        assert_eq!(c.mode_after_plan(), ConversationMode::Manual);
        assert!(!ConversationOrigin::Workspace.is_checkout());
    }

    // added by the coverage pass: conversation.rs

    #[test]
    fn a_conversation_about_a_workflow_names_it_and_the_others_do_not() {
        let id = crate::id::WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1));
        assert_eq!(ConversationOrigin::Workflow { id }.workflow(), Some(id));
        assert_eq!(ConversationOrigin::Node.workflow(), None);
    }
}
