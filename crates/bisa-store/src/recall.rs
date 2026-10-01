//! Recall: agent memory as blinded, encrypted addressable records (kind 33404).
//!
//! Each record is one `(agent, slug)` value. On disk the record is a signed
//! 33404 event whose `d` tag is `HMAC-SHA256(conversation_key(agent, owner),
//! "bisa-recall/v1/" || slug)` and whose content is NIP-44 ciphertext
//! between the agent and the owner — **the owner can always read everything
//! the agent remembers**. Writes are base-hash guarded.
//!
//! Layout: `agents/<id>/recall/state/33404-<d>.json` plus the local plaintext
//! sidecar `agents/<id>/recall_index.json`. Recall does not sync.

use crate::error::StoreError;
use crate::paths::Paths;
use crate::workspace::{now_secs, Workspace};
use bisa_core::kind::KIND_ENGRAM;
use bisa_core::AgentId;
use nostr::key::Keys;
use nostr::nips::nip44;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const RECALL_DOMAIN: &str = "bisa-recall/v1/";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecallRecord {
    pub slug: String,
    pub value: String,
    /// sha256 hex of the value bytes — the base hash for guarded updates.
    pub hash: String,
    pub updated_at: u64,
    /// `[[slug]]` wiki-links found in the value.
    pub links: Vec<String>,
    /// No other record links here.
    pub orphan: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct RecallSidecar {
    slugs: BTreeMap<String, String>,
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// HMAC-SHA256 with a 32-byte key (RFC 2104; block size 64).
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    debug_assert!(key.len() <= 64);
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for (i, b) in key.iter().enumerate() {
        ipad[i] ^= b;
        opad[i] ^= b;
    }
    let inner = {
        let mut h = Sha256::new();
        h.update(ipad);
        h.update(msg);
        h.finalize()
    };
    let mut h = Sha256::new();
    h.update(opad);
    h.update(inner);
    h.finalize().into()
}

fn parse_links(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end) = value[i + 2..].find("]]") {
                let slug = &value[i + 2..i + 2 + end];
                if !slug.is_empty() && !slug.contains('\n') && !out.contains(&slug.to_string()) {
                    out.push(slug.to_string());
                }
                i += end + 4;
                continue;
            }
        }
        i += 1;
    }
    out
}

#[derive(Serialize, Deserialize)]
struct RecallPlain {
    slug: String,
    value: String,
    updated_at: u64,
}

