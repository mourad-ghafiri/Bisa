//! The `git.*` settings the engine reads as typed values — today the one
//! that decides who commits in a repository the platform makes.
//!
//! The identity itself is never a setting: it is the person's global git
//! config, or the repository's own (ide/13). The *policy* is — `git.committer`,
//! workspace scope — because whether a new repository inherits that config,
//! pins it, or asks every time is a decision a workspace makes once, not a
//! fact about a person. The words are spelled here beside the registry that
//! declares the key, and a test holds them to the `def!`.

use crate::settings::Resolved;

/// The `git.*` keys the engine reads, spelled once.
pub mod keys {
    pub const COMMITTER: &str = "git.committer";
}

/// What a repository the platform makes does about who commits, when the
/// request names no config of its own and the repository has no identity
/// of its own.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CommitterPolicy {
    /// Commit as the global git config; ask only when none resolves.
    #[default]
    Inherit,
    /// Write the global pair into the repository's own config at creation,
    /// so it keeps its author even if the global one changes; ask when none
    /// resolves.
    Pin,
    /// Ask who commits in every new repository, whatever resolves.
    Ask,
}

impl CommitterPolicy {
    /// The choice words, in the registry's order.
    pub const WORDS: [&str; 3] = ["inherit", "pin", "ask"];

    pub fn as_str(self) -> &'static str {
        match self {
            CommitterPolicy::Inherit => "inherit",
            CommitterPolicy::Pin => "pin",
            CommitterPolicy::Ask => "ask",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "inherit" => Some(CommitterPolicy::Inherit),
            "pin" => Some(CommitterPolicy::Pin),
            "ask" => Some(CommitterPolicy::Ask),
            _ => None,
        }
    }

    /// The policy one resolved value names; anything else is the default.
    pub fn from_resolved(resolved: &Resolved) -> Self {
        resolved
            .value
            .as_str()
            .and_then(Self::parse)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Origin, SettingDef};
    use serde_json::json;

    fn resolved(value: serde_json::Value) -> Resolved {
        Resolved {
            key: keys::COMMITTER.to_string(),
            value,
            origin: Origin::Workspace,
        }
    }

    /// The words and the default are the registry's — the one guard against
    /// the `def!` and this type drifting apart.
    #[test]
    fn the_words_and_the_default_match_the_registry() {
        let def = SettingDef::lookup(keys::COMMITTER).expect("git.committer is registered");
        let words = match &def.kind {
            crate::settings::Kind::Choice(words) => words.to_vec(),
            _ => vec![],
        };
        assert_eq!(words, CommitterPolicy::WORDS.to_vec());
        assert_eq!(
            def.default.as_str().and_then(CommitterPolicy::parse),
            Some(CommitterPolicy::default())
        );
        for w in CommitterPolicy::WORDS {
            assert_eq!(CommitterPolicy::parse(w).unwrap().as_str(), w);
        }
    }

    #[test]
    fn a_resolved_value_names_the_policy_and_anything_else_is_the_default() {
        assert_eq!(
            CommitterPolicy::from_resolved(&resolved(json!("pin"))),
            CommitterPolicy::Pin
        );
        assert_eq!(
            CommitterPolicy::from_resolved(&resolved(json!("ask"))),
            CommitterPolicy::Ask
        );
        assert_eq!(
            CommitterPolicy::from_resolved(&resolved(json!("inherit"))),
            CommitterPolicy::Inherit
        );
        assert_eq!(
            CommitterPolicy::from_resolved(&resolved(json!("whatever"))),
            CommitterPolicy::Inherit
        );
        assert_eq!(
            CommitterPolicy::from_resolved(&resolved(json!(3))),
            CommitterPolicy::Inherit
        );
    }
}
