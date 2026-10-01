//! How a provider's failure is said to a person: `error-decision-…` in `locales/en/errors.ftl`.

use bisa_core::{Localize, Text};

impl Localize for crate::provider::ProviderError {
    fn text(&self) -> Text {
        match self {
            crate::provider::ProviderError::Misconfigured(v0) => {
                bisa_core::text!("error-decision-provider-misconfigured", v0 = v0.to_string())
            }
            crate::provider::ProviderError::Refused {
                status, message, ..
            } => bisa_core::text!(
                "error-decision-provider-refused",
                status = *status,
                message = message.to_string()
            ),
            crate::provider::ProviderError::Busy { status, .. } => {
                bisa_core::text!("error-decision-provider-busy", status = *status)
            }
            crate::provider::ProviderError::Unreachable(v0) => {
                bisa_core::text!("error-decision-provider-unreachable", v0 = v0.to_string())
            }
            crate::provider::ProviderError::Unreadable(v0) => {
                bisa_core::text!("error-decision-provider-unreadable", v0 = v0.to_string())
            }
            crate::provider::ProviderError::Contract(v0) => {
                bisa_core::text!("error-decision-provider-contract", v0 = v0.to_string())
            }
            crate::provider::ProviderError::TimedOut => {
                bisa_core::text!("error-decision-provider-timed-out")
            }
        }
    }
}