impl Workspace {
    /// The agent's recall index: absent is nothing remembered yet, present
    /// is read whole. A file that is there and does not parse is refused,
    /// never read as empty — the next store would write the emptied index
    /// over the agent's memory.
    fn read_sidecar(&self, agent: &AgentId) -> Result<RecallSidecar, StoreError> {
        let path = self.paths.agent(agent).recall_index();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::unreadable(&path, "recall index", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RecallSidecar::default()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    fn write_sidecar(&self, agent: &AgentId, sc: &RecallSidecar) -> Result<(), StoreError> {
        crate::paths::write_atomic(
            &self.paths.agent(agent).recall_index(),
            &serde_json::to_vec_pretty(sc)?,
        )
    }

    fn recall_d(&self, agent_keys: &Keys, slug: &str) -> Result<String, StoreError> {
        let ck =
            nip44::v2::ConversationKey::derive(agent_keys.secret_key(), &self.owner.public_key())
                .map_err(StoreError::nostr)?;
        let msg = format!("{RECALL_DOMAIN}{slug}");
        Ok(hex::encode(hmac_sha256(ck.as_bytes(), msg.as_bytes())))
    }

    fn decrypt_record(
        &self,
        agent_pubkey: &nostr::key::PublicKey,
        event: &nostr::event::Event,
    ) -> Result<RecallPlain, StoreError> {
        let plaintext = nip44::decrypt(self.owner.secret_key(), agent_pubkey, &event.content)
            .map_err(StoreError::nostr)?;
        Ok(serde_json::from_str(&plaintext)?)
    }

    /// Store (or update) one recall record. `base_hash` guards updates; `None`
    /// is accepted only for a NEW slug.
    pub fn recall_store(
        &self,
        agent: &AgentId,
        slug: &str,
        value: &str,
        base_hash: Option<&str>,
    ) -> Result<String, StoreError> {
        if slug.is_empty() || slug.contains(char::is_whitespace) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-recall-slug-must-be-non-empty-no",
                slug = format!("{slug:?}")
            )));
        }
        let (agent_keys, _) = self.signer_for(agent)?;
        let current = self.recall_get(agent, slug)?;
        match (&current, base_hash) {
            (Some(cur), Some(base)) if cur.hash != base => {
                return Err(StoreError::RecallConflict {
                    current_value: cur.value.clone(),
                    current_hash: cur.hash.clone(),
                });
            }
            (Some(cur), None) => {
                return Err(StoreError::RecallConflict {
                    current_value: cur.value.clone(),
                    current_hash: cur.hash.clone(),
                });
            }
            _ => {}
        }
        let d = self.recall_d(&agent_keys, slug)?;
        let at = now_secs();
        let plain = RecallPlain {
            slug: slug.to_string(),
            value: value.to_string(),
            updated_at: at,
        };
        let ns = Paths::ns_recall(agent);
        let existing = self.snapshots.get_raw(&ns, KIND_ENGRAM, &d)?;
        let existing_rev = existing
            .as_ref()
            .map(crate::snapshots::SnapshotStore::revision_of)
            .unwrap_or(0);
        let monotonic_at = at.max(existing.map(|ev| ev.created_at.as_secs() + 1).unwrap_or(0));
        self.snapshots.put(
            &ns,
            KIND_ENGRAM,
            &d,
            &plain,
            existing_rev + 1,
            &agent_keys,
            monotonic_at,
            Some(&self.owner.public_key()),
            &[],
        )?;
        let mut sc = self.read_sidecar(agent)?;
        sc.slugs.insert(slug.to_string(), d);
        self.write_sidecar(agent, &sc)?;
        Ok(sha256_hex(value.as_bytes()))
    }

    pub fn recall_get(
        &self,
        agent: &AgentId,
        slug: &str,
    ) -> Result<Option<RecallRecord>, StoreError> {
        let (agent_keys, _) = self.signer_for(agent)?;
        let d = self.recall_d(&agent_keys, slug)?;
        let ns = Paths::ns_recall(agent);
        let Some(event) = self.snapshots.get_raw(&ns, KIND_ENGRAM, &d)? else {
            return Ok(None);
        };
        let plain = self.decrypt_record(&agent_keys.public_key(), &event)?;
        Ok(Some(RecallRecord {
            hash: sha256_hex(plain.value.as_bytes()),
            links: parse_links(&plain.value),
            orphan: false,
            slug: plain.slug,
            value: plain.value,
            updated_at: plain.updated_at,
        }))
    }

    /// Every recall record of an agent, decrypted, with the link graph
    /// resolved.
    pub fn recall_list(&self, agent: &AgentId) -> Result<Vec<RecallRecord>, StoreError> {
        let (agent_keys, _) = self.signer_for(agent)?;
        let ns = Paths::ns_recall(agent);
        let mut records = Vec::new();
        for d in self.snapshots.list_ds(&ns, KIND_ENGRAM)? {
            let Some(event) = self.snapshots.get_raw(&ns, KIND_ENGRAM, &d)? else {
                continue;
            };
            match self.decrypt_record(&agent_keys.public_key(), &event) {
                Ok(plain) => records.push(RecallRecord {
                    hash: sha256_hex(plain.value.as_bytes()),
                    links: parse_links(&plain.value),
                    orphan: true,
                    slug: plain.slug,
                    value: plain.value,
                    updated_at: plain.updated_at,
                }),
                Err(e) => tracing::warn!("recall {d}: undecryptable, skipping: {e}"),
            }
        }
        let linked: Vec<String> = records
            .iter()
            .flat_map(|r| {
                r.links
                    .iter()
                    .filter(|l| **l != r.slug)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        for r in &mut records {
            r.orphan = !linked.contains(&r.slug);
        }
        records.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_is_deterministic_and_keyed() {
        let a = hmac_sha256(&[1u8; 32], b"x");
        assert_eq!(a, hmac_sha256(&[1u8; 32], b"x"));
        assert_ne!(a, hmac_sha256(&[2u8; 32], b"x"));
    }

    #[test]
    fn link_parsing() {
        assert_eq!(
            parse_links("see [[core]] and [[notes/style]] but not [broken]"),
            vec!["core".to_string(), "notes/style".to_string()]
        );
        assert_eq!(parse_links("[[dup]] [[dup]]"), vec!["dup".to_string()]);
    }
}
