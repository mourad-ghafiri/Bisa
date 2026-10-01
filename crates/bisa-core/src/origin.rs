//! Where a definition came from.
//!
//! A workspace opens with one agent and an opt-in catalog, so every agent,
//! skill, team and channel in it arrived one of three ways: somebody created it
//! here, somebody installed it from the catalog, or it is the platform's own
//! agent. That fact decides whether an id collides with a catalog entry, what
//! an uninstall may touch, and what a surface calls the thing — so it is
//! recorded on the object rather than guessed from its id.
//!
//! It replaces a `builtin: bool`, which could say only "seeded" and could not
//! say *which* bundled definition an object was, nor tell the one agent that
//! must always exist from the catalog's, which need not.
//!
//! **Provenance is recorded, never accepted from a caller.** Like `created_at`
//! and an agent's pubkey, it is set once at creation and preserved from the
//! stored record on every update: a client that could send its own origin
//! could rename a local agent into a catalog one and take over the id, or
//! demote the core agent into something removable.

use crate::id::{GoalId, RunId, WorkflowId};
use crate::workflow::StepId;
use serde::{Deserialize, Serialize};

/// Where a skill, team or channel came from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum Origin {
    /// Created in this workspace.
    #[default]
    Local,
    /// Installed from the bundled catalog, under this slug.
    Catalog { slug: String },
}

impl Origin {
    /// The catalog slug this came from, if it came from the catalog.
    pub fn catalog_slug(&self) -> Option<&str> {
        match self {
            Origin::Local => None,
            Origin::Catalog { slug } => Some(slug),
        }
    }
}

/// Where a workflow came from.
///
/// A separate type from [`Origin`] because a workflow has a case the others
/// cannot hold: a design drawn for **one goal** — the Workflow Agent's
/// proposal, or the goal's own Workflow tab. A goal-scoped design is the
/// goal's, not the library's: the Workflows destination and every picker show
/// the library (`Workspace` and `Catalog`) and leave a goal's designs on the
/// goal, and promoting one to the library is an explicit copy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "origin")]
pub enum WorkflowOrigin {
    /// Drawn for the library by a person in this workspace.
    #[default]
    Workspace,
    /// Installed from the bundled catalog, under this slug.
    Catalog { slug: String },
    /// Designed for one goal, and shown there rather than in the library.
    Goal { goal: GoalId },
}

impl WorkflowOrigin {
    /// The catalog slug this came from, if it came from the catalog.
    pub fn catalog_slug(&self) -> Option<&str> {
        match self {
            WorkflowOrigin::Catalog { slug } => Some(slug),
            _ => None,
        }
    }

    /// The goal this was designed for, when it is a goal's design.
    pub fn goal(&self) -> Option<GoalId> {
        match self {
            WorkflowOrigin::Goal { goal } => Some(*goal),
            _ => None,
        }
    }

    /// Whether the library shows it: the workspace's own designs and the
    /// installed templates. A goal's design is not the library's.
    pub fn is_library(&self) -> bool {
        !matches!(self, WorkflowOrigin::Goal { .. })
    }
}

/// The step that made a project: which run, which step, of which workflow.
/// Kept whichever tab the project sits in, because *who made this*
/// is history worth keeping even when the project is filed under its goal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepRef {
    pub run: RunId,
    pub step: StepId,
    pub workflow: WorkflowId,
}

/// Where a project came from. **Never an owner**: the goal ⇄ project
/// attachment stays the one relation, and
/// this is history — recorded once at creation, surviving the deletion of
/// whatever it names, resolved through by nothing.
///
/// A project made by a step sits with what the step's workflow belongs to
///: a step of the **goal's own design** makes the goal's project
/// (`Goal`, with the step named); a step of a **library** workflow — reused
/// across goals, run in the workspace, or begun by one of its start events —
/// makes the workflow's (`Step`).
///
/// Read by hand: `Step` flattens its [`StepRef`], and a derived `flatten` of
/// a struct lets a key nobody knows drop in silence, whatever the struct's
/// own rule says — so the keys are held to the variant's list first
/// ([`crate::workflow::refuse_unknown_keys`]), then read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "origin")]
pub enum ProjectOrigin {
    /// A person made it with no goal in hand.
    #[default]
    Workspace,
    /// Made from a goal — by a person on the goal's surfaces or `--goal`, by
    /// an agent in a goal's session outside any step, or by a step of the
    /// goal's own design (`step` names it then).
    Goal {
        goal: GoalId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<StepRef>,
    },
    /// Made by an agent running one step of a library workflow — on a goal
    /// (`goal` names it), or in a run of the workspace (no goal at all).
    Step {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal: Option<GoalId>,
        #[serde(flatten)]
        step: StepRef,
    },
}

