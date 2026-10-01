//! The guest replica: what this node keeps of one workspace it joined on
//! another node, at `<data_dir>/hosts/<host-pubkey>/`.
//!
//! - `host.json` — the host's card, the role granted, where the membership
//!   stands ([`Hosted`]);
//! - `channels.json` — the channels the host said this person reaches, whole,
//!   as of the last `Welcome` or `ChannelsChanged`;
//! - `members.json` — the people, as of the last directory the host sent;
//! - `conversations/<scope>.jsonl` — every conversation fact the host
//!   relayed and every one this person signed, verbatim;
//! - `seen.jsonl` — the ids already applied, so a catch-up replays nothing;
//! - `read.json` — where this person has read up to, per scope.
//!
//! Files are the truth; there is no index. A hosted membership is small —
//! a handful of channels, hundreds of messages — and a fold over one scope's
//! log is what a screen needs. Every write is atomic (write, then rename).

use crate::GuestError;
use bisa_collab::{Directory, HostCard};
use bisa_core::kind::{KIND_MESSAGE, KIND_REACTION, KIND_RETRACTION};
use bisa_core::{
    validate_slug_id, ArtifactRef, AttachmentRef, Channel, ChannelKind, MemberRole, MessageBody,
    PrincipalId,
};
use nostr::event::Event;
use nostr::key::PublicKey;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where a hosted membership stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum HostedState {
    /// The code was sent; the host has not answered yet, or asked to admit
    /// by hand.
    Requested,
    /// Admitted: the host serves what the role reaches.
    Member,
    /// The host declined the code, and said why.
    Refused { reason: String },
    /// The host removed this person.
    Removed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// This person left.
    Left,
}

impl HostedState {
    pub fn is_member(&self) -> bool {
        matches!(self, HostedState::Member)
    }

    /// The words a sidebar section shows under the host's name.
    pub fn words(&self) -> String {
        match self {
            HostedState::Requested => "waiting for the host".into(),
            HostedState::Member => String::new(),
            HostedState::Refused { reason } => format!("refused — {reason}"),
            HostedState::Removed { reason: Some(r) } => format!("removed — {r}"),
            HostedState::Removed { reason: None } => "removed by the host".into(),
            HostedState::Left => "you left".into(),
        }
    }
}

/// One workspace this person is hosted on: `host.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Hosted {
    pub host: HostCard,
    pub role: MemberRole,
    pub state: HostedState,
    /// When the code was sent.
    pub requested_at: u64,
    /// When the host welcomed this person; absent until it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<u64>,
    /// What this person asked to be called there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// One reaction on a hosted message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HostedReaction {
    pub author: PrincipalId,
    pub emoji: String,
}

/// A message in a hosted scope as a screen reads it: the fold of its event,
/// its reactions and its retraction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HostedMessage {
    pub id: String,
    pub scope: String,
    pub author: PrincipalId,
    pub at: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<PrincipalId>,
    /// Descriptors only: the bytes stay on the host.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<HostedReaction>,
    pub retracted: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct ReadFile {
    /// scope → unix seconds read up to.
    marks: HashMap<String, u64>,
}

/// The replica of one host.
pub struct GuestStore {
    dir: PathBuf,
    host_hex: String,
    seen: Mutex<HashSet<String>>,
}

const HOST_FILE: &str = "host.json";
const CHANNELS_FILE: &str = "channels.json";
const MEMBERS_FILE: &str = "members.json";
const SEEN_FILE: &str = "seen.jsonl";
const READ_FILE: &str = "read.json";
const CONVERSATIONS_DIR: &str = "conversations";

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), GuestError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn append_line(path: &Path, line: &str) -> Result<(), GuestError> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

fn read_json_or<T: serde::de::DeserializeOwned>(path: &Path, fallback: T) -> Result<T, GuestError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(fallback),
        Err(e) => Err(e.into()),
    }
}

/// The value of the first tag named `name`.
pub(crate) fn tag_value<'a>(event: &'a Event, name: &str) -> Option<&'a str> {
    event.tags.iter().find_map(|t| {
        let s = t.as_slice();
        (s.len() >= 2 && s[0] == name).then(|| s[1].as_str())
    })
}

