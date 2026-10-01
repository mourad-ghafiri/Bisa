//! The git config keys the platform reads and writes — one schema, three
//! surfaces: Settings → Git edits the **global** layer, a project's
//! creation and its Git tab edit the **local** layer, and every write anywhere
//! goes through [`crate::git::Git::config_set`], which validates against this
//! list. A key that is not here cannot be written by the platform at all; a
//! value the kind refuses never reaches git.
//!
//! Pure: nothing here spawns anything. The desktop renders its forms from the
//! schema the node serves, so the list below is the only place a key is named.

use crate::{VcsError, VcsResult};

/// The layer a write lands in. Reads look at both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigScope {
    Global,
    Local,
}

/// What a key's value looks like, for validation and for a form's control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKind {
    /// One line, not option-shaped.
    Text,
    /// `true` or `false`.
    Bool,
    /// One of these words.
    Choice(&'static [&'static str]),
}

/// One key the platform knows. Its words — the label, the hint, a choice's
/// word — are the message `git-config-<key>` in `locales/<lang>/settings.ftl`
/// (17 — Internationalisation), rendered by the node for a form and by the
/// docs; the schema says what a key is, not what it is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigKeyDef {
    pub key: &'static str,
    pub kind: ConfigKind,
    /// The layers this key may be written at. `init.defaultBranch` is global
    /// only: it names the branch a *new* repository starts on.
    pub scopes: &'static [ConfigScope],
}

impl ConfigKeyDef {
    /// The id of this key's message in the catalog — `git-config-<key>` with
    /// the dots as dashes; the value is the label, `.hint` the sentence
    /// under it, `.choice-<value>` the word for each value of a `Choice`.
    pub fn message_id(&self) -> String {
        format!("git-config-{}", self.key.replace('.', "-"))
    }
}

const BOTH: &[ConfigScope] = &[ConfigScope::Global, ConfigScope::Local];
const GLOBAL_ONLY: &[ConfigScope] = &[ConfigScope::Global];

/// Every key, in the order a form shows them.
pub const GIT_CONFIG_KEYS: &[ConfigKeyDef] = &[
    ConfigKeyDef {
        key: "user.name",
        kind: ConfigKind::Text,
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "user.email",
        kind: ConfigKind::Text,
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "user.useConfigOnly",
        kind: ConfigKind::Bool,
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "user.signingkey",
        kind: ConfigKind::Text,
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "commit.gpgsign",
        kind: ConfigKind::Bool,
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "pull.rebase",
        kind: ConfigKind::Bool,
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "core.autocrlf",
        kind: ConfigKind::Choice(&["true", "false", "input"]),
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "init.defaultBranch",
        kind: ConfigKind::Text,
        scopes: GLOBAL_ONLY,
    },
    ConfigKeyDef {
        key: "codehost.account",
        kind: ConfigKind::Text,
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "codehost.kind",
        kind: ConfigKind::Choice(&["github", "gitlab"]),
        scopes: BOTH,
    },
    ConfigKeyDef {
        key: "codehost.github.account",
        kind: ConfigKind::Text,
        scopes: GLOBAL_ONLY,
    },
    ConfigKeyDef {
        key: "codehost.gitlab.account",
        kind: ConfigKind::Text,
        scopes: GLOBAL_ONLY,
    },
    ConfigKeyDef {
        key: "codehost.bitbucket.account",
        kind: ConfigKind::Text,
        scopes: GLOBAL_ONLY,
    },
];

/// The key that names the code host account for a repository — the platform's
/// own section in git config, read in a checkout so a profile or a local pin
/// can set it. The default per kind is [`default_account_key`]'s.
pub const ACCOUNT_KEY: &str = "codehost.account";

/// The key that names which kind of code host a self-hosted remote is.
pub const KIND_KEY: &str = "codehost.kind";

/// The global key holding a kind's default account: `codehost.<kind>.account`
/// for `github`, `gitlab` and `bitbucket`; `None` for any other word.
pub fn default_account_key(kind: &str) -> Option<&'static str> {
    match kind {
        "github" => Some("codehost.github.account"),
        "gitlab" => Some("codehost.gitlab.account"),
        "bitbucket" => Some("codehost.bitbucket.account"),
        _ => None,
    }
}

/// Whether a key holds a code host login — `codehost.account` or a kind's default.
fn is_account_key(key: &str) -> bool {
    key == ACCOUNT_KEY || (key.starts_with("codehost.") && key.ends_with(".account"))
}

/// One login grammar for the three code hosts: letters, digits, `.`, `_` and
/// `-`, starting with a letter or digit, at most 255 — GitHub's dashes,
/// GitLab's dots and underscores, Bitbucket's underscores all fit.
pub fn validate_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 255
        && login
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// The definition of a key, when the platform knows it.
pub fn key_def(key: &str) -> Option<&'static ConfigKeyDef> {
    GIT_CONFIG_KEYS.iter().find(|d| d.key == key)
}

/// A key the platform may write at `scope`, or the refusal: unknown key, or a
/// key that does not live at that layer.
pub fn writable_key(key: &str, scope: ConfigScope) -> VcsResult<&'static ConfigKeyDef> {
    let def = key_def(key).ok_or_else(|| VcsError::InvalidArg {
        what: "git config key".to_string(),
        value: key.to_string(),
    })?;
    if !def.scopes.contains(&scope) {
        return Err(VcsError::InvalidArg {
            what: format!("git config key at the {} layer", scope_word(scope)),
            value: key.to_string(),
        });
    }
    Ok(def)
}

fn scope_word(scope: ConfigScope) -> &'static str {
    match scope {
        ConfigScope::Global => "global",
        ConfigScope::Local => "local",
    }
}

