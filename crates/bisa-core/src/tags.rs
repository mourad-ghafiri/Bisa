//! `Tags`: the one grouping mechanism, shared by every entity worth filing.
//!
//! Agents, teams, channels, skills, MCP servers, projects, goals, workflows
//! and connectors all needed categories, and a category system per entity
//! would be nine vocabularies that drift. One newtype means one normalization rule, one
//! index table, and one `?tag=` filter that answers the same way everywhere.
//!
//! **Construction is the validation point.** [`Tags::new`] normalizes and
//! *rejects* what it cannot make into a tag, so every write path — the CLI,
//! the HTTP API, the bundled TOML — refuses a bad tag at the door with a
//! message naming it.
//!
//! **Deserialization normalizes and drops instead.** Truth files must always
//! load: losing a whole agent because someone hand-edited `tags: ["#ops"]`
//! into its JSON would be a far worse failure than losing that one tag. Since
//! every write path goes through [`Tags::new`], a dropped tag can only come
//! from a hand edit, and the next write puts the file back in shape.
//!
//! Normalization is deliberately the boring slug rule everyone already knows —
//! trim, lowercase, spaces and underscores become dashes, runs of dashes
//! collapse. It is idempotent: a normalized tag normalizes to itself, which is
//! what makes a tag a filter you can trust rather than a string you hope
//! matches.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Most tags one entity may carry. A filing system that needs more than this
/// on one object is a filing system nobody reads.
pub const MAX_TAGS: usize = 16;

/// Longest a single tag may be, in bytes (== chars: the charset is ASCII).
pub const MAX_TAG_LEN: usize = 32;

/// The vocabulary the built-in library uses. Free-form tags stay legal —
/// this is the shared spine, not a whitelist — but the bundled agents, teams,
/// channels and skills draw only from here, so browsing by category lands on a
/// coherent set rather than twenty near-synonyms.
pub const VOCABULARY: &[&str] = &[
    "business",
    "code",
    "content",
    "data",
    "delivery",
    "design",
    "discovery",
    "engineering",
    "legal",
    "management",
    "marketing",
    "mobile",
    "ops",
    "planning",
    "product",
    "quality",
    "research",
    "review",
    "sales",
    "security",
    "thinking",
    "web",
    "writing",
];

/// What kind of object a tag files.
///
/// A typed enum rather than a bare string: an entity name is written in the
/// store, in the index, in `?entity=` and in the desktop, and a typo in any of
/// them would silently return an empty filter instead of an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TagEntity {
    Agent,
    Team,
    Channel,
    Skill,
    Mcp,
    Project,
    Goal,
    Workflow,
    Connector,
}

impl TagEntity {
    pub const ALL: &'static [TagEntity] = &[
        TagEntity::Agent,
        TagEntity::Team,
        TagEntity::Channel,
        TagEntity::Skill,
        TagEntity::Mcp,
        TagEntity::Project,
        TagEntity::Goal,
        TagEntity::Workflow,
        TagEntity::Connector,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TagEntity::Agent => "agent",
            TagEntity::Team => "team",
            TagEntity::Channel => "channel",
            TagEntity::Skill => "skill",
            TagEntity::Mcp => "mcp",
            TagEntity::Project => "project",
            TagEntity::Goal => "goal",
            TagEntity::Workflow => "workflow",
            TagEntity::Connector => "connector",
        }
    }
}

impl fmt::Display for TagEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TagEntity {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TagEntity::ALL
            .iter()
            .copied()
            .find(|e| e.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownTagEntity(s.to_string()))
    }
}

/// Whether a multi-tag filter means "carries any of these" or "carries all".
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TagMatch {
    /// The default: a wider filter is the one people expect from a tag list.
    #[default]
    Any,
    All,
}

/// A normalized, sorted, deduplicated category set.
///
/// Sorted because two entities tagged the same way must serialize the same
/// bytes — otherwise a snapshot's revision churns on a reordering that changed
/// nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Tags(Vec<String>);

