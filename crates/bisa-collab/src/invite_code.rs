//! The invite code a person carries: the host's `nprofile` (its pubkey and
//! relay hints) and a single-use secret — as a link, `bisa://join/<nprofile>/
//! <secret>`, that the desktop and a mobile app open, and as text,
//! `<nprofile>:<secret>`, that a terminal pastes. Either form parses.
//!
//! The secret exists once, inside the code; the host keeps its SHA-256 alone.

use crate::CollabError;
use bisa_core::PrincipalId;
use nostr::key::{Keys, PublicKey};
use nostr::nips::nip19::{FromBech32, Nip19Profile, ToBech32};
use nostr::types::RelayUrl;

pub const INVITE_LINK_PREFIX: &str = "bisa://join/";

/// A parsed code: whom to ask, where, with what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteCode {
    pub host: PrincipalId,
    pub relays: Vec<String>,
    pub secret: String,
}

impl InviteCode {
    /// Make a code for this host: its pubkey, the relays it reads, and the
    /// secret just minted.
    pub fn new(host: PublicKey, relays: &[String], secret: String) -> Self {
        Self {
            host: PrincipalId::new(host.to_hex()).expect("a pubkey is a principal"),
            relays: relays.to_vec(),
            secret,
        }
    }

    fn nprofile(&self) -> Result<String, CollabError> {
        let pk = PublicKey::from_hex(self.host.as_hex())
            .map_err(|e| CollabError::InviteCode(e.to_string()))?;
        let relays: Vec<RelayUrl> = self
            .relays
            .iter()
            .filter_map(|u| RelayUrl::parse(u).ok())
            .collect();
        Nip19Profile::new(pk, relays)
            .to_bech32()
            .map_err(|e| CollabError::InviteCode(e.to_string()))
    }

    /// The link form.
    pub fn link(&self) -> Result<String, CollabError> {
        Ok(format!(
            "{INVITE_LINK_PREFIX}{}/{}",
            self.nprofile()?,
            self.secret
        ))
    }

    /// The text form.
    pub fn text(&self) -> Result<String, CollabError> {
        Ok(format!("{}:{}", self.nprofile()?, self.secret))
    }

    /// Either form, whitespace around it forgiven.
    pub fn parse(code: &str) -> Result<Self, CollabError> {
        let code = code.trim();
        // The prefix in whatever case a clipboard left it; the rest as written.
        let link = code
            .get(..INVITE_LINK_PREFIX.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(INVITE_LINK_PREFIX));
        let (nprofile, secret) = if let Some(rest) = link.then(|| &code[INVITE_LINK_PREFIX.len()..])
        {
            // What a browser, a chat client or a link shortener hangs on a
            // link — `?utm=…`, `#top` — is no part of the code.
            let rest = rest.split(['?', '#']).next().unwrap_or(rest);
            let rest = rest.trim_end_matches('/');
            rest.split_once('/').ok_or_else(|| {
                CollabError::InviteCode("a link is bisa://join/<nprofile>/<secret>".into())
            })?
        } else {
            code.rsplit_once(':')
                .ok_or_else(|| CollabError::InviteCode("a code is <nprofile>:<secret>".into()))?
        };
        if secret.is_empty() || !secret.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CollabError::InviteCode("the secret is not hex".into()));
        }
        // In words a person can act on: the decoder's own error names a
        // checksum and a character position, which is nothing they pasted.
        let profile = Nip19Profile::from_bech32(nprofile).map_err(|_| {
            CollabError::InviteCode(
                "the part before the secret is not a host's address (nprofile1…) — copy the invitation again, whole".into(),
            )
        })?;
        Ok(Self {
            host: PrincipalId::new(profile.public_key.to_hex())
                .map_err(|e| CollabError::InviteCode(e.to_string()))?,
            relays: profile.relays.iter().map(|r| r.to_string()).collect(),
            secret: secret.to_ascii_lowercase(),
        })
    }
}

/// 32 bytes of CSPRNG entropy as hex — borrowed from a throwaway keypair, so
/// no second random source is pulled in.
pub fn mint_secret() -> String {
    Keys::generate().secret_key().to_secret_hex()
}

