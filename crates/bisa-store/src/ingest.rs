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
        // Read lossily: a torn multi-byte character costs its line, never
        // the file or the rebuild.
        let content = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
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
                    // LCOV_EXCL_START: `ingest_workflow_snapshot` decoded the workflow before filing it here
                    Err(e) => Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-ingest-refused",
                        detail = e.to_string()
                    ))),
                    // LCOV_EXCL_STOP
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
            // repository too (19 — Drawings) — and said, so an open canvas
            // hears what the store now holds.
            k if k == kind::KIND_DRAWING => {
                match serde_json::from_str::<bisa_core::Drawing>(&event.content) {
                    Ok(d) => {
                        self.adopt_drawing(&d);
                        self.emit_store_event(crate::workspace::StoreEvent::RemoteDrawingArrived {
                            drawing: d,
                        });
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
            _ => Ok(()), // LCOV_EXCL_LINE: every kind routed here has its arm above; the arm keeps the match total
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
            // LCOV_EXCL_START: every addressable kind is matched by name above or filed by its home (a goal, a run, a work item); the arm keeps the match total
            _ => match journal_addr_of(event) {
                Some(addr) => (addr.home, Parsed::Other),
                None => {
                    return Ok(reject(format!(
                        "snapshot kind {wire_kind} has no goal or run `a` tag to locate it"
                    )))
                }
            },
            // LCOV_EXCL_STOP
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
                        // LCOV_EXCL_START: a fact `decode` accepted is one the index takes; the arm keeps a peer's oddity from stopping the replay
                        tracing::debug!("goal {d}: journal fact not indexed: {e}");
                        // LCOV_EXCL_STOP
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
                            // LCOV_EXCL_START: a fact `decode` accepted is one the index takes; the arm keeps a peer's oddity from stopping the replay
                            tracing::debug!(run = %run.id, "journal fact not indexed: {e}");
                            // LCOV_EXCL_STOP
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
                | StoreEvent::RemoteDrawingArrived { .. }
                | StoreEvent::PeopleChanged { .. }
                | StoreEvent::InviteChanged { .. } => {
                    panic!("a local goal write is not a person's, a remote message or a peer's drawing")
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

    // added by the coverage pass: s3-ingest.rs
    #[test]
    fn a_goals_origin_and_birth_and_a_runs_workflow_making_and_start_never_change_and_a_gate_needs_no_second_decision(
    ) {
        let (_d, ws) = ws();
        let local = ws.create_goal(NewGoal::captured("x")).unwrap();
        let mut other_origin = local.clone();
        other_origin.origin = bisa_core::GoalOrigin::Spawned { parent: local.id };
        assert!(remote_goal_admissible(&local, &other_origin).is_err());
        let mut other_birth = local.clone();
        other_birth.created_at += 1;
        assert!(remote_goal_admissible(&local, &other_birth).is_err());
        staffed(&ws);
        let wf = ws
            .create_workflow(gated_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let (run, _) = ws
            .create_run(
                RunScope::Goal { goal: local.id },
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let yes = |_: &StepId| -> Result<bool, StoreError> { Ok(true) };
        let refused = |remote: &bisa_core::WorkflowRun| {
            matches!(
                run_snapshot_admissible(Some(&run), remote, yes).unwrap(),
                Err(Admissibility::Final(_))
            )
        };
        let mut other_workflow = run.clone();
        other_workflow.workflow.id = bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(9, 9));
        other_workflow.revision += 1;
        assert!(refused(&other_workflow));
        let mut other_making = run.clone();
        other_making.queued_at += 1;
        other_making.revision += 1;
        assert!(refused(&other_making));
        let mut other_start = run.clone();
        other_start.started_at = run.started_at.map(|t| t + 1);
        other_start.revision += 1;
        assert!(refused(&other_start));
        // An approval the remote holds no record of, and one both sides
        // already passed, need no decision.
        let mut no_record = run.clone();
        no_record.steps.remove(&sid("ship"));
        no_record.revision += 1;
        assert!(run_snapshot_admissible(Some(&run), &no_record, yes)
            .unwrap()
            .is_ok());
        let mut passed = run.clone();
        passed.steps.get_mut(&sid("ship")).unwrap().state = bisa_core::StepState::done();
        let mut passed_again = passed.clone();
        passed_again.revision += 1;
        let never = |_: &StepId| -> Result<bool, StoreError> { Ok(false) };
        assert!(run_snapshot_admissible(Some(&passed), &passed_again, never)
            .unwrap()
            .is_ok());
    }

    #[test]
    fn pruning_the_seen_set_keeps_the_young_rewrites_the_file_and_does_without_a_file_that_is_gone()
    {
        let (_d, ws) = ws();
        let keys = ws.owner_keys().clone();
        let old = EventBuilder::new(Kind::from(1u16), "old")
            .custom_created_at(Timestamp::from_secs(100))
            .finalize(&keys)
            .unwrap();
        let young = EventBuilder::new(Kind::from(1u16), "young")
            .custom_created_at(Timestamp::from_secs(1_000_000))
            .finalize(&keys)
            .unwrap();
        assert_eq!(ws.prune_seen(1_000_000).unwrap(), 0);
        ws.mark_seen(&old).unwrap();
        ws.mark_seen(&young).unwrap();
        assert_eq!(ws.prune_seen(100 + SEEN_RETENTION_SECS + 1).unwrap(), 1);
        let file = ws.paths().seen_file();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(&young.id.to_hex()) && !text.contains(&old.id.to_hex()));
        assert!(text.ends_with('\n'));
        ws.mark_seen(&old).unwrap();
        std::fs::rename(&file, file.with_extension("moved")).unwrap();
        assert_eq!(ws.prune_seen(100 + SEEN_RETENTION_SECS + 1).unwrap(), 1);
    }

    #[test]
    fn a_tampered_signature_a_secret_for_somebody_else_a_scopeless_fact_and_a_fact_that_is_no_body_are_refused(
    ) {
        let (_d, ws) = ws();
        let keys = ws.owner_keys().clone();
        let mut tampered = EventBuilder::new(Kind::from(kind::KIND_GOAL_NOTE), "x")
            .finalize(&keys)
            .unwrap();
        tampered.content = "y".into();
        let reason = |outcome: IngestOutcome| match outcome {
            IngestOutcome::Rejected { reason } => reason,
            other => panic!("{other:?}"),
        };
        assert!(reason(ws.ingest_remote_event(&tampered).unwrap()).contains("signature"));
        let for_nobody = EventBuilder::new(Kind::from(kind::KIND_ENGRAM), "{}")
            .tag(Tag::parse(["d", "m"]).unwrap())
            .finalize(&keys)
            .unwrap();
        assert!(reason(ws.ingest_remote_event(&for_nobody).unwrap()).contains("not addressed"));
        let for_us = EventBuilder::new(Kind::from(kind::KIND_ENGRAM), "{}")
            .tags([
                Tag::parse(["d", "m"]).unwrap(),
                Tag::parse(["p", &keys.public_key().to_hex()]).unwrap(),
            ])
            .finalize(&keys)
            .unwrap();
        assert!(reason(ws.ingest_remote_event(&for_us).unwrap()).contains("recall"));
        let scopeless = EventBuilder::new(Kind::from(kind::KIND_MESSAGE), "{}")
            .finalize(&keys)
            .unwrap();
        assert!(reason(ws.ingest_remote_event(&scopeless).unwrap()).contains("scope"));
        let general = Tag::parse([
            "a",
            &format!(
                "{}:{}:general",
                kind::KIND_CHANNEL,
                keys.public_key().to_hex()
            ),
        ])
        .unwrap();
        let not_a_body = EventBuilder::new(Kind::from(kind::KIND_MESSAGE), "{not json")
            .tag(general.clone())
            .finalize(&keys)
            .unwrap();
        assert!(reason(ws.ingest_remote_event(&not_a_body).unwrap()).contains("apply failed"));
        let reaction_to_nothing = EventBuilder::new(Kind::from(kind::KIND_REACTION), "👍")
            .tag(general.clone())
            .finalize(&keys)
            .unwrap();
        assert!(
            reason(ws.ingest_remote_event(&reaction_to_nothing).unwrap()).contains("apply failed")
        );
        let retraction_of_nothing = EventBuilder::new(Kind::from(kind::KIND_RETRACTION), "")
            .tag(general.clone())
            .finalize(&keys)
            .unwrap();
        assert!(
            reason(ws.ingest_remote_event(&retraction_of_nothing).unwrap())
                .contains("apply failed")
        );
        // A retraction by an attested agent of a post the owner made.
        let said = ws
            .post_message(
                "general",
                bisa_core::MessageBody::post("mine"),
                None,
                &[],
                &[],
                None,
                crate::workspace::PostOrigin::Asked,
            )
            .unwrap();
        let agent = Keys::generate();
        let auth = attest_agent(&keys, &agent.public_key().to_hex(), "").unwrap();
        let theirs = EventBuilder::new(Kind::from(kind::KIND_RETRACTION), "")
            .tags([general, Tag::parse(["e", &said]).unwrap(), auth])
            .finalize(&agent)
            .unwrap();
        assert!(reason(ws.ingest_remote_event(&theirs).unwrap()).contains("apply failed"));
        // A journal fact whose `a` tag names no home, and one that does not decode.
        let homeless = EventBuilder::new(Kind::from(kind::KIND_GOAL_NOTE), "{}")
            .tag(Tag::parse(["a", &format!("{}:x:y", kind::KIND_CHANNEL)]).unwrap())
            .finalize(&keys)
            .unwrap();
        assert!(reason(ws.ingest_remote_event(&homeless).unwrap()).contains("no goal or run"));
        let goal = ws.create_goal(NewGoal::captured("here")).unwrap();
        let undecodable = EventBuilder::new(Kind::from(kind::KIND_GOAL_NOTE), "{not a payload")
            .tag(
                Tag::parse([
                    "a",
                    &format!(
                        "{}:{}:{}",
                        kind::KIND_GOAL,
                        keys.public_key().to_hex(),
                        goal.id
                    ),
                ])
                .unwrap(),
            )
            .finalize(&keys)
            .unwrap();
        assert!(reason(ws.ingest_remote_event(&undecodable).unwrap()).contains("undecodable"));
    }

    #[test]
    fn a_goal_or_run_snapshot_that_does_not_decode_names_no_d_or_names_another_is_refused() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let keys = ws1.owner_keys().clone();
        let reason = |outcome: IngestOutcome| match outcome {
            IngestOutcome::Rejected { reason } => reason,
            other => panic!("{other:?}"),
        };
        let goal = ws2.create_goal(NewGoal::captured("theirs")).unwrap();
        let (_, snapshot) = raw_events(&ws2, goal.id);
        let craft = |k: u16, content: &str, d: Option<&str>| {
            let mut b = EventBuilder::new(Kind::from(k), content);
            if let Some(d) = d {
                b = b.tag(Tag::parse(["d", d]).unwrap());
            }
            b.finalize(&keys).unwrap()
        };
        assert!(reason(
            ws1.ingest_remote_event(&craft(kind::KIND_GOAL, "{}", Some("x")))
                .unwrap()
        )
        .contains("bad goal content"));
        assert!(reason(
            ws1.ingest_remote_event(&craft(kind::KIND_GOAL, &snapshot.content, None))
                .unwrap()
        )
        .contains("no d tag"));
        assert!(reason(
            ws1.ingest_remote_event(&craft(kind::KIND_GOAL, &snapshot.content, Some("other")))
                .unwrap()
        )
        .contains("does not match"));
        assert!(reason(
            ws1.ingest_remote_event(&craft(kind::KIND_WORKFLOW_RUN, "{}", Some("x")))
                .unwrap()
        )
        .contains("bad run content"));
        let (_goal, _run, _journal, _goal_snapshot, run_snapshot) = started_goal(&ws2);
        assert!(reason(
            ws1.ingest_remote_event(&craft(kind::KIND_WORKFLOW_RUN, &run_snapshot.content, None))
                .unwrap()
        )
        .contains("no d tag"));
        assert!(reason(
            ws1.ingest_remote_event(&craft(
                kind::KIND_WORKFLOW_RUN,
                &run_snapshot.content,
                Some("other")
            ))
            .unwrap()
        )
        .contains("does not match"));
        // A work item of a goal this node does not hold yet lands and waits for its home.
        assert!(reason(
            ws1.ingest_remote_event(&craft(kind::KIND_WORK_ITEM, "{}", Some("x")))
                .unwrap()
        )
        .contains("bad work item content"));
    }

    #[test]
    fn every_namespace_snapshot_lands_from_a_twin_and_one_that_will_not_decode_is_applied_but_said()
    {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let keys = ws1.owner_keys().clone();
        staffed(&ws2);
        ws2.create_team("Ops", None, vec![], Tags::default())
            .unwrap();
        ws2.create_skill(crate::skills::NewSkill {
            id: bisa_core::SkillId::new("tidy").unwrap(),
            name: "Tidy".into(),
            description: "keeps things neat".into(),
            tags: Tags::default(),
            markdown: "# Tidy\nkeep it neat".into(),
        })
        .unwrap();
        let project = ws2
            .create_project(crate::projects::NewProject::managed("web").unwrap())
            .unwrap();
        ws2.create_conversation(crate::conversations::NewConversation {
            origin: bisa_core::ConversationOrigin::Workstream {
                id: bisa_core::WorkstreamId::from_ulid(project.id.0),
                project: project.id,
            },
            title: Some("about the site".into()),
            mode: bisa_core::ConversationMode::Auto,
        })
        .unwrap();
        for (ns, k) in [
            (Paths::NS_AGENTS, kind::KIND_AGENT_PROFILE),
            (Paths::NS_TEAMS, kind::KIND_TEAM),
            (Paths::NS_SKILLS, kind::KIND_SKILL),
            (Paths::NS_PROJECTS, kind::KIND_PROJECT),
            (Paths::NS_CONVERSATIONS, kind::KIND_CONVERSATION),
        ] {
            for d in ws2.snapshots.list_ds(ns, k).unwrap() {
                // A record both hold (a core agent) is as old as this one's
                // or newer by the clock; only what this side lacks is certain.
                if ws1.snapshots.get_raw(ns, k, &d).unwrap().is_some() {
                    continue;
                }
                let event = ws2.snapshots.get_raw(ns, k, &d).unwrap().unwrap();
                assert_eq!(
                    ws1.ingest_remote_event(&event).unwrap(),
                    IngestOutcome::AppliedConversation { scope: d.clone() },
                    "{ns}/{d}"
                );
            }
        }
        // The twin's agent is indexed here (its key answers to its id); its
        // definition file is the twin's own. A peer's project record lands
        // whole, as `adopt_remote_project` says.
        let developer = ws2
            .get_agent(&bisa_core::AgentId::new("developer").unwrap())
            .unwrap();
        assert_eq!(
            ws1.idx()
                .agent_id_for_pubkey(developer.pubkey.as_hex())
                .unwrap()
                .as_deref(),
            Some("developer")
        );
        assert_eq!(ws1.list_projects().unwrap().len(), 1);
        // A `general` no newer than the one held (here: the very same) is stale.
        let general = ws1
            .snapshots
            .get_raw(Paths::NS_CHANNELS, kind::KIND_CHANNEL, "general")
            .unwrap()
            .unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&general).unwrap(),
            IngestOutcome::Rejected {
                reason: "stale".into()
            }
        );
        // A snapshot of each kind whose content will not decode is applied
        // as a file, said, and indexed by nobody.
        for k in [
            kind::KIND_CHANNEL,
            kind::KIND_AGENT_PROFILE,
            kind::KIND_TEAM,
            kind::KIND_SKILL,
            kind::KIND_CONNECTOR,
            kind::KIND_PROJECT,
            kind::KIND_ADDON,
            kind::KIND_DRAWING,
            kind::KIND_CONVERSATION,
        ] {
            let odd = EventBuilder::new(Kind::from(k), "{}")
                .tag(Tag::parse(["d", "odd"]).unwrap())
                .finalize(&keys)
                .unwrap();
            assert_eq!(
                ws1.ingest_remote_event(&odd).unwrap(),
                IngestOutcome::AppliedConversation {
                    scope: "odd".into()
                },
                "kind {k}"
            );
        }
        let nameless = EventBuilder::new(Kind::from(kind::KIND_CHANNEL), "{}")
            .finalize(&keys)
            .unwrap();
        assert!(ws1.ingest_remote_event(&nameless).is_err());
        let not_a_workflow = EventBuilder::new(Kind::from(kind::KIND_WORKFLOW), "{}")
            .tag(Tag::parse(["d", "w"]).unwrap())
            .finalize(&keys)
            .unwrap();
        assert!(matches!(
            ws1.ingest_remote_event(&not_a_workflow).unwrap(),
            IngestOutcome::Rejected { reason } if reason.contains("does not decode")
        ));
        let wf = ws2
            .create_workflow(
                notify_workflow("Twin"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let misnamed = EventBuilder::new(
            Kind::from(kind::KIND_WORKFLOW),
            raw_workflow_snapshot(&ws2, wf.id).content,
        )
        .tag(Tag::parse(["d", "other"]).unwrap())
        .finalize(&keys)
        .unwrap();
        assert!(matches!(
            ws1.ingest_remote_event(&misnamed).unwrap(),
            IngestOutcome::Rejected { reason } if reason.contains("not its id")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_seen_file_nobody_may_read_stops_the_rebuild_by_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, ws) = ws();
        let keys = ws.owner_keys().clone();
        let seen = EventBuilder::new(Kind::from(1u16), "x")
            .finalize(&keys)
            .unwrap();
        ws.mark_seen(&seen).unwrap();
        let file = ws.paths().seen_file();
        let was = std::fs::metadata(&file).unwrap().permissions();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let rebuilt = ws.rebuild_index();
        std::fs::set_permissions(&file, was).unwrap();
        assert!(matches!(rebuilt, Err(StoreError::Io { .. })), "{rebuilt:?}");
    }

    // added by the coverage pass: ingest.rs

    // --- the ingest module's remaining arms ---

    /// A message into a direct message from somebody who is not in it is
    /// refused; the owner's own words from a twin land as what they are.
    #[test]
    fn a_direct_message_takes_words_from_its_participants_alone() {
        let (_d, ws) = ws();
        staffed(&ws);
        let keys = ws.owner_keys().clone();
        let scout = ws
            .get_agent(&bisa_core::AgentId::new("developer").unwrap())
            .unwrap()
            .pubkey;
        let dm = ws.open_dm(std::slice::from_ref(&scout)).unwrap();
        let scope = |id: &str| {
            Tag::parse([
                "a",
                &format!("{}:{}:{id}", kind::KIND_CHANNEL, keys.public_key().to_hex()),
            ])
            .unwrap()
        };
        let body = serde_json::to_string(&bisa_core::MessageBody::post("psst")).unwrap();
        // A member of the workspace who is not in this direct message.
        let outsider = nostr::key::Keys::generate();
        ws.add_member(
            bisa_core::PrincipalId::new(outsider.public_key().to_hex()).unwrap(),
            bisa_core::MemberRole::Member,
            crate::members::Admission {
                label: Some("Outsider".into()),
                photo: None,
                invited_by: None,
                client: None,
            },
        )
        .unwrap();
        let stranger = EventBuilder::new(Kind::from(kind::KIND_MESSAGE), body.clone())
            .tag(scope(dm.id.as_str()))
            .finalize(&outsider)
            .unwrap();
        let outcome = ws.ingest_remote_event(&stranger).unwrap();
        assert!(
            matches!(&outcome, IngestOutcome::Rejected { reason } if reason.contains("participant")),
            "{outcome:?}"
        );
        let mine = EventBuilder::new(Kind::from(kind::KIND_MESSAGE), body)
            .tag(scope(dm.id.as_str()))
            .finalize(&keys)
            .unwrap();
        assert_eq!(
            ws.ingest_remote_event(&mine).unwrap(),
            IngestOutcome::AppliedConversation {
                scope: dm.id.to_string()
            }
        );
    }

    /// A twin's connector, run and work item land; a run already held at
    /// that revision is stale; the seen file nobody may read stops a prune
    /// by its path.
    #[test]
    fn a_twins_connector_run_and_work_item_land_and_a_run_held_already_is_stale() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        ws2.install(crate::catalog::CatalogKind::Connector, "slack")
            .unwrap();
        let slack = ws2
            .snapshots
            .get_raw(Paths::NS_CONNECTORS, kind::KIND_CONNECTOR, "slack")
            .unwrap()
            .unwrap();
        assert_eq!(
            ws1.ingest_remote_event(&slack).unwrap(),
            IngestOutcome::AppliedConversation {
                scope: "slack".into()
            }
        );
        staffed(&ws2);
        let wf = ws2
            .create_workflow(gated_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws2.create_goal(NewGoal::captured("twinned")).unwrap();
        let (run, _) = ws2
            .create_run(
                bisa_core::RunScope::Goal { goal: goal.id },
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let item = WorkItemSpec {
            id: bisa_core::WorkItemId::from_ulid(crate::workspace::mint_ulid()),
            home: Home::Goal { goal: goal.id },
            run: Some(run.id),
            step: None,
            instructions: "build".into(),
            state: bisa_core::WorkItemState::Open,
            project: None,
            harness_candidates: vec!["mock".into()],
            model: None,
            effort: None,
            output_schema: None,
            budget: Default::default(),
            assignees: vec![],
            tier_ceiling: bisa_core::ToolTier::Write,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        };
        ws2.put_work_item(&item).unwrap();
        let (_, goal_snapshot) = raw_events(&ws2, goal.id);
        assert!(ws1.ingest_remote_event(&goal_snapshot).is_ok());
        assert!(ws1
            .ingest_remote_event(&raw_workflow_snapshot(&ws2, wf.id))
            .is_ok());
        let older = raw_run_snapshot(&ws2, goal.id, run.id);
        ws2.record_run_event(
            run.id,
            bisa_core::RunEvent::Cancel {
                cause: bisa_core::CancelCause::Stopped { rationale: None },
            },
        )
        .unwrap();
        let newer = raw_run_snapshot(&ws2, goal.id, run.id);
        assert!(ws1.ingest_remote_event(&newer).is_ok());
        assert!(ws1.get_run(run.id).unwrap().is_finished());
        // The snapshot from before the cancel: held already at a later revision.
        let outcome = ws1.ingest_remote_event(&older).unwrap();
        assert!(
            matches!(&outcome, IngestOutcome::Rejected { reason } if reason.contains("stale")),
            "{outcome:?}"
        );
        let item_snapshot = ws2
            .snapshots
            .get_raw(
                &Paths::ns_goal(goal.id),
                kind::KIND_WORK_ITEM,
                &item.id.to_string(),
            )
            .unwrap()
            .unwrap();
        assert!(ws1.ingest_remote_event(&item_snapshot).is_ok());
        assert_eq!(
            ws1.home_of_work_item(item.id).unwrap(),
            Home::Goal { goal: goal.id }
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let seen = ws1.paths.seen_file();
            std::fs::set_permissions(&seen, std::fs::Permissions::from_mode(0o000)).unwrap();
            let pruned = ws1.prune_seen(u64::MAX / 2);
            std::fs::set_permissions(&seen, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(pruned, Err(StoreError::Io { .. })), "{pruned:?}");
        }
    }

    // added by the coverage pass: ingest-s7.rs

    /// A twin's journal facts land on the goal they are about once it is
    /// here; a work item of a goal this side does not hold is kept as its
    /// snapshot alone, indexed by nobody until the goal arrives.
    #[test]
    fn a_twins_facts_land_on_a_goal_held_and_an_item_of_a_goal_not_held_waits() {
        let (_d1, ws1) = ws();
        let (_d2, ws2) = twin(&ws1);
        let held = ws2.create_goal(NewGoal::captured("held here")).unwrap();
        let absent = ws2.create_goal(NewGoal::captured("not here")).unwrap();
        let (facts, snapshot) = raw_events(&ws2, held.id);
        assert!(ws1.ingest_remote_event(&snapshot).is_ok());
        for fact in &facts {
            assert!(ws1.ingest_remote_event(fact).is_ok());
        }
        assert!(!ws1
            .journal(&Home::Goal { goal: held.id })
            .unwrap()
            .is_empty());
        let item = WorkItemSpec {
            id: bisa_core::WorkItemId::from_ulid(crate::workspace::mint_ulid()),
            home: Home::Goal { goal: absent.id },
            run: None,
            step: None,
            instructions: "wait".into(),
            state: bisa_core::WorkItemState::Open,
            project: None,
            harness_candidates: vec!["mock".into()],
            model: None,
            effort: None,
            output_schema: None,
            budget: Default::default(),
            assignees: vec![],
            tier_ceiling: bisa_core::ToolTier::Write,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        };
        ws2.put_work_item(&item).unwrap();
        let item_snapshot = ws2
            .snapshots
            .get_raw(
                &Paths::ns_goal(absent.id),
                kind::KIND_WORK_ITEM,
                &item.id.to_string(),
            )
            .unwrap()
            .unwrap();
        assert!(ws1.ingest_remote_event(&item_snapshot).is_ok());
        assert!(
            ws1.home_of_work_item(item.id).is_err(),
            "indexed by nobody until its goal is here"
        );
    }
}
