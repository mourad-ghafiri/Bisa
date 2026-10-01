//! Gift-wrapping (NIP-59) — the one envelope both sides speak.
//!
//! NIP-59 rumors are unsigned by design, but a receiver re-verifies the
//! *inner* GEP event's signature — so the rumor is a **carrier**: a rumor of
//! kind [`KIND_GEP_CARRIER`] whose content is the full signed GEP event JSON,
//! sealed (kind 13, NIP-44 sender→recipient, signed by the sender) and wrapped
//! (kind 1059, ephemeral key, `p`-tag = recipient, created_at randomized up to
//! two days into the past per NIP-59).
//!
//! Control traffic — the invite flow, the host's word to a member, a member's
//! word back — rides the same envelope with rumor kind [`KIND_GEP_CONTROL`]
//! and a small JSON body ([`Control`]).

use crate::control::Control;
use crate::CollabError;
use nostr::event::{
    Event, EventBuilder, FinalizeEvent, FinalizeUnsignedEvent, Kind, UnsignedEvent,
};
use nostr::key::{Keys, PublicKey};
use nostr::nips::nip59::{GiftWrapBuilder, UnwrappedGift};

/// Rumor kind carrying one signed GEP event verbatim in its content.
pub const KIND_GEP_CARRIER: u16 = 1060;
/// Rumor kind carrying a control message.
pub const KIND_GEP_CONTROL: u16 = 1061;

/// What an unwrapped gift contained.
#[derive(Clone, Debug)]
pub enum Incoming {
    /// A signed GEP event; `sender` is the seal's verified author, which may
    /// differ from the inner event's own author when a host relays another
    /// member's fact.
    Gep { sender: PublicKey, event: Event },
    /// A control message; `sender` is the seal's verified author.
    Control { sender: PublicKey, control: Control },
    /// Something else (a plain NIP-17 DM, an unknown rumor kind) — ignored.
    Other,
}

fn rumor(keys: &Keys, kind: u16, content: String) -> UnsignedEvent {
    EventBuilder::new(Kind::Custom(kind), content).finalize_unsigned(keys.public_key())
}

/// Wrap one signed GEP event for one recipient.
pub fn wrap_for_member(
    keys: &Keys,
    member: PublicKey,
    event: &Event,
) -> Result<Event, CollabError> {
    let r = rumor(keys, KIND_GEP_CARRIER, event.as_json());
    GiftWrapBuilder::new(member, r)
        .finalize(keys)
        .map_err(|e| CollabError::Wrap(e.to_string()))
}

/// Wrap one control message for one recipient.
pub fn wrap_control(
    keys: &Keys,
    member: PublicKey,
    control: &Control,
) -> Result<Event, CollabError> {
    let body = serde_json::to_string(control).map_err(|e| CollabError::Wrap(e.to_string()))?;
    let r = rumor(keys, KIND_GEP_CONTROL, body);
    GiftWrapBuilder::new(member, r)
        .finalize(keys)
        .map_err(|e| CollabError::Wrap(e.to_string()))
}

/// Unwrap a received kind-1059 event addressed to `keys`.
///
/// The gift wrap and seal are verified by the nostr crate (including seal
/// sender consistency); the inner GEP event's signature is verified again by
/// whoever applies it. The carrier rumor must be authored by the seal
/// sender — otherwise a member could replay someone's wrap as their own.
pub fn unwrap_incoming(keys: &Keys, gift_wrap: &Event) -> Result<Incoming, CollabError> {
    let unwrapped = UnwrappedGift::from_gift_wrap(keys, gift_wrap)
        .map_err(|e| CollabError::Wrap(e.to_string()))?;
    let kind = unwrapped.rumor.kind.as_u16();
    if unwrapped.rumor.pubkey != unwrapped.sender {
        return Err(CollabError::Wrap("rumor author != seal sender".into()));
    }
    if kind == KIND_GEP_CARRIER {
        let inner = Event::from_json(&unwrapped.rumor.content)
            .map_err(|e| CollabError::Wrap(format!("carrier content is not an event: {e}")))?;
        Ok(Incoming::Gep {
            sender: unwrapped.sender,
            event: inner,
        })
    } else if kind == KIND_GEP_CONTROL {
        match serde_json::from_str::<Control>(&unwrapped.rumor.content) {
            Ok(control) => Ok(Incoming::Control {
                sender: unwrapped.sender,
                control,
            }),
            Err(_) => Ok(Incoming::Other),
        }
    } else {
        Ok(Incoming::Other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::Tag;

    fn signed_gep_event(keys: &Keys) -> Event {
        EventBuilder::new(Kind::Custom(3407), r#"{"type":"post","text":"hi"}"#)
            .tags([Tag::parse(["a", "33405:abc:general"]).unwrap()])
            .finalize(keys)
            .unwrap()
    }

    #[test]
    fn wrap_unwrap_roundtrip_preserves_inner_signature_and_names_the_sender() {
        let alice = Keys::generate();
        let bob = Keys::generate();
        let inner = signed_gep_event(&alice);
        let wrap = wrap_for_member(&alice, bob.public_key(), &inner).unwrap();
        assert_eq!(wrap.kind.as_u16(), 1059);
        assert_ne!(
            wrap.pubkey,
            alice.public_key(),
            "wrap uses an ephemeral key"
        );
        match unwrap_incoming(&bob, &wrap).unwrap() {
            Incoming::Gep { sender, event } => {
                assert_eq!(sender, alice.public_key());
                assert_eq!(event, inner);
                event
                    .verify()
                    .expect("inner signature survives the roundtrip");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn a_host_may_relay_another_member_s_fact_and_the_seal_still_names_the_host() {
        let host = Keys::generate();
        let carol = Keys::generate();
        let bob = Keys::generate();
        let carols = signed_gep_event(&carol);
        let wrap = wrap_for_member(&host, bob.public_key(), &carols).unwrap();
        match unwrap_incoming(&bob, &wrap).unwrap() {
            Incoming::Gep { sender, event } => {
                assert_eq!(sender, host.public_key());
                assert_eq!(event.pubkey, carol.public_key());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn control_roundtrip() {
        let alice = Keys::generate();
        let bob = Keys::generate();
        let c = Control::Join {
            secret: "s3cr3t".into(),
            label: Some("bob".into()),
            client: "desktop".into(),
        };
        let wrap = wrap_control(&bob, alice.public_key(), &c).unwrap();
        match unwrap_incoming(&alice, &wrap).unwrap() {
            Incoming::Control { sender, control } => {
                assert_eq!(sender, bob.public_key());
                assert_eq!(control, c);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn wrong_recipient_cannot_unwrap() {
        let alice = Keys::generate();
        let bob = Keys::generate();
        let eve = Keys::generate();
        let wrap = wrap_for_member(&alice, bob.public_key(), &signed_gep_event(&alice)).unwrap();
        assert!(unwrap_incoming(&eve, &wrap).is_err());
    }
}
