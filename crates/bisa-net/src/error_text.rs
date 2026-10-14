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

#[cfg(test)]
mod tests {
    use crate::NetError;
    use bisa_core::text::plain;
    use bisa_core::Localize;

    #[test]
    fn every_failure_of_the_pump_is_said_from_the_catalog_as_its_display_says() {
        let errors = [
            NetError::Config("c".into()),
            NetError::Relay("r".into()),
            NetError::Collab(bisa_collab::CollabError::Relay("x".into())),
            NetError::Store(bisa_store::StoreError::KeyStore("k".into())),
            NetError::Io(std::io::Error::other("no")),
        ];
        for e in &errors {
            let said = e.text();
            let id = said.id.to_string();
            assert!(plain::has(&id), "{id} is a message of the English catalog");
            assert_eq!(said.to_string(), e.to_string(), "{id}");
        }
    }
}
