//! Control messages: what a host and a hosted member say to each other
//! beside the facts. Every one rides a [`crate::wrap::wrap_control`]
//! envelope, pairwise, and is read only from the seal's verified sender —
//! a `Welcome` counts only from the host it was asked of, a `Join` only as
//! the claim of the key that signed it.

use bisa_core::{AttachmentRef, Channel, MemberRole, PrincipalId, MAX_FACE_BYTES};
use serde::{Deserialize, Serialize};

/// The words a joiner says it is, for the people list; never trusted.
pub const CLIENT_DESKTOP: &str = "desktop";
pub const CLIENT_MOBILE: &str = "mobile";
pub const CLIENT_CLI: &str = "cli";

/// The host as a guest keeps it: who, what it is called, where to reach it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HostCard {
    pub pubkey: PrincipalId,
    /// `collab.name`, else the host's pubkey's first letters.
    pub name: String,
    /// The relays the host reads, so a guest that lost its own list can
    /// still reach it.
    #[serde(default)]
    pub relays: Vec<String>,
}

/// One person of the workspace as the host tells its members: enough to
/// draw a name, a role and a face, never more. The owner is a row too.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Directory {
    pub pubkey: PrincipalId,
    pub role: MemberRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The person's face, by content hash — the bytes come on demand
    /// (`WantFace` → `Face`), once per face a node lacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<AttachmentRef>,
}

/// The most characters a face's base64 may run to: [`MAX_FACE_BYTES`]
/// encoded, plus padding — checked before a byte is decoded.
pub const MAX_FACE_B64_CHARS: usize = (MAX_FACE_BYTES as usize).div_ceil(3) * 4;

/// A person's face as bytes on the wire: a picture within
/// [`MAX_FACE_BYTES`], base64, named by its content hash. No declared type —
/// the bytes decide what they are.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Face {
    pub sha256: String,
    pub b64: String,
}

/// Why a face is not kept.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FaceRefusal {
    #[error("the face is {chars} characters of base64; the limit is {max}")]
    TooLong { chars: usize, max: usize },
    #[error("the face is not base64")]
    NotBase64,
    #[error("the face's bytes do not hash to {expected}")]
    HashLies { expected: String },
    #[error("the face is not a picture within the cap")]
    NotAFace,
}

impl Face {
    /// A face from bytes this node holds, hashed here.
    pub fn from_bytes(bytes: &[u8]) -> Face {
        use base64::Engine as _;
        use sha2::Digest as _;
        Face {
            sha256: hex::encode(sha2::Sha256::digest(bytes)),
            b64: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }

    /// The bytes, once the string is short enough to decode, the hash is
    /// theirs and they are a face (`bisa_core::is_face`) — in that order, so
    /// nothing is decoded, hashed or kept that fails a cheaper check.
    pub fn decode(&self) -> Result<Vec<u8>, FaceRefusal> {
        use base64::Engine as _;
        use sha2::Digest as _;
        if self.b64.len() > MAX_FACE_B64_CHARS {
            return Err(FaceRefusal::TooLong {
                chars: self.b64.len(),
                max: MAX_FACE_B64_CHARS,
            });
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(self.b64.as_bytes())
            .map_err(|_| FaceRefusal::NotBase64)?;
        if hex::encode(sha2::Sha256::digest(&bytes)) != self.sha256 {
            return Err(FaceRefusal::HashLies {
                expected: self.sha256.clone(),
            });
        }
        if !bisa_core::is_face(&bytes) {
            return Err(FaceRefusal::NotAFace);
        }
        Ok(bytes)
    }
}

/// A control message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Control {
    /// A joiner claims an invite: the code's secret, what it wants to be
    /// called, what it is.
    Join {
        secret: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        client: String,
    },
    /// The host admitted the joiner: who the host is, the role granted, the
    /// people, the channels the role reaches.
    Welcome {
        workspace: HostCard,
        role: MemberRole,
        #[serde(default)]
        members: Vec<Directory>,
        #[serde(default)]
        channels: Vec<Channel>,
    },
    /// The host did not admit the joiner, and why.
    Refused { reason: String },
    /// The host is admitting by hand; the joiner waits.
    Waiting,
    /// The host removed this member.
    Removed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The member's role changed.
    RoleChanged { role: MemberRole },
    /// The channels the member reaches, whole — sent when a roster, a
    /// channel or the member's role moved.
    ChannelsChanged {
        #[serde(default)]
        channels: Vec<Channel>,
    },
    /// The people, whole.
    MembersChanged {
        #[serde(default)]
        members: Vec<Directory>,
    },
    /// A member asks the host to open a direct channel with these people
    /// (the host adds the asker); the host answers with `ChannelsChanged`.
    OpenDm {
        #[serde(default)]
        participants: Vec<PrincipalId>,
    },
    /// A member leaves the workspace.
    Leave,
    /// A direct endpoint announcement: how to reach this node for attachment
    /// bytes over QUIC.
    IrohAddr {
        node_id: String,
        #[serde(default)]
        addrs: Vec<String>,
    },
    /// A member's profile as they set it on their node — right after the
    /// welcome, and whenever it changes: a label (`None` keeps the host's),
    /// a face by reference (`None` clears it) and its bytes, once.
    Profile {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        photo: Option<AttachmentRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<Face>,
    },
    /// A member asks the host for a face the directory names and it lacks.
    WantFace { sha256: String },
    /// The host answers with the bytes — a picture within the cap, or nothing.
    Face { face: Face },
}

impl Control {
    /// The word a log line uses.
    pub fn kind(&self) -> &'static str {
        match self {
            Control::Join { .. } => "join",
            Control::Welcome { .. } => "welcome",
            Control::Refused { .. } => "refused",
            Control::Waiting => "waiting",
            Control::Removed { .. } => "removed",
            Control::RoleChanged { .. } => "role_changed",
            Control::ChannelsChanged { .. } => "channels_changed",
            Control::MembersChanged { .. } => "members_changed",
            Control::OpenDm { .. } => "open_dm",
            Control::Leave => "leave",
            Control::IrohAddr { .. } => "iroh_addr",
            Control::Profile { .. } => "profile",
            Control::WantFace { .. } => "want_face",
            Control::Face { .. } => "face",
        }
    }

