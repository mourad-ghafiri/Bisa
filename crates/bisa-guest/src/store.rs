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
//! - `seen.jsonl` — the ids of the facts already applied, so a catch-up
//!   applies none twice;
//! - `read.json` — where this person has read up to, per scope;
//! - `heard.json` — when the newest of the host's words of each kind was
//!   said ([`Heard`]), so an older one handed back by a relay changes
//!   nothing;
//! - `unsent.json` — the ids of this person's own facts no relay has taken
//!   yet, in the order they were said;
//! - `waiting.jsonl` — facts that arrived before the room they are in was
//!   named, kept for it;
//! - `conversations/<scope>.<secs>.gone` — the log of a room the host
//!   deleted, or made again under the same id: kept, and read by nothing.
//!
//! Files are the truth; there is no index. A hosted membership is small —
//! a handful of channels, hundreds of messages — and a fold over one scope's
//! log is what a screen needs. A file that is replaced is written whole and
//! renamed into place; a log takes one line at a time, each on its own line.
//! A record that does not parse — `host.json`, `read.json`, `heard.json` —
//! is moved aside beside itself and read as absent: left where it was, it
//! failed every word and every read for good.

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
    /// This person's own, and no relay has taken it yet: it is kept here and
    /// sent on its own when one answers.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unsent: bool,
}

/// When the newest of the host's words of each kind was said, by the host's
/// own clock (the word's `at`). A relay hands words back in any order — after
/// a restart, every one it kept — and a word older than the newest of its
/// kind already applied changes nothing.
///
/// Four kinds, since a word of one says nothing of another: where the
/// membership stands (welcome, waiting, refused, removed), the role, the
/// rooms, the people. A welcome speaks to all four, each part against its
/// own mark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heard {
    /// The membership's newest word: its time, and its rank among words of
    /// one second ([`Heard::takes_membership`]).
    #[serde(default)]
    pub membership: (u64, u8),
    #[serde(default)]
    pub role: u64,
    #[serde(default)]
    pub channels: u64,
    #[serde(default)]
    pub members: u64,
}

impl Heard {
    /// Whether a membership word said at `at`, of `rank`, is newer than the
    /// one that stands. Strictly: the same word handed back changes
    /// nothing. Two of one second are told apart by rank — *waiting*, then
    /// *refused*, then *welcome*, then *removed*: the order a host says them in.
    pub fn takes_membership(&self, at: u64, rank: u8) -> bool {
        (at, rank) > self.membership
    }
}

/// A fact kept for a room not named yet, at most this many: the newest stay.
const MAX_WAITING: usize = 512;

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
const HEARD_FILE: &str = "heard.json";
const UNSENT_FILE: &str = "unsent.json";
const WAITING_FILE: &str = "waiting.jsonl";
const BIRTHS_FILE: &str = "births.json";
const CONVERSATIONS_DIR: &str = "conversations";
/// Where a forgotten membership's replica is kept, beside `hosts/`.
pub const FORGOTTEN_DIR: &str = "hosts-forgotten";

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), GuestError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    } // LCOV_EXCL_LINE a path with no parent; by construction
      // A name of its own for every write — two writers of one file in one
      // process never share a temporary — and the bytes on disk before the
      // rename, so what the name points at after a crash is a whole file.
    use std::io::Write;
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{n}", std::process::id()));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn append_line(path: &Path, line: &str) -> Result<(), GuestError> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    } // LCOV_EXCL_LINE a path with no parent; by construction
      // A line cut short by a stop — no newline after it — is ended before
      // the next is written. Glued to it, the next fact parsed as neither, and
      // both were lost: the one that was torn and the one that was sound.
    let torn = match std::fs::read(path) {
        Ok(bytes) => bytes.last().is_some_and(|b| *b != b'\n'),
        Err(_) => false,
    };
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    if torn {
        writeln!(f)?;
    }
    writeln!(f, "{line}")?;
    Ok(())
}

