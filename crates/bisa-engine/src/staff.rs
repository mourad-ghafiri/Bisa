//! Who a designer may name on a step: the enabled, non-core agents and the
//! enabled teams of this workspace.
//!
//! One type, built once, rendered for a prompt and served as an op — so the
//! roster the Workflow Agent reads in its directive and the one `list_staff`
//! answers cannot disagree. Validation judges an assignee against the enabled
//! staff (`workflow_validation_ctx`); this is the same population, with the
//! words a designer needs to choose between them: what each agent does, on
//! which harness, with which skills, and who is on each team.
//!
//! A goal may narrow it: when the goal names agents or teams to carry it —
//! or its nearest ancestor does (`Workspace::staff_scope`) — its design, its
//! repairs and its amendments name only those, a team whole or by one of its
//! members ([`StaffRoster::for_goal`], [`StaffRoster::scoped_to`]). A goal
//! that names neither reads the whole enabled staff.

use bisa_core::{
    Agent, AgentId, Assignee, GoalId, SkillId, Step, StepId, StepKind, Team, TeamId, ValueRef,
};
use bisa_store::{StoreError, Workspace};
use serde::Serialize;

/// One agent a step may name.
#[derive(Debug, Clone, Serialize)]
pub struct StaffAgent {
    pub id: AgentId,
    pub name: String,
    pub description: Option<String>,
    pub harness: String,
    pub skills: Vec<SkillId>,
    /// The enabled teams this agent is on.
    pub teams: Vec<TeamId>,
}

/// One team a step may name.
#[derive(Debug, Clone, Serialize)]
pub struct StaffTeam {
    pub id: TeamId,
    pub name: String,
    pub purpose: Option<String>,
    /// The enabled, non-core agents on it.
    pub agents: Vec<AgentId>,
    /// How many people are on it.
    pub humans: usize,
}

/// The staff a definition may be assigned to.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StaffRoster {
    pub agents: Vec<StaffAgent>,
    pub teams: Vec<StaffTeam>,
    /// A goal's roster: the agents and teams the goal names to carry it
    /// ([`Self::scoped_to`]), not the workspace's whole enabled staff.
    pub scoped: bool,
}

impl StaffRoster {
    /// Pure. Drops disabled agents, the General Agent and the Workflow Agent (they run the
    /// platform and are never assigned work), disabled teams, and disabled or
    /// core agents from a team's member list. Sorted by id, so the rendered
    /// roster reads the same on every wake.
    pub fn from_parts(agents: &[Agent], teams: &[Team]) -> Self {
        let mut keep: Vec<&Agent> = agents
            .iter()
            .filter(|a| a.enabled && !a.id.is_core_id())
            .collect();
        keep.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        let mut teams_out: Vec<StaffTeam> = teams
            .iter()
            .filter(|t| t.enabled)
            .map(|t| StaffTeam {
                id: t.id.clone(),
                name: t.name.clone(),
                purpose: t.purpose.clone(),
                agents: t
                    .members
                    .iter()
                    .filter_map(|m| match m {
                        Assignee::Agent(id) => keep
                            .iter()
                            .find(|a| a.id.as_str() == id.as_str())
                            .map(|a| a.id.clone()),
                        _ => None,
                    })
                    .collect(),
                humans: t
                    .members
                    .iter()
                    .filter(|m| matches!(m, Assignee::Human(_)))
                    .count(),
            })
            .collect();
        teams_out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        let agents_out = keep
            .iter()
            .map(|a| StaffAgent {
                id: a.id.clone(),
                name: a.name.clone(),
                description: a.description.clone(),
                harness: a.harness.clone(),
                skills: a.skills.clone(),
                teams: teams_out
                    .iter()
                    .filter(|t| t.agents.iter().any(|m| m == &a.id))
                    .map(|t| t.id.clone())
                    .collect(),
            })
            .collect();
        Self {
            agents: agents_out,
            teams: teams_out,
            scoped: false,
        }
    }

    /// The roster of this workspace as it stands.
    pub fn of(ws: &Workspace) -> Result<Self, StoreError> {
        Ok(Self::from_parts(&ws.list_agents()?, &ws.list_teams()?))
    }

    /// The roster a goal's design reads: the agents and teams the goal names
    /// to carry it — or its nearest ancestor does ([`Workspace::staff_scope`]) —
    /// and the whole enabled staff when none does.
    pub fn for_goal(ws: &Workspace, goal: GoalId) -> Result<Self, StoreError> {
        Ok(Self::of(ws)?.scoped_to(&ws.staff_scope(goal)))
    }