/// The scope a conversation fact is in: the last part of its `a`
/// coordinate, `33405:<host>:<scope>`. `None` when the tag is missing or
/// names a scope that is not a channel id.
pub fn scope_of(event: &Event) -> Option<String> {
    let coord = tag_value(event, "a")?;
    let mut parts = coord.splitn(3, ':');
    let (Some(kind), Some(_pk), Some(scope)) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    if kind.parse::<u16>().ok() != Some(bisa_core::kind::KIND_CHANNEL) {
        return None;
    }
    validate_slug_id("channel", scope).ok()?;
    Some(scope.to_string())
}

/// Every `imeta` tag as a descriptor; a malformed one is dropped.
fn imeta_tags(event: &Event) -> Vec<AttachmentRef> {
    let mut out = Vec::new();
    for tag in event.tags.iter() {
        let parts = tag.as_slice();
        if parts.first().map(String::as_str) != Some("imeta") {
            continue;
        }
        let (mut sha256, mut mime, mut size, mut name) = (None, None, None, None);
        for field in &parts[1..] {
            let Some((key, value)) = field.split_once(' ') else {
                continue;
            };
            match key {
                "x" => sha256 = Some(value.to_string()),
                "m" => mime = Some(value.to_string()),
                "size" => size = value.parse::<u64>().ok(),
                "name" => name = Some(value.to_string()),
                _ => {}
            }
        }
        let (Some(sha256), Some(mime), Some(size), Some(name)) = (sha256, mime, size, name) else {
            continue;
        };
        if !AttachmentRef::is_valid_hash(&sha256) {
            continue;
        }
        out.push(AttachmentRef {
            sha256,
            name,
            mime,
            size,
        });
    }
    out
}

