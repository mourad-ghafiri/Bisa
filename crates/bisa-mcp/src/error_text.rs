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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::IntakeError;

    /// Every failure of the intake client is said under its own id.
    #[test]
    fn every_intake_failure_is_said_under_its_own_id() {
        let connect = IntakeError::Connect {
            path: "/ws/run/intake.sock".into(),
            source: std::io::Error::other("refused"),
        }
        .text();
        assert_eq!(connect.id, "error-mcp-intake-connect");
        assert!(connect.args.contains_key("path") && connect.args.contains_key("source"));
        assert_eq!(
            IntakeError::Io(std::io::Error::other("broke")).text().id,
            "error-mcp-intake-io"
        );
        assert_eq!(
            IntakeError::BadReply("nope".into()).text().id,
            "error-mcp-intake-bad-reply"
        );
        assert_eq!(IntakeError::Closed.text().id, "error-mcp-intake-closed");
    }
}