    /// Whether a member may send this to a host — the rest is a host's word
    /// and a member sending it is ignored.
    pub fn is_members_word(&self) -> bool {
        matches!(
            self,
            Control::Join { .. }
                | Control::OpenDm { .. }
                | Control::Leave
                | Control::IrohAddr { .. }
                | Control::Profile { .. }
                | Control::WantFace { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_is_tagged_by_type_and_round_trips() {
        let pk = PrincipalId::new("ab".repeat(32)).unwrap();
        let controls = vec![
            Control::Join {
                secret: "s".into(),
                label: None,
                client: CLIENT_MOBILE.into(),
            },
            Control::Welcome {
                workspace: HostCard {
                    pubkey: pk.clone(),
                    name: "Acme".into(),
                    relays: vec!["wss://r.example".into()],
                },
                role: MemberRole::Guest,
                members: vec![Directory {
                    pubkey: pk.clone(),
                    role: MemberRole::Owner,
                    label: Some("Alice".into()),
                    photo: None,
                }],
                channels: vec![Channel::general(0)],
            },
            Control::Refused {
                reason: "expired".into(),
            },
            Control::Waiting,
            Control::Removed { reason: None },
            Control::RoleChanged {
                role: MemberRole::Member,
            },
            Control::ChannelsChanged { channels: vec![] },
            Control::MembersChanged { members: vec![] },
            Control::OpenDm {
                participants: vec![pk.clone()],
            },
            Control::Leave,
            Control::IrohAddr {
                node_id: "n".into(),
                addrs: vec![],
            },
            Control::Profile {
                label: Some("Bob".into()),
                photo: None,
                face: None,
            },
            Control::WantFace {
                sha256: "f".repeat(64),
            },
            Control::Face {
                face: Face::from_bytes(b"\x89PNG\r\n\x1a\n"),
            },
        ];
        for c in controls {
            let json = serde_json::to_value(&c).unwrap();
            assert_eq!(json["type"], c.kind(), "{c:?}");
            assert_eq!(serde_json::from_value::<Control>(json).unwrap(), c);
        }
        assert!(Control::Leave.is_members_word());
        assert!(
            Control::Profile {
                label: None,
                photo: None,
                face: None
            }
            .is_members_word()
                && Control::WantFace {
                    sha256: String::new()
                }
                .is_members_word(),
            "a member says its profile and asks for a face"
        );
        assert!(
            !Control::Face {
                face: Face::from_bytes(b"")
            }
            .is_members_word(),
            "a face is the host's word"
        );
        assert!(!Control::Waiting.is_members_word());
        assert!(!Control::RoleChanged {
            role: MemberRole::Admin
        }
        .is_members_word());
    }

    #[test]
    fn a_face_is_decoded_only_when_short_honest_and_a_small_picture() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.resize(2000, 7);
        let face = Face::from_bytes(&png);
        assert_eq!(face.sha256.len(), 64);
        assert_eq!(face.decode().unwrap(), png, "the bytes come back");

        let mut lying = face.clone();
        lying.sha256 = "0".repeat(64);
        assert!(
            matches!(lying.decode(), Err(FaceRefusal::HashLies { .. })),
            "a hash that lies is refused before anything is kept"
        );

        let not_b64 = Face {
            sha256: "0".repeat(64),
            b64: "***".into(),
        };
        assert_eq!(not_b64.decode(), Err(FaceRefusal::NotBase64));

        let too_long = Face {
            sha256: "0".repeat(64),
            b64: "A".repeat(MAX_FACE_B64_CHARS + 4),
        };
        assert!(
            matches!(too_long.decode(), Err(FaceRefusal::TooLong { .. })),
            "the string is refused before a byte is decoded"
        );

        let text = Face::from_bytes(b"not a picture");
        assert_eq!(text.decode(), Err(FaceRefusal::NotAFace));

        let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
        big.resize(MAX_FACE_BYTES as usize + 1, 0);
        assert_eq!(
            Face::from_bytes(&big).decode(),
            Err(FaceRefusal::NotAFace),
            "a picture over the cap is not a face"
        );
        let at_cap = {
            let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
            b.resize(MAX_FACE_BYTES as usize, 0);
            b
        };
        let face = Face::from_bytes(&at_cap);
        assert!(
            face.b64.len() <= MAX_FACE_B64_CHARS,
            "a full face fits the string cap"
        );
        assert!(face.decode().is_ok());
        // What rides a relay: the control as JSON, before the wrap — well under what public relays refuse.
        let json = serde_json::to_string(&Control::Face { face }).unwrap();
        assert!(json.len() < 24 * 1024, "{}", json.len());
    }
}