impl<'de> Deserialize<'de> for ProjectOrigin {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        /// The derived reading, once the keys are known to be the variant's.
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case", tag = "origin")]
        enum Shape {
            Workspace,
            Goal {
                goal: GoalId,
                #[serde(default)]
                step: Option<StepRef>,
            },
            Step {
                #[serde(default)]
                goal: Option<GoalId>,
                #[serde(flatten)]
                step: StepRef,
            },
        }
        let raw = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let tag = raw
            .get("origin")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| D::Error::missing_field("origin"))?;
        let own: &[&str] = match tag {
            "workspace" => &[],
            "goal" => &["goal", "step"],
            "step" => &["goal", "run", "step", "workflow"],
            other => return Err(D::Error::unknown_variant(other, &Self::NAMES)),
        };
        crate::workflow::refuse_unknown_keys::<D::Error>(&raw, "project origin", &["origin"], own)?;
        let shape: Shape =
            serde_json::from_value(serde_json::Value::Object(raw)).map_err(D::Error::custom)?;
        Ok(match shape {
            Shape::Workspace => ProjectOrigin::Workspace,
            Shape::Goal { goal, step } => ProjectOrigin::Goal { goal, step },
            Shape::Step { goal, step } => ProjectOrigin::Step { goal, step },
        })
    }
}

impl ProjectOrigin {
    /// The wire names, for anything that enumerates the three.
    pub const NAMES: [&'static str; 3] = ["workspace", "goal", "step"];

    /// A project made from a goal by a person, or by an agent outside any step.
    pub fn from_goal(goal: GoalId) -> Self {
        ProjectOrigin::Goal { goal, step: None }
    }

    /// **The one rule** for a step-born project: the goal's own
    /// design makes the goal's project, a library workflow makes the
    /// workflow's. `goal` is the goal the run is for — none for a run of the
    /// workspace; `workflow` is the origin of the run's frozen workflow.
    pub fn born_of_step(goal: Option<GoalId>, step: StepRef, workflow: &WorkflowOrigin) -> Self {
        match goal {
            Some(goal) if workflow.goal() == Some(goal) => ProjectOrigin::Goal {
                goal,
                step: Some(step),
            },
            goal => ProjectOrigin::Step { goal, step },
        }
    }

    /// The goal this was born of, when it was born of one.
    pub fn goal(&self) -> Option<GoalId> {
        match self {
            ProjectOrigin::Workspace => None,
            ProjectOrigin::Goal { goal, .. } => Some(*goal),
            ProjectOrigin::Step { goal, .. } => *goal,
        }
    }

    /// The step that made it, when a step did — under either goal or step.
    pub fn step_ref(&self) -> Option<&StepRef> {
        match self {
            ProjectOrigin::Goal { step, .. } => step.as_ref(),
            ProjectOrigin::Step { step, .. } => Some(step),
            ProjectOrigin::Workspace => None,
        }
    }

    /// The library workflow whose step made it — the one the Workflows tab
    /// files it under. A goal's design is not a library workflow: `None`.
    pub fn workflow(&self) -> Option<WorkflowId> {
        match self {
            ProjectOrigin::Step { step, .. } => Some(step.workflow),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectOrigin::Workspace => "workspace",
            ProjectOrigin::Goal { .. } => "goal",
            ProjectOrigin::Step { .. } => "step",
        }
    }
}

/// Where an agent came from.
///
/// A separate type from [`Origin`] because it has a third case the others
/// cannot hold: exactly one agent is the platform's own, and only an agent can
/// be. Folding both into one enum would mean a `Core` skill and a `Core` team
/// were representable and then validated away, and a type that permits an
/// invalid state and then rejects it is a worse type than one that does not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum AgentOrigin {
    /// Created in this workspace.
    #[default]
    Local,
    /// Installed from the bundled catalog, under this slug.
    Catalog { slug: String },
    /// The platform's own agent, one of the two that hold a record: always
    /// present, never removable, and only its harness, its model plan and its
    /// decision-making switch are editable.
    Core,
}

