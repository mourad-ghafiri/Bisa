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
            } // LCOV_EXCL_LINE: a log unreadable under the scan; a disk arm
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

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::kind::KIND_MESSAGE;
    use bisa_store::MemoryKeyStore;
    use nostr::event::{EventBuilder, FinalizeEvent, Kind, Tag};
    use nostr::key::Keys;

    fn fact(keys: &Keys, coordinate: Option<&str>) -> Event {
        let builder = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}");
        match coordinate {
            Some(a) => builder.tags([Tag::parse(["a", a]).unwrap()]),
            None => builder,
        }
        .finalize(keys)
        .unwrap()
    }

    /// The scan reads the `.jsonl` logs and the event lines in them, and
    /// nothing else that sits in the folder: a file of another kind and a
    /// line that is no event are passed over, never a reason to stop.
    #[test]
    fn the_scan_passes_over_what_is_no_log_and_what_is_no_event() {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        ws.post_message(
            "general",
            bisa_core::MessageBody::post("one"),
            None,
            &[],
            &[],
            None,
            bisa_store::PostOrigin::Asked,
        )
        .unwrap();
        let logs = ws.paths().conversation_dir();
        std::fs::write(logs.join("notes.txt"), "not a log\n").unwrap();
        let other = fact(&Keys::generate(), Some("33405:x:general"));
        std::fs::write(
            logs.join("zz-other.jsonl"),
            format!("\n{{\"not\": \"an event\"}}\n{}\n", other.as_json()),
        )
        .unwrap();
        let facts = conversation_facts(&ws).unwrap();
        assert_eq!(
            facts.len(),
            2,
            "the post and the one event line of the other log"
        );
        assert!(facts.iter().any(|f| f.id == other.id));
    }

    /// A fact's scope is the last part of a channel coordinate; a coordinate
    /// of fewer parts, or of another kind, names no scope — and a fact with
    /// no scope, or one in a channel this workspace does not have, reaches
    /// nobody.
    #[test]
    fn a_scope_is_a_channel_coordinate_s_last_part_and_nothing_else_reaches() {
        let dir = tempfile::tempdir().unwrap();
        let ws =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        let keys = Keys::generate();
        let person = PrincipalId::new(keys.public_key().to_hex()).unwrap();
        assert_eq!(
            scope_of(&fact(&keys, Some("33405:pk:general"))).map(|c| c.to_string()),
            Some("general".to_string())
        );
        assert_eq!(scope_of(&fact(&keys, Some("33405:pk"))), None, "two parts");
        assert_eq!(
            scope_of(&fact(&keys, Some("33400:pk:goal"))),
            None,
            "a goal's kind"
        );
        assert_eq!(
            scope_of(&fact(&keys, Some("33405:pk:"))),
            None,
            "an empty id"
        );
        assert!(!reaches_fact(
            &ws,
            &fact(&keys, None),
            &person,
            MemberRole::Member
        ));
        assert!(
            !reaches_fact(
                &ws,
                &fact(&keys, Some("33405:pk:nope")),
                &person,
                MemberRole::Member
            ),
            "a channel this workspace does not have"
        );
        assert!(reaches_fact(
            &ws,
            &fact(&keys, Some("33405:pk:general")),
            &person,
            MemberRole::Member
        ));
    }
}
