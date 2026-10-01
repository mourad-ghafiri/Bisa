//! What a call can fail with, and the one rule every message obeys: a secret
//! the call exposed is never in it.

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("the connector definition is unusable: {0}")]
    BadDefinition(String),
    #[error("parameter `{name}`: {why}")]
    BadParam { name: String, why: String },
    #[error("{{{0}}} has no value")]
    Unresolved(String),
    #[error("{host} is not a host this connector may reach ({allowed})")]
    HostRefused { host: String, allowed: String },
    #[error("not authenticated: {0}")]
    NotAuthenticated(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("the service refused ({status}): {reason}")]
    Refused { status: u16, reason: String },
    #[error("rate limited; try again in {retry_after}s")]
    RateLimited { retry_after: u64 },
    #[error("the service failed ({status}): {reason}")]
    Upstream { status: u16, reason: String },
    #[error("could not connect: {0}")]
    Unreachable(String),
    #[error("the connection failed: {0}")]
    Transport(String),
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error(
        "{host} failed {} calls in a row; calls to it are paused for {until_secs}s",
        crate::breaker::OPEN_AFTER
    )]
    Open { host: String, until_secs: u64 },
    #[error("the answer has no `{path}`")]
    SelectMissing { path: String },
    #[error("the answer is {0} bytes; the cap is 1 MiB")]
    TooLarge(usize),
    #[error("OAuth: {0}")]
    OAuth(String),
    #[error("credential store: {0}")]
    Store(String),
}

impl ConnectorError {
    /// Something the definition, the step or the person must fix; the rest
    /// is the network's or the service's, and a retry may see it pass.
    pub fn is_refusal(&self) -> bool {
        !matches!(
            self,
            ConnectorError::Transport(_)
                | ConnectorError::Unreachable(_)
                | ConnectorError::Timeout(_)
                | ConnectorError::Open { .. }
                | ConnectorError::Upstream { .. }
                | ConnectorError::RateLimited { .. }
                | ConnectorError::Store(_)
        )
    }

    /// The same error with every exposed secret scrubbed from its text.
    pub fn scrubbed(self, secrets: &[&str]) -> Self {
        let s = |t: String| scrub(&t, secrets);
        match self {
            ConnectorError::BadDefinition(t) => ConnectorError::BadDefinition(s(t)),
            ConnectorError::BadParam { name, why } => ConnectorError::BadParam {
                name: s(name),
                why: s(why),
            },
            ConnectorError::Unresolved(t) => ConnectorError::Unresolved(s(t)),
            ConnectorError::HostRefused { host, allowed } => ConnectorError::HostRefused {
                host: s(host),
                allowed: s(allowed),
            },
            ConnectorError::NotAuthenticated(t) => ConnectorError::NotAuthenticated(s(t)),
            ConnectorError::NotFound(t) => ConnectorError::NotFound(s(t)),
            ConnectorError::Refused { status, reason } => ConnectorError::Refused {
                status,
                reason: s(reason),
            },
            ConnectorError::Upstream { status, reason } => ConnectorError::Upstream {
                status,
                reason: s(reason),
            },
            ConnectorError::Transport(t) => ConnectorError::Transport(s(t)),
            ConnectorError::Unreachable(t) => ConnectorError::Unreachable(s(t)),
            ConnectorError::SelectMissing { path } => {
                ConnectorError::SelectMissing { path: s(path) }
            }
            ConnectorError::OAuth(t) => ConnectorError::OAuth(s(t)),
            ConnectorError::Store(t) => ConnectorError::Store(s(t)),
            ConnectorError::Open { host, until_secs } => ConnectorError::Open {
                host: s(host),
                until_secs,
            },
            other @ (ConnectorError::RateLimited { .. }
            | ConnectorError::Timeout(_)
            | ConnectorError::TooLarge(_)) => other,
        }
    }
}

/// `text` with every non-empty secret — and its base64 form, which is how a
/// Basic credential travels — replaced by `«redacted»`.
pub fn scrub(text: &str, secrets: &[&str]) -> String {
    use base64::Engine as _;
    let mut out = text.to_string();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        let encoded = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
        for needle in [*secret, encoded.as_str()] {
            if !needle.is_empty() && out.contains(needle) {
                out = out.replace(needle, "«redacted»");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_removes_every_secret_and_its_base64_form() {
        let text = "key=abc123 basic=YWJjMTIz other";
        assert_eq!(
            scrub(text, &["abc123"]),
            "key=«redacted» basic=«redacted» other"
        );
        assert_eq!(scrub(text, &[""]), text, "an empty secret scrubs nothing");
    }

    #[test]
    fn a_refusal_is_the_callers_to_fix() {
        assert!(ConnectorError::BadParam {
            name: "x".into(),
            why: "required".into()
        }
        .is_refusal());
        assert!(ConnectorError::NotAuthenticated("no token".into()).is_refusal());
        assert!(ConnectorError::HostRefused {
            host: "evil".into(),
            allowed: "good".into()
        }
        .is_refusal());
        assert!(!ConnectorError::Transport("down".into()).is_refusal());
        assert!(!ConnectorError::Upstream {
            status: 500,
            reason: "boom".into()
        }
        .is_refusal());
        assert!(!ConnectorError::RateLimited { retry_after: 3 }.is_refusal());
        assert!(
            !ConnectorError::Open {
                host: "h".into(),
                until_secs: 9
            }
            .is_refusal(),
            "a paused host is the platform's fault, not the caller's"
        );
        assert!(!ConnectorError::Timeout(Duration::from_secs(1)).is_refusal());
    }

    #[test]
    fn scrubbed_keeps_the_variant() {
        let e = ConnectorError::Refused {
            status: 400,
            reason: "bad key sekrit".into(),
        }
        .scrubbed(&["sekrit"]);
        assert!(matches!(e, ConnectorError::Refused { status: 400, .. }));
        assert!(!e.to_string().contains("sekrit"));
    }
}