impl AgentOrigin {
    /// Whether this is the platform's own agent — the one case that changes
    /// what an update and a delete are allowed to do.
    pub fn is_core(&self) -> bool {
        matches!(self, AgentOrigin::Core)
    }

    /// The catalog slug this came from, if it came from the catalog.
    pub fn catalog_slug(&self) -> Option<&str> {
        match self {
            AgentOrigin::Catalog { slug } => Some(slug),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shape both types are read back from, stated once here so a
    /// serde attribute cannot drift without a test noticing.
    #[test]
    fn origins_round_trip_in_snake_case() {
        for (origin, json) in [
            (Origin::Local, r#""local""#),
            (
                Origin::Catalog {
                    slug: "developer".into(),
                },
                r#"{"catalog":{"slug":"developer"}}"#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&origin).unwrap(), json);
            assert_eq!(serde_json::from_str::<Origin>(json).unwrap(), origin);
        }
        for (origin, json) in [
            (AgentOrigin::Local, r#""local""#),
            (AgentOrigin::Core, r#""core""#),
            (
                AgentOrigin::Catalog {
                    slug: "developer".into(),
                },
                r#"{"catalog":{"slug":"developer"}}"#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&origin).unwrap(), json);
            assert_eq!(serde_json::from_str::<AgentOrigin>(json).unwrap(), origin);
        }
    }

    #[test]
    fn workflow_and_project_origins_round_trip_internally_tagged() {
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(9, 9));
        for (origin, json) in [
            (
                WorkflowOrigin::Workspace,
                r#"{"origin":"workspace"}"#.to_string(),
            ),
            (
                WorkflowOrigin::Catalog {
                    slug: "bug-fix".into(),
                },
                r#"{"origin":"catalog","slug":"bug-fix"}"#.to_string(),
            ),
            (
                WorkflowOrigin::Goal { goal },
                format!(r#"{{"origin":"goal","goal":"{goal}"}}"#),
            ),
        ] {
            assert_eq!(serde_json::to_string(&origin).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<WorkflowOrigin>(&json).unwrap(),
                origin
            );
        }
        let run = RunId::from_ulid(ulid::Ulid::from_parts(9, 10));
        let workflow = WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 11));
        let step = StepId::new("implement").unwrap();
        let by_step = StepRef {
            run,
            step: step.clone(),
            workflow,
        };
        for origin in [
            ProjectOrigin::Workspace,
            ProjectOrigin::from_goal(goal),
            ProjectOrigin::Goal {
                goal,
                step: Some(by_step.clone()),
            },
            ProjectOrigin::Step {
                goal: Some(goal),
                step: by_step.clone(),
            },
            ProjectOrigin::Step {
                goal: None,
                step: by_step.clone(),
            },
        ] {
            let json = serde_json::to_value(&origin).unwrap();
            assert!(ProjectOrigin::NAMES.contains(&json["origin"].as_str().unwrap()));
            assert_eq!(
                serde_json::from_value::<ProjectOrigin>(json).unwrap(),
                origin
            );
        }
        // The wire shapes a client reads: a step origin is flat, a goal origin
        // names its step only when one made it.
        let flat = serde_json::to_value(ProjectOrigin::Step {
            goal: Some(goal),
            step: by_step.clone(),
        })
        .unwrap();
        assert_eq!(flat["run"], serde_json::json!(run.to_string()));
        assert_eq!(flat["step"], serde_json::json!("implement"));
        assert_eq!(flat["workflow"], serde_json::json!(workflow.to_string()));
        let workspace_run = serde_json::to_value(ProjectOrigin::Step {
            goal: None,
            step: by_step.clone(),
        })
        .unwrap();
        assert!(
            workspace_run.get("goal").is_none(),
            "a step of a run of the workspace names no goal"
        );
        let plain = serde_json::to_value(ProjectOrigin::from_goal(goal)).unwrap();
        assert!(plain.get("step").is_none(), "no step, no key");
        let designed = serde_json::to_value(ProjectOrigin::Goal {
            goal,
            step: Some(by_step),
        })
        .unwrap();
        assert_eq!(designed["step"]["step"], serde_json::json!("implement"));
        assert_eq!(
            designed["step"]["workflow"],
            serde_json::json!(workflow.to_string())
        );
    }

    /// Where a step-born project sits follows the step's workflow.
    #[test]
    fn a_step_of_the_goals_own_design_makes_the_goals_project_and_a_library_step_the_workflows() {
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(3, 1));
        let other = GoalId::from_ulid(ulid::Ulid::from_parts(3, 2));
        let by_step = StepRef {
            run: RunId::from_ulid(ulid::Ulid::from_parts(3, 3)),
            step: StepId::new("build").unwrap(),
            workflow: WorkflowId::from_ulid(ulid::Ulid::from_parts(3, 4)),
        };
        let own = ProjectOrigin::born_of_step(
            Some(goal),
            by_step.clone(),
            &WorkflowOrigin::Goal { goal },
        );
        assert_eq!(
            own,
            ProjectOrigin::Goal {
                goal,
                step: Some(by_step.clone())
            }
        );
        assert_eq!(own.as_str(), "goal");
        assert_eq!(
            own.step_ref(),
            Some(&by_step),
            "the step is kept as history"
        );
        assert_eq!(
            own.workflow(),
            None,
            "a design is not a library workflow to file under"
        );
        for library in [
            WorkflowOrigin::Workspace,
            WorkflowOrigin::Catalog {
                slug: "bug-fix".into(),
            },
            WorkflowOrigin::Goal { goal: other },
        ] {
            let of_step = ProjectOrigin::born_of_step(Some(goal), by_step.clone(), &library);
            assert_eq!(
                of_step,
                ProjectOrigin::Step {
                    goal: Some(goal),
                    step: by_step.clone()
                },
                "{library:?}"
            );
            assert_eq!(of_step.workflow(), Some(by_step.workflow));
            assert_eq!(of_step.goal(), Some(goal));
        }
        // A run of the workspace has no goal: its step's project is the
        // workflow's, born of nobody's goal.
        let of_workspace_run =
            ProjectOrigin::born_of_step(None, by_step.clone(), &WorkflowOrigin::Workspace);
        assert_eq!(
            of_workspace_run,
            ProjectOrigin::Step {
                goal: None,
                step: by_step.clone()
            }
        );
        assert_eq!(of_workspace_run.goal(), None);
        assert_eq!(of_workspace_run.workflow(), Some(by_step.workflow));
        assert_eq!(of_workspace_run.step_ref(), Some(&by_step));
    }

    #[test]
    fn a_goal_scoped_workflow_is_not_library_and_the_helpers_agree() {
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(1, 2));
        assert!(WorkflowOrigin::Workspace.is_library());
        assert!(WorkflowOrigin::Catalog { slug: "x".into() }.is_library());
        assert!(!WorkflowOrigin::Goal { goal }.is_library());
        assert_eq!(WorkflowOrigin::Goal { goal }.goal(), Some(goal));
        assert_eq!(WorkflowOrigin::Workspace.goal(), None);
        assert_eq!(
            WorkflowOrigin::Catalog { slug: "x".into() }.catalog_slug(),
            Some("x")
        );
        let origin = ProjectOrigin::Step {
            goal: Some(goal),
            step: StepRef {
                run: RunId::from_ulid(ulid::Ulid::from_parts(1, 3)),
                step: StepId::new("build").unwrap(),
                workflow: WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 4)),
            },
        };
        assert_eq!(origin.goal(), Some(goal));
        assert!(origin.workflow().is_some());
        assert_eq!(ProjectOrigin::Workspace.workflow(), None);
    }

    #[test]
    fn only_the_core_agent_is_core_and_only_catalog_entries_have_slugs() {
        assert!(AgentOrigin::Core.is_core());
        assert!(!AgentOrigin::Local.is_core());
        assert!(!AgentOrigin::Catalog {
            slug: "developer".into()
        }
        .is_core());

        assert_eq!(AgentOrigin::Core.catalog_slug(), None);
        assert_eq!(Origin::Local.catalog_slug(), None);
        assert_eq!(
            Origin::Catalog {
                slug: "engineering".into()
            }
            .catalog_slug(),
            Some("engineering"),
        );
    }
}
