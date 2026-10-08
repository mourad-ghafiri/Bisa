//! How the store's refusals are said to a person: one `Text` per variant
//! (`error-store-…` in `locales/en/errors.ftl`); a wrapped domain error says itself.

use bisa_core::{Localize, Text};

impl Localize for crate::error::StoreError {
    fn text(&self) -> Text {
        match self {
            crate::error::StoreError::Io { path, source, .. } => bisa_core::text!(
                "error-store-io",
                path = path.to_string(),
                source = source.to_string()
            ),
            crate::error::StoreError::KeyStore(v0) => {
                bisa_core::text!("error-store-key-store", v0 = v0.to_string())
            }
            crate::error::StoreError::KeyNotFound(v0) => {
                bisa_core::text!("error-store-key-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::Nostr(v0) => {
                bisa_core::text!("error-store-nostr", v0 = v0.to_string())
            }
            crate::error::StoreError::EncryptionRequired(v0) => {
                bisa_core::text!("error-store-encryption-required", v0 = *v0)
            }
            crate::error::StoreError::Sqlite(v0) => {
                bisa_core::text!("error-store-sqlite", v0 = v0.to_string())
            }
            crate::error::StoreError::Serde(v0) => {
                bisa_core::text!("error-store-serde", v0 = v0.to_string())
            }
            crate::error::StoreError::GoalNotFound(v0) => {
                bisa_core::text!("error-store-goal-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::WorkItemNotFound(v0) => {
                bisa_core::text!("error-store-work-item-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::SessionNotFound(v0) => {
                bisa_core::text!("error-store-session-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::WorkstreamNotFound(v0) => {
                bisa_core::text!("error-store-workstream-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::ProjectNotFound(v0) => {
                bisa_core::text!("error-store-project-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::ConversationNotFound(v0) => {
                bisa_core::text!("error-store-conversation-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::WorkflowNotFound(v0) => {
                bisa_core::text!("error-store-workflow-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::RunNotFound(v0) => {
                bisa_core::text!("error-store-run-not-found", v0 = v0.to_string())
            }
            crate::error::StoreError::DefinitionNotFound { kind, id, .. } => bisa_core::text!(
                "error-store-definition-not-found",
                kind = kind.to_string(),
                id = id.to_string()
            ),
            crate::error::StoreError::RunNotFinished { goal, run, .. } => bisa_core::text!(
                "error-store-run-not-finished",
                goal = goal.to_string(),
                run = run.to_string()
            ),
            crate::error::StoreError::RunNotQueued { goal, run, .. } => bisa_core::text!(
                "error-store-run-not-queued",
                run = run.to_string(),
                goal = goal.to_string()
            ),
            crate::error::StoreError::AlreadyDispatched { signal, run, .. } => bisa_core::text!(
                "error-store-already-dispatched",
                signal = signal.to_string(),
                run = run.to_string()
            ),
            crate::error::StoreError::WorkflowInvalid(v0) => bisa_core::text!(
                "error-store-workflow-invalid",
                a0 = (crate::error::describe_problems(v0)).to_string()
            ),
            crate::error::StoreError::RevisionConflict {
                kind,
                id,
                expected,
                actual,
                ..
            } => bisa_core::text!(
                "error-store-revision-conflict",
                kind = kind.to_string(),
                id = id.to_string(),
                actual = *actual,
                expected = *expected
            ),
            crate::error::StoreError::AlreadyLibrary(v0) => {
                bisa_core::text!("error-store-already-library", v0 = v0.to_string())
            }
            crate::error::StoreError::StillUsed(t) => t.clone(),
            crate::error::StoreError::Inputs(inner) => inner.text(),
            crate::error::StoreError::Unreadable {
                path, what, reason, ..
            } => bisa_core::text!(
                "error-store-unreadable",
                path = path.to_string(),
                what = what.to_string(),
                reason = reason.to_string()
            ),
            crate::error::StoreError::OwnerKeyUnreadable { path, reason } => bisa_core::text!(
                "error-store-owner-key-unreadable",
                path = path.to_string(),
                reason = reason.to_string()
            ),
            crate::error::StoreError::Run(inner) => inner.text(),
            crate::error::StoreError::WorkItem(inner) => inner.text(),
            crate::error::StoreError::Workstream(inner) => inner.text(),
            crate::error::StoreError::Channel(inner) => inner.text(),
            crate::error::StoreError::Agent(inner) => inner.text(),
            crate::error::StoreError::Team(inner) => inner.text(),
            crate::error::StoreError::Settings(inner) => inner.text(),
            crate::error::StoreError::Core(inner) => inner.text(),
            crate::error::StoreError::GateDecisionInvalid(v0) => {
                bisa_core::text!("error-store-gate-decision-invalid", v0 = v0.to_string())
            }
            crate::error::StoreError::GatePolicy(t) => t.clone(),
            crate::error::StoreError::BudgetExhausted(v0) => {
                bisa_core::text!("error-store-budget-exhausted", v0 = v0.to_string())
            }
            crate::error::StoreError::RecallConflict { current_hash, .. } => bisa_core::text!(
                "error-store-recall-conflict",
                current_hash = current_hash.to_string()
            ),
            crate::error::StoreError::EditConflict {
                what, current_hash, ..
            } => bisa_core::text!(
                "error-store-edit-conflict",
                what = what.to_string(),
                current_hash = current_hash.to_string()
            ),
            crate::error::StoreError::Invalid(t) => t.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::StoreError;
    use bisa_core::text::plain;
    use bisa_core::{
        AgentError, ChannelError, CoreError, InputError, Localize, RunError, SettingsError,
        TeamError, WorkItemError, WorkstreamError,
    };

    /// Ids whose English catalog sentence says the same thing in other
    /// words than `Display`, on purpose — none yet.
    const WORDED_ON_PURPOSE: &[(&str, &str)] = &[];

    fn judge(e: &StoreError, off: &mut Vec<String>) {
        let text = e.text();
        let id = text.id.to_string();
        assert!(plain::has(&id), "{id} is a message of the English catalog");
        let said = text.to_string();
        let shown = e.to_string();
        if let Some((_, carries)) = WORDED_ON_PURPOSE.iter().find(|(w, _)| *w == id) {
            assert!(said.contains(carries), "{id}: {said:?} carries {carries:?}");
            return;
        }
        if said != shown {
            off.push(format!("{id}:\n  catalog: {said:?}\n  display: {shown:?}"));
        }
    }

    fn words() -> bisa_core::Text {
        bisa_core::text!("error-store-goal-not-found", v0 = "g")
    }

    #[test]
    fn every_refusal_says_in_the_catalog_what_its_display_says() {
        let mut off = Vec::new();
        let every = vec![
            StoreError::io("a/b", std::io::Error::other("no")),
            StoreError::KeyStore("k".into()),
            StoreError::KeyNotFound("k".into()),
            StoreError::Nostr("n".into()),
            StoreError::EncryptionRequired(33404),
            StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows),
            StoreError::Serde(serde_json::from_str::<u8>("x").unwrap_err()),
            StoreError::GoalNotFound("g".into()),
            StoreError::WorkItemNotFound("w".into()),
            StoreError::SessionNotFound("s".into()),
            StoreError::WorkstreamNotFound("w".into()),
            StoreError::ProjectNotFound("p".into()),
            StoreError::ConversationNotFound("c".into()),
            StoreError::WorkflowNotFound("w".into()),
            StoreError::RunNotFound("r".into()),
            StoreError::DefinitionNotFound {
                kind: "addon",
                id: "x".into(),
            },
            StoreError::RunNotFinished {
                goal: "g".into(),
                run: "r".into(),
            },
            StoreError::RunNotQueued {
                goal: "g".into(),
                run: "r".into(),
            },
            StoreError::AlreadyDispatched {
                signal: "s".into(),
                run: "r".into(),
            },
            StoreError::WorkflowInvalid(vec![]),
            StoreError::RevisionConflict {
                kind: "workflow",
                id: "w".into(),
                expected: 1,
                actual: 2,
            },
            StoreError::AlreadyLibrary("w".into()),
            StoreError::StillUsed(words()),
            StoreError::Inputs(InputError::Missing { input: "n".into() }),
            StoreError::Unreadable {
                path: "a/b".into(),
                what: "goal",
                reason: "torn".into(),
            },
            StoreError::OwnerKeyUnreadable {
                path: "a/b".into(),
                reason: "torn".into(),
            },
            StoreError::Run(RunError::NotStarted),
            StoreError::WorkItem(WorkItemError::Terminal("accepted")),
            StoreError::Workstream(WorkstreamError::Terminal),
            StoreError::Channel(ChannelError::DirectNeedsAudience),
            StoreError::Agent(AgentError::EmptyName),
            StoreError::Team(TeamError::EmptyName),
            StoreError::Settings(SettingsError::UnknownKey("x".into())),
            StoreError::Core(CoreError::InvalidPrincipal("p".into())),
            StoreError::GateDecisionInvalid("d".into()),
            StoreError::GatePolicy(words()),
            StoreError::BudgetExhausted("g".into()),
            StoreError::RecallConflict {
                current_value: "v".into(),
                current_hash: "h".into(),
            },
            StoreError::EditConflict {
                what: "note",
                current: serde_json::Value::Null,
                current_hash: "h".into(),
            },
            StoreError::Invalid(words()),
        ];
        for e in &every {
            judge(e, &mut off);
        }
        assert!(off.is_empty(), "{}", off.join("\n"));
    }

    #[test]
    fn a_core_error_lands_in_its_own_arm_and_every_other_in_core() {
        assert!(matches!(
            StoreError::from(CoreError::Run(RunError::NotStarted)),
            StoreError::Run(_)
        ));
        assert!(matches!(
            StoreError::from(CoreError::WorkItem(WorkItemError::Terminal("accepted"))),
            StoreError::WorkItem(_)
        ));
        assert!(matches!(
            StoreError::from(CoreError::Workstream(WorkstreamError::Terminal)),
            StoreError::Workstream(_)
        ));
        assert!(matches!(
            StoreError::from(CoreError::Channel(ChannelError::DirectNeedsAudience)),
            StoreError::Channel(_)
        ));
        assert!(matches!(
            StoreError::from(CoreError::Agent(AgentError::EmptyName)),
            StoreError::Agent(_)
        ));
        assert!(matches!(
            StoreError::from(CoreError::Team(TeamError::EmptyName)),
            StoreError::Team(_)
        ));
        assert!(matches!(
            StoreError::from(CoreError::Settings(SettingsError::UnknownKey("x".into()))),
            StoreError::Settings(_)
        ));
        assert!(matches!(
            StoreError::from(CoreError::InvalidPrincipal("p".into())),
            StoreError::Core(CoreError::InvalidPrincipal(_))
        ));
        assert!(StoreError::from(CoreError::InvalidPrincipal("p".into())).is_refusal());
        assert!(!StoreError::KeyStore("k".into()).is_refusal());
    }
}
