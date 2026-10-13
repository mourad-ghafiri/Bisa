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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogError;
    use crate::error::HarnessError;

    /// Every failure of the catalog and of a harness is said under its own
    /// id in the errors catalog, carrying what the person needs to read.
    #[test]
    fn every_harness_failure_is_said_under_its_own_id() {
        let said = |t: Text| t.id.to_string();
        assert_eq!(
            said(CatalogError::ReservedId("claude-code".into()).text()),
            "error-harness-catalog-reserved-id"
        );
        assert_eq!(
            said(
                CatalogError::InvalidDescriptor {
                    path: "/h/x.json".into(),
                    message: "not json".into(),
                }
                .text()
            ),
            "error-harness-catalog-invalid-descriptor"
        );
        let io = CatalogError::Io {
            path: "/h".into(),
            message: "gone".into(),
        }
        .text();
        assert_eq!(io.id, "error-harness-catalog-io");
        assert!(io.args.contains_key("path") && io.args.contains_key("message"));

        assert_eq!(
            said(HarnessError::Unavailable("not on PATH".into()).text()),
            "error-harness-unavailable"
        );
        let model = HarnessError::model_unavailable("opus", "quota", Some(9)).text();
        assert_eq!(model.id, "error-harness-model-unavailable");
        assert!(model.args.contains_key("model") && model.args.contains_key("reason"));
        assert_eq!(said(HarnessError::Busy.text()), "error-harness-busy");
        assert_eq!(
            said(HarnessError::NotSupported("steer").text()),
            "error-harness-not-supported"
        );
        assert_eq!(
            said(HarnessError::protocol("bad frame").text()),
            "error-harness-protocol"
        );
        assert_eq!(
            said(HarnessError::Io(std::io::Error::other("pipe")).text()),
            "error-harness-io"
        );
        assert_eq!(
            said(HarnessError::Terminated.text()),
            "error-harness-terminated"
        );
    }
}
