//! Gate governance: who may sign which gate.
//!
//! Truth lives in `governance.json` (no index table — one small policy
//! document, read on demand). Default: every gate is owner-only.
//!
//! Enforcement points:
//! - `Workspace::record_decision` — a local decision the policy forbids is
//!   refused before anything is journaled.
//! - `Workspace::record_run_event` — a `Decided` event on an `approval` step
//!   only moves the run if the decision it names is signed by someone who
//!   passes the policy.
//! - `ingest_remote_event` — a remote kind:3401 Decision from an unauthorized
//!   signer is rejected, and a kind:33413 run snapshot that records an
//!   `approval` step as done is only accepted once a policy-passing approving
//!   Decision for that step is already in the local journal.
//!
//! A `governance.json` written by the lifecycle-shaped code (with `commit` and
//! `acceptance` keys) is refused as `Unreadable` at open, not defaulted over
//!: a policy silently replaced by the default is a policy nobody
//! chose.

use crate::error::StoreError;
#[cfg(test)]
use crate::members::Admission;
use crate::workspace::Workspace;
use bisa_core::{Gate, MemberRole, PrincipalId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(
    deny_unknown_fields,
    rename_all = "snake_case",
    tag = "policy",
    content = "pubkeys"
)]
pub enum GatePolicy {
    /// Only the owner.
    Owner,
    /// The owner and every admin.
    Admins,
    /// The owner, every admin and every member — never a guest.
    Members,
    /// Exactly these entries: pubkey hex strings and/or `team:<id>` refs (a
    /// team ref expands to the team's human members at check time). A listed
    /// person still has to be a member who may decide gates.
    Listed(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Governance {
    /// An `approval` step, an adoption or an amendment of a workflow.
    #[serde(default = "owner_policy")]
    pub approval: GatePolicy,
    #[serde(default = "owner_policy")]
    pub escalation: GatePolicy,
    /// Pushing a branch or opening a PR leaves the machine, so it passes a
    /// gate of its own.
    #[serde(default = "owner_policy")]
    pub publish: GatePolicy,
}

fn owner_policy() -> GatePolicy {
    GatePolicy::Owner
}

impl Default for Governance {
    fn default() -> Self {
        Self {
            approval: GatePolicy::Owner,
            escalation: GatePolicy::Owner,
            publish: GatePolicy::Owner,
        }
    }
}

impl Governance {
    pub fn for_gate(&self, gate: Gate) -> &GatePolicy {
        match gate {
            Gate::Approval => &self.approval,
            Gate::Escalation => &self.escalation,
            Gate::Publish => &self.publish,
        }
    }

    fn set(&mut self, gate: Gate, policy: GatePolicy) {
        match gate {
            Gate::Approval => self.approval = policy,
            Gate::Escalation => self.escalation = policy,
            Gate::Publish => self.publish = policy,
        }
    }
}

impl Workspace {
    /// Current governance document: the defaults when the file is absent, a
    /// loud refusal when it is there and this build cannot read it.
    pub fn governance(&self) -> Result<Governance, StoreError> {
        let path = self.paths.governance_file();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::unreadable(&path, "governance document", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Governance::default()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    pub fn set_gate_policy(
        &self,
        gate: Gate,
        policy: GatePolicy,
    ) -> Result<Governance, StoreError> {
        let mut gov = self.governance()?;
        gov.set(gate, policy);
        crate::paths::write_atomic(
            &self.paths.governance_file(),
            &serde_json::to_vec_pretty(&gov)?,
        )?;
        Ok(gov)
    }

    /// The assignee union for a piece of work: **who is on this**.
    ///
    /// Precedence, nearest first — `projects` (the project a work item runs
    /// in, or every project attached to the goal), then the goal's own
    /// assignees, then each ancestor goal's, walking the `spawned` origin's
    /// parent. Work with no goal — a run of the workspace's — has its
    /// projects' alone. Teams expand in place; nothing appears twice. **The
    /// one implementation of the union**: the engine's `workers`/`approvers`
    /// are filters over this list.
    ///
    /// The parent walk carries a visited set: a goal can end up its own
    /// ancestor through a chain of spawns, and an unbounded loop inside a gate
    /// check would be a very bad place for one.
    pub fn assignees_for(
        &self,
        goal: Option<bisa_core::GoalId>,
        projects: &[bisa_core::ProjectId],
    ) -> Vec<bisa_core::Assignee> {
        use bisa_core::Assignee;
        let mut raw: Vec<Assignee> = Vec::new();
        for id in projects {
            match self.get_project(*id) {
                Ok(p) => raw.extend(p.assignees),
                Err(e) => tracing::warn!("project {id} unreadable while resolving assignees: {e}"),
            }
        }
        let mut visited: HashSet<bisa_core::GoalId> = HashSet::new();
        let mut cursor = goal;
        while let Some(current) = cursor {
            if !visited.insert(current) {
                tracing::warn!("goal {current}: parent chain cycles; stopping the assignee walk");
                break;
            }
            let Ok(g) = self.get_goal(current) else {
                break;
            };
            raw.extend(g.assignees.iter().cloned());
            cursor = g.origin.parent();
        }
        self.expand_assignees(raw)
    }

    /// Teams expanded to their members, duplicates dropped at their nearest
    /// position. A disabled team expands to nothing.
    pub fn expand_assignees(&self, raw: Vec<bisa_core::Assignee>) -> Vec<bisa_core::Assignee> {
        use bisa_core::Assignee;
        let mut out: Vec<Assignee> = Vec::with_capacity(raw.len());
        let mut seen: HashSet<Assignee> = HashSet::new();
        let mut push = |out: &mut Vec<Assignee>, a: Assignee| {
            if seen.insert(a.clone()) {
                out.push(a);
            }
        };
        for assignee in raw {
            match assignee {
                Assignee::Team(ref id) => {
                    let Ok(team_id) = bisa_core::TeamId::new(id) else {
                        continue;
                    };
                    let Ok(team) = self.get_team(&team_id) else {
                        continue;
                    };
                    if !team.enabled {
                        continue;
                    }
                    for agent in team.agent_ids() {
                        push(&mut out, Assignee::Agent(agent.to_string()));
                    }
                    for human in team.humans() {
                        push(&mut out, Assignee::Human(human.clone()));
                    }
                }
                other => push(&mut out, other),
            }
        }
        out
    }

    /// Humans who may decide a goal's gates by assignment: the union above,
    /// over every project attached to the goal, filtered to people.
    pub fn assignee_approvers(&self, goal: bisa_core::GoalId) -> Vec<PrincipalId> {
        let projects: Vec<bisa_core::ProjectId> = self
            .projects_for(goal)
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.id)
            .collect();
        self.assignees_for(Some(goal), &projects)
            .into_iter()
            .filter_map(|a| match a {
                bisa_core::Assignee::Human(pk) => Some(pk),
                _ => None,
            })
            .collect()
    }

    /// May `signer` decide `gate` for `goal` under the current policy?
    ///
    /// 1. An **explicitly configured** policy is authoritative.
    /// 2. On the default `Owner` policy, the humans of the goal's assignee
    ///    union may decide **in addition to** the owner — except for
    ///    `Publish`, which never defers to an assignment
    ///    ([`Gate::defers_to_assignment`]): approving it spends the owner's
    ///    credentials.
    /// 3. Otherwise, owner-only.
    pub fn gate_policy_allows_for(
        &self,
        gate: Gate,
        signer: &PrincipalId,
        goal: Option<bisa_core::GoalId>,
    ) -> Result<bool, StoreError> {
        if self.gate_policy_allows(gate, signer)? {
            return Ok(true);
        }
        if !gate.defers_to_assignment() {
            return Ok(false);
        }
        if !matches!(self.governance()?.for_gate(gate), GatePolicy::Owner) {
            return Ok(false);
        }
        let Some(goal_id) = goal else {
            return Ok(false);
        };
        Ok(self.assignee_approvers(goal_id).contains(signer))
    }

    /// May `signer` decide `gate` under the stored policy alone?
    ///
    /// A role decides what a policy word reaches: `Admins` is the owner and
    /// the admins, `Members` adds the members; a guest decides no gate
    /// whatever the policy says, and a listed pubkey counts only while it is
    /// a member whose role may decide gates.
    pub fn gate_policy_allows(&self, gate: Gate, signer: &PrincipalId) -> Result<bool, StoreError> {
        let role = self.member_role(signer)?;
        let may_decide = role.is_some_and(|r| r.may(bisa_core::Permission::DecideGates));
        match self.governance()?.for_gate(gate) {
            GatePolicy::Owner => Ok(role == Some(MemberRole::Owner)),
            GatePolicy::Admins => Ok(matches!(role, Some(MemberRole::Owner | MemberRole::Admin))),
            GatePolicy::Members => Ok(may_decide),
            GatePolicy::Listed(list) => {
                if !may_decide {
                    return Ok(false);
                }
                for entry in list {
                    if let Some(team_id) = entry.strip_prefix("team:") {
                        let Ok(team_id) = bisa_core::TeamId::new(team_id) else {
                            continue;
                        };
                        match self.team_humans(&team_id) {
                            Ok(humans) if humans.contains(signer) => return Ok(true),
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!("governance: team ref {entry:?} unresolvable: {e}")
                            }
                        }
                    } else if entry == signer.as_hex() {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::workspace::NewGoal;
    use nostr::key::Keys;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    #[test]
    fn defaults_crud_and_policy_checks() {
        let (_dir, ws) = ws();
        assert_eq!(ws.governance().unwrap(), Governance::default());
        let owner = ws.owner_principal();
        let stranger = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        let member = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        let admin = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        let guest = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        ws.add_member(member.clone(), MemberRole::Member, Admission::default())
            .unwrap();
        ws.add_member(admin.clone(), MemberRole::Admin, Admission::default())
            .unwrap();
        ws.add_member(guest.clone(), MemberRole::Guest, Admission::default())
            .unwrap();
        assert!(ws.gate_policy_allows(Gate::Approval, &owner).unwrap());
        assert!(!ws.gate_policy_allows(Gate::Approval, &member).unwrap());
        assert!(!ws.gate_policy_allows(Gate::Approval, &admin).unwrap());
        ws.set_gate_policy(Gate::Approval, GatePolicy::Admins)
            .unwrap();
        assert!(ws.gate_policy_allows(Gate::Approval, &admin).unwrap());
        assert!(!ws.gate_policy_allows(Gate::Approval, &member).unwrap());
        ws.set_gate_policy(Gate::Approval, GatePolicy::Members)
            .unwrap();
        assert!(ws.gate_policy_allows(Gate::Approval, &member).unwrap());
        assert!(ws.gate_policy_allows(Gate::Approval, &admin).unwrap());
        assert!(
            !ws.gate_policy_allows(Gate::Approval, &guest).unwrap(),
            "a guest decides no gate"
        );
        assert!(!ws.gate_policy_allows(Gate::Approval, &stranger).unwrap());
        ws.set_gate_policy(
            Gate::Escalation,
            GatePolicy::Listed(vec![
                member.as_hex().to_string(),
                guest.as_hex().to_string(),
            ]),
        )
        .unwrap();
        assert!(!ws.gate_policy_allows(Gate::Escalation, &owner).unwrap());
        assert!(ws.gate_policy_allows(Gate::Escalation, &member).unwrap());
        assert!(
            !ws.gate_policy_allows(Gate::Escalation, &guest).unwrap(),
            "listed, still a guest"
        );
    }

    #[test]
    fn an_assigned_human_may_decide_every_gate_but_publish() {
        let (_dir, ws) = ws();
        let human = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        let goal = ws.create_goal(NewGoal::captured("delegated")).unwrap();
        ws.set_goal_assignees(goal.id, vec![bisa_core::Assignee::Human(human.clone())])
            .unwrap();
        assert!(ws
            .gate_policy_allows_for(Gate::Approval, &human, Some(goal.id))
            .unwrap());
        assert!(!ws
            .gate_policy_allows_for(Gate::Publish, &human, Some(goal.id))
            .unwrap());
        // An explicit policy is never widened by an assignment.
        ws.set_gate_policy(Gate::Approval, GatePolicy::Listed(vec![]))
            .unwrap();
        assert!(!ws
            .gate_policy_allows_for(Gate::Approval, &human, Some(goal.id))
            .unwrap());
    }

    #[test]
    fn record_decision_enforces_policy_and_persists() {
        let (dir, ws) = ws();
        let collaborator = PrincipalId::new(Keys::generate().public_key().to_hex()).unwrap();
        ws.set_gate_policy(
            Gate::Approval,
            GatePolicy::Listed(vec![collaborator.as_hex().to_string()]),
        )
        .unwrap();
        let goal = ws.create_goal(NewGoal::captured("governed")).unwrap();
        let err = ws.record_decision(
            &bisa_core::Home::Goal { goal: goal.id },
            Gate::Approval,
            true,
            "adopt",
            None,
            None,
        );
        assert!(matches!(err, Err(StoreError::GatePolicy(_))), "{err:?}");
        drop(ws);
        let ws2 =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        assert!(matches!(
            ws2.governance().unwrap().approval,
            GatePolicy::Listed(ref l) if l.len() == 1
        ));
    }

    #[test]
    fn an_old_governance_file_is_refused_as_unreadable() {
        let (_dir, ws) = ws();
        crate::paths::write_atomic(
            &ws.paths().governance_file(),
            br#"{"commit":{"policy":"owner"},"acceptance":{"policy":"owner"},"escalation":{"policy":"owner"},"publish":{"policy":"owner"}}"#,
        )
        .unwrap();
        match ws.governance() {
            Err(StoreError::Unreadable { what, .. }) => assert_eq!(what, "governance document"),
            other => panic!("expected Unreadable, got {other:?}"),
        }
    }
}
