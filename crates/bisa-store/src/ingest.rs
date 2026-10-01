//! Remote-event ingest: how another node's signed events enter this
//! workspace.
//!
//! Every event is verified (Schnorr), deduped (O(1) via `seen_events`, truth in
//! `seen.jsonl`), **admitted by role** (14-collaboration) — this node's own
//! key and its own attested agents write anything; a hosted member's own key
//! writes a human's acts and nothing else, an admin's the room's channels
//! too; a member's agents never write here — then routed by kind: journal
//! facts append verbatim (never re-signed), addressable state applies
//! latest-wins across authors, ephemeral kinds are refused. Ingested events
//! never re-emit as a local write on the store-event bus — that would echo
//! loops; a hosted member's message is said once, as its own event, so the
//! engine may read it before an agent does.
//!
//! **A run snapshot is held to the same rule a local decision is.** A remote
//! kind:33413 that records an `approval` step as done is admitted only once a
//! policy-passing approving Decision for that very step is already in the
//! local journal ([`run_snapshot_admissible`]); until then it is rejected and
//! not marked seen, so the retry after the decision lands succeeds
//! (self-healing ordering). A goal snapshot may not reopen a goal this node
//! holds closed ([`remote_goal_admissible`]) — that one is refused for good.

use crate::error::StoreError;
use crate::journal::{JournalAddr, JsonlEventLog};
use crate::paths::Paths;
use crate::runs::approval_subject;
use crate::workspace::Workspace;
use bisa_core::event::JournalPayload;
use bisa_core::kind;
use bisa_core::workitem::WorkItemSpec;
use bisa_core::{Gate, Goal, Home, StepId, StepKind, StepState, Workflow, WorkflowRun};
use nostr::event::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub enum IngestOutcome {
    /// A fact appended to a home's journal: a goal's, or a run of the
    /// workspace's.
    AppliedJournal {
        home: Home,
    },
    /// A snapshot filed under a home applied.
    AppliedSnapshot {
        kind: u16,
        home: Home,
    },
    /// A conversation fact or a non-goal snapshot applied to a scope.
    AppliedConversation {
        scope: String,
    },
    Duplicate,
    Rejected {
        reason: String,
    },
}

#[derive(Serialize, Deserialize)]
struct SeenLine {
    id: String,
    at: u64,
}

/// How long a seen id is remembered. A replay older than this is caught by
/// the snapshot's own latest-wins rule or by the journal's append-only shape;
/// what the set protects against is the near-term echo.
pub const SEEN_RETENTION_SECS: u64 = 90 * 24 * 3600;

fn tag_value<'a>(event: &'a Event, name: &str) -> Option<&'a str> {
    event.tags.iter().find_map(|t| {
        let s = t.as_slice();
        (s.len() >= 2 && s[0] == name).then(|| s[1].as_str())
    })
}

/// Parse any `<kind>:<pubkey>:<d>` coordinate from the `a` tag.
fn scope_coord(event: &Event) -> Option<(u16, String)> {
    let coord = tag_value(event, "a")?;
    let mut parts = coord.splitn(3, ':');
    let k: u16 = parts.next()?.parse().ok()?;
    let _pubkey = parts.next()?;
    let d = parts.next()?;
    Some((k, d.to_string()))
}

/// Parse a journal's address from the `a` tag: a goal's
/// (`33400:<pubkey>:<goal>`), or a run of the workspace's
/// (`33413:<pubkey>:<run>`).
fn journal_addr_of(event: &Event) -> Option<JournalAddr> {
    let coord = tag_value(event, "a")?;
    let mut parts = coord.splitn(3, ':');
    let k: u16 = parts.next()?.parse().ok()?;
    let pubkey = parts.next()?;
    let d = parts.next()?;
    let home = match k {
        kind::KIND_GOAL => Home::Goal {
            goal: d.parse().ok()?,
        },
        kind::KIND_WORKFLOW_RUN => Home::Run {
            run: d.parse().ok()?,
        },
        _ => return None,
    };
    Some(JournalAddr {
        author_pubkey_hex: pubkey.to_string(),
        home,
    })
}

/// What a hosted member's own key may write, by role: a human's acts, and
/// the room's channels for an admin.
fn hosted_may_write(role: bisa_core::MemberRole, wire_kind: u16) -> bool {
    kind::is_human_kind(wire_kind)
        || (role.may(bisa_core::Permission::ManageChannels) && kind::is_manager_kind(wire_kind))
}

fn reject(reason: impl Into<String>) -> IngestOutcome {
    IngestOutcome::Rejected {
        reason: reason.into(),
    }
}

/// May a known goal at `local` be replaced by the snapshot `remote`?
///
/// A goal has no lifecycle to walk, so the rule is about identity and the one
/// move a goal makes on its own: the author, the origin and the birth are
/// fixed, and a goal this node holds closed never reopens — a peer that
/// wants the work done again spawns a new goal.
pub fn remote_goal_admissible(local: &Goal, remote: &Goal) -> Result<(), String> {
    if local.author != remote.author {
        return Err("a goal's author never changes".into());
    }
    if local.origin != remote.origin {
        return Err("a goal's origin is recorded at capture and never changes".into());
    }
    if local.created_at != remote.created_at {
        return Err("a goal's birth never changes".into());
    }
    if local.is_closed() && !remote.is_closed() {
        return Err("a closed goal never reopens".into());
    }
    Ok(())
}

/// May a run snapshot `remote` be admitted beside `local` (the copy this node
/// holds, if any)?
///
/// `decided(step)` answers whether the local journal holds an authorised
/// approving Decision for that `approval` step. The outer `Result` is the
/// store's; the inner is the admissibility verdict: `Err(reason)` is a
/// rejection, and the caller decides whether it is final (marked seen) or a
/// retry (a decision may still arrive).
pub fn run_snapshot_admissible(
    local: Option<&WorkflowRun>,
    remote: &WorkflowRun,
    mut decided: impl FnMut(&StepId) -> Result<bool, StoreError>,
) -> Result<Result<(), Admissibility>, StoreError> {
    if let Some(local) = local {
        if local.scope != remote.scope {
            return Ok(Err(Admissibility::Final(
                "a run never changes scope".into(),
            )));
        }
        if local.workflow.id != remote.workflow.id {
            return Ok(Err(Admissibility::Final(
                "a run never changes the workflow it is a run of".into(),
            )));
        }
        if local.queued_at != remote.queued_at {
            return Ok(Err(Admissibility::Final(
                "a run's making never changes".into(),
            )));
        }
        if local.started_at.is_some() && local.started_at != remote.started_at {
            return Ok(Err(Admissibility::Final(
                "a run's start never changes".into(),
            )));
        }
        // A queued run by hand learns its entry when it starts.
        if local.start.is_some() && local.start != remote.start {
            return Ok(Err(Admissibility::Final(
                "a run's entry never changes".into(),
            )));
        }
        if local.event != remote.event {
            return Ok(Err(Admissibility::Final(
                "the event a run began on never changes".into(),
            )));
        }
        if local.dispatched != remote.dispatched {
            return Ok(Err(Admissibility::Final(
                "the signal that made a run never changes".into(),
            )));
        }
        if local.revision >= remote.revision {
            return Ok(Err(Admissibility::Final("stale".into())));
        }
    }
    for step in &remote.workflow.steps {
        if !matches!(step.kind, StepKind::Approval { .. }) {
            continue;
        }
        let Some(record) = remote.steps.get(&step.id) else {
            continue;
        };
        // Only a passed approval needs a decision: one a boundary diverted
        // passed nothing, and its flows are the timeout's, not the gate's.
        if !matches!(record.state, StepState::Done { .. }) {
            continue;
        }
        let already = local
            .and_then(|l| l.steps.get(&step.id))
            .is_some_and(|r| matches!(r.state, StepState::Done { .. }));
        if already {
            continue;
        }
        if !decided(&step.id)? {
            return Ok(Err(Admissibility::Retry(format!(
                "gate policy: run passes approval step `{}` without an authorised decision in the journal",
                step.id
            ))));
        }
    }
    Ok(Ok(()))
}

