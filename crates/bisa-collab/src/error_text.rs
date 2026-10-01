//! How the wire's refusals are said to a person: `error-collab-…` in `locales/en/errors.ftl`.

use bisa_core::{Localize, Text};

impl Localize for crate::control::FaceRefusal {
    fn text(&self) -> Text {
        match self {
            crate::control::FaceRefusal::TooLong { chars, max, .. } => bisa_core::text!(
                "error-collab-face-refusal-too-long",
                chars = *chars,
                max = *max
            ),
            crate::control::FaceRefusal::NotBase64 => {
                bisa_core::text!("error-collab-face-refusal-not-base64")
            }
            crate::control::FaceRefusal::HashLies { expected, .. } => bisa_core::text!(
                "error-collab-face-refusal-hash-lies",
                expected = expected.to_string()
            ),
            crate::control::FaceRefusal::NotAFace => {
                bisa_core::text!("error-collab-face-refusal-not-a-face")
            }
        }
    }
}

impl Localize for crate::CollabError {
    fn text(&self) -> Text {
        match self {
            crate::CollabError::Wrap(v0) => {
                bisa_core::text!("error-collab-wrap", v0 = v0.to_string())
            }
            crate::CollabError::InviteCode(v0) => {
                bisa_core::text!("error-collab-invite-code", v0 = v0.to_string())
            }
            crate::CollabError::Relay(v0) => {
                bisa_core::text!("error-collab-relay", v0 = v0.to_string())
            }
        }
    }
}
