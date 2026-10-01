//! The facts a host relays: every conversation fact in the workspace's
//! logs, read from the truth files — and the one rule that says which of
//! them a person reaches.
//!
//! Only conversation facts travel to a hosted member: messages, reactions
//! and retractions in the channels their role reaches. Goals, journals,
//! agents, workflows and the rest stay on the host — a person on another
//! node is a human in the workspace's channels, not a replica of it.
//! Channel definitions travel as the host's word (`ChannelsChanged`), not as
//! events.

use crate::NetError;
use bisa_core::{reaches, ChannelId, MemberRole, PrincipalId};
use bisa_store::Workspace;
use nostr::event::Event;

/// Every conversation fact in this workspace's logs, one log per scope,
/// in file order. Reached through [`bisa_store::Paths`], never spelled.
pub fn conversation_facts(ws: &Workspace) -> Result<Vec<Event>, NetError> {
    let mut facts: Vec<Event> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(ws.paths().conversation_dir()) {
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(body) = std::fs::read_to_string(&path) {
                for line in body.lines().filter(|l| !l.trim().is_empty()) {
                    if let Ok(ev) = Event::from_json(line) {
                        facts.push(ev);
                    }
                }
            }
        }
    }
    Ok(facts)
}

/// The scope a conversation fact is in: the last part of its `a`
/// coordinate when that coordinate names a channel.
pub fn scope_of(event: &Event) -> Option<ChannelId> {
    let coord = event.tags.iter().find_map(|t| {
        let s = t.as_slice();
        (s.len() >= 2 && s[0] == "a").then(|| s[1].as_str())
    })?;
    let mut parts = coord.splitn(3, ':');
    let (Some(kind), Some(_pk), Some(scope)) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    if kind.parse::<u16>().ok() != Some(bisa_core::kind::KIND_CHANNEL) {
        return None;
    }
    ChannelId::new(scope).ok()
}

/// Whether a person at `role` reaches the channel a fact is in — the one
/// rule ([`bisa_core::reaches`]) applied to an event. A fact outside any
/// channel reaches nobody on another node.
pub fn reaches_fact(ws: &Workspace, event: &Event, person: &PrincipalId, role: MemberRole) -> bool {
    let Some(scope) = scope_of(event) else {
        return false;
    };
    match ws.get_channel(&scope) {
        Ok(channel) => reaches(&channel, role, person),
        Err(_) => false,
    }
}
