//! How the pump's failures are said to a person: `error-net-…` in `locales/en/errors.ftl`.

use bisa_core::{Localize, Text};

impl Localize for crate::NetError {
    fn text(&self) -> Text {
        match self {
            crate::NetError::Config(v0) => {
                bisa_core::text!("error-net-config", v0 = v0.to_string())
            }
            crate::NetError::Relay(v0) => bisa_core::text!("error-net-relay", v0 = v0.to_string()),
            crate::NetError::Collab(inner) => inner.text(),
            crate::NetError::Store(inner) => inner.text(),
            crate::NetError::Io(inner) => {
                bisa_core::text!("error-net-io", detail = inner.to_string())
            }
        }
    }
}
