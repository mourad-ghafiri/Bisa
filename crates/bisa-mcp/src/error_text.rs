//! How the intake client's failures are said to a person: `error-mcp-…` in `locales/en/errors.ftl`.

use bisa_core::{Localize, Text};

impl Localize for crate::client::IntakeError {
    fn text(&self) -> Text {
        match self {
            crate::client::IntakeError::Connect { path, source, .. } => bisa_core::text!(
                "error-mcp-intake-connect",
                path = format!("{}", path.display()),
                source = source.to_string()
            ),
            crate::client::IntakeError::Io(v0) => {
                bisa_core::text!("error-mcp-intake-io", v0 = v0.to_string())
            }
            crate::client::IntakeError::BadReply(v0) => {
                bisa_core::text!("error-mcp-intake-bad-reply", v0 = v0.to_string())
            }
            crate::client::IntakeError::Closed => bisa_core::text!("error-mcp-intake-closed"),
        }
    }
}
