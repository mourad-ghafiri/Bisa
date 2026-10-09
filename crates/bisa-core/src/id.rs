//! Typed identifiers.
//!
//! Two families. **ULID ids** (`GoalId`, `ProjectId`, …) are minted at the
//! edges and are sortable and collision-resistant. **Slug ids** (`AgentId`,
//! `TeamId`, `SkillId`, `McpId`, `ChannelId`) are the human-chosen name that
//! *is* the identity — and, because several of them become file names, they
//! are parsed rather than validated: an invalid one cannot be constructed.
//!
//! Principals are secp256k1 public keys carried as lowercase hex.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

macro_rules! ulid_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub ulid::Ulid);

        impl $name {
            /// Generation lives at the edges (store/engine), not in this crate,
            /// so callers pass a ULID they minted.
            pub fn from_ulid(u: ulid::Ulid) -> Self {
                Self(u)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }

        impl FromStr for $name {
            type Err = ulid::DecodeError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(ulid::Ulid::from_str(s)?))
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "description": "ULID (26-char Crockford base32)",
                    "pattern": "^[0-9A-HJKMNP-TV-Z]{26}$"
                })
            }
        }
    };
}

ulid_id!(
    /// Identifier of a [`crate::goal::Goal`].
    GoalId
);
ulid_id!(
    /// Identifier of a [`crate::workflow::Workflow`] definition.
    WorkflowId
);
ulid_id!(
    /// Identifier of a schedulable work item.
    WorkItemId
);
ulid_id!(
    /// Identifier of a harness session (owned by the orchestrator, never by
    /// the harness — the harness's own id lives in `ResumeToken`).
    SessionId
);
ulid_id!(
    /// Identifier of a [`crate::run::WorkflowRun`]: one execution of a
    /// workflow on a goal. The engine's live-session handle is a different
    /// thing with a different name (`LiveRunId`), because a run outlives every
    /// session that works on it.
    RunId
);
ulid_id!(
    /// Identifier of a workspace.
    WorkspaceId
);
ulid_id!(
    /// Identifier of a [`crate::project::Project`]. Stable across renames.
    ProjectId
);
ulid_id!(
    /// Identifier of a [`crate::workstream::Workstream`]. Local state.
    WorkstreamId
);
ulid_id!(
    /// Identifier of a [`crate::conversation::Conversation`]: a saved exchange
    /// a person starts with agents. Travels, like a channel.
    ConversationId
);
ulid_id!(
    /// Identifier of one turn of a conversation in its change ledger
    /// ([`crate::changes::ChangeLedger`]). Local state: a checkout is this
    /// machine's.
    TurnId
);
ulid_id!(
    /// Identifier of an [`crate::invite::Invite`]: one code the host made
    /// for one person. Local to the host.
    InviteId
);
ulid_id!(
    /// Identifier of a note or a review note. Local state.
    NoteId
);
ulid_id!(
    /// Identifier of a [`crate::draw::Drawing`]: a picture on a canvas.
    /// Travels, like an addon's record.
    DrawingId
);
ulid_id!(
    /// Identifier of a [`crate::connector::ConnectorAccount`]: one login to an
    /// outside platform, on this machine. Local state — a workflow step names
    /// one only through a fixed id or an input of kind `account`.
    AccountId
);

/// Is `s` a slug-shaped id: 1–64 chars of `a-z`, `0-9`, `-`, `_`, `.`, `:`,
/// starting with a letter or digit?
///
/// Wider than a project [`crate::project::Slug`] on purpose — agent ids such
/// as `acp:goose` carry a colon, and pet ids carry dots — and still an
/// allowlist, because every one of these becomes a file name.
pub fn validate_slug_id(what: &str, s: &str) -> Result<(), crate::CoreError> {
    let invalid = || crate::CoreError::InvalidId {
        what: what.to_string(),
        value: s.to_string(),
    };
    if s.is_empty() || s.len() > 64 {
        return Err(invalid());
    }
    let first = s.as_bytes()[0];
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(invalid());
    }
    if s.contains("..") {
        return Err(invalid());
    }
    for b in s.bytes() {
        let ok = b.is_ascii_lowercase()
            || b.is_ascii_digit()
            || b == b'-'
            || b == b'_'
            || b == b'.'
            || b == b':';
        if !ok {
            return Err(invalid());
        }
    }
    Ok(())
}