impl GuestStore {
    /// Open (or make) the replica of `host` under `hosts_root`.
    pub fn open(hosts_root: &Path, host: &PublicKey) -> Result<Self, GuestError> {
        let host_hex = host.to_hex();
        let dir = hosts_root.join(&host_hex);
        std::fs::create_dir_all(dir.join(CONVERSATIONS_DIR))?;
        let mut seen = HashSet::new();
        if let Ok(body) = std::fs::read_to_string(dir.join(SEEN_FILE)) {
            seen.extend(
                body.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string),
            );
        }
        Ok(Self {
            dir,
            host_hex,
            seen: Mutex::new(seen),
        })
    }

    /// The host pubkeys with a replica under `hosts_root`, in name order.
    pub fn list(hosts_root: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(hosts_root) else {
            return Vec::new();
        };
        let mut out: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().join(HOST_FILE).is_file())
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|name| PublicKey::from_hex(name).is_ok())
            .collect();
        out.sort();
        out
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn host_hex(&self) -> &str {
        &self.host_hex
    }

    // -- host, channels, people ------------------------------------------

    pub fn hosted(&self) -> Result<Option<Hosted>, GuestError> {
        read_json_or(&self.dir.join(HOST_FILE), None)
    }

    pub fn set_hosted(&self, hosted: &Hosted) -> Result<(), GuestError> {
        write_atomic(
            &self.dir.join(HOST_FILE),
            &serde_json::to_vec_pretty(hosted)?,
        )
    }

    pub fn channels(&self) -> Result<Vec<Channel>, GuestError> {
        read_json_or(&self.dir.join(CHANNELS_FILE), Vec::new())
    }

    pub fn set_channels(&self, channels: &[Channel]) -> Result<(), GuestError> {
        write_atomic(
            &self.dir.join(CHANNELS_FILE),
            &serde_json::to_vec_pretty(channels)?,
        )
    }

    pub fn channel(&self, scope: &str) -> Result<Option<Channel>, GuestError> {
        Ok(self
            .channels()?
            .into_iter()
            .find(|c| c.id.as_str() == scope))
    }

    /// The standing channels, then the direct ones, each in the host's order.
    pub fn channels_of_kind(&self, kind: ChannelKind) -> Result<Vec<Channel>, GuestError> {
        Ok(self
            .channels()?
            .into_iter()
            .filter(|c| c.kind == kind)
            .collect())
    }

    pub fn members(&self) -> Result<Vec<Directory>, GuestError> {
        read_json_or(&self.dir.join(MEMBERS_FILE), Vec::new())
    }

    pub fn set_members(&self, members: &[Directory]) -> Result<(), GuestError> {
        write_atomic(
            &self.dir.join(MEMBERS_FILE),
            &serde_json::to_vec_pretty(members)?,
        )
    }

    // -- facts --------------------------------------------------------------

    fn conversation_path(&self, scope: &str) -> PathBuf {
        self.dir
            .join(CONVERSATIONS_DIR)
            .join(format!("{scope}.jsonl"))
    }

    /// Keep one conversation fact in its scope's log. `false` when it was
    /// already here. The scope is the event's own (`a` tag); an event with
    /// no scope is refused.
    pub fn append(&self, event: &Event) -> Result<bool, GuestError> {
        let scope = scope_of(event)
            .ok_or_else(|| GuestError::Store("a fact with no channel scope".into()))?;
        let id = event.id.to_hex();
        {
            let mut seen = self
                .seen
                .lock()
                .map_err(|_| GuestError::Store("seen".into()))?;
            if !seen.insert(id.clone()) {
                return Ok(false);
            }
        }
        append_line(&self.conversation_path(&scope), &event.as_json())?;
        append_line(&self.dir.join(SEEN_FILE), &id)?;
        Ok(true)
    }

    pub fn has_seen(&self, event_id: &str) -> bool {
        self.seen
            .lock()
            .map(|s| s.contains(event_id))
            .unwrap_or(false)
    }

    /// Every fact in a scope, in arrival order.
    pub fn events(&self, scope: &str) -> Result<Vec<Event>, GuestError> {
        validate_slug_id("channel", scope).map_err(|e| GuestError::Store(e.to_string()))?;
        let body = match std::fs::read_to_string(self.conversation_path(scope)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        Ok(body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| Event::from_json(l).ok())
            .collect())
    }

    /// The fact with this id, wherever it is.
    pub fn event(&self, event_id: &str) -> Result<Option<Event>, GuestError> {
        if !self.has_seen(event_id) {
            return Ok(None);
        }
        for channel in self.channels()? {
            if let Some(ev) = self
                .events(channel.id.as_str())?
                .into_iter()
                .find(|e| e.id.to_hex() == event_id)
            {
                return Ok(Some(ev));
            }
        }
        Ok(None)
    }

    /// The messages of a scope as a screen reads them, oldest first: those
    /// at or before `before` (unix seconds; `None` = the latest), the last
    /// `limit` of them.
    pub fn messages(
        &self,
        scope: &str,
        before: Option<u64>,
        limit: usize,
    ) -> Result<Vec<HostedMessage>, GuestError> {
        let events = self.events(scope)?;
        let mut messages: Vec<HostedMessage> = Vec::new();
        let mut reactions: Vec<(String, HostedReaction)> = Vec::new();
        let mut retracted: HashSet<String> = HashSet::new();
        for ev in &events {
            let Ok(author) = PrincipalId::new(ev.pubkey.to_hex()) else {
                continue;
            };
            match ev.kind.as_u16() {
                k if k == KIND_MESSAGE => {
                    let Ok(MessageBody::Post {
                        text, artifacts, ..
                    }) = serde_json::from_str::<MessageBody>(&ev.content)
                    else {
                        continue;
                    };
                    let mentions = ev
                        .tags
                        .iter()
                        .filter_map(|t| {
                            let s = t.as_slice();
                            (s.len() >= 2 && s[0] == "p")
                                .then(|| PrincipalId::new(s[1].clone()).ok())
                                .flatten()
                        })
                        .collect();
                    messages.push(HostedMessage {
                        id: ev.id.to_hex(),
                        scope: scope.to_string(),
                        author,
                        at: ev.created_at.as_secs(),
                        text,
                        reply_to: tag_value(ev, "e").map(str::to_string),
                        mentions,
                        attachments: imeta_tags(ev),
                        artifacts,
                        reactions: Vec::new(),
                        retracted: false,
                    });
                }
                k if k == KIND_REACTION => {
                    if let Some(target) = tag_value(ev, "e") {
                        reactions.push((
                            target.to_string(),
                            HostedReaction {
                                author,
                                emoji: ev.content.clone(),
                            },
                        ));
                    }
                }
                k if k == KIND_RETRACTION => {
                    if let Some(target) = tag_value(ev, "e") {
                        retracted.insert(target.to_string());
                    }
                }
                _ => {}
            }
        }
        // A retracted reaction is gone; a retracted message stays as a stub.
        let retracted_reactions: HashSet<String> = events
            .iter()
            .filter(|e| e.kind.as_u16() == KIND_REACTION)
            .map(|e| e.id.to_hex())
            .filter(|id| retracted.contains(id))
            .collect();
        let live_reactions: Vec<(String, HostedReaction)> = events
            .iter()
            .filter(|e| {
                e.kind.as_u16() == KIND_REACTION && !retracted_reactions.contains(&e.id.to_hex())
            })
            .filter_map(|e| {
                let target = tag_value(e, "e")?.to_string();
                let author = PrincipalId::new(e.pubkey.to_hex()).ok()?;
                Some((
                    target,
                    HostedReaction {
                        author,
                        emoji: e.content.clone(),
                    },
                ))
            })
            .collect();
        drop(reactions);
        for m in &mut messages {
            if retracted.contains(&m.id) {
                m.retracted = true;
                m.text.clear();
                m.attachments.clear();
                m.artifacts.clear();
            }
            for (target, r) in &live_reactions {
                if *target == m.id && !m.reactions.contains(r) {
                    m.reactions.push(r.clone());
                }
            }
        }
        messages.sort_by_key(|m| m.at);
        let mut page: Vec<HostedMessage> = match before {
            Some(t) => messages.into_iter().filter(|m| m.at <= t).collect(),
            None => messages,
        };
        if page.len() > limit {
            page.drain(..page.len() - limit);
        }
        Ok(page)
    }

    /// When the latest message in a scope was posted.
    pub fn latest_at(&self, scope: &str) -> Result<Option<u64>, GuestError> {
        Ok(self
            .events(scope)?
            .iter()
            .filter(|e| e.kind.as_u16() == KIND_MESSAGE)
            .map(|e| e.created_at.as_secs())
            .max())
    }

    // -- read marks --------------------------------------------------------

    fn read_file(&self) -> Result<ReadFile, GuestError> {
        read_json_or(&self.dir.join(READ_FILE), ReadFile::default())
    }

    /// Mark a scope read up to `at`.
    pub fn mark_read(&self, scope: &str, at: u64) -> Result<(), GuestError> {
        let mut file = self.read_file()?;
        file.marks.insert(scope.to_string(), at);
        write_atomic(
            &self.dir.join(READ_FILE),
            &serde_json::to_vec_pretty(&file)?,
        )
    }

    pub fn read_mark(&self, scope: &str) -> Result<Option<u64>, GuestError> {
        Ok(self.read_file()?.marks.get(scope).copied())
    }

    /// Messages by others after the read mark.
    pub fn unread(&self, scope: &str, me: &PrincipalId) -> Result<usize, GuestError> {
        let mark = self.read_mark(scope)?.unwrap_or(0);
        Ok(self
            .events(scope)?
            .iter()
            .filter(|e| e.kind.as_u16() == KIND_MESSAGE)
            .filter(|e| e.created_at.as_secs() > mark)
            .filter(|e| e.pubkey.to_hex() != me.as_hex())
            .count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::{EventBuilder, FinalizeEvent, Kind, Tag};
    use nostr::key::Keys;

    fn fact(keys: &Keys, kind: u16, content: &str, scope: &str, e: Option<&str>, at: u64) -> Event {
        let mut tags = vec![Tag::parse([
            "a",
            &format!("33405:{}:{scope}", keys.public_key().to_hex()),
        ])
        .unwrap()];
        if let Some(id) = e {
            tags.push(Tag::parse(["e", id]).unwrap());
        }
        EventBuilder::new(Kind::Custom(kind), content)
            .tags(tags)
            .custom_created_at(nostr::types::Timestamp::from_secs(at))
            .finalize(keys)
            .unwrap()
    }

    fn post(keys: &Keys, text: &str, scope: &str, at: u64) -> Event {
        let body = serde_json::to_string(&MessageBody::post(text)).unwrap();
        fact(keys, KIND_MESSAGE, &body, scope, None, at)
    }

    #[test]
    fn a_replica_round_trips_its_host_channels_and_people() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        assert!(store.hosted().unwrap().is_none());
        let hosted = Hosted {
            host: HostCard {
                pubkey: PrincipalId::new(host.public_key().to_hex()).unwrap(),
                name: "Acme".into(),
                relays: vec!["wss://r.example".into()],
            },
            role: MemberRole::Guest,
            state: HostedState::Member,
            requested_at: 1,
            joined_at: Some(2),
            label: Some("Bob".into()),
        };
        store.set_hosted(&hosted).unwrap();
        assert_eq!(store.hosted().unwrap(), Some(hosted));
        store.set_channels(&[Channel::general(0)]).unwrap();
        assert_eq!(store.channels().unwrap().len(), 1);
        assert!(store.channel("general").unwrap().is_some());
        assert!(store.channel("nope").unwrap().is_none());
        store
            .set_members(&[Directory {
                pubkey: PrincipalId::new(host.public_key().to_hex()).unwrap(),
                role: MemberRole::Owner,
                label: None,
                photo: None,
            }])
            .unwrap();
        assert_eq!(store.members().unwrap().len(), 1);
        assert_eq!(
            GuestStore::list(root.path()),
            vec![host.public_key().to_hex()]
        );
    }

    #[test]
    fn facts_are_kept_once_and_fold_into_messages_with_reactions_and_retractions() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let alice = Keys::generate();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        store.set_channels(&[Channel::general(0)]).unwrap();
        let m1 = post(&host, "hello", "general", 10);
        let m2 = post(&alice, "hi back", "general", 20);
        assert!(store.append(&m1).unwrap());
        assert!(!store.append(&m1).unwrap(), "a second append is a no-op");
        assert!(store.append(&m2).unwrap());
        let react = fact(
            &alice,
            KIND_REACTION,
            "👍",
            "general",
            Some(&m1.id.to_hex()),
            21,
        );
        store.append(&react).unwrap();
        let retract = fact(
            &alice,
            KIND_RETRACTION,
            "",
            "general",
            Some(&m2.id.to_hex()),
            22,
        );
        store.append(&retract).unwrap();

        let all = store.messages("general", None, 50).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].text, "hello");
        assert_eq!(all[0].reactions.len(), 1);
        assert_eq!(all[0].reactions[0].emoji, "👍");
        assert!(all[1].retracted);
        assert!(all[1].text.is_empty());
        let page = store.messages("general", Some(15), 50).unwrap();
        assert_eq!(page.len(), 1);
        let last = store.messages("general", None, 1).unwrap();
        assert_eq!(last[0].id, m2.id.to_hex());
        assert_eq!(store.latest_at("general").unwrap(), Some(20));
        assert!(store.event(&m1.id.to_hex()).unwrap().is_some());
        assert!(store.has_seen(&react.id.to_hex()));

        // Reopening reads the same seen set back.
        let again = GuestStore::open(root.path(), &host.public_key()).unwrap();
        assert!(!again.append(&m1).unwrap());
    }

    #[test]
    fn read_marks_count_what_others_said_after_them() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let me = Keys::generate();
        let me_id = PrincipalId::new(me.public_key().to_hex()).unwrap();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        store.append(&post(&host, "one", "general", 10)).unwrap();
        store.append(&post(&me, "mine", "general", 11)).unwrap();
        store.append(&post(&host, "two", "general", 12)).unwrap();
        assert_eq!(store.unread("general", &me_id).unwrap(), 2);
        store.mark_read("general", 10).unwrap();
        assert_eq!(store.read_mark("general").unwrap(), Some(10));
        assert_eq!(store.unread("general", &me_id).unwrap(), 1);
    }

    #[test]
    fn a_fact_without_a_channel_scope_is_refused_and_a_bad_scope_reads_nothing() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        let bare = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "{}")
            .finalize(&host)
            .unwrap();
        assert!(store.append(&bare).is_err());
        assert!(scope_of(&bare).is_none());
        assert!(store.events("../etc").is_err());
    }
}
