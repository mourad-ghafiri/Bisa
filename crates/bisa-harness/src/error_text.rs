//! How a harness failure is said to a person: `error-harness-…` in `locales/en/errors.ftl`.

use bisa_core::{Localize, Text};

impl Localize for crate::catalog::CatalogError {
    fn text(&self) -> Text {
        match self {
            crate::catalog::CatalogError::ReservedId(v0) => {
                bisa_core::text!("error-harness-catalog-reserved-id", v0 = format!("{v0:?}"))
            }
            crate::catalog::CatalogError::InvalidDescriptor { path, message, .. } => {
                bisa_core::text!(
                    "error-harness-catalog-invalid-descriptor",
                    path = path.to_string(),
                    message = message.to_string()
                )
            }
            crate::catalog::CatalogError::Io { path, message, .. } => bisa_core::text!(
                "error-harness-catalog-io",
                path = path.to_string(),
                message = message.to_string()
            ),
        }
    }
}

impl Localize for crate::error::HarnessError {
    fn text(&self) -> Text {
        match self {
            crate::error::HarnessError::Unavailable(v0) => {
                bisa_core::text!("error-harness-unavailable", v0 = v0.to_string())
            }
            crate::error::HarnessError::ModelUnavailable { model, reason, .. } => bisa_core::text!(
                "error-harness-model-unavailable",
                model = model.to_string(),
                reason = reason.to_string()
            ),
            crate::error::HarnessError::Busy => bisa_core::text!("error-harness-busy"),
            crate::error::HarnessError::NotSupported(v0) => {
                bisa_core::text!("error-harness-not-supported", v0 = v0.to_string())
            }
            crate::error::HarnessError::Protocol(v0) => {
                bisa_core::text!("error-harness-protocol", v0 = v0.to_string())
            }
            crate::error::HarnessError::Io(v0) => {
                bisa_core::text!("error-harness-io", v0 = v0.to_string())
            }
            crate::error::HarnessError::Terminated => bisa_core::text!("error-harness-terminated"),
        }
    }
}