macro_rules! slug_id {
    ($(#[$doc:meta])* $name:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Result<Self, crate::CoreError> {
                let s = s.into();
                validate_slug_id($what, &s)?;
                Ok(Self(s))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({:?})"), self.0)
            }
        }

        impl FromStr for $name {
            type Err = crate::CoreError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::new(s)
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::new(s).map_err(serde::de::Error::custom)
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "description": concat!("A ", $what, " id: 1-64 chars of a-z, 0-9, '-', '_', '.', ':'"),
                    "pattern": "^[a-z0-9][a-z0-9_.:-]{0,63}$"
                })
            }
        }
    };
}

slug_id!(
    /// An agent definition's id. The id is the slug.
    AgentId,
    "agent"
);
slug_id!(
    /// A team's id.
    TeamId,
    "team"
);
slug_id!(
    /// A skill's id. Immutable: agents reference it.
    SkillId,
    "skill"
);
slug_id!(
    /// A registered MCP server's id.
    McpId,
    "mcp server"
);
slug_id!(
    /// A channel's id. Catalog channels use slugs; a direct message's id is a
    /// lowercase ULID-derived token, which fits the same allowlist.
    ChannelId,
    "channel"
);
slug_id!(
    /// A connector's id — the declarative definition of one outside
    /// platform's API. The id is the slug, so a workflow step names it the
    /// way a template names a catalog agent.
    ConnectorId,
    "connector"
);
slug_id!(
    /// One operation of a connector, unique within it.
    OperationId,
    "operation"
);
slug_id!(
    /// An addon's id — also its folder under `addons/` and the `d` tag of
    /// its record. A built-in's is its catalog slug; a community addon
    /// namespaces its own (`acme.weather-pro`).
    AddonId,
    "addon"
);

impl AgentId {
    /// The platform's own agent: triage, staffing and delegation. Ensured at
    /// every workspace open; never removable, never disableable.
    pub const GENERAL: &'static str = "general-agent";
    /// The platform's workflow designer: proposes, validates and repairs a
    /// goal's workflow. The second core agent, held to the same rules.
    pub const WORKFLOW: &'static str = "workflow-agent";
    /// The platform's judge: a decision point asks it a typed question and
    /// gets numbers back. The third core agent, and the one that holds no
    /// record — no key, no prompt, no conversation — so the id is reserved
    /// and no agent record may take it. Who answers for it is the
    /// `decisions.*` settings.
    pub const DECISION_MAKING: &'static str = "decision-making-agent";
    /// The core agents that hold a record — a key, a prompt, a conversation,
    /// a place in every room — in the order they are ensured and listed. The
    /// Decision-Making Agent holds none, so it is not here.
    pub const CORE: [&'static str; 2] = [Self::GENERAL, Self::WORKFLOW];

    pub fn general() -> Self {
        Self(Self::GENERAL.to_string())
    }

    pub fn workflow() -> Self {
        Self(Self::WORKFLOW.to_string())
    }

    pub fn decision_making() -> Self {
        Self(Self::DECISION_MAKING.to_string())
    }

    pub fn is_general(&self) -> bool {
        self.0 == Self::GENERAL
    }

    pub fn is_workflow(&self) -> bool {
        self.0 == Self::WORKFLOW
    }

    pub fn is_decision_making(&self) -> bool {
        self.0 == Self::DECISION_MAKING
    }

    /// Is this one of the core agents that hold a record?
    pub fn is_core_id(&self) -> bool {
        Self::CORE.contains(&self.0.as_str())
    }

    /// Is this string one of the core ids? For callers holding an
    /// [`crate::assignee::Assignee::Agent`]'s bare string.
    pub fn is_core_str(s: &str) -> bool {
        Self::CORE.contains(&s)
    }
}

impl ChannelId {
    /// The one channel a fresh workspace has. Seeded, permanent, everyone.
    pub const GENERAL: &'static str = "general";

