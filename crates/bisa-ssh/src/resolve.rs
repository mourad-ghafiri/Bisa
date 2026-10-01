//! What `ssh -G <host>` says ssh would do — the configuration resolved
//! offline, no connection made: the real hostname behind an alias, the user,
//! the port, and the identity files it would offer. Pure over the text.

use std::path::{Path, PathBuf};

#[derive(
    Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Resolved {
    /// The host ssh would connect to — what an alias stands for.
    pub hostname: String,
    pub user: String,
    pub port: u16,
    /// The private keys ssh would offer, in order, `~` expanded. Only the
    /// paths: whether each exists is the caller's to check against its
    /// `.pub` files.
    pub identity_files: Vec<PathBuf>,
    pub identities_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
}

/// Read `ssh -G`'s `key value` lines. `home` is what `~` expands to.
pub fn parse_ssh_g(text: &str, home: &Path) -> Resolved {
    let mut r = Resolved {
        port: 22,
        ..Default::default()
    };
    for line in text.lines() {
        let line = line.trim();
        let (key, value) = match line.split_once(' ') {
            Some(kv) => kv,
            None => continue,
        };
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "hostname" => r.hostname = value.to_string(),
            "user" => r.user = value.to_string(),
            "port" => r.port = value.parse().unwrap_or(22),
            "identityfile" => r.identity_files.push(expand_home(value, home)),
            "identitiesonly" => r.identities_only = value.eq_ignore_ascii_case("yes"),
            "identityagent" if !value.eq_ignore_ascii_case("none") => {
                r.identity_agent = Some(value.to_string());
            }
            _ => {}
        }
    }
    r
}

fn expand_home(value: &str, home: &Path) -> PathBuf {
    match value.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if value == "~" => home.to_path_buf(),
        None => PathBuf::from(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alias_resolves_to_its_hostname_and_the_identity_files_expand_home() {
        let text = "user git\nhostname github.com\nport 22\nidentitiesonly yes\nidentityfile ~/.ssh/id_ed25519_acme\nidentityagent SSH_AUTH_SOCK\nbatchmode yes\n";
        let r = parse_ssh_g(text, Path::new("/home/ada"));
        assert_eq!(r.hostname, "github.com");
        assert_eq!(r.user, "git");
        assert_eq!(r.port, 22);
        assert!(r.identities_only);
        assert_eq!(
            r.identity_files,
            [PathBuf::from("/home/ada/.ssh/id_ed25519_acme")]
        );
        assert_eq!(r.identity_agent.as_deref(), Some("SSH_AUTH_SOCK"));
    }

    #[test]
    fn an_unconfigured_host_reads_as_itself_with_the_default_identities() {
        let text = "user ada\nhostname github-nope\nport 22\nidentitiesonly no\nidentityfile ~/.ssh/id_rsa\nidentityfile ~/.ssh/id_ed25519\nidentityagent none\n";
        let r = parse_ssh_g(text, Path::new("/home/ada"));
        assert_eq!(
            r.hostname, "github-nope",
            "no alias, so the name is the host"
        );
        assert!(!r.identities_only);
        assert_eq!(r.identity_files.len(), 2);
        assert_eq!(r.identity_agent, None, "`none` is no agent");
        assert_eq!(parse_ssh_g("", Path::new("/h")).port, 22);
    }
}
