//! A **git profile**: who a person is for one owner on one code host — the
//! author, and optionally the SSH key and the code host account — as git
//! itself understands it (ide/04 §Profiles by organization).
//!
//! A profile is one file the platform owns under the workspace's
//! `identity/git/profiles/`, wired into the person's global git config with
//! `includeIf "hasconfig:remote.*.url:<glob>"` entries, so a checkout whose
//! `origin` belongs to the owner resolves the profile's values and every other
//! checkout does not. Git evaluates those globs on the **raw** remote URL, so
//! the platform does too — [`GitProfile::url_globs`] is the one list, and
//! [`GitProfile::matches`] is the prefix test git's `wildmatch` makes on a
//! `<prefix>**` glob. The platform and git cannot disagree about which profile
//! a remote falls under, because they read the same rule.
//!
//! Pure: this module validates and computes, and writes nothing. The engine's
//! `gitprofiles` module does the writing through `bisa-vcs`.

use crate::project::{validate_slug, Slug};
use serde::{Deserialize, Serialize};

/// The longest login or owner the platform takes — GitLab's ceiling, the
/// widest of the three code hosts; GitHub's 39 and Bitbucket's fit under it.
pub const MAX_LOGIN_LEN: usize = 255;

/// A profile as the platform keeps it: the slug names the file, the label is
/// the person's word for it, `host` and `owner` say which remotes it is for,
/// `aliases` are the SSH `Host` aliases (`github-work`) that stand for `host`
/// in a remote URL, and the rest is what the profile's file holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GitProfile {
    pub slug: Slug,
    pub label: String,
    pub host: String,
    pub owner: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub name: String,
    pub email: String,
    /// An absolute path to the private key `core.sshCommand` names; the
    /// platform never opens it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key: Option<String>,
    /// The code host login whose token answers for these repositories
    /// (`codehost.account`), lowercased.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

/// What a person asks for when they create or edit a profile — the same
/// fields, before validation; [`GitProfile::from_spec`] is the door.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileSpec {
    pub label: String,
    pub host: String,
    pub owner: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub ssh_key: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitProfileError {
    #[error("a profile needs a label")]
    EmptyLabel,
    #[error("{0:?} is not a host name: letters, digits, dots and dashes, nothing else")]
    Host(String),
    #[error("{0:?} is not an SSH host alias: letters, digits, dots, dashes and underscores")]
    Alias(String),
    #[error("{0:?} is not an owner: a user, an organization, a workspace or a group path like acme/platform — segments of letters, digits, dots, underscores and dashes joined by /, up to 255")]
    Owner(String),
    #[error("{0:?} is not a name git will take: one line, not starting with a dash")]
    Name(String),
    #[error("{0:?} is not an email: one @ with something on both sides, no spaces")]
    Email(String),
    #[error("{0:?} is not a key path the platform will name: absolute, and only letters, digits and . _ / @ + -")]
    SshKey(String),
    #[error("{0:?} is not a code host login: letters, digits, dots, underscores and dashes, starting with a letter or digit, up to 255")]
    Account(String),
}

