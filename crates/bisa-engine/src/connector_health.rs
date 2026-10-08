//! What this machine's connector accounts answered when last checked (03 §
//! Connectors): in memory, per account, for this engine's lifetime — a
//! health is a fact about a moment, not a record — and a check on ask alone:
//! no schedule, no request at start. One check per account at a time; a
//! second ask while one runs joins it rather than making a second request as
//! the same account. A change to the account's secrets or parameters, to its
//! definition, or the catalog's refresh of that definition forgets the last
//! answer — what *Check* said was about the way in the account had then.
//!
//! The request itself is [`crate::connectors::check_account`]'s, under its
//! budget; this module owns the rules around it: a connector that names no
//! check operation is answered in words and nothing is kept or announced,
//! the bus hears every finished check (`connectors.checked`), and a check
//! whose caller went away before it answered — a closed window, a deadline
//! above the call — leaves no slot behind for the next ask to wait on. A
//! verb beside a running node cannot leave this stale: an embedded engine
//! needs the engine lock the node holds.

use crate::connectors::{self, AccountCheck, AccountCheckState};
use crate::events::{EngineEvent, EnginePayload};
use crate::{EngineError, Inner};
use bisa_core::sync::Locked;
use bisa_core::{AccountId, ConnectorId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::watch;

/// What is known about one account: the last check and when it came.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct LastCheck {
    pub check: AccountCheck,
    pub checked_at: u64,
}

/// Where an account's health stands: nothing yet, fine, or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountHealthState {
    Unknown,
    Ok,
    Failing,
}

impl AccountHealthState {
    /// The word the wire and the command line say.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Ok => "ok",
            Self::Failing => "failing",
        }
    }
}

/// An account's health as the node answers it on every account row: nothing
/// yet, fine, or not — with what the last check found. Read back as well as
/// written: the command line takes the node's rows whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountHealthView {
    pub state: AccountHealthState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<u64>,
    /// What the last check found, in its own word.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<AccountCheckState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl AccountHealthView {
    pub fn unknown() -> Self {
        Self {
            state: AccountHealthState::Unknown,
            checked_at: None,
            check: None,
            status: None,
            reason: None,
        }
    }

    pub fn of(last: &LastCheck) -> Self {
        let c = &last.check;
        Self {
            state: if c.state == AccountCheckState::Connected {
                AccountHealthState::Ok
            } else {
                AccountHealthState::Failing
            },
            checked_at: Some(last.checked_at),
            check: Some(c.state),
            status: c.status,
            reason: c.reason.clone(),
        }
    }
}

/// The sentence a joiner answers with when the check it joined went away
/// before it answered.
pub const DROPPED: &str = "the check was dropped before it answered; check again";

type Key = (ConnectorId, AccountId);

enum Slot {
    /// A check is running; the receiver resolves when it finishes.
    Running(watch::Receiver<Option<AccountCheck>>),
    /// Boxed: a check is many times the receiver's size, and a map of slots
    /// should not pay the largest variant's price for every account.
    Done(Box<LastCheck>),
}

#[derive(Default)]
pub struct ConnectorHealth {
    slots: Mutex<HashMap<Key, Slot>>,
}

/// The runner's hold on its slot: let go without an answer — the future
/// cancelled mid-check — it takes the `Running` slot away, so the next ask
/// becomes the runner instead of waiting on nobody.
struct Runner<'a> {
    slots: &'a Mutex<HashMap<Key, Slot>>,
    key: Key,
    done: bool,
}

impl Drop for Runner<'_> {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        let mut slots = self.slots.locked();
        if matches!(slots.get(&self.key), Some(Slot::Running(_))) {
            slots.remove(&self.key);
        }
    }
}

impl ConnectorHealth {
    /// The last answer for an account, if one was ever asked for.
    pub fn health_of(&self, connector: &ConnectorId, account: AccountId) -> Option<LastCheck> {
        match self.slots.locked().get(&(connector.clone(), account)) {
            Some(Slot::Done(last)) => Some(last.as_ref().clone()),
            _ => None,
        }
    }

