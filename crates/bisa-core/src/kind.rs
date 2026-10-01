//! The GEP (Goal Engineering Protocol) kind registry.
//!
//! This module is simultaneously the wire registry, the encryption-audience
//! policy, and the enforcement input. Every chokepoint that decides who may read or
//! whether an event persists MUST consult the sets below rather than matching
//! kinds ad hoc.
//!
//! Number ranges follow NIP-01 semantics: regular 1000..10000, ephemeral
//! 20000..30000, addressable 30000..40000. All assignments are chosen clear
//! of known public and reserved Nostr kinds (30174-30179, 30300, 30350, 3061x-3062x,
//! 39000-39006, 40000-49999).

// ---------------------------------------------------------------------------
// Addressable state (latest per (author, kind, d) wins; `d` = ULID)
// ---------------------------------------------------------------------------

/// Goal: the first-class durable object. Content: statement, the workflow it
/// runs, its current run, its closure, its origin, budget, assignees.
pub const KIND_GOAL: u16 = 33400;
/// Drawing: a picture on a canvas — its title, what it is attached to, and
/// its scene (Excalidraw's element JSON, vector only, under
/// [`crate::draw::MAX_SCENE_BYTES`]). `d` = the drawing id, in the
/// `drawings/` namespace. A drawing reads the same everywhere, so it travels;
/// the `.excalidraw` file beside it is this machine's export for the
/// repository. The number was retired once — a plan — and is **reissued**
/// here on purpose, by the rule that reissued `33407`: no workspace this
/// version opens can hold an event of the old kind
/// ([09](../../../docs/architecture/09-protocol-gep.md)).
pub const KIND_DRAWING: u16 = 33401;
/// Work item: one executable unit (spec, the step it runs for, output schema,
/// status).
pub const KIND_WORK_ITEM: u16 = 33402;
/// Agent profile / persona.
pub const KIND_AGENT_PROFILE: u16 = 33403;
/// Engram (Recall record): agent memory, NIP-44-encrypted to the
/// agent<->owner conversation key, `d` = HMAC-blinded slug.
pub const KIND_ENGRAM: u16 = 33404;
/// Channel: a standing conversation. Content `{name, topic, kind:
/// standing|dm, audience}`; empty audience = the whole workspace.
pub const KIND_CHANNEL: u16 = 33405;
/// Addon: the record of an installed overlay widget — its manifest, where it
/// came from, whether it is enabled, what the person granted. `d` = the addon
/// id, in the `addons/` namespace. The record reads the same everywhere and
/// travels; the bundle's bytes and the window's placement are one machine's
/// and never do (a peer lists the record with its files absent). The number
/// was retired once — a goal's living document — and is **reissued** here on
/// purpose ([09](../../../docs/architecture/09-protocol-gep.md)).
pub const KIND_ADDON: u16 = 33407;

/// Team: agents + humans working together. Content `{name, purpose,
/// members: [{human: pk} | {agent: id}]}`.
pub const KIND_TEAM: u16 = 33408;
/// Project: a folder a goal owns, git or not. `d` = ProjectId. Content
/// carries the root kind, vcs config and assignees — never a credential.
pub const KIND_PROJECT: u16 = 33409;
// 33410 is a retired hole: it numbered a trigger, a standalone object that
// named a workflow to start. A workflow's own `start` steps say that now, and
// whether a library workflow listens is this machine's switch — local, like a
// workstream — so nothing takes the number.
/// Skill: a named markdown procedure an agent follows. `d` = the skill id.
/// Content `{name, description, tags, markdown}`. A skill is a document that
/// reads the same on every node, so unlike a workstream or an MCP command it
/// earns a kind.
pub const KIND_SKILL: u16 = 33411;
/// Workflow: a definition — inputs, steps, flows. `d` = the workflow id, in
/// the `workflows/` namespace. Reads the same on every node, so it travels.
pub const KIND_WORKFLOW: u16 = 33412;
/// Workflow run: one execution of a workflow on a goal — the frozen copy of the
/// definition plus every step's record. `d` = the run id, under the goal's
/// `state/`. Ingest admits a run snapshot only when every `approval` step it
/// records as done has an authorised decision on the journal.
pub const KIND_WORKFLOW_RUN: u16 = 33413;
/// Connector: the declarative definition of one outside platform's API — its
/// base URL, the hosts it may reach, its auth scheme, its operations. `d` =
/// the connector id, in the `connectors/` namespace. A definition reads the
/// same on every node, so it travels; an **account** — the login and its
/// secrets — never does, and no kind exists for one.
pub const KIND_CONNECTOR: u16 = 33414;
/// Conversation: a saved exchange a person started with agents — its origin,
/// its title, whether it is archived. `d` = the conversation id, in the
/// `conversations/` namespace. Its messages ride [`KIND_MESSAGE`] under the
/// conversation's own scope. A conversation about a workstream travels too:
/// its origin carries the project, which a peer does know.
pub const KIND_CONVERSATION: u16 = 33415;

