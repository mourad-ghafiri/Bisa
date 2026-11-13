//! What the pump heard last from each person, and what it last told them
//! (`net_people.json`, beside the ledger).
//!
//! **Heard.** A member's words — a join, a leave, a profile — carry the time
//! they were said, and a relay hands them back in any order: after a
//! restart, every one it kept. The newest of its kind stands; an older one
//! is dropped, so an old leave never removes somebody invited again and an
//! old profile never puts a name back.
//!
//! **Told.** The host's words were sent once: one no relay took was gone,
//! and a change made while no pump ran was never said. The pump keeps, per
//! person, what a relay last took from it — that they were welcomed, their
//! role, a digest of their rooms and of the directory — and
//! `host::reconcile` sends whatever differs from what stands, until a relay
//! takes it. The record is written after the relay took the word.

use bisa_core::MemberRole;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A join among a member's words of one second: said before a leave.
pub(crate) const RANK_JOIN: u8 = 0;
/// A leave among a member's words of one second: said after a join.
pub(crate) const RANK_LEAVE: u8 = 1;

/// What a relay last took from the pump for one person. Present from their
/// welcome to the word that told them they were removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Told {
    pub(crate) role: MemberRole,
    /// A digest of the rooms they were last told they reach.
    #[serde(default)]
    pub(crate) rooms: String,
    /// A digest of the directory they were last sent.
    #[serde(default)]
    pub(crate) directory: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Person {
    /// Their newest join or leave: when it was said, and its rank among
    /// words of that second.
    #[serde(default)]
    pub(crate) said: (u64, u8),
    /// When their newest profile was said.
    #[serde(default)]
    pub(crate) profile: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) told: Option<Told>,
}

impl Person {
    /// Whether a join or a leave said at `at`, of `rank`, is newer than the
    /// one that stands — strictly: the same word handed back is no word.
    pub(crate) fn takes(&self, at: u64, rank: u8) -> bool {
        (at, rank) > self.said
    }

    /// Whether anything is kept of them.
    fn is_empty(&self) -> bool {
        *self == Person::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeopleFile {
    /// By pubkey hex.
    #[serde(default)]
    pub(crate) people: BTreeMap<String, Person>,
    /// The invitations whose refusal a relay took.
    #[serde(default)]
    pub(crate) refusals: BTreeSet<String>,
}

impl PeopleFile {
    pub(crate) fn person(&mut self, hex: &str) -> &mut Person {
        self.people.entry(hex.to_string()).or_default()
    }

    pub(crate) fn told(&self, hex: &str) -> Option<&Told> {
        self.people.get(hex).and_then(|p| p.told.as_ref())
    }

    /// Whether a join or a leave from `hex` is the newest heard from them.
    pub(crate) fn takes(&self, hex: &str, at: u64, rank: u8) -> bool {
        self.people.get(hex).is_none_or(|p| p.takes(at, rank))
    }

    /// Whether a profile from `hex` is the newest heard from them — or of
    /// the newest's own second: a person who sets a name and then a face
    /// says two in one, and the same profile handed back changes nothing
    /// when it is applied again.
    pub(crate) fn takes_profile(&self, hex: &str, at: u64) -> bool {
        self.people.get(hex).is_none_or(|p| at >= p.profile)
    }

    /// Whether anything was heard from `hex` — a join or a leave.
    pub(crate) fn heard_from(&self, hex: &str) -> bool {
        self.people.get(hex).is_some_and(|p| p.said != (0, 0))
    }
}

pub(crate) fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("net_people.json")
}

/// The record as it was kept; `None` when there is none — a workspace no
/// pump of this build has served — or it does not read.
pub(crate) fn load(data_dir: &Path) -> Option<PeopleFile> {
    let bytes = std::fs::read(path(data_dir)).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(file) => Some(file),
        Err(e) => {
            tracing::warn!("net_people.json did not read ({e}); what stands is taken as told");
            None
        }
    }
}

/// Write the record whole: a temporary beside it, then the rename.
pub(crate) fn save(data_dir: &Path, file: &PeopleFile) {
    let mut kept = file.clone();
    kept.people.retain(|_, p| !p.is_empty());
    let target = path(data_dir);
    let tmp = target.with_extension(format!("json.tmp.{}", std::process::id()));
    let written = serde_json::to_vec_pretty(&kept)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(&tmp, bytes))
        .and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(e) = written {
        // LCOV_EXCL_START: the pump's record unwritable; a disk arm
        tracing::warn!("net_people.json was not written: {e}");
        // LCOV_EXCL_STOP
    }
}

/// A digest of anything said to a person, to know it again.
pub(crate) fn digest<T: Serialize>(value: &T) -> String {
    bisa_collab::hash_secret(&serde_json::to_string(value).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The newest join or leave stands; two of one second are a join, then
    /// a leave; the same word handed back is no word.
    #[test]
    fn the_newest_of_a_members_words_stands() {
        let mut file = PeopleFile::default();
        assert!(file.takes("aa", 10, RANK_LEAVE), "nothing heard yet");
        assert!(!file.heard_from("aa"));
        file.person("aa").said = (10, RANK_JOIN);
        assert!(file.heard_from("aa"));
        assert!(!file.takes("aa", 10, RANK_JOIN), "the same word");
        assert!(!file.takes("aa", 9, RANK_LEAVE), "an older leave");
        assert!(file.takes("aa", 10, RANK_LEAVE), "the leave of that second");
        assert!(file.takes("aa", 11, RANK_JOIN));
        assert!(
            file.takes("bb", 1, RANK_JOIN),
            "another person's mark is theirs"
        );
        assert!(file.takes_profile("aa", 1));
        file.person("aa").profile = 5;
        assert!(!file.takes_profile("aa", 4));
        assert!(
            file.takes_profile("aa", 5),
            "two of one second are both taken"
        );
        assert!(file.takes_profile("aa", 6));
    }

    /// The record round-trips, keeps nothing of a person nothing is known
    /// of, and reads as none when it is not there or does not parse.
    #[test]
    fn the_record_round_trips_and_an_unreadable_one_is_none() {
        let held = tempfile::tempdir().unwrap();
        let dir = held.path().to_path_buf();
        assert!(load(&dir).is_none());
        let mut file = PeopleFile::default();
        file.person("aa").told = Some(Told {
            role: MemberRole::Guest,
            rooms: digest(&vec!["general"]),
            directory: digest(&vec!["alice"]),
        });
        file.person("bb");
        file.refusals.insert("inv-1".into());
        save(&dir, &file);
        let read = load(&dir).unwrap();
        assert_eq!(read.told("aa"), file.told("aa"));
        assert!(
            !read.people.contains_key("bb"),
            "nothing known, nothing kept"
        );
        assert!(read.refusals.contains("inv-1"));
        assert_ne!(digest(&vec!["general"]), digest(&vec!["general", "design"]));
        std::fs::write(path(&dir), b"{ not json").unwrap();
        assert!(load(&dir).is_none());
    }
}
