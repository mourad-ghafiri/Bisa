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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderError;

    /// Every failure of a provider is said under its own id.
    #[test]
    fn every_provider_failure_is_said_under_its_own_id() {
        let id = |e: ProviderError| e.text().id.to_string();
        assert_eq!(
            id(ProviderError::Misconfigured("no key".into())),
            "error-decision-provider-misconfigured"
        );
        assert_eq!(
            id(ProviderError::Refused {
                status: 401,
                message: "no".into()
            }),
            "error-decision-provider-refused"
        );
        assert_eq!(
            id(ProviderError::Busy {
                status: 429,
                retry_after: None
            }),
            "error-decision-provider-busy"
        );
        assert_eq!(
            id(ProviderError::Unreachable("down".into())),
            "error-decision-provider-unreachable"
        );
        assert_eq!(
            id(ProviderError::Unreadable("nope".into())),
            "error-decision-provider-unreadable"
        );
        assert_eq!(
            id(ProviderError::Contract(
                bisa_core::DecisionContractError::NoQuestions
            )),
            "error-decision-provider-contract"
        );
        assert_eq!(
            id(ProviderError::TimedOut),
            "error-decision-provider-timed-out"
        );
    }
}