// There is deliberately **no kind for a Workstream**. GEP carries facts that are
// true everywhere, and a workstream names a path on one machine — publishing it
// to another node would be a lie. What *is* globally true (the branch was
// pushed, PR #12 opened) rides [`KIND_PROGRESS`], so it syncs, verifies and
// appears in Pulse without a new kind. The same test denies an **MCP server
// config** a kind: a stdio transport names a command on one disk, so it is
// local state that agents reference by id, and the id is what travels.

// ---------------------------------------------------------------------------
// Regular facts (immutable; `a`-tag to their goal/work-item; NIP-10 threads)
// ---------------------------------------------------------------------------

/// Clarification / revision rationale — the "why" behind each goal update.
pub const KIND_GOAL_NOTE: u16 = 3400;
/// Signed gate decision: approve/reject/defer of a plan, result, or escalation.
pub const KIND_DECISION: u16 = 3401;
/// Claim/assignment of a work item by an agent or human (harness + session id).
pub const KIND_CLAIM: u16 = 3402;
/// Coarse progress update (verb/object/outcome triple).
pub const KIND_PROGRESS: u16 = 3403;
/// Structured result conforming to the work item's declared schema.
pub const KIND_RESULT: u16 = 3404;
/// Durable per-turn token/cost metrics, encrypted to the owner.
pub const KIND_TURN_METRICS: u16 = 3406;

/// Message: one kind for every message stream (channel chat, goal
/// threads, conversations). Scope via `a` tag to the channel/goal/
/// conversation address; replies via `["e", parent, "", "reply"]`; mentions `p`.
pub const KIND_MESSAGE: u16 = 3407;
/// Retraction: author-only tombstone targeting `e` (a message or reaction).
/// v1 has no edits — retract and repost.
pub const KIND_RETRACTION: u16 = 3408;
/// Signal: the durable fact of the occurrence that started a run, journaled
/// on the home of the run it started (its goal's, or the run's own). Payload
/// capped at `signal::MAX_SIGNAL_PAYLOAD_BYTES`; the listener and the
/// occurrence's dedupe key make a replayed dispatch a no-op.
pub const KIND_SIGNAL: u16 = 3410;
/// Step: one step of a run changed, or the run itself started, was amended,
/// finished or was cancelled. The rationale-bearing fact beside the run
/// snapshot; `a`-tagged to the goal.
pub const KIND_STEP: u16 = 3411;

// ---------------------------------------------------------------------------
// Ephemeral (never stored, never synced to external relays)
// ---------------------------------------------------------------------------

/// Live encrypted telemetry stream agent -> owner.
pub const KIND_OBSERVER_FRAME: u16 = 23400;

// ---------------------------------------------------------------------------
// Standard NIP kinds Bisa adopts as GEP traffic
// ---------------------------------------------------------------------------

/// NIP-25 reaction: content = emoji, `e` = target, `a` = scope.
/// Removal is a [`KIND_RETRACTION`] targeting the reaction event.
pub const KIND_REACTION: u16 = 7;

// ---------------------------------------------------------------------------
// Access-control sets — the policy surface
// ---------------------------------------------------------------------------

/// All GEP kinds this node produces or accepts as GEP traffic.
pub const GEP_KINDS: &[u16] = &[
    KIND_GOAL,
    KIND_WORK_ITEM,
    KIND_AGENT_PROFILE,
    KIND_ENGRAM,
    KIND_GOAL_NOTE,
    KIND_DECISION,
    KIND_CLAIM,
    KIND_PROGRESS,
    KIND_RESULT,
    KIND_TURN_METRICS,
    KIND_MESSAGE,
    KIND_RETRACTION,
    KIND_REACTION,
    KIND_CHANNEL,
    KIND_TEAM,
    KIND_PROJECT,
    KIND_SKILL,
    KIND_WORKFLOW,
    KIND_WORKFLOW_RUN,
    KIND_CONNECTOR,
    KIND_SIGNAL,
    KIND_STEP,
    KIND_OBSERVER_FRAME,
    KIND_CONVERSATION,
    KIND_ADDON,
    KIND_DRAWING,
];

/// Kinds whose content MUST be NIP-44-encrypted to the owner (or the
/// agent<->owner conversation key) before leaving the process. A publisher
/// encountering one of these with plaintext content MUST refuse to emit it.
pub const ENCRYPT_TO_OWNER_KINDS: &[u16] = &[KIND_ENGRAM, KIND_TURN_METRICS, KIND_OBSERVER_FRAME];

