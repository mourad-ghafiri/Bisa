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