impl GitProfile {
    /// Validate a spec into a profile under `slug`. Every field is checked the
    /// way the file and the include line will spell it, so nothing
    /// option-shaped, multi-line or glob-breaking ever reaches git.
    pub fn from_spec(slug: Slug, spec: ProfileSpec) -> Result<Self, GitProfileError> {
        let label = spec.label.trim().to_string();
        if label.is_empty() {
            return Err(GitProfileError::EmptyLabel);
        }
        let host = spec.host.trim().to_ascii_lowercase();
        if !is_host(&host) {
            return Err(GitProfileError::Host(spec.host));
        }
        let mut aliases = Vec::new();
        for alias in spec.aliases {
            let a = alias.trim().to_string();
            if !is_alias(&a) {
                return Err(GitProfileError::Alias(alias));
            }
            if a != host && !aliases.contains(&a) {
                aliases.push(a);
            }
        }
        let owner = spec.owner.trim().trim_matches('/').to_string();
        if !is_owner(&owner) {
            return Err(GitProfileError::Owner(spec.owner));
        }
        let name = spec.name.trim().to_string();
        if name.is_empty() || name.starts_with('-') || name.contains(['\r', '\n']) {
            return Err(GitProfileError::Name(spec.name));
        }
        let email = spec.email.trim().to_string();
        if !is_email(&email) {
            return Err(GitProfileError::Email(spec.email));
        }
        let ssh_key = match spec.ssh_key.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(path) if is_key_path(path) => Some(path.to_string()),
            Some(_) => return Err(GitProfileError::SshKey(spec.ssh_key.unwrap_or_default())),
        };
        let account = match spec.account.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(login) if is_login(login) => Some(login.to_ascii_lowercase()),
            Some(_) => return Err(GitProfileError::Account(spec.account.unwrap_or_default())),
        };
        Ok(Self {
            slug,
            label,
            host,
            owner,
            aliases,
            name,
            email,
            ssh_key,
            account,
        })
    }

    /// The `hasconfig:remote.*.url:` globs that select this profile — every
    /// spelling of the owner's URLs on the host and on each alias. Each is
    /// `<prefix>**`, which is what makes [`Self::matches`] a prefix test.
    pub fn url_globs(&self) -> Vec<String> {
        let mut globs = vec![
            format!("https://{}/{}/**", self.host, self.owner),
            format!("ssh://git@{}/{}/**", self.host, self.owner),
            format!("git@{}:{}/**", self.host, self.owner),
        ];
        for alias in &self.aliases {
            globs.push(format!("ssh://git@{alias}/{}/**", self.owner));
            globs.push(format!("git@{alias}:{}/**", self.owner));
        }
        globs
    }

    /// Whether a raw remote URL falls under this profile — the same answer
    /// git gives when it evaluates the includes: one of the globs, read as its
    /// prefix, starts the URL. Case follows the URL, as git's `wildmatch` does.
    pub fn matches(&self, remote_url: &str) -> bool {
        let url = remote_url.trim();
        self.url_globs().iter().any(|glob| {
            let prefix = glob.trim_end_matches("**");
            url.starts_with(prefix)
        })
    }
}

/// The slug a label earns: lowercased, runs of anything else collapsed to one
/// dash, trimmed to the slug alphabet. `None` when nothing survives.
pub fn slug_for_profile(label: &str) -> Option<Slug> {
    let mut out = String::new();
    let mut dash = false;
    for c in label.trim().chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    let out: String = out.chars().take(crate::project::MAX_SLUG_LEN).collect();
    let out = out.trim_end_matches('-').to_string();
    validate_slug(&out).ok().map(|()| Slug::from_validated(out))
}