/// Kinds that MUST NOT be persisted in any store and MUST NOT be forwarded to
/// external relays. Delivered to live local subscribers only.
pub const EPHEMERAL_KINDS: &[u16] = &[KIND_OBSERVER_FRAME];

/// Kinds that are addressable state (latest-wins per `(author, kind, d)`).
pub const ADDRESSABLE_KINDS: &[u16] = &[
    KIND_GOAL,
    KIND_WORK_ITEM,
    KIND_AGENT_PROFILE,
    KIND_ENGRAM,
    KIND_CHANNEL,
    KIND_TEAM,
    KIND_PROJECT,
    KIND_SKILL,
    KIND_WORKFLOW,
    KIND_WORKFLOW_RUN,
    KIND_CONNECTOR,
    KIND_CONVERSATION,
    KIND_ADDON,
    KIND_DRAWING,
];

/// Kinds whose emission requires an authority check beyond authorship:
/// a [`KIND_DECISION`] is only valid from a principal holding the gate's role.
pub const AUTHORITY_CHECKED_KINDS: &[u16] = &[KIND_DECISION];

/// What a **hosted member's own key** may write into a workspace: a human's
/// acts, and nothing an agent produces. Ingest admits a hosted author for
/// these kinds alone; every other kind from them is refused by role, and a
/// hosted member's agents are refused whatever they attest.
pub const HUMAN_KINDS: &[u16] = &[KIND_MESSAGE, KIND_RETRACTION, KIND_REACTION, KIND_DECISION];

/// What an **admin** may write beyond [`HUMAN_KINDS`]: the room's own
/// definitions.
pub const MANAGER_KINDS: &[u16] = &[KIND_CHANNEL];

pub fn is_human_kind(kind: u16) -> bool {
    HUMAN_KINDS.contains(&kind)
}

pub fn is_manager_kind(kind: u16) -> bool {
    MANAGER_KINDS.contains(&kind)
}

/// Kinds whose readable audience may be a channel's `audience` list rather
/// than the whole workspace (DMs). The sync layer MUST wrap these pairwise to
/// the listed participants only — never to the workspace key — and ingest
/// MUST reject a DM-scoped event whose author is outside the audience.
pub const AUDIENCE_SCOPED_KINDS: &[u16] = &[KIND_MESSAGE, KIND_RETRACTION, KIND_REACTION];

pub fn is_audience_scoped(kind: u16) -> bool {
    AUDIENCE_SCOPED_KINDS.contains(&kind)
}

pub fn is_gep_kind(kind: u16) -> bool {
    GEP_KINDS.contains(&kind)
}

pub fn is_addressable(kind: u16) -> bool {
    ADDRESSABLE_KINDS.contains(&kind)
}

pub fn is_ephemeral(kind: u16) -> bool {
    EPHEMERAL_KINDS.contains(&kind)
}

