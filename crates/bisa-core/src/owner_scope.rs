//! `OwnerScope`: what a record that hangs off something else is attached to —
//! a note's scope and a drawing's are the same six choices, so the type is
//! one, and the wire (`?scope=`) and the index (`scope_kind`) share the words
//! of [`OwnerScope::KINDS`].

use crate::id::{ChannelId, GoalId, ProjectId, WorkflowId};
use serde::{Deserialize, Serialize};

/// What a note or a drawing is attached to. `Workspace` is a real choice, not
/// a fallback; `Node` is this machine's own — the harnesses, the setup, what
/// is true here and nowhere else. The seven tabs the Notes and Draw overlays
/// draw are these six plus *All*, which is no scope at all.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "kind")]
pub enum OwnerScope {
    Workspace,
    Goal { id: GoalId },
    Project { id: ProjectId },
    Workflow { id: WorkflowId },
    Channel { id: ChannelId },
    Node,
}

impl OwnerScope {
    /// Every kind, in the order the tabs draw them — the vocabulary the wire
    /// (`?scope=`) and the index (`scope_kind`) share.
    pub const KINDS: [&'static str; 6] = [
        "workspace",
        "project",
        "goal",
        "workflow",
        "channel",
        "node",
    ];

    pub fn kind(&self) -> &'static str {
        match self {
            OwnerScope::Workspace => "workspace",
            OwnerScope::Goal { .. } => "goal",
            OwnerScope::Project { .. } => "project",
            OwnerScope::Workflow { .. } => "workflow",
            OwnerScope::Channel { .. } => "channel",
            OwnerScope::Node => "node",
        }
    }

    pub fn id(&self) -> Option<String> {
        match self {
            OwnerScope::Workspace | OwnerScope::Node => None,
            OwnerScope::Goal { id } => Some(id.to_string()),
            OwnerScope::Project { id } => Some(id.to_string()),
            OwnerScope::Workflow { id } => Some(id.to_string()),
            OwnerScope::Channel { id } => Some(id.to_string()),
        }
    }

    /// Whether a kind names one record (a goal, a project, a workflow, a
    /// channel) or stands alone (the workspace, the node).
    pub fn kind_takes_id(kind: &str) -> bool {
        !matches!(kind, "workspace" | "node")
    }

    /// The scope a kind and an id name, as the wire and the index carry them.
    /// `None` for a kind outside [`Self::KINDS`], a standalone kind with an
    /// id, a record kind without one, or an id that does not parse.
    pub fn from_parts(kind: &str, id: Option<&str>) -> Option<Self> {
        match (kind, id) {
            ("workspace", None) => Some(OwnerScope::Workspace),
            ("node", None) => Some(OwnerScope::Node),
            ("goal", Some(id)) => id.parse().ok().map(|id| OwnerScope::Goal { id }),
            ("project", Some(id)) => id.parse().ok().map(|id| OwnerScope::Project { id }),
            ("workflow", Some(id)) => id.parse().ok().map(|id| OwnerScope::Workflow { id }),
            ("channel", Some(id)) => id.parse().ok().map(|id| OwnerScope::Channel { id }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_rebuilt_from_its_kind_and_id_and_only_from_a_pairing_that_makes_sense() {
        let wf = WorkflowId::from_ulid(ulid::Ulid::from_parts(4, 1));
        let ch = ChannelId::new("engineering").unwrap();
        for scope in [
            OwnerScope::Workspace,
            OwnerScope::Node,
            OwnerScope::Goal {
                id: GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            },
            OwnerScope::Project {
                id: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            },
            OwnerScope::Workflow { id: wf },
            OwnerScope::Channel { id: ch.clone() },
        ] {
            assert!(OwnerScope::KINDS.contains(&scope.kind()));
            assert_eq!(
                OwnerScope::from_parts(scope.kind(), scope.id().as_deref()),
                Some(scope.clone()),
                "{scope:?} round-trips through its parts"
            );
            assert_eq!(
                OwnerScope::kind_takes_id(scope.kind()),
                scope.id().is_some()
            );
        }
        assert_eq!(OwnerScope::from_parts("workspace", Some("x")), None);
        assert_eq!(OwnerScope::from_parts("node", Some("x")), None);
        assert_eq!(OwnerScope::from_parts("workflow", None), None);
        assert_eq!(OwnerScope::from_parts("workflow", Some("not an id")), None);
        assert_eq!(OwnerScope::from_parts("channel", Some("Not A Slug")), None);
        assert_eq!(OwnerScope::from_parts("team", Some("x")), None);
        assert_eq!(OwnerScope::Workspace.id(), None);
        assert_eq!(OwnerScope::Workspace.kind(), "workspace");
        assert_eq!(OwnerScope::Node.id(), None);
        assert_eq!(OwnerScope::Node.kind(), "node");
    }
}
