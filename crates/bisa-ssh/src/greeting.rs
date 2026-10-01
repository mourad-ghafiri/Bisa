//! What a git host says to `ssh -T` — the crate's one prose boundary, like
//! `bisa-vcs::classify`. Each host's sentence is read once into a typed
//! outcome; an unknown host still reads as authenticated or refused from
//! OpenSSH's own words, only without a login.

/// The outcome of one authentication handshake.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HostGreeting {
    /// The host took the key. `login` is who it says you are, when its
    /// greeting names you (GitHub, GitLab, Bitbucket do).
    Authenticated { login: Option<String> },
    /// The host refused every key offered.
    Refused { reason: String },
    /// The host's key is not in `known_hosts` — the person connects once from
    /// a terminal to accept it; the platform never accepts one for them.
    HostKeyUnknown { reason: String },
    /// The name did not resolve, the connection was refused or timed out.
    Unreachable { reason: String },
}

/// Read `ssh -T user@host`'s exit code and streams. GitHub exits **1** on
/// success (*Hi X! You've successfully authenticated, but GitHub does not
/// provide shell access.*); GitLab exits 0 with *Welcome to GitLab, @X!*;
/// Bitbucket says *authenticated via ssh key* and *logged in as X*.
pub fn parse_greeting(host: &str, code: i32, stdout: &str, stderr: &str) -> HostGreeting {
    let text = format!("{stdout}\n{stderr}");
    let first = |needle: &str| -> Option<String> {
        text.lines()
            .find(|l| l.contains(needle))
            .map(|l| l.trim().to_string())
    };
    if text.contains("Host key verification failed") {
        return HostGreeting::HostKeyUnknown {
            reason: first("Host key verification failed").unwrap_or_default(),
        };
    }
    if text.contains("Permission denied") {
        return HostGreeting::Refused {
            reason: first("Permission denied").unwrap_or_default(),
        };
    }
    if let Some(line) = first("Could not resolve hostname")
        .or_else(|| first("Connection refused"))
        .or_else(|| first("Connection timed out"))
        .or_else(|| first("Operation timed out"))
        .or_else(|| first("Network is unreachable"))
        .or_else(|| first("Connection closed by"))
    {
        return HostGreeting::Unreachable { reason: line };
    }
    let login = login_in_greeting(host, &text);
    if login.is_some()
        || code == 0
        || text.contains("successfully authenticated")
        || text.contains("authenticated via")
    {
        return HostGreeting::Authenticated { login };
    }
    HostGreeting::Unreachable {
        reason: text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("ssh said nothing")
            .to_string(),
    }
}

/// The login a known host's greeting names.
fn login_in_greeting(host: &str, text: &str) -> Option<String> {
    let host = host.to_ascii_lowercase();
    if host.ends_with("github.com") || text.contains("GitHub does not provide shell access") {
        // "Hi octocat! You've successfully authenticated, ..."
        let after = text.split("Hi ").nth(1)?;
        let login = after.split('!').next()?.trim();
        return (!login.is_empty()).then(|| login.to_string());
    }
    if host.ends_with("gitlab.com") || text.contains("Welcome to GitLab") {
        let after = text.split("Welcome to GitLab, @").nth(1)?;
        let login = after.split('!').next()?.trim();
        return (!login.is_empty()).then(|| login.to_string());
    }
    if host.ends_with("bitbucket.org") || text.contains("logged in as") {
        let after = text.split("logged in as ").nth(1)?;
        let login = after
            .split(|c: char| c.is_whitespace() || c == '.')
            .next()?
            .trim();
        return (!login.is_empty()).then(|| login.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hosts_greeting_names_the_login_and_the_exit_code_is_not_a_failure() {
        let gh = parse_greeting("github.com", 1, "", "Hi octocat! You've successfully authenticated, but GitHub does not provide shell access.\n");
        assert_eq!(
            gh,
            HostGreeting::Authenticated {
                login: Some("octocat".into())
            }
        );
        let alias = parse_greeting("github-work", 1, "", "Hi ada-acme! You've successfully authenticated, but GitHub does not provide shell access.\n");
        assert_eq!(
            alias,
            HostGreeting::Authenticated {
                login: Some("ada-acme".into())
            },
            "an alias is read by the sentence, not the host"
        );
        let gl = parse_greeting("gitlab.com", 0, "Welcome to GitLab, @ada!\n", "");
        assert_eq!(
            gl,
            HostGreeting::Authenticated {
                login: Some("ada".into())
            }
        );
        let bb = parse_greeting("bitbucket.org", 0, "authenticated via ssh key.\n\nYou can use git to connect to Bitbucket. Shell access is disabled\n\nThis deploy key has read access to the following repositories:\n", "logged in as ada.\n");
        assert_eq!(
            bb,
            HostGreeting::Authenticated {
                login: Some("ada".into())
            }
        );
        let other = parse_greeting("git.example", 0, "", "");
        assert_eq!(
            other,
            HostGreeting::Authenticated { login: None },
            "an unknown host that let us in names nobody"
        );
    }

    #[test]
    fn refusals_unknown_host_keys_and_unreachable_hosts_are_told_apart() {
        assert!(matches!(
            parse_greeting("github.com", 255, "", "git@github.com: Permission denied (publickey).\n"),
            HostGreeting::Refused { reason } if reason.contains("Permission denied")
        ));
        assert!(matches!(
            parse_greeting("github.com", 255, "", "Host key verification failed.\n"),
            HostGreeting::HostKeyUnknown { .. }
        ));
        assert!(matches!(
            parse_greeting("nope.invalid", 255, "", "ssh: Could not resolve hostname nope.invalid: nodename nor servname provided\n"),
            HostGreeting::Unreachable { reason } if reason.contains("Could not resolve")
        ));
        assert!(matches!(
            parse_greeting(
                "github.com",
                255,
                "",
                "ssh: connect to host github.com port 22: Connection refused\n"
            ),
            HostGreeting::Unreachable { .. }
        ));
        assert!(matches!(
            parse_greeting("github.com", 255, "", "ssh_exchange_identification: read: Connection reset by peer\n"),
            HostGreeting::Unreachable { reason } if reason.contains("Connection reset")
        ));
    }
}
