//! What ssh-agent holds, read from `ssh-add -l`. Pure over the program's
//! answer; the exit code carries as much as the text: 0 lists keys, 1 is
//! *no identities*, 2 is *no agent to ask*.

/// One key ssh-agent holds — a fingerprint and a comment, never the key.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AgentKey {
    pub bits: u32,
    /// `SHA256:…`
    pub fingerprint: String,
    pub comment: String,
    /// `ED25519`, `RSA` … as `ssh-add` prints it in parentheses.
    pub algorithm: String,
}

/// ssh-agent as the platform sees it.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct AgentState {
    /// An agent answered — `SSH_AUTH_SOCK` reaches one.
    pub available: bool,
    pub keys: Vec<AgentKey>,
    /// The program's words when it is not available, for the panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl AgentState {
    pub fn holds(&self, fingerprint: &str) -> bool {
        self.keys.iter().any(|k| k.fingerprint == fingerprint)
    }
}

/// Read `ssh-add -l`'s answer: `256 SHA256:… comment (ED25519)` per line.
pub fn parse_ssh_add_list(code: i32, stdout: &str, stderr: &str) -> AgentState {
    match code {
        0 => AgentState {
            available: true,
            keys: stdout.lines().filter_map(parse_line).collect(),
            reason: None,
        },
        1 => AgentState {
            available: true,
            keys: Vec::new(),
            reason: None,
        },
        _ => AgentState {
            available: false,
            keys: Vec::new(),
            reason: Some(
                stderr
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("ssh-agent did not answer")
                    .to_string(),
            ),
        },
    }
}

fn parse_line(line: &str) -> Option<AgentKey> {
    let line = line.trim();
    let (bits, rest) = line.split_once(' ')?;
    let (fingerprint, rest) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
    let rest = rest.trim();
    let (comment, algorithm) = match rest.rsplit_once(" (") {
        Some((c, a)) => (c.trim(), a.trim_end_matches(')').trim()),
        None => (rest, ""),
    };
    Some(AgentKey {
        bits: bits.parse().ok()?,
        fingerprint: fingerprint.to_string(),
        comment: comment.to_string(),
        algorithm: algorithm.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_exit_codes_read_as_keys_none_and_no_agent() {
        let listed = parse_ssh_add_list(
            0,
            "256 SHA256:abcDEF123 ada@laptop (ED25519)\n3072 SHA256:zzz work key with spaces (RSA)\n",
            "",
        );
        assert!(listed.available);
        assert_eq!(listed.keys.len(), 2);
        assert_eq!(
            listed.keys[0],
            AgentKey {
                bits: 256,
                fingerprint: "SHA256:abcDEF123".into(),
                comment: "ada@laptop".into(),
                algorithm: "ED25519".into()
            }
        );
        assert_eq!(listed.keys[1].comment, "work key with spaces");
        assert_eq!(listed.keys[1].algorithm, "RSA");
        assert!(listed.holds("SHA256:zzz") && !listed.holds("SHA256:nope"));

        let empty = parse_ssh_add_list(1, "", "The agent has no identities.\n");
        assert!(empty.available && empty.keys.is_empty() && empty.reason.is_none());

        let none = parse_ssh_add_list(
            2,
            "",
            "Could not open a connection to your authentication agent.\n",
        );
        assert!(!none.available);
        assert_eq!(
            none.reason.as_deref(),
            Some("Could not open a connection to your authentication agent.")
        );
    }
}
