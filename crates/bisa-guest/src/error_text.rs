//! How a guest's refusals are said to a person: `error-guest-…` in `locales/en/errors.ftl`.

use bisa_core::{Localize, Text};

impl Localize for crate::GuestError {
    fn text(&self) -> Text {
        match self {
            crate::GuestError::Store(v0) => {
                bisa_core::text!("error-guest-store", v0 = v0.to_string())
            }
            crate::GuestError::NotHosted => bisa_core::text!("error-guest-not-hosted"),
            crate::GuestError::Refused(v0) => {
                bisa_core::text!("error-guest-refused", v0 = v0.to_string())
            }
            crate::GuestError::Collab(inner) => inner.text(),
            crate::GuestError::Io(inner) => {
                bisa_core::text!("error-guest-io", detail = inner.to_string())
            }
            crate::GuestError::Json(inner) => {
                bisa_core::text!("error-guest-json", detail = inner.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::GuestError;
    use bisa_core::text::plain;
    use bisa_core::Localize;

    #[test]
    fn every_refusal_of_a_guest_is_said_from_the_catalog_as_its_display_says() {
        let errors = [
            GuestError::Store("s".into()),
            GuestError::NotHosted,
            GuestError::Refused("r".into()),
            GuestError::Collab(bisa_collab::CollabError::Relay("x".into())),
            GuestError::Io(std::io::Error::other("no")),
            GuestError::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        ];
        for e in &errors {
            let said = e.text();
            let id = said.id.to_string();
            assert!(plain::has(&id), "{id} is a message of the English catalog");
            assert_eq!(said.to_string(), e.to_string(), "{id}");
        }
    }
}
