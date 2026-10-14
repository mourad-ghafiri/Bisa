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

#[cfg(test)]
mod tests {
    use crate::control::FaceRefusal;
    use crate::CollabError;
    use bisa_core::text::plain;
    use bisa_core::Localize;

    fn judge(said: bisa_core::Text, shown: String) {
        let id = said.id.to_string();
        assert!(plain::has(&id), "{id} is a message of the English catalog");
        assert_eq!(said.to_string(), shown, "{id}");
    }

    #[test]
    fn every_refusal_of_the_wire_says_in_the_catalog_what_its_display_says() {
        let faces = [
            FaceRefusal::TooLong { chars: 9, max: 4 },
            FaceRefusal::NotBase64,
            FaceRefusal::HashLies {
                expected: "ab".into(),
            },
            FaceRefusal::NotAFace,
        ];
        for refusal in &faces {
            judge(refusal.text(), refusal.to_string());
        }
        let errors = [
            CollabError::Wrap("w".into()),
            CollabError::InviteCode("i".into()),
            CollabError::Relay("r".into()),
        ];
        for e in &errors {
            judge(e.text(), e.to_string());
        }
    }
}