    /// Pure. This roster narrowed to `scope`, the agents and teams a goal
    /// names: each named team that is on the roster, each named agent that
    /// is, and every agent on a kept team — a team is named whole or by one
    /// of its members. An agent's `teams` keep only the kept teams. A scope
    /// that names no agent and no team narrows nothing: the roster comes
    /// back as it was, unscoped. Names that are not on the roster — a
    /// disabled agent, a team stood down, one since deleted — drop out.
    pub fn scoped_to(self, scope: &[Assignee]) -> Self {
        let named_agents: Vec<&str> = scope.iter().filter_map(Assignee::as_agent).collect();
        let named_teams: Vec<&str> = scope
            .iter()
            .filter_map(|a| match a {
                Assignee::Team(id) => Some(id.as_str()),
                _ => None,
            })
            .collect();
        if named_agents.is_empty() && named_teams.is_empty() {
            return self;
        }
        let teams: Vec<StaffTeam> = self
            .teams
            .into_iter()
            .filter(|t| named_teams.contains(&t.id.as_str()))
            .collect();
        let agents: Vec<StaffAgent> = self
            .agents
            .into_iter()
            .filter(|a| {
                named_agents.contains(&a.id.as_str())
                    || teams.iter().any(|t| t.agents.contains(&a.id))
            })
            .map(|mut a| {
                a.teams.retain(|id| teams.iter().any(|t| &t.id == id));
                a
            })
            .collect();
        Self {
            agents,
            teams,
            scoped: true,
        }
    }

    /// How many may be named, in the words a staffing problem ends on:
    /// "2 agents and 1 team are enabled here", or, for a goal's roster,
    /// "… carry this goal".
    pub fn standing(&self) -> String {
        let (a, t) = (self.agents.len(), self.teams.len());
        format!(
            "{a} agent{} and {t} team{} {}",
            if a == 1 { "" } else { "s" },
            if t == 1 { "" } else { "s" },
            if self.scoped {
                "carry this goal"
            } else {
                "are enabled here"
            }
        )
    }

