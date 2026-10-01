//! `AttachmentRef`: a file carried by a message.
//!
//! # Why this is a descriptor and not the bytes
//!
//! A message is a signed Nostr event, and every event is gift-wrapped **once
//! per member** before it is published — then re-wrapped in full when somebody
//! new joins the workspace. A photo embedded in an event would therefore be
//! re-encrypted and re-uploaded per person, per join, and the iroh transport
//! caps a frame at 1 MiB, so it would not fit down that pipe at all.
//!
//! So the event carries this: a name, a media type, a size and a content hash.
//! Small enough to ride the wire, and enough for a second node to **act** on —
//! it can name the file, dedupe it against what it already has, and ask the
//! sender for the bytes. That is the test `docs/reference/gep.md` puts to every candidate
//! fact, and it is what separates an attachment from a workstream path, which no
//! peer could ever act on.
//!
//! The bytes live in the workspace's content-addressed store and move over the
//! iroh session on demand. `sha256` is both the address and the proof: a
//! receiver verifies the bytes it was handed before it stores them, so a peer
//! cannot answer a request for one file with a different one.

use serde::{Deserialize, Serialize};

/// Hard cap on one attachment.
///
/// Large enough for the things people actually send a colleague — a screenshot,
/// a photo, a PDF, a slide deck — and small enough that both ends can hold one
/// in memory while it moves, and that a transfer over the chunked iroh session
/// finishes in a reasonable number of frames. Refused above this with a `413`
/// naming the limit, the same shape a webhook body over
/// [`crate::MAX_SIGNAL_PAYLOAD_BYTES`] gets.
pub const MAX_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;

/// Hard cap on a **picture** — a project's, a group's, an agent's, a team's
/// photo (ide/14 §Photos, [`crate::photo::PhotoProfile::Picture`]).
///
/// A photo is drawn under 32 px, and the desktop scales one to a 256 px
/// square before it uploads it — a few dozen kilobytes. This is the honest
/// upper bound for one set by another client: an icon, never a camera's
/// file, so every row that draws it stays cheap to fetch, decode and sync.
pub const MAX_PHOTO_BYTES: u64 = 512 * 1024;

/// Hard cap on a person's **face** ([`crate::photo::PhotoProfile::Face`]).
///
/// A face travels: it rides the collaboration control channel with the
/// person's profile, gift-wrapped once per recipient, and public relays
/// refuse an event past a few dozen kilobytes. The desktop scales one to a
/// 96 px JPEG square, which fits well under this; base64 inflates it by a
/// third on the wire and it still does.
pub const MAX_FACE_BYTES: u64 = 16 * 1024;

/// The number of hex characters a SHA-256 digest is written in.
///
/// A hash becomes a path segment in the blob store, so it is validated against
/// this and the hex alphabet before it is ever joined to a directory — *a name
/// that becomes a path is an allowlist*, the same rule a project slug follows.
pub const SHA256_HEX_LEN: usize = 64;

/// One file carried by a message.
///
/// Rides an `imeta` tag on the message event, follows NIP-92/94's shape, and is
/// the same struct the HTTP body, the index row and the harness input use —
/// there is deliberately not a second spelling of it per layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRef {
    /// Lowercase hex. The blob's address, and the proof of what it contains.
    pub sha256: String,
    /// What the sender called it. Display only — it is **never** a path, and
    /// nothing joins it to a directory.
    pub name: String,
    /// The media type the sender declared. Display and harness routing only:
    /// the byte route re-checks the actual bytes before serving anything
    /// inline, because a claim about a file is not evidence about it.
    pub mime: String,
    pub size: u64,
}

impl AttachmentRef {
    /// Whether `hash` could address a blob: 64 lowercase hex characters.
    ///
    /// Rejects an empty string, an uppercase digest, and — the reason this
    /// exists — anything containing `/`, `.` or a NUL that would otherwise
    /// escape the store's directory when joined to it.
    pub fn is_valid_hash(hash: &str) -> bool {
        hash.len() == SHA256_HEX_LEN
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    /// Is this something a viewer could reasonably be shown inline?
    ///
    /// A hint from the declared type, used to decide what to *try*. The byte
    /// route still checks the file's magic number before serving it as an
    /// image, so a mislabelled file is a chip rather than a rendering bug.
    pub fn looks_like_an_image(&self) -> bool {
        self.mime.starts_with("image/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash is about to become a directory name, so this is a containment
    /// check rather than tidiness.
    #[test]
    fn a_hash_that_could_escape_the_store_is_not_a_hash() {
        let ok = "a".repeat(64);
        assert!(AttachmentRef::is_valid_hash(&ok));
        assert!(AttachmentRef::is_valid_hash(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ));

        for bad in [
            "",
            "abc",
            &"a".repeat(63),
            &"a".repeat(65),
            // Uppercase is a different string for the same digest; one
            // spelling means one path.
            &"A".repeat(64),
            // The ones that matter.
            &format!("{}/x", "a".repeat(62)),
            &format!("..{}", "a".repeat(62)),
            &format!("{}\0", "a".repeat(63)),
            &format!("{}g", "a".repeat(63)),
        ] {
            assert!(!AttachmentRef::is_valid_hash(bad), "{bad:?} was accepted");
        }
    }

    #[test]
    fn an_image_is_recognised_by_its_declared_type_only_as_a_hint() {
        let img = AttachmentRef {
            sha256: "a".repeat(64),
            name: "shot.png".into(),
            mime: "image/png".into(),
            size: 10,
        };
        assert!(img.looks_like_an_image());
        assert!(!AttachmentRef {
            mime: "application/pdf".into(),
            ..img.clone()
        }
        .looks_like_an_image());
        // A `.png` name proves nothing; only the declared type is consulted
        // here, and even that is re-checked against the bytes before anything
        // is served inline.
        assert!(!AttachmentRef {
            name: "actually.png".into(),
            mime: "application/octet-stream".into(),
            ..img
        }
        .looks_like_an_image());
    }
}