/// The value as git will store it, or the refusal. Trimmed; a text value is
/// one line and never option-shaped (so it cannot smuggle a flag), the two
/// identity keys pass the identity rule, a bool is `true`/`false`, a choice is
/// one of its words. Pure.
pub fn validate_config_value(def: &ConfigKeyDef, value: &str) -> VcsResult<String> {
    let v = value.trim();
    let bad = || VcsError::InvalidArg {
        what: def.key.to_string(),
        value: value.to_string(),
    };
    match def.kind {
        ConfigKind::Bool => match v {
            "true" | "false" => Ok(v.to_string()),
            _ => Err(bad()),
        },
        ConfigKind::Choice(options) => {
            if options.contains(&v) {
                Ok(v.to_string())
            } else {
                Err(bad())
            }
        }
        ConfigKind::Text => {
            if v.is_empty() || v.starts_with('-') || v.contains(['\r', '\n']) {
                return Err(bad());
            }
            match def.key {
                "user.name" => crate::git::validate_identity(v, "x@y.z").map_err(|_| bad())?,
                "user.email" => crate::git::validate_identity("x", v).map_err(|_| bad())?,
                // A login compares case-insensitively on every code host, so
                // it is stored one way.
                k if is_account_key(k) => {
                    if !validate_login(v) {
                        return Err(bad());
                    }
                    return Ok(v.to_ascii_lowercase());
                }
                _ => {}
            }
            Ok(v.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_names_each_key_once_with_a_message_and_a_layer() {
        let keys: Vec<_> = GIT_CONFIG_KEYS.iter().map(|d| d.key).collect();
        let mut unique = keys.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "no key twice");
        for d in GIT_CONFIG_KEYS {
            assert!(d.message_id().starts_with("git-config-"), "{}", d.key);
            assert!(
                !d.message_id().contains('.'),
                "{}: dots become dashes",
                d.key
            );
            assert!(!d.scopes.is_empty(), "{}", d.key);
        }
        assert_eq!(
            keys[..3],
            ["user.name", "user.email", "user.useConfigOnly"],
            "identity first"
        );
        assert_eq!(key_def("init.defaultBranch").unwrap().scopes, GLOBAL_ONLY);
    }

    #[test]
    fn a_key_is_writable_only_where_it_lives_and_only_when_known() {
        assert!(writable_key("user.name", ConfigScope::Local).is_ok());
        assert!(writable_key("user.name", ConfigScope::Global).is_ok());
        assert!(writable_key("init.defaultBranch", ConfigScope::Global).is_ok());
        assert!(matches!(
            writable_key("init.defaultBranch", ConfigScope::Local),
            Err(VcsError::InvalidArg { what, .. }) if what.contains("local")
        ));
        assert!(matches!(
            writable_key("core.hooksPath", ConfigScope::Local),
            Err(VcsError::InvalidArg { what, .. }) if what == "git config key"
        ));
        assert!(writable_key("", ConfigScope::Global).is_err());
    }

    #[test]
    fn values_are_validated_by_kind_and_trimmed() {
        let name = key_def("user.name").unwrap();
        assert_eq!(
            validate_config_value(name, "  Ada Lovelace ").unwrap(),
            "Ada Lovelace"
        );
        assert!(
            validate_config_value(name, "--dash-first").is_err(),
            "an option-shaped value is refused"
        );
        assert!(validate_config_value(name, "").is_err());
        assert!(validate_config_value(name, "a\nb").is_err());
        let email = key_def("user.email").unwrap();
        assert_eq!(
            validate_config_value(email, "ada@example.invalid").unwrap(),
            "ada@example.invalid"
        );
        assert!(validate_config_value(email, "no-at").is_err());
        assert!(validate_config_value(email, "a b@c.d").is_err());
        let only = key_def("user.useConfigOnly").unwrap();
        assert_eq!(validate_config_value(only, " true ").unwrap(), "true");
        assert!(validate_config_value(only, "yes").is_err());
        let crlf = key_def("core.autocrlf").unwrap();
        assert_eq!(validate_config_value(crlf, "input").unwrap(), "input");
        assert!(validate_config_value(crlf, "maybe").is_err());
        let branch = key_def("init.defaultBranch").unwrap();
        assert_eq!(validate_config_value(branch, "main").unwrap(), "main");
        assert!(validate_config_value(branch, "-x").is_err());
        let account = key_def(ACCOUNT_KEY).unwrap();
        assert_eq!(
            account.scopes, BOTH,
            "a profile, the global default, or a local pin"
        );
        assert_eq!(
            validate_config_value(account, " Ada-Acme ").unwrap(),
            "ada-acme",
            "lowercased: logins compare case-insensitively"
        );
        assert!(validate_config_value(account, "ada acme").is_err());
        assert!(validate_config_value(account, "-ada").is_err());
        assert!(validate_config_value(account, &"a".repeat(256)).is_err());
        assert_eq!(
            validate_config_value(account, "ada.lovelace_1").unwrap(),
            "ada.lovelace_1",
            "GitLab's dots and underscores fit"
        );
        let default = key_def("codehost.gitlab.account").unwrap();
        assert_eq!(default.scopes, GLOBAL_ONLY, "a kind's default is global");
        assert_eq!(
            validate_config_value(default, "Ada").unwrap(),
            "ada",
            "a login key by any name is lowercased"
        );
        assert_eq!(
            default_account_key("gitlab"),
            Some("codehost.gitlab.account")
        );
        assert_eq!(default_account_key("gitea"), None);
        let kind = key_def(KIND_KEY).unwrap();
        assert_eq!(validate_config_value(kind, "gitlab").unwrap(), "gitlab");
        assert!(
            validate_config_value(kind, "bitbucket").is_err(),
            "Bitbucket Server is not Bitbucket Cloud's API"
        );
    }
}