/// One login grammar for the three code hosts — GitHub's dashes, GitLab's
/// dots and underscores, Bitbucket's underscores: letters, digits, `.`, `_`
/// and `-`, starting with a letter or digit, up to [`MAX_LOGIN_LEN`]. The
/// same rule `bisa-vcs::config_schema::validate_login` applies to the
/// account keys, so a login a profile takes is one git config takes.
pub fn is_login(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_LOGIN_LEN
        && s.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// An owner as a remote spells it: a user, an organization or a workspace —
/// one login — or a GitLab group path, logins joined by `/`, since a remote's
/// owner is every path segment but the last (`acme/platform/web` is the
/// repository `web` of `acme/platform`). Up to [`MAX_LOGIN_LEN`] in all.
pub fn is_owner(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_LOGIN_LEN && s.split('/').all(is_login)
}

fn is_host(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && !s.starts_with(['-', '.'])
        && !s.ends_with(['-', '.'])
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

fn is_alias(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_')
}

fn is_email(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && !s.contains(char::is_whitespace)
        && s.matches('@').count() == 1
        && !s.starts_with('@')
        && !s.ends_with('@')
}

/// A path `core.sshCommand` can carry without quoting: absolute, and only the
/// characters a key file is ever named with. No space, no quote, no `;`.
pub fn is_key_path(s: &str) -> bool {
    s.starts_with('/')
        && !s.contains("..")
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'@' | b'+' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ProfileSpec {
        ProfileSpec {
            label: "Acme".into(),
            host: "GitHub.com".into(),
            owner: "acme".into(),
            aliases: vec!["github-acme".into(), "github.com".into()],
            name: " Ada Lovelace ".into(),
            email: "ada@acme.example".into(),
            ssh_key: Some("/Users/ada/.ssh/id_ed25519_acme".into()),
            account: Some("Ada-Acme".into()),
        }
    }

    fn profile() -> GitProfile {
        GitProfile::from_spec(Slug::new("acme").unwrap(), spec()).unwrap()
    }

    #[test]
    fn a_spec_is_validated_trimmed_and_lowercased_where_git_is_case_blind() {
        let p = profile();
        assert_eq!(p.host, "github.com", "hosts are case-insensitive");
        assert_eq!(p.name, "Ada Lovelace", "trimmed");
        assert_eq!(
            p.account.as_deref(),
            Some("ada-acme"),
            "logins compare case-insensitively, so they are stored lowercased"
        );
        assert_eq!(
            p.aliases,
            ["github-acme"],
            "the host itself is not an alias"
        );
        assert_eq!(
            p.ssh_key.as_deref(),
            Some("/Users/ada/.ssh/id_ed25519_acme")
        );
    }

    #[test]
    fn every_field_has_a_refusal_that_names_it() {
        let slug = || Slug::new("acme").unwrap();
        let bad = |f: fn(&mut ProfileSpec)| {
            let mut s = spec();
            f(&mut s);
            GitProfile::from_spec(slug(), s).unwrap_err()
        };
        assert_eq!(bad(|s| s.label = "  ".into()), GitProfileError::EmptyLabel);
        assert!(matches!(
            bad(|s| s.host = "git hub.com".into()),
            GitProfileError::Host(_)
        ));
        assert!(matches!(
            bad(|s| s.host = "-github.com".into()),
            GitProfileError::Host(_)
        ));
        assert!(matches!(
            bad(|s| s.aliases = vec!["-x".into()]),
            GitProfileError::Alias(_)
        ));
        assert!(
            matches!(
                bad(|s| s.owner = "acme//web".into()),
                GitProfileError::Owner(_)
            ),
            "an empty segment"
        );
        assert!(matches!(
            bad(|s| s.owner = "-acme".into()),
            GitProfileError::Owner(_)
        ));
        assert!(matches!(
            bad(|s| s.owner = "acme web".into()),
            GitProfileError::Owner(_)
        ));
        assert!(matches!(
            bad(|s| s.owner = "a".repeat(256)),
            GitProfileError::Owner(_)
        ));
        assert!(matches!(
            bad(|s| s.name = "--dash".into()),
            GitProfileError::Name(_)
        ));
        assert!(matches!(
            bad(|s| s.name = "two\nlines".into()),
            GitProfileError::Name(_)
        ));
        assert!(matches!(
            bad(|s| s.email = "no-at".into()),
            GitProfileError::Email(_)
        ));
        assert!(matches!(
            bad(|s| s.email = "a b@c.d".into()),
            GitProfileError::Email(_)
        ));
        assert!(
            matches!(
                bad(|s| s.ssh_key = Some("~/.ssh/id".into())),
                GitProfileError::SshKey(_)
            ),
            "not absolute"
        );
        assert!(
            matches!(
                bad(|s| s.ssh_key = Some("/tmp/a key".into())),
                GitProfileError::SshKey(_)
            ),
            "a space would need quoting"
        );
        assert!(matches!(
            bad(|s| s.ssh_key = Some("/tmp/x;rm".into())),
            GitProfileError::SshKey(_)
        ));
        assert!(matches!(
            bad(|s| s.ssh_key = Some("/tmp/../x".into())),
            GitProfileError::SshKey(_)
        ));
        assert!(matches!(
            bad(|s| s.account = Some("bad login".into())),
            GitProfileError::Account(_)
        ));
        let mut empty = spec();
        empty.ssh_key = Some("  ".into());
        empty.account = Some(String::new());
        let p = GitProfile::from_spec(slug(), empty).unwrap();
        assert_eq!(
            (p.ssh_key, p.account),
            (None, None),
            "blank optional fields mean none"
        );
    }

    #[test]
    fn the_globs_spell_every_url_form_and_matching_is_the_same_prefix_test_git_makes() {
        let p = profile();
        assert_eq!(
            p.url_globs(),
            [
                "https://github.com/acme/**",
                "ssh://git@github.com/acme/**",
                "git@github.com:acme/**",
                "ssh://git@github-acme/acme/**",
                "git@github-acme:acme/**",
            ]
        );
        for url in [
            "https://github.com/acme/web.git",
            "ssh://git@github.com/acme/web",
            "git@github.com:acme/web.git",
            "git@github-acme:acme/web.git",
            "ssh://git@github-acme/acme/web.git",
        ] {
            assert!(p.matches(url), "{url}");
        }
        for url in [
            "https://github.com/acme-labs/web.git",
            "https://github.com/other/acme",
            "git@gitlab.com:acme/web.git",
            "https://github.com/acmeweb",
            "/tmp/acme/web.git",
        ] {
            assert!(!p.matches(url), "{url}");
        }
    }

    #[test]
    fn a_label_earns_a_slug_or_none() {
        assert_eq!(
            slug_for_profile("Acme Corp (work)").unwrap().as_str(),
            "acme-corp-work"
        );
        assert_eq!(slug_for_profile("  ACME  ").unwrap().as_str(), "acme");
        assert_eq!(slug_for_profile("--- 42 ---").unwrap().as_str(), "42");
        assert!(slug_for_profile("!!!").is_none());
        assert!(slug_for_profile("").is_none());
        let long = slug_for_profile(&"a".repeat(100)).unwrap();
        assert_eq!(long.as_str().len(), crate::project::MAX_SLUG_LEN);
    }

    #[test]
    fn the_login_grammar_takes_all_three_hosts_and_an_owner_takes_a_group_path() {
        assert!(is_login("octocat"));
        assert!(is_login("ada-acme"));
        assert!(is_login("ada_acme"), "Bitbucket and GitLab underscores");
        assert!(is_login("ada.lovelace"), "GitLab dots");
        assert!(!is_login("-ada"));
        assert!(!is_login("ada acme"));
        assert!(!is_login("ada/acme"), "a login is one segment");
        assert!(!is_login(""));
        assert!(!is_login(&"a".repeat(256)));

        assert!(is_owner("acme"));
        assert!(is_owner("acme/platform"), "a GitLab group path");
        assert!(is_owner("acme/platform/infra"));
        assert!(!is_owner("acme/"));
        assert!(!is_owner("/acme"));
        assert!(!is_owner("acme//platform"));

        // A group path's globs spell the path, so a subgroup's repository
        // matches its group's profile and a sibling group's does not.
        let mut s = spec();
        s.host = "gitlab.com".into();
        s.owner = "/acme/platform/".into();
        s.aliases = vec![];
        let p = GitProfile::from_spec(Slug::new("acme-platform").unwrap(), s).unwrap();
        assert_eq!(
            p.owner, "acme/platform",
            "the slashes at the ends are trimmed"
        );
        assert_eq!(p.url_globs()[0], "https://gitlab.com/acme/platform/**");
        assert!(p.matches("git@gitlab.com:acme/platform/web.git"));
        assert!(
            !p.matches("git@gitlab.com:acme/web.git"),
            "the group itself is not the subgroup"
        );
    }
}