pub fn requires_owner_encryption(kind: u16) -> bool {
    ENCRYPT_TO_OWNER_KINDS.contains(&kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_are_subsets_of_registry() {
        for set in [
            ENCRYPT_TO_OWNER_KINDS,
            EPHEMERAL_KINDS,
            ADDRESSABLE_KINDS,
            AUTHORITY_CHECKED_KINDS,
            AUDIENCE_SCOPED_KINDS,
            HUMAN_KINDS,
            MANAGER_KINDS,
        ] {
            for k in set {
                assert!(
                    is_gep_kind(*k),
                    "kind {k} in a policy set but not in GEP_KINDS"
                );
            }
        }
    }

    #[test]
    fn a_hosted_human_writes_acts_never_definitions_or_a_goal_s_journal() {
        for k in [KIND_MESSAGE, KIND_RETRACTION, KIND_REACTION, KIND_DECISION] {
            assert!(is_human_kind(k));
        }
        for k in [
            KIND_GOAL,
            KIND_AGENT_PROFILE,
            KIND_TEAM,
            KIND_SKILL,
            KIND_PROJECT,
            KIND_WORKFLOW,
            KIND_CLAIM,
            KIND_PROGRESS,
            KIND_RESULT,
            KIND_STEP,
            KIND_CONVERSATION,
            KIND_ADDON,
        ] {
            assert!(!is_human_kind(k), "kind {k} is not a human's act");
            assert!(k == KIND_CHANNEL || !is_manager_kind(k));
        }
        assert!(is_manager_kind(KIND_CHANNEL));
        assert!(
            !is_human_kind(KIND_CHANNEL),
            "a channel is a manager's, not everyone's"
        );
    }

    #[test]
    fn ranges_follow_nip01_semantics() {
        for k in ADDRESSABLE_KINDS {
            assert!(
                (30000..40000).contains(k),
                "addressable kind {k} out of range"
            );
        }
        for k in EPHEMERAL_KINDS {
            assert!(
                (20000..30000).contains(k),
                "ephemeral kind {k} out of range"
            );
        }
        for k in [
            KIND_GOAL_NOTE,
            KIND_DECISION,
            KIND_CLAIM,
            KIND_PROGRESS,
            KIND_RESULT,
            KIND_TURN_METRICS,
            KIND_MESSAGE,
            KIND_RETRACTION,
            KIND_SIGNAL,
            KIND_STEP,
        ] {
            assert!((1000..10000).contains(&k), "regular kind {k} out of range");
        }
        // KIND_REACTION is the standard NIP-25 kind 7 — outside the custom
        // ranges by design.
        assert_eq!(KIND_REACTION, 7);
    }

    #[test]
    fn no_collision_with_reserved_public_kinds() {
        for k in GEP_KINDS {
            let k = *k as u32;
            assert!(!(30174..=30179).contains(&k));
            assert!(k != 30300 && k != 30350);
            assert!(!(30617..=30622).contains(&k));
            assert!(!(39000..=39006).contains(&k));
            assert!(!(40000..=49999).contains(&k));
            assert!(k != 24200 && k != 44200); // reserved telemetry/metrics kinds
            assert!(!(45001..=45003).contains(&k)); // reserved forum kinds
            assert!(!(41001..=41012).contains(&k)); // reserved direct-message kinds
        }
    }

    #[test]
    fn every_kind_is_registered_exactly_once() {
        let mut seen = GEP_KINDS.to_vec();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate kind in GEP_KINDS");
    }

    #[test]
    fn no_kind_exists_for_a_workstream_an_mcp_server_or_a_connector_account() {
        // All three are local state — a workstream names a path on one
        // machine, an MCP stdio config a command on one machine, a connector
        // account a login and its secrets on one machine — so no number is
        // reserved for any of them. This test is the rule's enforcement
        // point: the next number after the conversation's stays free, and
        // adding one here would be a design change, not a detail.
        assert!(!is_gep_kind(33416));
        assert_eq!(KIND_WORKFLOW, 33412);
        assert_eq!(KIND_WORKFLOW_RUN, 33413);
        assert_eq!(KIND_CONNECTOR, 33414);
        assert_eq!(KIND_CONVERSATION, 33415);
    }

    #[test]
    fn the_four_retired_numbers_stay_holes_and_two_were_reissued() {
        // 33406 and 3409 (a removed feature's two objects), 3405 (a
        // criterion's verification) and 33410 (a trigger) stay unassigned.
        // Two numbers were reissued, each when no workspace this version
        // opens could hold an event of the old kind (09 — GEP): 33407, a
        // goal's living document, is the addon record's; 33401, a plan, is
        // the drawing's.
        for hole in [33406, 3405, 3409, 33410] {
            assert!(!is_gep_kind(hole), "{hole} is a retired hole");
            assert!(!is_addressable(hole));
        }
        assert_eq!(KIND_SIGNAL, 3410, "the occurrence's fact keeps its number");
        assert!(is_gep_kind(KIND_SIGNAL) && !is_addressable(KIND_SIGNAL));
        assert_eq!(KIND_ADDON, 33407);
        assert_eq!(KIND_DRAWING, 33401);
        for reissued in [KIND_ADDON, KIND_DRAWING] {
            assert!(is_gep_kind(reissued));
            assert!(is_addressable(reissued));
            assert!(!is_human_kind(reissued) && !is_manager_kind(reissued));
            assert!(!requires_owner_encryption(reissued));
        }
    }

    #[test]
    fn a_run_snapshot_is_addressable_and_a_step_fact_is_not() {
        assert!(is_addressable(KIND_WORKFLOW));
        assert!(is_addressable(KIND_WORKFLOW_RUN));
        assert!(!is_addressable(KIND_STEP));
        assert!(is_gep_kind(KIND_STEP));
        assert!(is_addressable(KIND_CONNECTOR));
        assert!(is_addressable(KIND_CONVERSATION));
        // The counts 09-protocol-gep.md states: twenty-six kinds, fourteen addressable.
        assert_eq!(GEP_KINDS.len(), 26);
        assert_eq!(ADDRESSABLE_KINDS.len(), 14);
    }

    #[test]
    fn ephemeral_kinds_are_never_addressable() {
        for k in EPHEMERAL_KINDS {
            assert!(!is_addressable(*k));
        }
    }
}
