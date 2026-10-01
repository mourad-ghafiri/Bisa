//! What an error may carry out of a probe: never a header or an environment
//! value, never a URL's userinfo. The keys may show — a person needs to
//! know *which* variable — the values are the machine's alone.

use bisa_core::McpServerConfig;

/// The placeholder a secret reads as inside an error.
pub const REDACTED: &str = "[redacted]";

/// `text` with every value of the transport's `env`/`headers` replaced, and
/// any `user:pass@` in front of a host removed.
pub fn sanitize(text: &str, config: &McpServerConfig) -> String {
    let mut out = text.to_string();
    let values: Vec<&String> = match config {
        McpServerConfig::Stdio { env, .. } => env.values().collect(),
        McpServerConfig::Http { headers, .. } | McpServerConfig::Sse { headers, .. } => {
            headers.values().collect()
        }
    };
    for v in values {
        if v.len() >= 3 && v != bisa_core::mcp::MASK {
            out = out.replace(v.as_str(), REDACTED);
        }
    }
    strip_userinfo(&out)
}

/// `https://user:pass@host/x` → `https://host/x`, wherever a URL appears in the text.
pub fn strip_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let authority_end = tail.find(['/', '?', '#', ' ', '\n']).unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        match authority.rfind('@') {
            Some(i) => out.push_str(&authority[i + 1..]),
            None => out.push_str(authority),
        }
        rest = &tail[authority_end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn values_and_userinfo_never_leave_an_error() {
        let config = McpServerConfig::Http {
            name: "docs".into(),
            url: "https://x.test/mcp".into(),
            headers: BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer sk-live-123".to_string(),
            )]),
        };
        let e = sanitize(
            "401 from https://bob:pw@x.test/mcp with Bearer sk-live-123",
            &config,
        );
        assert_eq!(e, format!("401 from https://x.test/mcp with {REDACTED}"));
        let proc = McpServerConfig::Stdio {
            name: "pg".into(),
            command: "pg-mcp".into(),
            args: vec![],
            env: BTreeMap::from([
                ("PGPASSWORD".to_string(), "hunter2".to_string()),
                ("N".to_string(), "1".to_string()),
            ]),
            cwd: None,
        };
        assert_eq!(
            sanitize("auth failed for hunter2 on db 1", &proc),
            format!("auth failed for {REDACTED} on db 1"),
            "a one-character value is not redacted out of every digit"
        );
        assert_eq!(strip_userinfo("no url here"), "no url here");
    }
}
