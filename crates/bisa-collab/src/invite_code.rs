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
        let profile = Nip19Profile::from_bech32(nprofile)
            .map_err(|e| CollabError::InviteCode(format!("bad nprofile: {e}")))?;
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
        assert!(InviteCode::parse("hello").is_err());
        assert!(InviteCode::parse("bisa://join/").is_err());
        assert!(InviteCode::parse("nprofile1qqs:notHEX").is_err());
        let host = Keys::generate();
        let code = InviteCode::new(host.public_key(), &[], "ab".repeat(32));
        assert!(code.link().unwrap().contains("/abab"));
        assert!(InviteCode::parse("bisa://join/nprofile1garbage/abcd").is_err());
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
