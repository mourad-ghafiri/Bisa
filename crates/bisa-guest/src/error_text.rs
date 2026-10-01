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