    /// As the node answers it on the account's row.
    pub fn view_of(&self, connector: &ConnectorId, account: AccountId) -> AccountHealthView {
        self.health_of(connector, account)
            .map(|l| AccountHealthView::of(&l))
            .unwrap_or_else(AccountHealthView::unknown)
    }

    /// Forget what an account answered — its secrets or parameters moved,
    /// it was connected again, or it is gone.
    pub fn invalidate(&self, connector: &ConnectorId, account: AccountId) {
        self.slots.locked().remove(&(connector.clone(), account));
    }

    /// Forget every account of a connector — its definition moved or is gone.
    pub fn invalidate_connector(&self, connector: &ConnectorId) {
        self.slots.locked().retain(|(c, _), _| c != connector);
    }

    /// Check an account, keep the answer, and tell the bus. A check already
    /// running for the account is joined, not doubled; a connector that
    /// names no check is answered in words, kept nowhere and told to nobody.
    pub async fn check(
        &self,
        inner: &Inner,
        connector: &ConnectorId,
        account: AccountId,
        budget_secs: Option<u64>,
    ) -> Result<AccountCheck, EngineError> {
        // The records first: an unknown connector or account is the store's
        // refusal, and leaves no slot behind.
        let def = inner.ws.get_connector(connector)?;
        inner.ws.get_connector_account(connector, account)?;
        let budget = connectors::check_budget(budget_secs);
        if def
            .check
            .as_ref()
            .and_then(|id| def.operation(id))
            .is_none()
        {
            return connectors::check_account(inner, connector, account, budget).await;
        }
        let key = (connector.clone(), account);
        // Join a check already running for this account, or become its
        // runner. A runner that went away without an answer — its sender
        // gone — is taken over, never joined. The decision is taken under
        // the lock and the lock is let go before any await.
        let role = {
            let mut slots = self.slots.locked();
            match slots.get(&key) {
                Some(Slot::Running(rx)) if rx.has_changed().is_ok() => Err(rx.clone()),
                _ => {
                    let (tx, rx) = watch::channel(None);
                    slots.insert(key.clone(), Slot::Running(rx));
                    Ok(tx)
                }
            }
        };
        let mut rx = match role {
            Ok(tx) => {
                let mut runner = Runner {
                    slots: &self.slots,
                    key: key.clone(),
                    done: false,
                };
                let check = connectors::check_account(inner, connector, account, budget).await?;
                let last = LastCheck {
                    check: check.clone(),
                    checked_at: now_secs(),
                };
                self.slots.locked().insert(key, Slot::Done(Box::new(last)));
                runner.done = true;
                // A joiner that stopped listening is not a fault.
                if tx.send(Some(check.clone())).is_err() {
                    tracing::debug!(target: "bisa_engine::connector_health", %connector, %account, "nobody waited for the check");
                }
                let ok = check.state == AccountCheckState::Connected;
                tracing::info!(
                    target: "bisa_engine::connector_health",
                    %connector,
                    %account,
                    state = check.state.as_str(),
                    status = ?check.status,
                    "connector account checked"
                );
                inner.emit(EngineEvent::global(EnginePayload::ConnectorChecked {
                    connector: connector.clone(),
                    account,
                    ok,
                }));
                return Ok(check);
            }
            Err(rx) => rx,
        };
        loop {
            if let Some(c) = rx.borrow().clone() {
                return Ok(c);
            }
            if rx.changed().await.is_err() {
                // The runner went away without an answer: read what it kept.
                return Ok(self
                    .health_of(connector, account)
                    .map(|l| l.check)
                    .unwrap_or_else(|| AccountCheck {
                        state: AccountCheckState::Unreachable,
                        status: None,
                        reason: Some(DROPPED.into()),
                    }));
            }
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