/// SHA-256 hex of an invite secret.
pub fn hash_secret(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(secret.as_bytes()))
}

/// Constant-time equality on two hex hash strings.
pub fn hashes_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_forms_carry_the_host_the_relays_and_the_secret() {
        let host = Keys::generate();
        let relays = vec!["wss://relay.example".to_string()];
        let secret = mint_secret();
        assert_eq!(secret.len(), 64);
        let code = InviteCode::new(host.public_key(), &relays, secret.clone());
        let link = code.link().unwrap();
        assert!(link.starts_with("bisa://join/nprofile1"));
        assert!(link.ends_with(&format!("/{secret}")));
        let text = code.text().unwrap();
        assert!(text.starts_with("nprofile1"));
        assert!(text.ends_with(&format!(":{secret}")));
        for form in [link, text, format!("  {}  ", code.text().unwrap())] {
            let back = InviteCode::parse(&form).unwrap();
            assert_eq!(back.host.as_hex(), host.public_key().to_hex());
            assert_eq!(back.relays, relays);
            assert_eq!(back.secret, secret);
        }
    }

    #[test]
    fn a_code_that_is_not_one_is_refused_in_words() {
        let refused = |code: &str| InviteCode::parse(code).unwrap_err().to_string();
        assert!(refused("hello").contains("<nprofile>:<secret>"));
        assert!(refused("bisa://join/").contains("bisa://join/<nprofile>/<secret>"));
        assert!(refused("nprofile1qqs:notHEX").contains("not hex"));
        let host = Keys::generate();
        let code = InviteCode::new(host.public_key(), &[], "ab".repeat(32));
        assert!(code.link().unwrap().contains("/abab"));
        // A host part that is no address is said in words a person can act
        // on — never the decoder's own error.
        let garbage = refused("bisa://join/nprofile1garbage/abcd");
        assert!(garbage.contains("not a host's address"), "{garbage}");
        assert!(!garbage.to_lowercase().contains("checksum"), "{garbage}");
    }

    /// What a browser or a chat client hangs on a link is no part of the
    /// code: a query, a fragment, a trailing slash. Read as part of the
    /// secret, the link was refused as *not hex*.
    #[test]
    fn a_link_with_a_query_or_a_fragment_is_still_its_code() {
        let host = Keys::generate();
        let code = InviteCode::new(host.public_key(), &[], "ab".repeat(32));
        let link = code.link().unwrap();
        for hung in ["?utm_source=chat", "#top", "/?x=1#y", "/"] {
            let parsed = InviteCode::parse(&format!("{link}{hung}")).unwrap();
            assert_eq!(parsed, code, "{hung}");
        }
    }

    /// A clipboard or a chat client may recase a link's prefix; the desktop
    /// reads it in any case, and so does this parser — the rest as written.
    #[test]
    fn a_links_prefix_is_read_in_any_case_and_the_rest_as_written() {
        let host = Keys::generate();
        let secret = mint_secret();
        let code = InviteCode::new(host.public_key(), &[], secret.clone());
        let link = code.link().unwrap();
        let rest = &link[INVITE_LINK_PREFIX.len()..];
        for prefix in ["Bisa://Join/", "BISA://JOIN/", "bisa://JOIN/"] {
            let back = InviteCode::parse(&format!("{prefix}{rest}")).unwrap();
            assert_eq!(back.host.as_hex(), host.public_key().to_hex(), "{prefix}");
            assert_eq!(back.secret, secret, "{prefix}");
        }
        assert!(InviteCode::parse(&format!("bisa://joined/{rest}")).is_err());
    }

    #[test]
    fn a_secret_is_kept_as_its_hash_and_compared_in_constant_time() {
        let h = hash_secret("secret");
        assert_eq!(h.len(), 64);
        assert!(hashes_equal(&h, &hash_secret("secret")));
        assert!(!hashes_equal(&h, &hash_secret("secre")));
        assert!(!hashes_equal(&h, "short"));
    }
}