impl Tags {
    /// Normalize, validate, sort and dedupe. Rejects a tag that cannot be
    /// slugified (anything outside `a-z 0-9 - _` and whitespace) and a set
    /// larger than [`MAX_TAGS`].
    pub fn new<I, S>(tags: I) -> Result<Self, crate::CoreError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Vec::new();
        for raw in tags {
            let raw = raw.as_ref();
            let tag =
                normalize(raw).ok_or_else(|| crate::CoreError::InvalidTag(raw.to_string()))?;
            out.push(tag);
        }
        out.sort();
        out.dedup();
        if out.len() > MAX_TAGS {
            return Err(crate::CoreError::TooManyTags(out.len()));
        }
        Ok(Self(out))
    }

    /// Normalize, keeping what survives and dropping what does not. The
    /// deserialization path and nothing else — a caller that wants to know a
    /// tag was bad calls [`Tags::new`].
    pub fn sanitize<I, S>(tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out: Vec<String> = tags
            .into_iter()
            .filter_map(|t| normalize(t.as_ref()))
            .collect();
        out.sort();
        out.dedup();
        out.truncate(MAX_TAGS);
        Self(out)
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.0.iter()
    }

    pub fn contains(&self, tag: &str) -> bool {
        self.0.iter().any(|t| t == tag)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// The slug rule, in one place. `None` when nothing usable is left.
fn normalize(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        match ch {
            'a'..='z' | '0'..='9' => out.push(ch),
            'A'..='Z' => out.push(ch.to_ascii_lowercase()),
            // The separators people actually type, all meaning the same thing.
            ' ' | '\t' | '_' | '-' | '/' | '.' => {
                if !out.ends_with('-') {
                    out.push('-');
                }
            }
            // Anything else is not a slug and never will be.
            _ => return None,
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() || trimmed.len() > MAX_TAG_LEN {
        return None;
    }
    Some(trimmed.to_string())
}

impl fmt::Display for Tags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join(", "))
    }
}

impl<'de> Deserialize<'de> for Tags {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Tags::sanitize(Vec::<String>::deserialize(d)?))
    }
}

impl<'a> IntoIterator for &'a Tags {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl schemars::JsonSchema for Tags {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Tags")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "array",
            "description": "Normalized categories: lowercase a-z, 0-9 and '-', \
                            sorted and deduplicated",
            "items": {"type": "string", "pattern": "^[a-z0-9]([a-z0-9-]*[a-z0-9])?$"},
            "maxItems": MAX_TAGS
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slug_rule_is_the_one_people_expect() {
        let t = Tags::new(["Go To Market", "  DevOps  ", "ui_ux", "a/b.c"]).unwrap();
        assert_eq!(
            t.as_slice(),
            ["a-b-c", "devops", "go-to-market", "ui-ux"],
            "sorted, lowercased, one separator"
        );
    }

    /// The property the index depends on: filtering by a stored tag finds the
    /// row that stored it. A normalization that is not idempotent breaks that
    /// the moment a value round-trips through a form.
    #[test]
    fn normalization_is_idempotent() {
        for raw in ["Go To Market", "ops", "a--b", "-x-", "UI/UX"] {
            let once = normalize(raw).unwrap();
            assert_eq!(normalize(&once).as_deref(), Some(once.as_str()), "{raw:?}");
        }
    }

    #[test]
    fn every_vocabulary_tag_normalizes_to_itself() {
        for tag in VOCABULARY {
            assert_eq!(normalize(tag).as_deref(), Some(*tag), "{tag:?}");
        }
        let mut sorted = VOCABULARY.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, VOCABULARY, "keep the vocabulary sorted to read it");
    }

    #[test]
    fn duplicates_collapse_however_they_were_spelled() {
        let t = Tags::new(["Ops", "ops", "  OPS  "]).unwrap();
        assert_eq!(t.as_slice(), ["ops"]);
    }

    #[test]
    fn new_rejects_what_it_cannot_slugify() {
        assert!(Tags::new(["#ops"]).is_err());
        assert!(Tags::new(["café"]).is_err());
        assert!(Tags::new(["   "]).is_err());
        assert!(Tags::new(["x".repeat(MAX_TAG_LEN + 1)]).is_err());
        assert!(matches!(
            Tags::new((0..=MAX_TAGS).map(|i| format!("t{i}"))),
            Err(crate::CoreError::TooManyTags(_))
        ));
    }

    /// A hand-edited truth file must still load. Losing one bad tag beats
    /// losing the agent that carried it.
    #[test]
    fn deserialization_drops_instead_of_failing() {
        let t: Tags = serde_json::from_str(r##"["ops", "#bad", "Engineering"]"##).unwrap();
        assert_eq!(t.as_slice(), ["engineering", "ops"]);
    }

    #[test]
    fn round_trips_through_json() {
        let t = Tags::new(["engineering", "code"]).unwrap();
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#"["code","engineering"]"#);
        assert_eq!(serde_json::from_str::<Tags>(&json).unwrap(), t);
    }
}