    pub fn general() -> Self {
        Self(Self::GENERAL.to_string())
    }

    pub fn is_general(&self) -> bool {
        self.0 == Self::GENERAL
    }
}

/// A principal: a human or an agent, identified by a secp256k1 public key
/// (64 lowercase hex chars, x-only, Nostr-style).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrincipalId(String);

impl PrincipalId {
    /// Validates shape (64 lowercase hex chars). Key validity (on-curve) is
    /// checked at the crypto edge, not here.
    pub fn new(hex: impl Into<String>) -> Result<Self, crate::CoreError> {
        let hex = hex.into();
        let ok = hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
        if ok {
            Ok(Self(hex))
        } else {
            Err(crate::CoreError::InvalidPrincipal(hex))
        }
    }

    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrincipalId({}…)", &self.0[..8.min(self.0.len())])
    }
}

impl FromStr for PrincipalId {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl schemars::JsonSchema for PrincipalId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("PrincipalId")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "secp256k1 public key, 64 lowercase hex chars",
            "pattern": "^[0-9a-f]{64}$"
        })
    }
}

/// A git commit id as the version-control layer reports it: 40 (or, for a
/// SHA-256 repository, 64) lowercase hex characters. Carried as a string in
/// the domain — `bisa-vcs` owns the typed form — but shape-checked here so
/// a branch name cannot be handed to a function that wants a commit.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct CommitIdStr(pub String);

impl CommitIdStr {
    pub fn new(hex: impl Into<String>) -> Result<Self, crate::CoreError> {
        let hex = hex.into();
        let ok = (hex.len() == 40 || hex.len() == 64)
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if ok {
            Ok(Self(hex))
        } else {
            Err(crate::CoreError::InvalidId {
                what: "commit".into(),
                value: hex,
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first seven characters, as people read a commit.
    pub fn short(&self) -> &str {
        &self.0[..7.min(self.0.len())]
    }
}

impl fmt::Display for CommitIdStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for CommitIdStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CommitIdStr({})", self.short())
    }
}

impl<'de> Deserialize<'de> for CommitIdStr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for CommitIdStr {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("CommitId")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A git commit id, 40 or 64 lowercase hex chars",
            "pattern": "^([0-9a-f]{40}|[0-9a-f]{64})$"
        })
    }
}

/// A SHA-256 digest as 64 lowercase hex characters. The address of a blob,
/// the identity of a diff hunk, the hash a compare-and-swap write states.
///
/// A name that becomes a path is an allowlist: an invalid one cannot exist.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Sha256(String);

impl Sha256 {
    pub const HEX_LEN: usize = 64;

    pub fn new(hex: impl Into<String>) -> Result<Self, crate::CoreError> {
        let hex = hex.into();
        if Self::is_valid(&hex) {
            Ok(Self(hex))
        } else {
            Err(crate::CoreError::InvalidHash(hex))
        }
    }

    pub fn is_valid(hex: &str) -> bool {
        hex.len() == Self::HEX_LEN
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256({}…)", &self.0[..8.min(self.0.len())])
    }
}

impl FromStr for Sha256 {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Sha256 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Sha256")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "SHA-256 digest, 64 lowercase hex chars",
            "pattern": "^[0-9a-f]{64}$"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_rejects_bad_shapes() {
        assert!(PrincipalId::new("ab").is_err());
        assert!(PrincipalId::new("G".repeat(64)).is_err());
        assert!(PrincipalId::new("A".repeat(64)).is_err());
        assert!(PrincipalId::new("a1".repeat(32)).is_ok());
    }

    #[test]
    fn ulid_ids_roundtrip_via_string() {
        let id = GoalId::from_ulid(ulid::Ulid::from_parts(1_700_000_000_000, 42));
        assert_eq!(id.to_string().parse::<GoalId>().unwrap(), id);
    }