    /// Who an assignee off this roster is not, for its problem: "not
    /// installed and enabled here", or, for a goal's roster, "not among the
    /// agents and teams that carry this goal".
    pub fn outsider(&self) -> &'static str {
        if self.scoped {
            "not among the agents and teams that carry this goal"
        } else {
            "not installed and enabled here"
        }
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.teams.is_empty()
    }

    /// Does the roster hold this assignee — an enabled agent or team?
    pub fn holds(&self, assignee: &Assignee) -> bool {
        match assignee {
            Assignee::Agent(id) => self.agents.iter().any(|a| a.id.as_str() == id.as_str()),
            Assignee::Team(id) => self.teams.iter().any(|t| t.id.as_str() == id.as_str()),
            Assignee::Human(_) => false,
        }
    }

    /// The roster as a prompt reads it: one compact line per agent and per
    /// team under a `STAFF` heading that says how to name them, or the
    /// sentence for an empty roster. Descriptions are cut to their first
    /// sentence and 120 characters, because the whole roster rides every wake.
    pub fn render(&self) -> String {
        if self.is_empty() && self.scoped {
            return "STAFF — the person named the agents and teams that carry this goal, and \
                    none of them is enabled here now. Name nobody else: leave agent steps \
                    unassigned (they run on the step's harness), or declare an input of kind \
                    `assignee` and name it, and say in the goal's thread that the goal's \
                    agents and teams need enabling; you cannot install."
                .to_string();
        }
        if self.is_empty() {
            return "STAFF — nobody is installed and enabled here besides the platform's own \
                    agents. Leave agent steps unassigned (they run on the step's harness), or \
                    declare an input of kind `assignee` and name it; you cannot install."
                .to_string();
        }
        let mut out = String::from(if self.scoped {
            "STAFF — the person named who carries this goal; name one of these on every agent \
             step as {\"agent\": \"<id>\"} or {\"team\": \"<id>\"} — a team, or one of its \
             members. Nobody else may be named, though others are installed.\n"
        } else {
            "STAFF — installed and enabled here; name one on every agent step as \
             {\"agent\": \"<id>\"} or {\"team\": \"<id>\"}. Nobody else exists.\n"
        });
        out.push_str(&format!("Agents ({}):\n", self.agents.len()));
        for a in &self.agents {
            let mut facts: Vec<String> = vec![a.harness.clone()];
            if !a.skills.is_empty() {
                facts.push(format!(
                    "skills: {}",
                    a.skills
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !a.teams.is_empty() {
                facts.push(format!(
                    "teams: {}",
                    a.teams
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            out.push_str(&format!(
                "- {} \"{}\"{} [{}]\n",
                a.id,
                a.name,
                first_sentence(a.description.as_deref()),
                facts.join("; ")
            ));
        }
        out.push_str(&format!("Teams ({}):\n", self.teams.len()));
        for t in &self.teams {
            out.push_str(&format!(
                "- {} \"{}\"{} [members: {}{}]\n",
                t.id,
                t.name,
                first_sentence(t.purpose.as_deref()),
                if t.agents.is_empty() {
                    "no enabled agents".to_string()
                } else {
                    t.agents
                        .iter()
                        .map(|a| a.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                if t.humans > 0 {
                    format!(
                        "; {} person{}",
                        t.humans,
                        if t.humans == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                }
            ));
        }
        out
    }
}

/// ` — <first sentence>`, cut at 120 characters; nothing for no description.
fn first_sentence(text: Option<&str>) -> String {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return String::new();
    };
    let sentence = text
        .split_inclusive(['.', '!', '?'])
        .next()
        .unwrap_or(text)
        .trim();
    let cut: String = if sentence.chars().count() > 120 {
        let mut s: String = sentence.chars().take(117).collect();
        s.push('…');
        s
    } else {
        sentence.to_string()
    };
    format!(" — {cut}")
}

/// The agent steps that name nobody while there is somebody to name. Empty
/// when the roster is empty: with no staff installed, an unassigned agent step
/// is the only kind that can run (the executor falls back to the step's
/// harness). A step whose assignee is an input names somebody at run time.
pub fn unstaffed_steps<'a>(steps: &'a [Step], roster: &StaffRoster) -> Vec<&'a StepId> {
    if roster.is_empty() {
        return Vec::new();
    }
    steps
        .iter()
        .filter(|s| matches!(&s.kind, StepKind::Agent { assignee: None, .. }))
        .map(|s| &s.id)
        .collect()
}

/// The agent steps that name an assignee the roster does not hold — a fixed
/// agent or team that is not installed and enabled here, or, on a goal's
/// roster, one the goal does not name. An input-held assignee is checked
/// when the run starts, not here.
pub fn misstaffed_steps<'a>(steps: &'a [Step], roster: &StaffRoster) -> Vec<(&'a StepId, String)> {
    steps
        .iter()
        .filter_map(|s| match &s.kind {
            StepKind::Agent {
                assignee: Some(ValueRef::Fixed(who)),
                ..
            } if !roster.holds(who) => Some((&s.id, who.to_string())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{AgentOrigin, ModelPlan, Origin, PrincipalId, RespondPolicy, Tags};

    fn agent(id: &str, enabled: bool) -> Agent {
        Agent {
            id: AgentId::new(id).unwrap(),
            name: id.to_uppercase(),
            photo: None,
            description: Some(format!("Does {id} things. And more.")),
            system_prompt: "p".into(),
            harness: "claude-code".into(),
            models: ModelPlan::default(),
            skills: vec![SkillId::new("git-workflow").unwrap()],
            mcps: vec![],
            tags: Tags::default(),
            respond: RespondPolicy::default(),
            decision_making: false,
            pubkey: PrincipalId::new("ab".repeat(32)).unwrap(),
            origin: AgentOrigin::Local,
            enabled,
            created_at: 0,
        }
    }

    fn team(id: &str, members: &[&str], enabled: bool) -> Team {
        Team {
            id: TeamId::new(id).unwrap(),
            name: id.to_uppercase(),
            purpose: Some(format!("The {id} team")),
            photo: None,
            members: members
                .iter()
                .map(|m| Assignee::Agent(m.to_string()))
                .collect(),
            enabled,
            tags: Tags::default(),
            origin: Origin::Local,
            created_at: 0,
        }
    }

    #[test]
    fn from_parts_keeps_only_enabled_non_core_staff() {
        let agents = vec![
            agent("developer", true),
            agent("qa-engineer", false),
            agent(AgentId::WORKFLOW, true),
            agent(AgentId::GENERAL, true),
            agent("architect", true),
        ];
        let teams = vec![
            team(
                "engineering",
                &["developer", "qa-engineer", "architect", AgentId::GENERAL],
                true,
            ),
            team("design", &["developer"], false),
        ];
        let roster = StaffRoster::from_parts(&agents, &teams);
        let ids: Vec<&str> = roster.agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["architect", "developer"],
            "sorted, enabled, non-core"
        );
        assert_eq!(roster.teams.len(), 1);
        let eng = &roster.teams[0];
        assert_eq!(eng.id.as_str(), "engineering");
        assert_eq!(
            eng.agents.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
            vec!["developer", "architect"],
            "a disabled member and a core agent vanish from the team"
        );
        assert_eq!(
            roster.agents[1]
                .teams
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>(),
            vec!["engineering"]
        );
        assert!(roster.holds(&Assignee::Agent("developer".into())));
        assert!(!roster.holds(&Assignee::Agent("qa-engineer".into())));
        assert!(roster.holds(&Assignee::Team("engineering".into())));
        assert!(!roster.holds(&Assignee::Team("design".into())));
    }

    #[test]
    fn render_names_every_id_once_and_says_how_to_name_them() {
        let roster = StaffRoster::from_parts(
            &[agent("developer", true), agent("architect", true)],
            &[team("engineering", &["developer", "architect"], true)],
        );
        let text = roster.render();
        assert!(text.starts_with("STAFF"), "{text}");
        assert!(text.contains("{\"agent\": \"<id>\"}"), "{text}");
        assert!(text.contains("{\"team\": \"<id>\"}"), "{text}");
        assert_eq!(text.matches("- developer ").count(), 1, "{text}");
        assert_eq!(text.matches("- architect ").count(), 1, "{text}");
        assert!(text.contains("- engineering \"ENGINEERING\" — The engineering team [members: developer, architect]"), "{text}");
        assert!(
            text.contains("Does developer things."),
            "the first sentence of the description"
        );
        assert!(!text.contains("And more."), "and only the first sentence");
        assert!(text.contains("skills: git-workflow"), "{text}");
        assert!(text.contains("teams: engineering"), "{text}");
    }

    #[test]
    fn render_of_an_empty_roster_says_so() {
        let roster = StaffRoster::from_parts(&[agent(AgentId::WORKFLOW, true)], &[]);
        assert!(roster.is_empty());
        let text = roster.render();
        assert!(text.contains("nobody is installed"), "{text}");
        assert!(text.contains("kind `assignee`"), "{text}");
    }

    fn agent_step(id: &str, assignee: Option<ValueRef<Assignee>>) -> Step {
        Step {
            id: StepId::new(id).unwrap(),
            name: id.into(),
            kind: StepKind::Agent {
                instructions: "x".into(),
                assignee,
                project: None,
                harness: vec![],
                model: None,
                effort: None,
                output_schema: None,
                tier_ceiling: bisa_core::ToolTier::Write,
            },
            then: vec![],
            boundaries: vec![],
            join: bisa_core::Join::All,
            on_fail: bisa_core::OnFail::Fail,
            retries: 0,
            max_visits: 3,
            position: None,
        }
    }

    #[test]
    fn unstaffed_steps_is_empty_when_nobody_is_installed() {
        let steps = vec![agent_step("a", None)];
        assert!(unstaffed_steps(&steps, &StaffRoster::default()).is_empty());
        let roster = StaffRoster::from_parts(&[agent("developer", true)], &[]);
        assert_eq!(
            unstaffed_steps(&steps, &roster),
            vec![&StepId::new("a").unwrap()]
        );
    }

    #[test]
    fn unstaffed_steps_accepts_a_fixed_agent_a_team_or_an_input() {
        let roster = StaffRoster::from_parts(
            &[agent("developer", true)],
            &[team("engineering", &["developer"], true)],
        );
        let steps = vec![
            agent_step(
                "a",
                Some(ValueRef::Fixed(Assignee::Agent("developer".into()))),
            ),
            agent_step(
                "b",
                Some(ValueRef::Fixed(Assignee::Team("engineering".into()))),
            ),
            agent_step(
                "c",
                Some(ValueRef::Input {
                    input: bisa_core::InputName::new("who").unwrap(),
                }),
            ),
            agent_step("d", None),
            agent_step("e", Some(ValueRef::Fixed(Assignee::Agent("nobody".into())))),
        ];
        assert_eq!(
            unstaffed_steps(&steps, &roster),
            vec![&StepId::new("d").unwrap()]
        );
        let wrong = misstaffed_steps(&steps, &roster);
        assert_eq!(wrong.len(), 1);
        assert_eq!(wrong[0].0.as_str(), "e");
        assert_eq!(wrong[0].1, "agent:nobody");
    }

    fn roster() -> StaffRoster {
        StaffRoster::from_parts(
            &[
                agent("developer", true),
                agent("qa-engineer", true),
                agent("designer", true),
                agent("writer", true),
                agent("archived", false),
            ],
            &[
                team("mobile", &["developer", "qa-engineer"], true),
                team("brand", &["designer"], true),
                team("old", &["writer"], false),
            ],
        )
    }

    #[test]
    fn a_scope_that_names_no_agent_and_no_team_narrows_nothing() {
        let whole = roster();
        for scope in [
            vec![],
            vec![Assignee::Human(PrincipalId::new("cd".repeat(32)).unwrap())],
        ] {
            let same = roster().scoped_to(&scope);
            assert!(!same.scoped, "{scope:?}");
            assert_eq!(same.agents.len(), whole.agents.len());
            assert_eq!(same.teams.len(), whole.teams.len());
            assert_eq!(
                same.render(),
                whole.render(),
                "the whole staff, word for word"
            );
        }
    }

    #[test]
    fn a_goals_scope_keeps_its_agents_and_its_teams_with_their_members() {
        let scoped = roster().scoped_to(&[
            Assignee::Team("mobile".into()),
            Assignee::Agent("writer".into()),
            Assignee::Agent("archived".into()),
            Assignee::Team("old".into()),
            Assignee::Agent("nobody".into()),
        ]);
        assert!(scoped.scoped);
        assert_eq!(
            scoped
                .teams
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["mobile"],
            "a team stood down drops out"
        );
        assert_eq!(
            scoped.agents.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            vec!["developer", "qa-engineer", "writer"],
            "the team's members and the named agent — never a disabled or unknown one, never brand's designer"
        );
        assert!(scoped.holds(&Assignee::Team("mobile".into())));
        assert!(
            scoped.holds(&Assignee::Agent("developer".into())),
            "a member, named alone"
        );
        assert!(
            !scoped.holds(&Assignee::Agent("designer".into())),
            "installed, but not this goal's"
        );
        assert!(!scoped.holds(&Assignee::Team("brand".into())));
        let writer = scoped
            .agents
            .iter()
            .find(|a| a.id.as_str() == "writer")
            .unwrap();
        assert!(
            writer.teams.is_empty(),
            "the old team it was on is not this goal's"
        );
        assert_eq!(scoped.standing(), "3 agents and 1 team carry this goal");
        assert_eq!(
            scoped.outsider(),
            "not among the agents and teams that carry this goal"
        );
        let text = scoped.render();
        assert!(
            text.starts_with("STAFF — the person named who carries this goal"),
            "{text}"
        );
        assert!(
            text.contains("Nobody else may be named") && !text.contains("designer"),
            "{text}"
        );
    }

    #[test]
    fn a_goals_scope_with_nobody_left_says_so_and_asks_for_nobody_else() {
        let scoped = roster().scoped_to(&[
            Assignee::Agent("archived".into()),
            Assignee::Team("old".into()),
        ]);
        assert!(scoped.scoped && scoped.is_empty());
        let text = scoped.render();
        assert!(text.contains("none of them is enabled here now"), "{text}");
        assert!(text.contains("Name nobody else"), "{text}");
        let steps = vec![
            agent_step(
                "a",
                Some(ValueRef::Fixed(Assignee::Agent("designer".into()))),
            ),
            agent_step("b", None),
        ];
        assert!(
            unstaffed_steps(&steps, &scoped).is_empty(),
            "with nobody to name, an unassigned step is the one that can run"
        );
        assert_eq!(
            misstaffed_steps(&steps, &scoped).len(),
            1,
            "but someone the goal does not name is still refused"
        );
    }

    #[test]
    fn the_whole_staffs_words_are_unchanged() {
        let whole = roster();
        assert_eq!(whole.standing(), "4 agents and 2 teams are enabled here");
        assert_eq!(whole.outsider(), "not installed and enabled here");
        assert!(whole
            .render()
            .starts_with("STAFF — installed and enabled here; name one on every agent step"));
    }
}