/// Why a run snapshot was not admitted, and whether asking again could help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admissibility {
    /// Refused for good; the event is marked seen.
    Final(String),
    /// Refused for now; the decision it needs may still arrive.
    Retry(String),
}

impl Workspace {
    pub(crate) fn mark_seen(&self, event: &Event) -> Result<(), StoreError> {
        let line = serde_json::to_string(&SeenLine {
            id: event.id.to_hex(),
            at: event.created_at.as_secs(),
        })?;
        crate::paths::append_line(&self.paths.seen_file(), &line)?;
        self.idx()
            .mark_seen(&event.id.to_hex(), event.created_at.as_secs())
    }

    /// Retention: forget seen ids older than [`SEEN_RETENTION_SECS`] and
    /// rewrite the truth file to match. Returns how many were pruned.
    pub fn prune_seen(&self, now: u64) -> Result<usize, StoreError> {
        let cutoff = now.saturating_sub(SEEN_RETENTION_SECS);
        let pruned = self.idx().prune_seen_before(cutoff)?;
        if pruned == 0 {
            return Ok(0);
        }
        let path = self.paths.seen_file();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(pruned),
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        };
        let kept: Vec<&str> = content
            .lines()
            .filter(|l| {
                serde_json::from_str::<SeenLine>(l)
                    .map(|s| s.at >= cutoff)
                    .unwrap_or(false)
            })
            .collect();
        let mut body = kept.join("\n");
        if !body.is_empty() {
            body.push('\n');
        }
        crate::paths::write_atomic(&path, body.as_bytes())?;
        Ok(pruned)
    }

    pub(crate) fn reindex_seen(&self) -> Result<(), StoreError> {
        let path = self.paths.seen_file();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        };
        let idx = self.idx();
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<SeenLine>(line) {
                Ok(l) => idx.mark_seen(&l.id, l.at)?,
                Err(e) => tracing::warn!("seen.jsonl: bad line: {e}"),
            }
        }
        Ok(())
    }

    /// Ingest one remote signed event. Never re-signs, never rewrites, never
    /// re-emits on the store-event bus.
    pub fn ingest_remote_event(&self, event: &Event) -> Result<IngestOutcome, StoreError> {
        if let Err(e) = event.verify() {
            return Ok(reject(format!("invalid id/signature: {e}")));
        }
        if self.idx().is_seen(&event.id.to_hex())? {
            return Ok(IngestOutcome::Duplicate);
        }
        let wire_kind = event.kind.as_u16();
        if kind::is_ephemeral(wire_kind) {
            return Ok(reject("ephemeral"));
        }
        if !kind::is_gep_kind(wire_kind) {
            return Ok(reject(format!("unknown kind {wire_kind}")));
        }

        // Admission by role. The owner's own key — this workspace on another
        // machine — and the owner's own attested agents write anything. A
        // hosted member writes as a human: their own key, the human kinds,
        // the room's channels when they are an admin. A member's agents,
        // however attested, never write here: agents stay on their own node.
        let author = bisa_core::PrincipalId::new(event.pubkey.to_hex())?;
        let hosted = match self.member_role(&author)? {
            Some(bisa_core::MemberRole::Owner) => None,
            Some(role) => {
                if !hosted_may_write(role, wire_kind) {
                    return Ok(reject(format!(
                        "a hosted member writes as a human only — kind {wire_kind} is not a person's act"
                    )));
                }
                Some(role)
            }
            None => match crate::identity::verify_attestation(event) {
                Some(owner_hex) if owner_hex == self.owner_principal().as_hex() => None,
                Some(_) => {
                    return Ok(reject(
                        "an agent of another node never writes here — its person may",
                    ))
                }
                None => {
                    return Ok(reject(format!(
                        "author {} is not a member",
                        &author.as_hex()[..8]
                    )))
                }
            },
        };

        // Encrypted kinds must be addressed to this node's owner.
        if kind::requires_owner_encryption(wire_kind) {
            let ours = self.owner_keys().public_key().to_hex();
            let addressed_to_us = event.tags.iter().any(|t| {
                let s = t.as_slice();
                s.len() >= 2 && s[0] == "p" && s[1] == ours
            });
            if !addressed_to_us {
                return Ok(reject("not addressed to this node"));
            }
        }

        if matches!(
            wire_kind,
            kind::KIND_MESSAGE | kind::KIND_RETRACTION | kind::KIND_REACTION
        ) {
            return self.ingest_conversation_fact(event, hosted);
        }
        match wire_kind {
            kind::KIND_CHANNEL => return self.ingest_ns_snapshot(event, Paths::NS_CHANNELS),
            kind::KIND_CONVERSATION => {
                return self.ingest_ns_snapshot(event, Paths::NS_CONVERSATIONS)
            }
            kind::KIND_AGENT_PROFILE => return self.ingest_ns_snapshot(event, Paths::NS_AGENTS),
            kind::KIND_TEAM => return self.ingest_ns_snapshot(event, Paths::NS_TEAMS),
            kind::KIND_SKILL => return self.ingest_ns_snapshot(event, Paths::NS_SKILLS),
            kind::KIND_CONNECTOR => return self.ingest_ns_snapshot(event, Paths::NS_CONNECTORS),
            kind::KIND_PROJECT => return self.ingest_ns_snapshot(event, Paths::NS_PROJECTS),
            kind::KIND_WORKFLOW => return self.ingest_workflow_snapshot(event),
            kind::KIND_ADDON => return self.ingest_ns_snapshot(event, Paths::NS_ADDONS),
            kind::KIND_DRAWING => return self.ingest_ns_snapshot(event, Paths::NS_DRAWINGS),
            kind::KIND_ENGRAM => return Ok(reject("recall does not sync")),
            _ => {}
        }
        if kind::is_addressable(wire_kind) {
            self.ingest_snapshot(event, wire_kind)
        } else {
            self.ingest_journal(event, wire_kind)
        }
    }

    /// A conversation fact: resolve its scope, enforce the audience and — for
    /// a hosted author — the reach, apply. Unknown scopes reject WITHOUT
    /// marking seen — the fact retries once the scope's snapshot arrives.
    fn ingest_conversation_fact(
        &self,
        event: &Event,
        hosted: Option<bisa_core::MemberRole>,
    ) -> Result<IngestOutcome, StoreError> {
        let Some((_, scope_id)) = scope_coord(event) else {
            return Ok(reject("conversation fact has no scope `a` tag"));
        };
        let scope = match self.resolve_scope(&scope_id) {
            Ok(s) => s,
            Err(_) => return Ok(reject(format!("unknown scope {scope_id} (yet)"))),
        };
        let author = bisa_core::PrincipalId::new(event.pubkey.to_hex())?;
        if let crate::workspace::EventAudience::Restricted(aud) = &scope.audience {
            let direct = aud.contains(&author);
            let via_owner = crate::identity::verify_attestation(event)
                .and_then(|owner_hex| bisa_core::PrincipalId::new(owner_hex).ok())
                .map(|owner| aud.contains(&owner))
                .unwrap_or(false);
            if !direct && !via_owner {
                return Ok(reject("author is not a participant of this conversation"));
            }
        }
        // A person on another node reaches channels and direct messages they
        // are on, and nothing else — not a goal's thread, not a conversation.
        if let Some(role) = hosted {
            let channel = bisa_core::ChannelId::new(&scope_id)
                .ok()
                .and_then(|id| self.get_channel(&id).ok());
            let reached = channel
                .as_ref()
                .is_some_and(|c| bisa_core::reaches(c, role, &author));
            if !reached || !role.may(bisa_core::Permission::PostInChannels) {
                return Ok(reject(format!(
                    "{} does not reach {scope_id} as a {role}",
                    &author.as_hex()[..8]
                )));
            }
        }
        // `None`: a peer's fact is applied to local storage and re-published
        // by nobody. A hosted person's message is then said once, as what it
        // is, so the engine may read it before an agent hears it.
        if let Err(e) = self.apply_conversation_fact(&scope, event, None) {
            return Ok(reject(format!("conversation apply failed: {e}")));
        }
        self.mark_seen(event)?;
        if let Some(role) = hosted {
            if event.kind.as_u16() == kind::KIND_MESSAGE {
                self.emit_store_event(crate::workspace::StoreEvent::RemoteMessageArrived {
                    scope: scope.id.clone(),
                    event: event.clone(),
                    audience: scope.audience.clone(),
                    author,
                    role,
                });
            }
        }
        Ok(IngestOutcome::AppliedConversation { scope: scope_id })
    }

    /// A non-goal addressable snapshot: latest-wins into its namespace plus
    /// index side effects. An undecodable body is a peer's bug, and the index
    /// row is skipped rather than the snapshot refused.
    /// A peer's workflow: its `origin` says which namespace it is filed in — a
    /// goal's design under that goal, a library workflow in `workflows/`. A
    /// design that arrives before its goal is retried, not dropped, exactly as
    /// a run is: a goal directory must never exist without its goal.
    fn ingest_workflow_snapshot(&self, event: &Event) -> Result<IngestOutcome, StoreError> {
        let w: Workflow = match serde_json::from_str(&event.content) {
            Ok(w) => w,
            Err(e) => {
                self.mark_seen(event)?;
                return Ok(reject(format!("workflow snapshot does not decode: {e}")));
            }
        };
        if tag_value(event, "d") != Some(w.id.to_string().as_str()) {
            self.mark_seen(event)?;
            return Ok(reject("workflow snapshot's d tag is not its id"));
        }
        if let Some(goal) = w.origin.goal() {
            if self.get_goal(goal).is_err() {
                return Ok(reject(format!("design for unknown goal {goal} (yet)")));
            }
        }
        self.ingest_ns_snapshot(event, &crate::workflows::workflow_namespace(&w.origin))
    }

    fn ingest_ns_snapshot(&self, event: &Event, ns: &str) -> Result<IngestOutcome, StoreError> {
        let d = tag_value(event, "d").map(str::to_string).ok_or_else(|| {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-snapshot-has-no-d-tag"
            ))
        })?;
        let applied = self.snapshots.apply_remote(ns, event)?;
        if !applied {
            self.mark_seen(event)?;
            return Ok(reject("stale"));
        }
        let indexed: Result<(), StoreError> = match event.kind.as_u16() {
            k if k == kind::KIND_CHANNEL => {
                match serde_json::from_str::<bisa_core::Channel>(&event.content) {
                    Ok(c) => self.index_channel(&c),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            k if k == kind::KIND_AGENT_PROFILE => {
                match serde_json::from_str::<bisa_core::Agent>(&event.content) {
                    Ok(a) => self.index_agent(&a),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            k if k == kind::KIND_TEAM => {
                match serde_json::from_str::<bisa_core::Team>(&event.content) {
                    Ok(t) => self.index_team(&t),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            k if k == kind::KIND_SKILL => {
                match serde_json::from_str::<bisa_core::Skill>(&event.content) {
                    Ok(s) => self.index_skill(&s),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            k if k == kind::KIND_CONNECTOR => {
                match serde_json::from_str::<bisa_core::Connector>(&event.content) {
                    Ok(c) => self.index_connector(&c),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            k if k == kind::KIND_PROJECT => {
                match serde_json::from_str::<bisa_core::Project>(&event.content) {
                    Ok(p) => self.adopt_remote_project(&p),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            k if k == kind::KIND_WORKFLOW => {
                match serde_json::from_str::<Workflow>(&event.content) {
                    Ok(w) => self.index_workflow(&w),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            // A peer's addon: the record lands, the bundle does not — the
            // addon lists here with its files absent (18 — Addons).
            k if k == kind::KIND_ADDON => {
                match serde_json::from_str::<bisa_core::AddonRecord>(&event.content) {
                    Ok(r) => self.adopt_remote_addon(&r),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            // A peer's drawing: indexed and drawn into this machine's
            // repository too (19 — Drawings).
            k if k == kind::KIND_DRAWING => {
                match serde_json::from_str::<bisa_core::Drawing>(&event.content) {
                    Ok(d) => {
                        self.adopt_drawing(&d);
                        Ok(())
                    }
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            k if k == kind::KIND_CONVERSATION => {
                match serde_json::from_str::<bisa_core::Conversation>(&event.content) {
                    Ok(c) => self.index_conversation(&c, &event.pubkey.to_hex()),
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                }
            }
            _ => Ok(()),
        };
        if let Err(e) = indexed {
            tracing::warn!("{ns}/{d}: snapshot applied but not indexed: {e}");
        }
        self.mark_seen(event)?;
        Ok(IngestOutcome::AppliedConversation { scope: d })
    }

    /// Whether the index holds the home a fact or a snapshot lands under —
    /// the rows that need it wait for the one that brings it otherwise.
    fn home_indexed(&self, home: &Home) -> Result<bool, StoreError> {
        match home {
            Home::Goal { goal } => self.idx().goal_exists(&goal.to_string()),
            Home::Run { run } => Ok(self.idx().get_run(&run.to_string())?.is_some()),
        }
    }

    fn ingest_journal(&self, event: &Event, wire_kind: u16) -> Result<IngestOutcome, StoreError> {
        let Some(addr) = journal_addr_of(event) else {
            return Ok(reject(format!(
                "journal kind {wire_kind} has no goal or run `a` tag"
            )));
        };
        let je = match JsonlEventLog::decode(event, &addr, self.owner_keys()) {
            Ok(je) => je,
            Err(reason) => return Ok(reject(format!("undecodable: {reason}"))),
        };
        // A Decision only enters the journal if its signer may decide that
        // gate. Not marked seen: if governance later admits the signer, a
        // re-sync retry succeeds.
        if let JournalPayload::Decision { gate, .. } = &je.payload {
            if !self.gate_policy_allows_for(*gate, &je.author, je.home.goal())? {
                return Ok(reject(format!(
                    "gate policy: {} may not decide the {gate} gate",
                    &je.author.as_hex()[..8]
                )));
            }
        }
        // Journal-before-snapshot is tolerated: the directory is created on
        // demand and the snapshot may arrive later. The index rows that need
        // the home's row wait for the rebuild that follows it.
        self.log.append_raw(&addr.home, event)?;
        if self.home_indexed(&addr.home)? {
            self.index_journal_fact(event, &je)?;
        }
        self.mark_seen(event)?;
        Ok(IngestOutcome::AppliedJournal { home: addr.home })
    }

    fn ingest_snapshot(&self, event: &Event, wire_kind: u16) -> Result<IngestOutcome, StoreError> {
        enum Parsed {
            Goal(Goal),
            /// A run, and whether this node held no copy of it before.
            Run(Box<WorkflowRun>, bool),
            Item(WorkItemSpec),
            Other,
        }
        let (home, parsed) = match wire_kind {
            kind::KIND_GOAL => {
                let goal: Goal = match serde_json::from_str(&event.content) {
                    Ok(g) => g,
                    Err(e) => return Ok(reject(format!("bad goal content: {e}"))),
                };
                let Some(d) = tag_value(event, "d") else {
                    return Ok(reject("goal snapshot has no d tag"));
                };
                if d != goal.id.to_string() {
                    return Ok(reject("goal snapshot d tag does not match content id"));
                }
                if let Ok(local) = self.get_goal(goal.id) {
                    if let Err(why) = remote_goal_admissible(&local, &goal) {
                        self.mark_seen(event)?;
                        return Ok(reject(format!("goal: {why}")));
                    }
                }
                (Home::Goal { goal: goal.id }, Parsed::Goal(goal))
            }
            kind::KIND_WORKFLOW_RUN => {
                let run: WorkflowRun = match serde_json::from_str(&event.content) {
                    Ok(r) => r,
                    Err(e) => return Ok(reject(format!("bad run content: {e}"))),
                };
                let Some(d) = tag_value(event, "d") else {
                    return Ok(reject("run snapshot has no d tag"));
                };
                if d != run.id.to_string() {
                    return Ok(reject("run snapshot d tag does not match content id"));
                }
                // A goal's run before its goal is retried, not dropped: the
                // goal's snapshot is what authorises anything on it. A run of
                // the workspace is its own home and waits for nobody.
                if let Some(goal) = run.scope.goal() {
                    if self.get_goal(goal).is_err() {
                        return Ok(reject(format!("run for unknown goal {goal} (yet)")));
                    }
                }
                let home = run.home();
                let local = self.get_run(run.id).ok();
                let verdict = run_snapshot_admissible(local.as_ref(), &run, |step| {
                    self.has_authorized_decision(
                        &home,
                        Gate::Approval,
                        Some(&approval_subject(run.id, step)),
                    )
                })?;
                match verdict {
                    Ok(()) => {}
                    Err(Admissibility::Final(why)) => {
                        self.mark_seen(event)?;
                        return Ok(reject(format!("run: {why}")));
                    }
                    Err(Admissibility::Retry(why)) => return Ok(reject(why)),
                }
                let first = local.is_none();
                (home, Parsed::Run(Box::new(run), first))
            }
            kind::KIND_WORK_ITEM => {
                let spec: WorkItemSpec = match serde_json::from_str(&event.content) {
                    Ok(s) => s,
                    Err(e) => return Ok(reject(format!("bad work item content: {e}"))),
                };
                (spec.home, Parsed::Item(spec))
            }
            _ => match journal_addr_of(event) {
                Some(addr) => (addr.home, Parsed::Other),
                None => {
                    return Ok(reject(format!(
                        "snapshot kind {wire_kind} has no goal or run `a` tag to locate it"
                    )))
                }
            },
        };

        let ns = Paths::ns_home(&home);
        let applied = self.snapshots.apply_remote(&ns, event)?;
        if !applied {
            self.mark_seen(event)?;
            return Ok(reject("stale"));
        }
        match parsed {
            Parsed::Goal(goal) => {
                self.index_goal(&goal, None)?;
                // Journal facts that arrived before this snapshot could not be
                // indexed then; replay them now so the cache matches truth.
                let addr = self.goal_addr(&goal);
                let d = goal.id.to_string();
                for (ev, je) in crate::journal::EventLog::replay(&self.log, &addr, &self.owner)? {
                    if let Err(e) = self.index_journal_fact(&ev, &je) {
                        tracing::debug!("goal {d}: journal fact not indexed: {e}");
                    }
                }
                // Runs and items that arrived before the goal row existed.
                self.reindex_runs_of(goal.id)?;
                self.reindex_work_items_of(&home)?;
            }
            Parsed::Run(run, first) => {
                self.index_run(&run)?;
                if let Some(goal) = run.scope.goal().and_then(|g| self.get_goal(g).ok()) {
                    if goal.run == Some(run.id) {
                        self.index_goal(&goal, Some(&run))?;
                    }
                }
                // A run of the workspace is its own home: the facts its
                // journal took before its row existed are indexed once, now
                // it has landed; every later fact finds the row.
                if first && run.scope.is_workspace() {
                    let addr = self.run_addr(run.id);
                    for (ev, je) in crate::journal::EventLog::replay(&self.log, &addr, &self.owner)?
                    {
                        if let Err(e) = self.index_journal_fact(&ev, &je) {
                            tracing::debug!(run = %run.id, "journal fact not indexed: {e}");
                        }
                    }
                }
                // Items that pointed at this run before it landed.
                self.reindex_work_items_of(&home)?;
            }
            Parsed::Item(spec) => {
                if self.home_indexed(&spec.home)? {
                    self.index_work_item(&spec, event.created_at.as_secs())?;
                }
            }
            Parsed::Other => {}
        }
        self.mark_seen(event)?;
        Ok(IngestOutcome::AppliedSnapshot {
            kind: wire_kind,
            home,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{attest_agent, MemoryKeyStore};
    use crate::workflows::tests::{agent_step, notify_workflow, sid, step};
    use crate::workflows::NewWorkflow;
    use crate::workspace::{NewGoal, StoreEvent};
    use bisa_core::{
        ClosureReason, GoalId, GoalStatus, MemberRole, RunEvent, RunScope, RunStatus, Tags,
    };
    use nostr::event::{EventBuilder, FinalizeEvent, Kind, Tag};
    use nostr::key::Keys;
    use nostr::types::Timestamp;
    use std::collections::BTreeMap;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, ws)
    }

    /// All raw events (journal lines + goal snapshot) of a goal, through Paths.
    fn raw_events(ws: &Workspace, goal: GoalId) -> (Vec<Event>, Event) {
        let lines = std::fs::read_to_string(ws.paths().goal(goal).journal()).unwrap();
        let journal: Vec<Event> = lines
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let snap_path = ws
            .paths()
            .goal(goal)
            .state()
            .join(format!("{}-{goal}.json", kind::KIND_GOAL));
        let snapshot: Event = serde_json::from_slice(&std::fs::read(snap_path).unwrap()).unwrap();
        (journal, snapshot)
    }

    fn raw_run_snapshot(ws: &Workspace, goal: GoalId, run: bisa_core::RunId) -> Event {
        serde_json::from_slice(&std::fs::read(ws.paths().goal(goal).run_snapshot(run)).unwrap())
            .unwrap()
    }

    fn raw_workflow_snapshot(ws: &Workspace, id: bisa_core::WorkflowId) -> Event {
        serde_json::from_slice(&std::fs::read(ws.paths().library_workflow_snapshot(id)).unwrap())
            .unwrap()
    }

    fn raw_design_snapshot(ws: &Workspace, goal: GoalId, id: bisa_core::WorkflowId) -> Event {
        serde_json::from_slice(&std::fs::read(ws.paths().goal(goal).workflow_snapshot(id)).unwrap())
            .unwrap()
    }

    /// The same person on another machine: a workspace opened with `of`'s
    /// owner key. What writes goals, runs and snapshots into this one over the
    /// wire — a hosted member never does.
    fn twin(of: &Workspace) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ks = MemoryKeyStore::default();
        crate::identity::KeyStore::set(
            &ks,
            crate::identity::OWNER_KEY_NAME,
            &of.owner_keys().secret_key().to_secret_hex(),
        )
        .unwrap();
        let ws = Workspace::open_with_keystore(dir.path(), Box::new(ks)).unwrap();
        assert_eq!(ws.owner_principal(), of.owner_principal());
        (dir, ws)
    }

    /// A person on another node, admitted here at a role.
    fn hosted(dst: &Workspace, role: MemberRole) -> (tempfile::TempDir, Workspace) {
        let (dir, ws) = ws();
        dst.add_member(
            ws.owner_principal(),
            role,
            crate::members::Admission::default(),
        )
        .unwrap();
        (dir, ws)
    }

    fn staffed(ws: &Workspace) {
        ws.add_agent(crate::agents::NewAgent {
            name: "Developer".into(),
            harness: "mock".into(),
            system_prompt: "build".into(),
            ..Default::default()
        })
        .unwrap();
    }

    /// agent → approval → end, so a run crosses the one gate there is.
    fn gated_workflow() -> NewWorkflow {
        NewWorkflow {
            name: "Gated".into(),
            description: String::new(),
            inputs: vec![],
            steps: vec![
                agent_step("build", "developer", &["ship"]),
                step(
                    "ship",
                    StepKind::Approval {
                        prompt: "Ship?".into(),
                    },
                    &["end"],
                ),
                step(
                    "end",
                    StepKind::End {
                        finish: bisa_core::Finish::Done,
                    },
                    &[],
                ),
            ],
            tags: Tags::default(),
            decision_making: false,
        }
    }

    /// A goal on ws2 with a run under way, its build step done, so the next
    /// thing that moves it is the approval.
    fn started_goal(ws2: &Workspace) -> (GoalId, bisa_core::RunId, Vec<Event>, Event, Event) {
        staffed(ws2);
        let wf = ws2
            .create_workflow(gated_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws2
            .create_goal(NewGoal::captured("governed remote"))
            .unwrap();
        let (run, _) = ws2
            .create_run(
                RunScope::Goal { goal: goal.id },
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        ws2.record_run_event(
            run.id,
            RunEvent::StepDone {
                step: sid("build"),
                output: serde_json::json!({}),
            },
        )
        .unwrap();
        let (journal, goal_snapshot) = raw_events(ws2, goal.id);
        let run_snapshot = raw_run_snapshot(ws2, goal.id, run.id);
        (goal.id, run.id, journal, goal_snapshot, run_snapshot)
    }

    #[test]
    fn a_run_snapshot_needs_an_authorised_decision_per_approval_step_it_passed() {
        // Pure: the verdict over the two records, with the journal answered
        // by a closure.
        let (_d, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(gated_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("pure")).unwrap();
        let (run, _) = ws
            .create_run(
                RunScope::Goal { goal: goal.id },
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let mut waiting = run.clone();
        waiting
            .apply(
                RunEvent::StepDone {
                    step: sid("build"),
                    output: serde_json::json!({}),
                },
                10,
            )
            .unwrap();
        waiting.revision = 2;
        let mut passed = waiting.clone();
        passed
            .apply(
                RunEvent::Decided {
                    step: sid("ship"),
                    approve: true,
                    approval: bisa_core::ApprovalId("dec".into()),
                },
                11,
            )
            .unwrap();
        passed.revision = 3;

        let never = |_: &StepId| Ok(false);
        let always = |_: &StepId| Ok(true);
        assert_eq!(
            run_snapshot_admissible(None, &waiting, never).unwrap(),
            Ok(())
        );
        assert_eq!(
            run_snapshot_admissible(Some(&run), &waiting, never).unwrap(),
            Ok(())
        );
        assert!(matches!(
            run_snapshot_admissible(Some(&waiting), &passed, never).unwrap(),
            Err(Admissibility::Retry(_))
        ));
        assert_eq!(
            run_snapshot_admissible(Some(&waiting), &passed, always).unwrap(),
            Ok(())
        );
        // A first copy that already passed the gate still needs the decision.
        assert!(matches!(
            run_snapshot_admissible(None, &passed, never).unwrap(),
            Err(Admissibility::Retry(_))
        ));
        // Stale, or a different run wearing the id: final.
        assert!(matches!(
            run_snapshot_admissible(Some(&passed), &waiting, always).unwrap(),
            Err(Admissibility::Final(_))
        ));
        let mut other_goal = passed.clone();
        other_goal.scope = RunScope::Goal {
            goal: GoalId::from_ulid(ulid::Ulid::from_parts(7, 7)),
        };
        other_goal.revision = 9;
        assert!(matches!(
            run_snapshot_admissible(Some(&passed), &other_goal, always).unwrap(),
            Err(Admissibility::Final(_))
        ));
        // Nor does a goal's run become a run of the workspace, or back.
        let mut unscoped = passed.clone();
        unscoped.scope = RunScope::Workspace {
            budget: Default::default(),
        };
        unscoped.revision = 9;
        assert_eq!(
            run_snapshot_admissible(Some(&passed), &unscoped, always).unwrap(),
            Err(Admissibility::Final("a run never changes scope".into()))
        );
        assert_eq!(
            run_snapshot_admissible(Some(&unscoped), &passed, always).unwrap(),
            Err(Admissibility::Final("a run never changes scope".into()))
        );
        // Where a run began, on what, and the signal that made it are its
        // birth: a peer never rewrites them.
        let mut rewired = passed.clone();
        rewired.start = Some(sid("ship"));
        rewired.revision = 9;
        assert_eq!(
            run_snapshot_admissible(Some(&passed), &rewired, always).unwrap(),
            Err(Admissibility::Final("a run's entry never changes".into()))
        );
        let mut re_evented = passed.clone();
        re_evented.event = Some(bisa_core::Signal {
            id: "01EVENT".into(),
            listener: None,
            source: bisa_core::SignalSource::Hook,
            name: None,
            at: 1,
            payload: serde_json::json!({}),
            scope: bisa_core::SignalScope::Goal { goal: goal.id },
            chain: bisa_core::Chain::default(),
            dedupe_key: None,
        });
        re_evented.revision = 9;
        assert_eq!(
            run_snapshot_admissible(Some(&passed), &re_evented, always).unwrap(),
            Err(Admissibility::Final(
                "the event a run began on never changes".into()
            ))
        );
        let mut redispatched = passed.clone();
        redispatched.dispatched = Some("01OTHER".into());
        redispatched.revision = 9;
        assert_eq!(
            run_snapshot_admissible(Some(&passed), &redispatched, always).unwrap(),
            Err(Admissibility::Final(
                "the signal that made a run never changes".into()
            ))
        );
        // An approval a boundary diverted passed nothing: no decision owed.
        let mut diverted = waiting.clone();
        diverted
            .steps
            .get_mut(&sid("ship"))
            .expect("the approval was entered")
            .state = StepState::Diverted {
            by: bisa_core::Branch::new("late").unwrap(),
        };
        diverted.revision = 3;
        assert_eq!(
            run_snapshot_admissible(Some(&waiting), &diverted, never).unwrap(),
            Ok(())
        );
        // A goal never reopens, never changes hands.
        let mut closed = ws.get_goal(goal.id).unwrap();
        closed.closed = Some(bisa_core::Closure {
            reason: ClosureReason::Abandoned { rationale: None },
            at: 1,
        });
        let open = ws.get_goal(goal.id).unwrap();
        assert!(remote_goal_admissible(&closed, &open).is_err());
        assert!(remote_goal_admissible(&open, &closed).is_ok());
        let mut other_author = open.clone();
        other_author.author = bisa_core::PrincipalId::new("cd".repeat(32)).unwrap();
        assert!(remote_goal_admissible(&open, &other_author).is_err());
    }

    #[test]
    fn ingest_happy_path_and_duplicate() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let goal = ws2
            .create_goal(NewGoal::captured("remote collaboration test"))
            .unwrap();
        ws2.append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Note {
                text: "a remote note".into(),
            },
            ws2.owner_keys(),
            None,
        )
        .unwrap();
        let (journal, snapshot) = raw_events(&ws2, goal.id);
        assert_eq!(
            ws1.ingest_remote_event(&snapshot).unwrap(),
            IngestOutcome::AppliedSnapshot {
                kind: kind::KIND_GOAL,
                home: Home::Goal { goal: goal.id }
            }
        );
        for ev in &journal {
            assert_eq!(
                ws1.ingest_remote_event(ev).unwrap(),
                IngestOutcome::AppliedJournal {
                    home: Home::Goal { goal: goal.id }
                }
            );
        }
        let got = ws1.get_goal(goal.id).unwrap();
        assert_eq!(got.statement, "remote collaboration test");
        assert_eq!(got.author, ws2.owner_principal());
        assert_eq!(ws1.list_goals(Some(GoalStatus::Draft)).unwrap().len(), 1);
        assert_eq!(
            ws1.journal(&Home::Goal { goal: goal.id }).unwrap().len(),
            journal.len()
        );
        assert_eq!(
            ws1.ingest_remote_event(&journal[0]).unwrap(),
            IngestOutcome::Duplicate
        );
        assert_eq!(
            ws1.ingest_remote_event(&snapshot).unwrap(),
            IngestOutcome::Duplicate
        );
        ws1.rebuild_index().unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&journal[0]).unwrap(),
            IngestOutcome::Duplicate
        );
        assert!(
            ws1.search("remote").unwrap().contains(&goal.id),
            "the note is indexed"
        );
    }

    #[test]
    fn journal_before_snapshot_is_tolerated_and_indexed_when_the_snapshot_lands() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let goal = ws2
            .create_goal(NewGoal::captured("out of order arrival"))
            .unwrap();
        let p = ws2
            .create_project(crate::projects::NewProject::managed("web").unwrap())
            .unwrap();
        ws2.attach(goal.id, p.id).unwrap();
        let (journal, snapshot) = raw_events(&ws2, goal.id);
        for ev in &journal {
            assert_eq!(
                ws1.ingest_remote_event(ev).unwrap(),
                IngestOutcome::AppliedJournal {
                    home: Home::Goal { goal: goal.id }
                }
            );
        }
        assert!(ws1.get_goal(goal.id).is_err());
        // The project arrives, then the goal: the attachment fact that came
        // first is indexed when the snapshot lands.
        let project_snapshot: Event = serde_json::from_slice(
            &std::fs::read(ws2.paths().state_dir(Paths::NS_PROJECTS).join(format!(
                "{}-{}.json",
                kind::KIND_PROJECT,
                p.id
            )))
            .unwrap(),
        )
        .unwrap();
        ws1.ingest_remote_event(&project_snapshot).unwrap();
        ws1.ingest_remote_event(&snapshot).unwrap();
        assert!(ws1.get_goal(goal.id).is_ok());
        assert_eq!(
            ws1.get_project(p.id).unwrap().slug,
            p.slug,
            "a peer's project is a project here too"
        );
        assert_eq!(
            ws1.journal(&Home::Goal { goal: goal.id }).unwrap().len(),
            journal.len()
        );
        assert_eq!(ws1.projects_for(goal.id).unwrap().len(), 1);
    }

    #[test]
    fn non_member_rejected_and_attested_agent_accepted() {
        let (_d1, ws1) = ws();
        // A stranger's goal is refused; the owner's own twin writes it.
        let (_ds, stranger_ws) = ws();
        let theirs = stranger_ws
            .create_goal(NewGoal::captured("not ours"))
            .unwrap();
        let (_, their_snapshot) = raw_events(&stranger_ws, theirs.id);
        match ws1.ingest_remote_event(&their_snapshot).unwrap() {
            IngestOutcome::Rejected { reason } => {
                assert!(reason.contains("not a member"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
        let (_d2, ws2) = twin(&ws1);
        let goal = ws2.create_goal(NewGoal::captured("authz test")).unwrap();
        let (journal, snapshot) = raw_events(&ws2, goal.id);
        ws1.ingest_remote_event(&snapshot).unwrap();
        for ev in &journal {
            ws1.ingest_remote_event(ev).unwrap();
        }
        let agent = Keys::generate();
        let auth = attest_agent(ws2.owner_keys(), &agent.public_key().to_hex(), "").unwrap();
        ws2.append_journal(
            &Home::Goal { goal: goal.id },
            JournalPayload::Note {
                text: "agent progress".into(),
            },
            &agent,
            Some(auth),
        )
        .unwrap();
        let (journal2, _) = raw_events(&ws2, goal.id);
        let agent_event = journal2.last().unwrap();
        assert_eq!(
            ws1.ingest_remote_event(agent_event).unwrap(),
            IngestOutcome::AppliedJournal {
                home: Home::Goal { goal: goal.id }
            }
        );
        // A hosted member's agent, however attested, never writes here; the
        // member's own key writes a human's acts and nothing else.
        let (_d3, bob_ws) = hosted(&ws1, MemberRole::Member);
        let bobs_agent = Keys::generate();
        let bobs_auth =
            attest_agent(bob_ws.owner_keys(), &bobs_agent.public_key().to_hex(), "").unwrap();
        let a_tag = Tag::parse([
            "a",
            &format!("{}:{}:{}", kind::KIND_GOAL, ws1.owner_principal(), goal.id),
        ])
        .unwrap();
        let note = |keys: &Keys, auth: Option<Tag>| {
            let mut b = EventBuilder::new(
                Kind::from(kind::KIND_GOAL_NOTE),
                serde_json::to_string(&JournalPayload::Note {
                    text: "from outside".into(),
                })
                .unwrap(),
            )
            .tag(a_tag.clone());
            if let Some(t) = auth {
                b = b.tag(t);
            }
            b.finalize(keys).unwrap()
        };
        match ws1
            .ingest_remote_event(&note(&bobs_agent, Some(bobs_auth)))
            .unwrap()
        {
            IngestOutcome::Rejected { reason } => {
                assert!(reason.contains("never writes here"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
        match ws1
            .ingest_remote_event(&note(bob_ws.owner_keys(), None))
            .unwrap()
        {
            IngestOutcome::Rejected { reason } => {
                assert!(reason.contains("human only"), "{reason}")
            }
            other => panic!("a goal note is not a person's act: {other:?}"),
        }
        let stranger = Keys::generate();
        let a_tag = Tag::parse([
            "a",
            &format!(
                "{}:{}:{}",
                kind::KIND_GOAL,
                ws2.owner_keys().public_key().to_hex(),
                goal.id
            ),
        ])
        .unwrap();
        let rogue = EventBuilder::new(
            Kind::from(kind::KIND_GOAL_NOTE),
            serde_json::to_string(&JournalPayload::Note {
                text: "rogue".into(),
            })
            .unwrap(),
        )
        .tag(a_tag)
        .finalize(&stranger)
        .unwrap();
        assert!(matches!(
            ws1.ingest_remote_event(&rogue).unwrap(),
            IngestOutcome::Rejected { .. }
        ));
    }

    #[test]
    fn snapshot_latest_wins_across_authors_and_stale_rejection() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        // A second author the owner vouches for: one of its agents.
        let member2 = Keys::generate();
        let auth = attest_agent(ws1.owner_keys(), &member2.public_key().to_hex(), "").unwrap();
        let goal = ws2
            .create_goal(NewGoal::captured("contested state"))
            .unwrap();
        let (_, snapshot) = raw_events(&ws2, goal.id);
        ws1.ingest_remote_event(&snapshot).unwrap();
        let base_at = snapshot.created_at.as_secs();
        let build_snapshot = |author: &Keys, statement: &str, revision: u64, at: u64| {
            let mut alt = ws2.get_goal(goal.id).unwrap();
            alt.statement = statement.to_string();
            alt.revision = revision;
            EventBuilder::new(
                Kind::from(kind::KIND_GOAL),
                serde_json::to_string(&alt).unwrap(),
            )
            .tags([
                Tag::parse(["d", &goal.id.to_string()]).unwrap(),
                Tag::parse(["revision", &revision.to_string()]).unwrap(),
                auth.clone(),
            ])
            .custom_created_at(Timestamp::from_secs(at))
            .finalize(author)
            .unwrap()
        };
        let stale = build_snapshot(&member2, "stale version", 1, base_at.saturating_sub(10));
        assert_eq!(
            ws1.ingest_remote_event(&stale).unwrap(),
            IngestOutcome::Rejected {
                reason: "stale".into()
            }
        );
        assert_eq!(
            ws1.ingest_remote_event(&stale).unwrap(),
            IngestOutcome::Duplicate
        );
        let newer = build_snapshot(&member2, "newer version", 2, base_at + 10);
        assert_eq!(
            ws1.ingest_remote_event(&newer).unwrap(),
            IngestOutcome::AppliedSnapshot {
                kind: kind::KIND_GOAL,
                home: Home::Goal { goal: goal.id }
            }
        );
        assert_eq!(ws1.get_goal(goal.id).unwrap().statement, "newer version");
    }

    #[test]
    fn ephemeral_and_unknown_kinds_rejected() {
        let (_d1, ws1) = ws();
        let keys = ws1.owner_keys().clone();
        let ephemeral = EventBuilder::new(Kind::from(kind::KIND_OBSERVER_FRAME), "x")
            .finalize(&keys)
            .unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&ephemeral).unwrap(),
            IngestOutcome::Rejected {
                reason: "ephemeral".into()
            }
        );
        let unknown = EventBuilder::new(Kind::from(1u16), "hello")
            .finalize(&keys)
            .unwrap();
        assert!(matches!(
            ws1.ingest_remote_event(&unknown).unwrap(),
            IngestOutcome::Rejected { .. }
        ));
        // A retired kind is unknown, whatever it once meant.
        let retired = EventBuilder::new(Kind::from(33406u16), "{}")
            .finalize(&keys)
            .unwrap();
        assert!(matches!(
            ws1.ingest_remote_event(&retired).unwrap(),
            IngestOutcome::Rejected { .. }
        ));
    }

    #[test]
    fn store_events_fire_for_local_writes_but_not_ingest() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let mut rx = ws1.subscribe_store_events();
        let local = ws1.create_goal(NewGoal::captured("local one")).unwrap();
        let mut local_events = 0;
        while let Ok(ev) = rx.try_recv() {
            match ev {
                StoreEvent::JournalAppended { home, .. }
                | StoreEvent::SnapshotWritten { home, .. } => {
                    assert_eq!(home, Home::Goal { goal: local.id });
                    local_events += 1;
                }
                StoreEvent::ConversationAppended { .. }
                | StoreEvent::ConversationSnapshot { .. }
                | StoreEvent::ReadMarkerSet { .. } => {}
                StoreEvent::RemoteMessageArrived { .. }
                | StoreEvent::PeopleChanged { .. }
                | StoreEvent::InviteChanged { .. } => {
                    panic!("a local goal write is not a person's or a remote message")
                }
            }
        }
        assert!(local_events >= 2, "journal append + snapshot write");
        let remote = ws2.create_goal(NewGoal::captured("remote one")).unwrap();
        let (journal, snapshot) = raw_events(&ws2, remote.id);
        ws1.ingest_remote_event(&snapshot).unwrap();
        for ev in &journal {
            ws1.ingest_remote_event(ev).unwrap();
        }
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn ingest_rejects_unauthorized_decision_until_policy_admits() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let (goal_id, run_id, journal, goal_snapshot, _run_snapshot) = started_goal(&ws2);
        ws1.ingest_remote_event(&goal_snapshot).unwrap();
        for ev in &journal {
            ws1.ingest_remote_event(ev).unwrap();
        }
        // A hosted member's decision: the owner's twin records one so the
        // shape is right, and the member signs the same fact with its own
        // key — a decision is a human kind, so the role admits it and the
        // governance policy decides.
        let dec = ws2
            .record_decision(
                &Home::Goal { goal: goal_id },
                Gate::Approval,
                true,
                &approval_subject(run_id, &sid("ship")),
                None,
                None,
            )
            .unwrap();
        let (journal2, _) = raw_events(&ws2, goal_id);
        let owners = journal2.iter().find(|ev| ev.id.to_hex() == dec.0).unwrap();
        let (_d3, bob_ws) = hosted(&ws1, MemberRole::Member);
        let bobs = EventBuilder::new(owners.kind, owners.content.clone())
            .tags(owners.tags.iter().cloned())
            .finalize(bob_ws.owner_keys())
            .unwrap();
        match ws1.ingest_remote_event(&bobs).unwrap() {
            IngestOutcome::Rejected { reason } => {
                assert!(reason.contains("gate policy"), "{reason}")
            }
            other => panic!("expected gate-policy rejection, got {other:?}"),
        }
        ws1.set_gate_policy(Gate::Approval, crate::governance::GatePolicy::Admins)
            .unwrap();
        assert!(
            matches!(
                ws1.ingest_remote_event(&bobs).unwrap(),
                IngestOutcome::Rejected { .. }
            ),
            "a member is not an admin"
        );
        ws1.set_gate_policy(Gate::Approval, crate::governance::GatePolicy::Members)
            .unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&bobs).unwrap(),
            IngestOutcome::AppliedJournal {
                home: Home::Goal { goal: goal_id }
            }
        );
    }

    #[test]
    fn a_run_that_passed_an_approval_lands_only_after_the_decision() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let (goal_id, run_id, journal, goal_snapshot, waiting_run) = started_goal(&ws2);
        // ws1 learns the workflow, the goal, the journal so far, and the run
        // as it stands before the gate — all admitted.
        let wf_id = ws2.get_goal(goal_id).unwrap().workflow.unwrap();
        ws1.ingest_remote_event(&raw_workflow_snapshot(&ws2, wf_id))
            .unwrap();
        assert!(
            ws1.get_workflow(wf_id).is_ok(),
            "a workflow lands in its namespace"
        );
        assert_eq!(
            ws1.ingest_remote_event(&goal_snapshot).unwrap(),
            IngestOutcome::AppliedSnapshot {
                kind: kind::KIND_GOAL,
                home: Home::Goal { goal: goal_id }
            }
        );
        for ev in &journal {
            ws1.ingest_remote_event(ev).unwrap();
        }
        assert_eq!(
            ws1.ingest_remote_event(&waiting_run).unwrap(),
            IngestOutcome::AppliedSnapshot {
                kind: kind::KIND_WORKFLOW_RUN,
                home: Home::Goal { goal: goal_id }
            }
        );
        assert_eq!(ws1.get_run(run_id).unwrap().status(), RunStatus::Waiting);
        assert_eq!(ws1.dump_goal_rows().unwrap()[0].status, "waiting");

        // ws2 decides and the run finishes.
        let dec = ws2
            .record_decision(
                &Home::Goal { goal: goal_id },
                Gate::Approval,
                true,
                &approval_subject(run_id, &sid("ship")),
                None,
                None,
            )
            .unwrap();
        ws2.record_run_event(
            run_id,
            RunEvent::Decided {
                step: sid("ship"),
                approve: true,
                approval: dec.clone(),
            },
        )
        .unwrap();
        let (journal2, _) = raw_events(&ws2, goal_id);
        let done_run = raw_run_snapshot(&ws2, goal_id, run_id);
        // The step facts arrive; the decision is held back.
        for ev in journal2
            .iter()
            .filter(|ev| ev.id.to_hex() != dec.0 && !journal.iter().any(|o| o.id == ev.id))
        {
            ws1.ingest_remote_event(ev).unwrap();
        }
        match ws1.ingest_remote_event(&done_run).unwrap() {
            IngestOutcome::Rejected { reason } => {
                assert!(reason.contains("gate policy"), "{reason}")
            }
            other => panic!("expected gate-policy rejection, got {other:?}"),
        }
        assert_eq!(
            ws1.ingest_remote_event(&done_run).unwrap(),
            IngestOutcome::Rejected {
                reason: "gate policy: run passes approval step `ship` without an authorised decision in the journal"
                    .to_string()
            },
            "not marked seen: it retries"
        );
        let decision = journal2.iter().find(|ev| ev.id.to_hex() == dec.0).unwrap();
        assert_eq!(
            ws1.ingest_remote_event(decision).unwrap(),
            IngestOutcome::AppliedJournal {
                home: Home::Goal { goal: goal_id }
            }
        );
        assert_eq!(
            ws1.ingest_remote_event(&done_run).unwrap(),
            IngestOutcome::AppliedSnapshot {
                kind: kind::KIND_WORKFLOW_RUN,
                home: Home::Goal { goal: goal_id }
            }
        );
        assert_eq!(ws1.get_run(run_id).unwrap().status(), RunStatus::Done);
        assert_eq!(ws1.dump_goal_rows().unwrap()[0].status, "done");
    }

    #[test]
    fn a_goal_snapshot_that_reopens_a_closed_goal_is_refused_for_good() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let goal = ws2.create_goal(NewGoal::captured("to close")).unwrap();
        ws2.set_goal_closed(goal.id, ClosureReason::Abandoned { rationale: None })
            .unwrap();
        let (_journal, closed_snapshot) = raw_events(&ws2, goal.id);
        ws1.ingest_remote_event(&closed_snapshot).unwrap();
        assert!(ws1.get_goal(goal.id).unwrap().is_closed());
        let mut back = ws2.get_goal(goal.id).unwrap();
        back.closed = None;
        back.revision += 5;
        let bogus = EventBuilder::new(
            Kind::from(kind::KIND_GOAL),
            serde_json::to_string(&back).unwrap(),
        )
        .tags([
            Tag::parse(["d", &goal.id.to_string()]).unwrap(),
            Tag::parse(["revision", &back.revision.to_string()]).unwrap(),
        ])
        .custom_created_at(Timestamp::from_secs(
            closed_snapshot.created_at.as_secs() + 100,
        ))
        .finalize(ws2.owner_keys())
        .unwrap();
        match ws1.ingest_remote_event(&bogus).unwrap() {
            IngestOutcome::Rejected { reason } => {
                assert!(reason.contains("never reopens"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            ws1.ingest_remote_event(&bogus).unwrap(),
            IngestOutcome::Duplicate
        );
    }

    #[test]
    fn a_run_before_its_goal_is_retried_not_dropped() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let (goal_id, run_id, _journal, goal_snapshot, run_snapshot) = started_goal(&ws2);
        match ws1.ingest_remote_event(&run_snapshot).unwrap() {
            IngestOutcome::Rejected { reason } => assert!(reason.contains("yet"), "{reason}"),
            other => panic!("{other:?}"),
        }
        ws1.ingest_remote_event(&goal_snapshot).unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&run_snapshot).unwrap(),
            IngestOutcome::AppliedSnapshot {
                kind: kind::KIND_WORKFLOW_RUN,
                home: Home::Goal { goal: goal_id }
            }
        );
        assert_eq!(ws1.get_current_run(goal_id).unwrap().unwrap().id, run_id);
        // The workflow the run is a copy of need not be here: the run carries it.
        assert!(ws1.list_workflows().unwrap().is_empty());
    }

    #[test]
    fn a_workflow_snapshot_lands_in_its_namespace_and_is_indexed() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let mut new = notify_workflow("Shared");
        new.tags = Tags::new(["ops"]).unwrap();
        let wf = ws2
            .create_workflow(new, bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&raw_workflow_snapshot(&ws2, wf.id))
                .unwrap(),
            IngestOutcome::AppliedConversation {
                scope: wf.id.to_string()
            }
        );
        assert_eq!(ws1.get_workflow(wf.id).unwrap(), wf);
        assert!(
            ws1.paths().library_workflow_snapshot(wf.id).is_file(),
            "a library workflow is filed in workflows/"
        );
        assert_eq!(ws1.list_workflows().unwrap().len(), 1);
        assert_eq!(
            ws1.idx()
                .tags_of(bisa_core::TagEntity::Workflow, &wf.id.to_string())
                .unwrap(),
            vec!["ops".to_string()]
        );
    }

    /// A goal's design lands under that goal on the peer — never in its library.
    #[test]
    fn a_goals_design_lands_under_its_goal_on_the_peer_and_not_in_its_library() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let goal = ws2
            .create_goal(NewGoal::captured("designed remotely"))
            .unwrap();
        let design = ws2
            .create_workflow(
                notify_workflow("Design"),
                bisa_core::WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let (_journal, goal_snapshot) = raw_events(&ws2, goal.id);
        ws1.ingest_remote_event(&goal_snapshot).unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&raw_design_snapshot(&ws2, goal.id, design.id))
                .unwrap(),
            IngestOutcome::AppliedConversation {
                scope: design.id.to_string()
            }
        );
        assert!(ws1
            .paths()
            .goal(goal.id)
            .workflow_snapshot(design.id)
            .is_file());
        assert!(!ws1.paths().library_workflow_snapshot(design.id).exists());
        assert!(ws1
            .list_workflows_in(crate::WorkflowScope::Library)
            .unwrap()
            .is_empty());
        assert_eq!(
            ws1.list_workflows_in(crate::WorkflowScope::Goal(goal.id))
                .unwrap(),
            vec![design.clone()]
        );
        assert_eq!(ws1.get_workflow(design.id).unwrap(), design);
    }

    /// A design before its goal is retried, exactly as a run is: a goal
    /// directory never exists without its goal.
    #[test]
    fn a_design_before_its_goal_is_retried_not_dropped() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let goal = ws2
            .create_goal(NewGoal::captured("designed remotely"))
            .unwrap();
        let design = ws2
            .create_workflow(
                notify_workflow("Design"),
                bisa_core::WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let design_event = raw_design_snapshot(&ws2, goal.id, design.id);
        match ws1.ingest_remote_event(&design_event).unwrap() {
            IngestOutcome::Rejected { reason } => assert!(reason.contains("yet"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(
            !ws1.idx().is_seen(&design_event.id.to_hex()).unwrap(),
            "not dropped"
        );
        assert!(
            !ws1.paths().goal(goal.id).dir().exists(),
            "no goal folder was invented"
        );
        let (_journal, goal_snapshot) = raw_events(&ws2, goal.id);
        ws1.ingest_remote_event(&goal_snapshot).unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&design_event).unwrap(),
            IngestOutcome::AppliedConversation {
                scope: design.id.to_string()
            }
        );
        assert_eq!(ws1.get_workflow(design.id).unwrap(), design);
    }

    /// A run of the workspace travels as its own home: its facts under the
    /// run's coordinate and its snapshot into its own folder on the peer, with
    /// no goal to wait for — and the facts that came first are indexed, once,
    /// when it lands.
    #[test]
    fn a_workspace_run_lands_in_its_own_folder_on_the_peer() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        staffed(&ws2);
        let wf = ws2
            .create_workflow(gated_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let (run, _) = ws2
            .create_run(
                RunScope::Workspace {
                    budget: Default::default(),
                },
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let home = run.home();
        let journal: Vec<Event> = std::fs::read_to_string(ws2.paths().home(&home).journal())
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert!(!journal.is_empty(), "the start is a fact on the run");
        let snapshot: Event = serde_json::from_slice(
            &std::fs::read(ws2.paths().home(&home).run_snapshot(run.id)).unwrap(),
        )
        .unwrap();

        // The facts first: filed in the run's folder, their rows waiting.
        for ev in &journal {
            assert_eq!(
                ws1.ingest_remote_event(ev).unwrap(),
                IngestOutcome::AppliedJournal { home }
            );
        }
        assert!(ws1.get_run(run.id).is_err(), "no snapshot yet");
        ws1.ingest_remote_event(&raw_workflow_snapshot(&ws2, wf.id))
            .unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&snapshot).unwrap(),
            IngestOutcome::AppliedSnapshot {
                kind: kind::KIND_WORKFLOW_RUN,
                home
            }
        );
        assert_eq!(ws1.get_run(run.id).unwrap().scope, run.scope);
        assert!(ws1.paths().home(&home).run_snapshot(run.id).is_file());
        assert!(
            ws1.list_goals(None).unwrap().is_empty(),
            "no goal was invented for it"
        );
        assert_eq!(ws1.journal(&home).unwrap().len(), journal.len());
        assert_eq!(
            ws1.live_workspace_runs(Some(wf.id))
                .unwrap()
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            vec![run.id]
        );
        let rows = ws1.activity_page(None, None, 50).unwrap();
        let filed: Vec<_> = rows
            .iter()
            .filter(|r| r.source_id == wf.id.to_string())
            .collect();
        assert_eq!(
            filed.len(),
            journal.len(),
            "each fact that came first is one feed row, filed under the workflow: {rows:?}"
        );
        assert!(filed
            .iter()
            .all(|r| r.source_kind == "workflow" && r.concept == "workflows"));
        // Seen once: the same snapshot again changes nothing.
        assert_eq!(
            ws1.ingest_remote_event(&snapshot).unwrap(),
            IngestOutcome::Duplicate
        );
    }

    #[test]
    fn seen_events_are_pruned_by_age_in_both_truth_and_cache() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let goal = ws2.create_goal(NewGoal::captured("old news")).unwrap();
        let (_journal, snapshot) = raw_events(&ws2, goal.id);
        ws1.ingest_remote_event(&snapshot).unwrap();
        let before = ws1.idx().seen_count().unwrap();
        assert!(before >= 1);
        let far_future = snapshot.created_at.as_secs() + SEEN_RETENTION_SECS + 10;
        assert!(ws1.prune_seen(far_future).unwrap() >= 1);
        assert_eq!(ws1.idx().seen_count().unwrap(), 0);
        ws1.rebuild_index().unwrap();
        assert_eq!(
            ws1.idx().seen_count().unwrap(),
            0,
            "the truth file was rewritten too"
        );
    }
}