    #[test]
    fn slug_ids_are_an_allowlist() {
        for ok in [
            "developer",
            "general-agent",
            "acp:goose",
            "bisa-pets.moon",
            "a1",
            "x_y",
            "tic-tac-toe",
            "acme.weather-pro",
        ] {
            AgentId::new(ok).unwrap_or_else(|e| panic!("{ok:?}: {e}"));
            AddonId::new(ok).unwrap_or_else(|e| panic!("{ok:?}: {e}"));
        }
        for bad in [
            "",
            "Developer",
            "../x",
            "a/b",
            "a b",
            ".hidden",
            "-x",
            "a..b",
            "café",
        ] {
            assert!(AgentId::new(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(serde_json::from_str::<TeamId>(r#""../x""#).is_err());
        assert_eq!(
            serde_json::from_str::<TeamId>(r#""engineering""#).unwrap(),
            "engineering"
        );
    }

    #[test]
    fn the_permanent_ids_are_named_once() {
        assert!(AgentId::general().is_general());
        assert_eq!(AgentId::general().as_str(), "general-agent");
        assert!(AgentId::workflow().is_workflow());
        assert_eq!(AgentId::workflow().as_str(), "workflow-agent");
        assert!(AgentId::general().is_core_id());
        assert!(AgentId::workflow().is_core_id());
        assert!(!AgentId::new("developer").unwrap().is_core_id());
        assert!(AgentId::is_core_str("general-agent"));
        assert!(!AgentId::is_core_str("developer"));
        assert_eq!(AgentId::CORE.len(), 2);
        assert!(ChannelId::general().is_general());
        assert!(!ChannelId::new("engineering").unwrap().is_general());
    }

    #[test]
    fn a_hash_that_could_escape_a_store_is_not_a_hash() {
        assert!(Sha256::new("a".repeat(64)).is_ok());
        for bad in [
            String::new(),
            "a".repeat(63),
            "A".repeat(64),
            format!("{}/x", "a".repeat(62)),
            format!("{}g", "a".repeat(63)),
        ] {
            assert!(Sha256::new(bad.clone()).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_commit_id_is_forty_or_sixty_four_lowercase_hex_and_reads_short() {
        let sha1 = "a".repeat(40);
        let sha256 = "b".repeat(64);
        assert_eq!(CommitIdStr::new(sha1.clone()).unwrap().short(), "aaaaaaa");
        assert_eq!(CommitIdStr::new(sha256.clone()).unwrap().as_str(), sha256);
        let bad: [String; 7] = [
            "".into(),
            "abc".into(),
            "a".repeat(39),
            "a".repeat(41),
            "A".repeat(40),
            "g".repeat(40),
            "main".into(),
        ];
        for b in &bad {
            assert!(CommitIdStr::new(b.clone()).is_err(), "{b:?}");
        }
        assert_eq!(CommitIdStr::new(sha1.clone()).unwrap().to_string(), sha1);
    }

    #[test]
    fn a_principal_is_sixty_four_lowercase_hex_and_a_hash_the_same() {
        let hex = "0f".repeat(32);
        assert_eq!(PrincipalId::new(hex.clone()).unwrap().as_hex(), hex);
        assert!(
            PrincipalId::new("0F".repeat(32)).is_err(),
            "uppercase is refused"
        );
        assert!(PrincipalId::new("0f".repeat(31)).is_err());
        assert!(Sha256::is_valid(&hex));
        assert!(!Sha256::is_valid(&"0F".repeat(32)));
        assert!(!Sha256::is_valid(&hex[..63]));
        assert!(
            !Sha256::is_valid(&format!("{}/", &hex[..63])),
            "a separator is never hex"
        );
        assert_eq!(Sha256::new(hex.clone()).unwrap().as_str(), hex);
        assert_eq!(Sha256::HEX_LEN, 64);
    }

    #[test]
    fn the_core_agent_ids_are_recognised_by_value_and_by_string() {
        assert!(AgentId::general().is_general());
        assert!(AgentId::workflow().is_workflow());
        assert!(AgentId::general().is_core_id());
        assert!(AgentId::workflow().is_core_id());
        assert!(!AgentId::general().is_workflow());
        assert!(!AgentId::new("developer").unwrap().is_core_id());
        for core in AgentId::CORE {
            assert!(AgentId::is_core_str(core), "{core}");
        }
        assert!(!AgentId::is_core_str("developer"));
        assert!(!AgentId::is_core_str(""));
    }

    #[test]
    fn the_three_core_agents_have_three_distinct_ids() {
        let ids = [
            AgentId::general(),
            AgentId::workflow(),
            AgentId::decision_making(),
        ];
        assert_eq!(ids[2].as_str(), "decision-making-agent");
        assert_eq!(ids[2].as_str(), AgentId::DECISION_MAKING);
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[0], ids[2]);
        assert_ne!(ids[1], ids[2]);
        assert!(ids[2].is_decision_making());
        assert!(!ids[0].is_decision_making());
        assert!(!ids[1].is_decision_making());
        assert!(!ids[2].is_general());
        assert!(!ids[2].is_workflow());
    }

    #[test]
    fn the_decision_making_agent_holds_no_record_so_core_does_not_name_it() {
        assert!(!AgentId::decision_making().is_core_id());
        assert!(!AgentId::is_core_str(AgentId::DECISION_MAKING));
        assert!(!AgentId::CORE.contains(&AgentId::DECISION_MAKING));
        assert_eq!(AgentId::CORE, [AgentId::GENERAL, AgentId::WORKFLOW]);
    }

    // added by the coverage pass: id.rs

    #[test]
    fn a_slug_id_prints_in_debug_derefs_to_its_word_compares_with_text_and_parses() {
        let id = AgentId::new("ada").unwrap();
        assert_eq!(format!("{id:?}"), "AgentId(\"ada\")");
        assert_eq!(&*id, "ada");
        assert!(id == *"ada");
        assert_eq!("ada".parse::<AgentId>().unwrap(), id);
        assert!("Not A Slug".parse::<AgentId>().is_err());
    }

    #[test]
    fn a_principal_a_commit_and_a_hash_print_short_in_debug_and_parse_from_text() {
        let hex = "ab".repeat(32);
        let p = PrincipalId::new(hex.clone()).unwrap();
        assert_eq!(format!("{p:?}"), "PrincipalId(abababab…)");
        assert_eq!(hex.parse::<PrincipalId>().unwrap(), p);
        assert!("zz".parse::<PrincipalId>().is_err());

        let commit = "0123456789abcdef0123456789abcdef01234567";
        let c = CommitIdStr::new(commit).unwrap();
        assert_eq!(format!("{c:?}"), "CommitIdStr(0123456)");
        let read: CommitIdStr = serde_json::from_value(serde_json::json!(commit)).unwrap();
        assert_eq!(read, c);
        assert!(serde_json::from_value::<CommitIdStr>(serde_json::json!("not hex")).is_err());

        let zeros = "0".repeat(64);
        let h = Sha256::new(zeros.clone()).unwrap();
        assert_eq!(h.to_string(), zeros);
        assert_eq!(format!("{h:?}"), "Sha256(00000000…)");
        assert_eq!(zeros.parse::<Sha256>().unwrap(), h);
        assert!("0".repeat(63).parse::<Sha256>().is_err());
    }

    // added by the coverage pass: b4-id.rs
    #[test]
    fn every_hand_written_id_schema_says_it_is_a_string_and_a_slug_id_reads_as_text() {
        let value = |s: schemars::Schema| serde_json::to_value(s).unwrap();
        for schema in [
            value(schemars::schema_for!(GoalId)),
            value(schemars::schema_for!(AgentId)),
            value(schemars::schema_for!(PrincipalId)),
            value(schemars::schema_for!(CommitIdStr)),
            value(schemars::schema_for!(Sha256)),
        ] {
            assert_eq!(schema["type"], "string", "{schema}");
        }
        let agent = AgentId::new("dev").unwrap();
        let text: &str = agent.as_ref();
        assert_eq!(text, "dev");
        assert_eq!(
            format!("{:?}", GoalId::from_ulid(ulid::Ulid::from_parts(1, 1))),
            format!(
                "GoalId({})",
                GoalId::from_ulid(ulid::Ulid::from_parts(1, 1))
            )
        );
    }
}