/// A record read whole, `fallback` when it is not there — and when it is
/// there and does not parse: it is moved aside beside itself
/// (`<name>.unreadable.<secs>`), said in the log, and read as absent. An
/// I/O error stays an error: a file nobody may read is not a broken one.
fn read_json_or<T: serde::de::DeserializeOwned>(path: &Path, fallback: T) -> Result<T, GuestError> {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(value),
            Err(e) => {
                let aside = path.with_extension(format!("unreadable.{}", now_secs()));
                std::fs::rename(path, &aside)?;
                // LCOV_EXCL_START a tracing event's fields, evaluated under a subscriber only
                tracing::warn!(
                    "{} did not read ({e}); moved to {} and read as absent",
                    path.display(),
                    aside.display()
                );
                // LCOV_EXCL_STOP
                Ok(fallback)
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(fallback),
        Err(e) => Err(e.into()),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
        // And every fact a log holds: a stop between a fact's line and its
        // id's left the fact kept and not seen, and it was kept a second
        // time when a relay handed it back.
        let logs = std::fs::read_dir(dir.join(CONVERSATIONS_DIR))?;
        for log in logs.flatten().map(|e| e.path()) {
            // A log moved aside (`.gone`) is read by nothing.
            if log.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            seen.extend(
                std::fs::read_to_string(&log)
                    .unwrap_or_default()
                    .lines()
                    .filter_map(|l| Event::from_json(l).ok())
                    .map(|e| e.id.to_hex()),
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

    /// Take the membership's record away — a claim that never left a node
    /// with nothing recorded before it. The replica's folder stays.
    pub fn unset_hosted(&self) -> Result<(), GuestError> {
        match std::fs::remove_file(self.dir.join(HOST_FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()), // LCOV_EXCL_LINE a record that cannot be removed; disk-only
        }
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

    /// The rooms as the host says them now, whole. A room that comes back
    /// with another birth (`created_at`: deleted on the host and made again
    /// under the id) has its log moved aside
    /// (`conversations/<scope>.<secs>.gone`): the room made again starts
    /// empty here as it does on the host, and the old words are kept, read
    /// by nothing. The births are remembered past a room's leaving the list
    /// (`births.json`), so the rule holds when the copy heard the list
    /// without the room in between — and a room this person was taken off
    /// and put back on, the same room, keeps what it held.
    pub fn set_channels(&self, channels: &[Channel]) -> Result<(), GuestError> {
        let births_path = self.dir.join(BIRTHS_FILE);
        let mut births: HashMap<String, u64> = read_json_or(&births_path, HashMap::new())?;
        // A copy older than the record of births: what it lists is what it knows.
        for was in self.channels()? {
            births.entry(was.id.to_string()).or_insert(was.created_at);
        }
        let mut moved = false;
        for now in channels {
            let known = births.insert(now.id.to_string(), now.created_at);
            moved |= known != Some(now.created_at);
            if known.is_some_and(|born| born != now.created_at) {
                let log = self.conversation_path(now.id.as_str());
                if log.exists() {
                    let aside = log.with_extension(format!("{}.gone", now_secs()));
                    std::fs::rename(&log, &aside)?;
                }
            }
        }
        if moved {
            write_atomic(&births_path, &serde_json::to_vec(&births)?)?;
        }
        write_atomic(
            &self.dir.join(CHANNELS_FILE),
            &serde_json::to_vec_pretty(channels)?,
        )
    }

    // -- what was heard, what waits to be sent, what waits for its room -----

    pub fn heard(&self) -> Result<Heard, GuestError> {
        read_json_or(&self.dir.join(HEARD_FILE), Heard::default())
    }

    pub fn set_heard(&self, heard: &Heard) -> Result<(), GuestError> {
        write_atomic(&self.dir.join(HEARD_FILE), &serde_json::to_vec(heard)?)
    }

    /// The ids of this person's facts no relay has taken, oldest first.
    pub fn unsent(&self) -> Result<Vec<String>, GuestError> {
        read_json_or(&self.dir.join(UNSENT_FILE), Vec::new())
    }

    fn set_unsent(&self, ids: &[String]) -> Result<(), GuestError> {
        write_atomic(&self.dir.join(UNSENT_FILE), &serde_json::to_vec(ids)?)
    }

    /// Note a fact as not sent yet, once, after the ones already waiting.
    pub fn mark_unsent(&self, event_id: &str) -> Result<(), GuestError> {
        let mut ids = self.unsent()?;
        if !ids.iter().any(|id| id == event_id) {
            ids.push(event_id.to_string());
            self.set_unsent(&ids)?;
        }
        Ok(())
    }

    /// A relay took it.
    pub fn clear_unsent(&self, event_id: &str) -> Result<(), GuestError> {
        let mut ids = self.unsent()?;
        let before = ids.len();
        ids.retain(|id| id != event_id);
        if ids.len() != before {
            self.set_unsent(&ids)?;
        }
        Ok(())
    }

    /// Keep a fact for a room the host has not named yet — it arrived
    /// before the word that names its room. Bounded: the newest
    /// [`MAX_WAITING`] stay.
    pub fn park(&self, event: &Event) -> Result<(), GuestError> {
        let path = self.dir.join(WAITING_FILE);
        let mut lines: Vec<String> = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect();
        let line = event.as_json();
        if lines.contains(&line) {
            return Ok(());
        }
        lines.push(line);
        if lines.len() > MAX_WAITING {
            lines.drain(..lines.len() - MAX_WAITING);
        }
        write_atomic(&path, (lines.join("\n") + "\n").as_bytes())
    }

    /// The facts kept for rooms now named, taken off the waiting list; the
    /// rest wait on.
    pub fn take_parked(&self) -> Result<Vec<Event>, GuestError> {
        let path = self.dir.join(WAITING_FILE);
        let body = match std::fs::read_to_string(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()), // LCOV_EXCL_LINE the waiting list unreadable; disk-only
        };
        let rooms: HashSet<String> = self
            .channels()?
            .into_iter()
            .map(|c| c.id.to_string())
            .collect();
        let (mut ready, mut waits) = (Vec::new(), Vec::new());
        for line in body.lines().filter(|l| !l.trim().is_empty()) {
            match Event::from_json(line) {
                Ok(ev) if scope_of(&ev).is_some_and(|s| rooms.contains(&s)) => ready.push(ev),
                Ok(_) => waits.push(line.to_string()),
                // A line that is no fact — torn by a stop — is dropped at
                // the next hand-over.
                Err(_) => continue,
            }
        }
        if !ready.is_empty() {
            let mut out = waits.join("\n");
            if !out.is_empty() {
                out.push('\n');
            }
            write_atomic(&path, out.as_bytes())?;
        }
        Ok(ready)
    }

    /// Forget a membership: its replica is moved out of `hosts/` to
    /// `hosts-forgotten/<host>.<secs>/` beside it — kept whole, listed and
    /// read by nothing. Answers where it went.
    pub fn forget(hosts_root: &Path, host_hex: &str) -> Result<PathBuf, GuestError> {
        let from = hosts_root.join(host_hex);
        let aside = hosts_root
            .parent()
            .unwrap_or(hosts_root)
            .join(FORGOTTEN_DIR);
        std::fs::create_dir_all(&aside)?;
        let mut to = aside.join(format!("{host_hex}.{}", now_secs()));
        let mut n = 1;
        while to.exists() {
            to = aside.join(format!("{host_hex}.{}.{n}", now_secs()));
            n += 1;
        }
        std::fs::rename(&from, &to)?;
        Ok(to)
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
        let unsent: HashSet<String> = self.unsent()?.into_iter().collect();
        // A fact is shown once, however often its line was written.
        let mut drawn: HashSet<String> = HashSet::new();
        for ev in &events {
            if !drawn.insert(ev.id.to_hex()) {
                continue;
            }
            let Ok(author) = PrincipalId::new(ev.pubkey.to_hex()) else {
                continue; // LCOV_EXCL_LINE a relayed key is always a principal; by construction
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
                        unsent: unsent.contains(&ev.id.to_hex()),
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

    /// Mark a scope read up to `at`. The mark never moves backwards, as the
    /// host's own never does: a read of an older moment leaves it standing.
    pub fn mark_read(&self, scope: &str, at: u64) -> Result<(), GuestError> {
        let mut file = self.read_file()?;
        let mark = file.marks.entry(scope.to_string()).or_insert(0);
        if at <= *mark {
            return Ok(());
        }
        *mark = at;
        write_atomic(
            &self.dir.join(READ_FILE),
            &serde_json::to_vec_pretty(&file)?,
        )
    }

    pub fn read_mark(&self, scope: &str) -> Result<Option<u64>, GuestError> {
        Ok(self.read_file()?.marks.get(scope).copied())
    }

    /// Messages by others after the read mark — the messages a screen is
    /// shown ([`Self::messages`]): a join or a leave is no post, and a post
    /// taken back is nothing to read. Counted from the raw facts, the badge
    /// named messages the thread did not draw.
    pub fn unread(&self, scope: &str, me: &PrincipalId) -> Result<usize, GuestError> {
        let mark = self.read_mark(scope)?.unwrap_or(0);
        Ok(self
            .messages(scope, None, usize::MAX)?
            .iter()
            .filter(|m| !m.retracted && m.at > mark && m.author != *me)
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

    fn room(id: &str, born: u64) -> Channel {
        Channel {
            id: bisa_core::ChannelId::new(id).unwrap(),
            name: id.into(),
            ..Channel::general(born)
        }
    }

    fn gone_logs(store: &GuestStore) -> Vec<String> {
        std::fs::read_dir(store.dir().join(CONVERSATIONS_DIR))
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|n| n.ends_with(".gone"))
            .collect()
    }

    /// The channels programme's rule reaches the copy: a room deleted on the
    /// host and made again under its id starts empty here too, its old words
    /// kept aside — whether or not the copy heard the list without it in
    /// between. The same room, taken away and given back, keeps its words.
    #[test]
    fn a_room_made_again_starts_empty_in_the_copy() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        store
            .set_channels(&[room("general", 1), room("design", 5)])
            .unwrap();
        store
            .append(&post(&host, "the old room", "design", 10))
            .unwrap();
        store.append(&post(&host, "hello", "general", 11)).unwrap();

        // Taken off the room and put back on it: the same room.
        store.set_channels(&[room("general", 1)]).unwrap();
        store
            .set_channels(&[room("general", 1), room("design", 5)])
            .unwrap();
        assert_eq!(store.messages("design", None, 10).unwrap().len(), 1);
        assert!(gone_logs(&store).is_empty());

        // Deleted, the list heard without it, then made again.
        store.set_channels(&[room("general", 1)]).unwrap();
        store
            .set_channels(&[room("general", 1), room("design", 9)])
            .unwrap();
        assert!(store.messages("design", None, 10).unwrap().is_empty());
        assert_eq!(store.messages("general", None, 10).unwrap().len(), 1);
        let gone = gone_logs(&store);
        assert_eq!(gone.len(), 1, "{gone:?}");
        assert!(gone[0].starts_with("design."), "{gone:?}");
        store
            .append(&post(&host, "the new room", "design", 20))
            .unwrap();
        assert_eq!(store.messages("design", None, 10).unwrap().len(), 1);

        // Made again between two words: the list never read without it.
        store
            .set_channels(&[room("general", 2), room("design", 9)])
            .unwrap();
        assert!(store.messages("general", None, 10).unwrap().is_empty());
        assert_eq!(store.messages("design", None, 10).unwrap().len(), 1);

        // A room made again that never held a word has no log to move; a
        // reopened copy reads no log that was moved aside.
        store
            .set_channels(&[room("general", 2), room("design", 9), room("kitchen", 1)])
            .unwrap();
        store
            .set_channels(&[room("general", 2), room("design", 9), room("kitchen", 2)])
            .unwrap();
        assert_eq!(gone_logs(&store).len(), 2);
        let reopened = GuestStore::open(root.path(), &host.public_key()).unwrap();
        assert_eq!(reopened.messages("design", None, 10).unwrap().len(), 1);
        assert!(reopened.messages("general", None, 10).unwrap().is_empty());

        // A copy from before births were remembered knows what it lists.
        std::fs::remove_file(store.dir().join(BIRTHS_FILE)).unwrap();
        store
            .set_channels(&[room("general", 2), room("design", 12)])
            .unwrap();
        assert!(store.messages("design", None, 10).unwrap().is_empty());
    }

    /// A line cut short by a stop is ended before the next is written: the
    /// torn fact is lost, the sound one after it is not.
    #[test]
    fn a_torn_line_costs_one_event_not_two() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        store.set_channels(&[room("general", 1)]).unwrap();
        store.append(&post(&host, "one", "general", 10)).unwrap();
        let torn = post(&host, "torn", "general", 11).as_json();
        let mut log = std::fs::OpenOptions::new()
            .append(true)
            .open(store.conversation_path("general"))
            .unwrap();
        log.write_all(&torn.as_bytes()[..torn.len() / 2]).unwrap();
        drop(log);
        store.append(&post(&host, "two", "general", 12)).unwrap();
        let texts: Vec<String> = store
            .messages("general", None, 10)
            .unwrap()
            .into_iter()
            .map(|m| m.text)
            .collect();
        assert_eq!(texts, ["one", "two"]);
    }

    /// A stop between a fact's line and its id's: the reopened copy knows
    /// the fact from its log and keeps it once; and a log that holds a fact
    /// twice — written by a build that did not — shows it once.
    #[test]
    fn a_fact_in_the_log_is_seen_and_shown_once() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let fact = post(&host, "once", "general", 10);
        {
            let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
            store.set_channels(&[room("general", 1)]).unwrap();
            assert!(store.append(&fact).unwrap());
            std::fs::remove_file(store.dir().join(SEEN_FILE)).unwrap();
        }
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        assert!(store.has_seen(&fact.id.to_hex()));
        assert!(!store.append(&fact).unwrap(), "kept already");
        append_line(&store.conversation_path("general"), &fact.as_json()).unwrap();
        assert_eq!(store.events("general").unwrap().len(), 2);
        assert_eq!(store.messages("general", None, 10).unwrap().len(), 1);
    }

    /// What waits: a fact not sent is noted once and in order, and cleared
    /// when a relay took it; a fact for a room not named is kept once, the
    /// newest when there are too many, and handed over when its room is.
    #[test]
    fn what_waits_is_kept_once_in_order_and_within_bounds() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        store.set_channels(&[room("general", 1)]).unwrap();
        assert!(store.unsent().unwrap().is_empty());
        store.mark_unsent("a").unwrap();
        store.mark_unsent("b").unwrap();
        store.mark_unsent("a").unwrap();
        assert_eq!(store.unsent().unwrap(), ["a", "b"]);
        store.clear_unsent("a").unwrap();
        store.clear_unsent("never").unwrap();
        assert_eq!(store.unsent().unwrap(), ["b"]);
        let mine = post(&host, "mine", "general", 10);
        store.append(&mine).unwrap();
        store.mark_unsent(&mine.id.to_hex()).unwrap();
        let shown = store.messages("general", None, 10).unwrap();
        assert!(shown[0].unsent);
        let json = serde_json::to_value(&shown[0]).unwrap();
        assert_eq!(json["unsent"], true);
        store.clear_unsent(&mine.id.to_hex()).unwrap();
        let sent = serde_json::to_value(&store.messages("general", None, 10).unwrap()[0]).unwrap();
        assert!(sent.get("unsent").is_none(), "absent when sent: {sent}");

        assert!(store.take_parked().unwrap().is_empty());
        let early = post(&host, "early", "design", 10);
        std::fs::write(store.dir().join(WAITING_FILE), b"{ torn\n").unwrap();
        store.park(&early).unwrap();
        store.park(&early).unwrap();
        store.park(&post(&host, "other", "kitchen", 11)).unwrap();
        assert!(store.take_parked().unwrap().is_empty(), "no room named yet");
        store
            .set_channels(&[room("general", 1), room("design", 2)])
            .unwrap();
        let ready = store.take_parked().unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, early.id);
        assert!(store.take_parked().unwrap().is_empty(), "taken once");
        // The other waits on, past the hand-over.
        store
            .set_channels(&[room("general", 1), room("design", 2), room("kitchen", 3)])
            .unwrap();
        assert_eq!(store.take_parked().unwrap().len(), 1);

        for i in 0..(MAX_WAITING as u64 + 3) {
            store
                .park(&post(&host, &format!("n{i}"), "later", 100 + i))
                .unwrap();
        }
        store
            .set_channels(&[room("general", 1), room("later", 4)])
            .unwrap();
        let kept = store.take_parked().unwrap();
        assert_eq!(kept.len(), MAX_WAITING);
        assert!(
            kept[0].content.contains("\"n3\""),
            "the newest stay: {}",
            kept[0].content
        );
    }

    /// A copy forgotten is moved whole, and never over one forgotten before
    /// it — not in the same second either; a record taken away twice is
    /// taken away.
    #[test]
    fn a_forgotten_copy_never_lands_on_another() {
        let root = tempfile::tempdir().unwrap();
        let hosts = root.path().join("hosts");
        let host = Keys::generate();
        let mut kept = Vec::new();
        for n in 0..3 {
            let store = GuestStore::open(&hosts, &host.public_key()).unwrap();
            store.set_channels(&[room("general", n)]).unwrap();
            store.unset_hosted().unwrap();
            store.unset_hosted().unwrap();
            drop(store);
            let aside = GuestStore::forget(&hosts, &host.public_key().to_hex()).unwrap();
            assert!(aside.starts_with(root.path().join(FORGOTTEN_DIR)));
            assert!(aside.join(CHANNELS_FILE).is_file());
            assert!(!hosts.join(host.public_key().to_hex()).exists());
            kept.push(aside);
        }
        kept.sort();
        kept.dedup();
        assert_eq!(kept.len(), 3, "three copies, none written over");
    }

    /// The marks of what was heard round-trip, and read as nothing heard on
    /// a copy made before they were kept.
    #[test]
    fn what_was_heard_is_kept_and_an_older_copy_has_heard_nothing() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        assert_eq!(store.heard().unwrap(), Heard::default());
        let heard = Heard {
            membership: (20, 2),
            role: 30,
            channels: 40,
            members: 50,
        };
        store.set_heard(&heard).unwrap();
        assert_eq!(store.heard().unwrap(), heard);
        assert!(heard.takes_membership(20, 3), "a later kind of one second");
        assert!(heard.takes_membership(21, 0));
        assert!(!heard.takes_membership(20, 2), "the same word handed back");
        assert!(!heard.takes_membership(20, 1));
        assert!(!heard.takes_membership(20, 0));
        assert!(!heard.takes_membership(19, 2));
        std::fs::write(store.dir().join(HEARD_FILE), b"{}").unwrap();
        assert_eq!(store.heard().unwrap(), Heard::default());
        // And each state in the words a section says under its title.
        let words = |state: HostedState| state.words();
        assert_eq!(words(HostedState::Requested), "waiting for the host");
        assert_eq!(words(HostedState::Member), "");
        assert_eq!(
            words(HostedState::Refused {
                reason: "used".into()
            }),
            "refused — used"
        );
        assert_eq!(
            words(HostedState::Removed { reason: None }),
            "removed by the host"
        );
        assert_eq!(words(HostedState::Left), "you left");
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

    /// A read mark never moves back, as the host's never does: a read of an
    /// older moment leaves it where it stands. Set to whatever was named, a
    /// late mark put read messages back unread.
    #[test]
    fn a_read_mark_never_moves_back() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let me = Keys::generate();
        let me_id = PrincipalId::new(me.public_key().to_hex()).unwrap();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        store.append(&post(&host, "one", "general", 10)).unwrap();
        store.append(&post(&host, "two", "general", 12)).unwrap();
        store.mark_read("general", 12).unwrap();
        store.mark_read("general", 10).unwrap();
        assert_eq!(store.read_mark("general").unwrap(), Some(12));
        assert_eq!(store.unread("general", &me_id).unwrap(), 0);
        store.mark_read("general", 20).unwrap();
        assert_eq!(store.read_mark("general").unwrap(), Some(20));
    }

    /// The badge counts what the thread draws: a post taken back and a join
    /// or a leave are no messages to read.
    #[test]
    fn unread_counts_what_messages_shows() {
        let root = tempfile::tempdir().unwrap();
        let host = Keys::generate();
        let me = Keys::generate();
        let me_id = PrincipalId::new(me.public_key().to_hex()).unwrap();
        let store = GuestStore::open(root.path(), &host.public_key()).unwrap();
        let kept = post(&host, "kept", "general", 10);
        let taken_back = post(&host, "taken back", "general", 11);
        store.append(&kept).unwrap();
        store.append(&taken_back).unwrap();
        let joined = EventBuilder::new(
            Kind::Custom(KIND_MESSAGE),
            serde_json::json!({
                "body": "membership", "channel": "general",
                "member": {"agent": "developer"}, "change": "joined", "cause": "enabled", "at": 12
            })
            .to_string(),
        )
        .tag(
            Tag::parse([
                "a",
                &format!("33405:{}:general", host.public_key().to_hex()),
            ])
            .unwrap(),
        )
        .custom_created_at(nostr::types::Timestamp::from_secs(12))
        .finalize(&host)
        .unwrap();
        store.append(&joined).unwrap();
        let retraction = EventBuilder::new(Kind::Custom(bisa_core::kind::KIND_RETRACTION), "")
            .tag(
                Tag::parse([
                    "a",
                    &format!("33405:{}:general", host.public_key().to_hex()),
                ])
                .unwrap(),
            )
            .tag(Tag::parse(["e", &taken_back.id.to_hex()]).unwrap())
            .custom_created_at(nostr::types::Timestamp::from_secs(13))
            .finalize(&host)
            .unwrap();
        store.append(&retraction).unwrap();
        let shown: Vec<_> = store
            .messages("general", None, 50)
            .unwrap()
            .into_iter()
            .filter(|m| !m.retracted)
            .collect();
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert_eq!(store.unread("general", &me_id).unwrap(), 1);
    }

    /// Two writes of one file at once each land whole: each has a temporary
    /// of its own. One name a process, the second writer renamed the
    /// first's half-written bytes into place.
    #[test]
    fn two_writes_of_one_file_at_once_both_land_whole() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("read.json");
        let writers: Vec<_> = (0..8u8)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let body = vec![b'a' + n; 64 * 1024];
                    for _ in 0..20 {
                        write_atomic(&path, &body).unwrap();
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 64 * 1024);
        assert!(
            bytes.iter().all(|b| *b == bytes[0]),
            "one writer's bytes, whole"
        );
        let left: Vec<_> = std::fs::read_dir(root.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            left,
            vec!["read.json".to_string()],
            "no temporary is left behind"
        );
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

#[cfg(test)]
mod edge_tests {
    use super::*;
    use nostr::event::{EventBuilder, FinalizeEvent, Kind, Tag};
    use nostr::key::Keys;

    fn fact(keys: &Keys, kind: u16, content: &str, tags: Vec<Tag>) -> Event {
        EventBuilder::new(Kind::Custom(kind), content)
            .tags(tags)
            .finalize(keys)
            .unwrap()
    }

    fn coord(host: &PublicKey, scope: &str) -> Tag {
        Tag::parse([
            "a".to_string(),
            format!(
                "{}:{}:{scope}",
                bisa_core::kind::KIND_CHANNEL,
                host.to_hex()
            ),
        ])
        .unwrap()
    }

    fn sealed(path: &Path) -> std::fs::Permissions {
        let open = std::fs::metadata(path).unwrap().permissions();
        let mut shut = open.clone();
        std::os::unix::fs::PermissionsExt::set_mode(&mut shut, 0o000);
        std::fs::set_permissions(path, shut).unwrap();
        open
    }

    #[test]
    fn a_replica_reads_odd_facts_and_tags_as_what_they_are_and_says_what_it_cannot_read() {
        let keys = Keys::generate();
        let host = keys.public_key();
        let dir = tempfile::tempdir().unwrap();
        let store = GuestStore::open(&dir.path().join("hosts"), &host).unwrap();
        assert!(store.dir().starts_with(dir.path()));
        assert_eq!(store.host_hex(), host.to_hex());
        store.set_channels(&[Channel::general(0)]).unwrap();

        // A coordinate short of its scope, and one of another kind, name no
        // channel; a fact with no channel cannot be kept.
        let short = fact(
            &keys,
            KIND_MESSAGE,
            "{}",
            vec![Tag::parse(["a", "33405:abc"]).unwrap()],
        );
        assert_eq!(scope_of(&short), None);
        let other_kind = fact(
            &keys,
            KIND_MESSAGE,
            "{}",
            vec![Tag::parse(["a".to_string(), format!("1:{}:general", host.to_hex())]).unwrap()],
        );
        assert_eq!(scope_of(&other_kind), None);
        assert!(matches!(store.append(&short), Err(GuestError::Store(_))));

        // A message whose content is no message body is skipped; one with
        // imeta tags carries its attachments, the malformed ones dropped; a
        // kind nobody reads is kept and shown as nothing.
        let odd = fact(
            &keys,
            KIND_MESSAGE,
            "not json",
            vec![coord(&host, "general")],
        );
        assert!(store.append(&odd).unwrap());
        assert!(!store.append(&odd).unwrap(), "seen once");
        let sha = "ab".repeat(32);
        let with = fact(
            &keys,
            KIND_MESSAGE,
            &serde_json::to_string(&MessageBody::post("see attached")).unwrap(),
            vec![
                coord(&host, "general"),
                Tag::parse(vec![
                    "imeta".to_string(),
                    format!("x {sha}"),
                    "m image/png".to_string(),
                    "size 12".to_string(),
                    "name a.png".to_string(),
                    "odd".to_string(),
                    "zz 1".to_string(),
                ])
                .unwrap(),
                Tag::parse(["imeta", "x nothex", "m image/png", "size 1", "name b.png"]).unwrap(),
                Tag::parse(["imeta", "m image/png"]).unwrap(),
            ],
        );
        assert!(store.append(&with).unwrap());
        let unread_kind = fact(&keys, 1, "hello", vec![coord(&host, "general")]);
        assert!(store.append(&unread_kind).unwrap());
        let messages = store.messages("general", None, 10).unwrap();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert_eq!(messages[0].attachments.len(), 1);
        assert_eq!(messages[0].attachments[0].name, "a.png");
        assert_eq!(messages[0].attachments[0].size, 12);

        // A fact never seen is none; one seen in a scope no channel names
        // is none too.
        assert!(store.event(&"0".repeat(64)).unwrap().is_none());
        let elsewhere = fact(&keys, KIND_MESSAGE, "{}", vec![coord(&host, "kitchen")]);
        assert!(store.append(&elsewhere).unwrap());
        assert!(store.event(&elsewhere.id.to_hex()).unwrap().is_none());

        // A file that cannot be read is an error, never an empty answer.
        let conversation = store.dir().join(CONVERSATIONS_DIR).join("general.jsonl");
        let open = sealed(&conversation);
        assert!(store.events("general").is_err());
        std::fs::set_permissions(&conversation, open).unwrap();
        let channels = store.dir().join(CHANNELS_FILE);
        let open = sealed(&channels);
        assert!(store.channels().is_err());
        std::fs::set_permissions(&channels, open).unwrap();
    }
}
