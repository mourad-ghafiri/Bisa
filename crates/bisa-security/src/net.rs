//! The one outbound-network rule the platform's own HTTP obeys: a connector
//! reaches the hosts its definition declares, minus what a person forbids,
//! plus what a person allows. Pure over strings — the engine reads the two
//! settings lists, asks [`decide_host`] before a request is built, and
//! journals a refusal the way it journals a refused command.
//!
//! Deny wins over every allow, because a person who wrote a host down under
//! *deny* meant it whatever a definition — shipped or pasted — says. A host
//! nobody declared and nobody allowed is refused with the connector's own
//! list in the reason, so the fix is one line in one of two places.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The two lists a person keeps: `security.net.deny_hosts` and
/// `security.net.allow_hosts`, concatenated across the workspace and the
/// machine. A pattern is `host[:port]` or `*.suffix`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostPolicy {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// Who let a host through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostAllowedBy {
    /// The connector's own definition names it.
    Declared,
    /// `security.net.allow_hosts` names it.
    Policy,
}

/// The verdict on one host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostVerdict {
    Allow { by: HostAllowedBy },
    Deny { reason: String },
}

impl HostVerdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, HostVerdict::Allow { .. })
    }
}

/// An entry of a host list that is not a host pattern, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "HostProblem")]
pub struct HostProblem {
    /// The entry's index in its list, as `#<n>`.
    pub entry: String,
    pub reason: String,
}

/// Read a settings value as a host list. Entries that are not strings or not
/// host patterns are reported by index and skipped; a value that is not a
/// list is an empty list with one problem.
pub fn parse_hosts(value: &Value) -> (Vec<String>, Vec<HostProblem>) {
    let mut hosts = Vec::new();
    let mut problems = Vec::new();
    let Some(items) = value.as_array() else {
        if !value.is_null() {
            problems.push(HostProblem {
                entry: "#0".into(),
                reason: "the value is not a list".into(),
            });
        }
        return (hosts, problems);
    };
    for (i, item) in items.iter().enumerate() {
        match item.as_str().map(str::trim) {
            Some(s) if is_host_pattern(s) => hosts.push(s.to_ascii_lowercase()),
            Some(s) => problems.push(HostProblem {
                entry: format!("#{i}"),
                reason: format!("{s:?} is not a host: write host[:port] or *.suffix"),
            }),
            None => problems.push(HostProblem {
                entry: format!("#{i}"),
                reason: "not a string".into(),
            }),
        }
    }
    (hosts, problems)
}

pub use bisa_netrules::{host_matches, is_host_pattern};

/// Judge one resolved host (`host` or `host:port`, as the URL carries it)
/// against the person's policy and the connector's declared hosts. Deny
/// first, then declared, then allow, else refused with the declared list.
pub fn decide_host(policy: &HostPolicy, declared: &[String], host: &str) -> HostVerdict {
    let host = host.to_ascii_lowercase();
    if policy.deny.iter().any(|p| host_matches(p, &host)) {
        return HostVerdict::Deny {
            reason: format!("security.net.deny_hosts forbids {host}"),
        };
    }
    if declared.iter().any(|p| host_matches(p, &host)) {
        return HostVerdict::Allow {
            by: HostAllowedBy::Declared,
        };
    }
    if policy.allow.iter().any(|p| host_matches(p, &host)) {
        return HostVerdict::Allow {
            by: HostAllowedBy::Policy,
        };
    }
    HostVerdict::Deny {
        reason: if declared.is_empty() {
            format!("{host} is not a host the connector declares (it declares none)")
        } else {
            format!(
                "{host} is not a host the connector declares (it declares {})",
                declared.join(", ")
            )
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy(allow: &[&str], deny: &[&str]) -> HostPolicy {
        HostPolicy {
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn declared(hosts: &[&str]) -> Vec<String> {
        hosts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn deny_wins_then_declared_then_allow_then_refused() {
        let p = policy(&["*.trusted.io"], &["evil.example.com", "*.blocked.net"]);
        let d = declared(&["api.slack.com", "*.atlassian.net"]);
        assert_eq!(
            decide_host(&p, &d, "api.slack.com"),
            HostVerdict::Allow {
                by: HostAllowedBy::Declared
            }
        );
        assert_eq!(
            decide_host(&p, &d, "x.trusted.io"),
            HostVerdict::Allow {
                by: HostAllowedBy::Policy
            }
        );
        assert_eq!(
            decide_host(&p, &d, "evil.example.com"),
            HostVerdict::Deny {
                reason: "security.net.deny_hosts forbids evil.example.com".into()
            }
        );
        // A declared host that the person denied is still denied.
        let denied = policy(&[], &["api.slack.com"]);
        assert!(!decide_host(&denied, &d, "api.slack.com").is_allowed());
        let HostVerdict::Deny { reason } = decide_host(&p, &d, "other.example.org") else {
            panic!("an undeclared host is refused");
        };
        assert!(reason.contains("other.example.org"), "{reason}");
        assert!(
            reason.contains("api.slack.com, *.atlassian.net"),
            "{reason}"
        );
        let HostVerdict::Deny { reason } = decide_host(&p, &[], "other.example.org") else {
            panic!();
        };
        assert!(reason.contains("declares none"), "{reason}");
    }

    #[test]
    fn a_host_list_is_read_leniently_and_problems_are_named_by_index() {
        let (hosts, problems) = parse_hosts(&json!(["API.Slack.com", 3, "not a host", "*.x.io"]));
        assert_eq!(hosts, vec!["api.slack.com", "*.x.io"]);
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0].entry, "#1");
        assert_eq!(problems[1].entry, "#2");
        assert!(problems[1].reason.contains("not a host"));
        let (hosts, problems) = parse_hosts(&json!("nope"));
        assert!(hosts.is_empty());
        assert_eq!(problems.len(), 1);
        assert_eq!(parse_hosts(&Value::Null), (vec![], vec![]));
    }
}
