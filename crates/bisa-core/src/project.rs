//! `Project`: a folder the workspace works in, git or not. A **place**.
//!
//! A project belongs to the workspace. A goal is an optional association —
//! never a container, never part of a project's identity, never part of its
//! path. There is no owning goal on this struct: not a field, not a
//! column, not a path segment.

use crate::assignee::Assignee;
use crate::id::{GoalId, PrincipalId, ProjectId};
use crate::tags::Tags;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Longest permitted slug. Directory names are cheap; ambiguity is not.
pub const MAX_SLUG_LEN: usize = 64;

/// A project's directory name — and therefore a **path-traversal boundary**.
///
/// Parse, do not validate: a `Slug` can only exist in a valid state, so a
/// function that joins a path takes a `Slug` and the compiler refuses a
/// `String` that happens to have been checked somewhere upstream. The rule is
/// an allowlist — lowercase ASCII alphanumerics plus `-`/`_`, first character
/// alphanumeric — so `.`, `/`, `\`, NUL and every non-ASCII code point are
/// outside it by construction.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    pub fn new(slug: impl Into<String>) -> Result<Self, crate::CoreError> {
        let slug = slug.into();
        validate_slug(&slug)?;
        Ok(Self(slug))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A slug the caller has already shaped to the alphabet — the placement
    /// rule's own constructions, which never leave the crate unvalidated:
    /// `debug_assert` holds them to it.
    pub(crate) fn from_validated(slug: String) -> Self {
        debug_assert!(
            validate_slug(&slug).is_ok(),
            "an unvalidated slug: {slug:?}"
        );
        Self(slug)
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Slug({:?})", self.0)
    }
}

impl FromStr for Slug {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Slug::new(s)
    }
}

impl std::ops::Deref for Slug {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<std::path::Path> for Slug {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

impl<'de> Deserialize<'de> for Slug {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Slug::new(s).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Slug {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Slug")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A directory name: 1-64 chars of lowercase a-z, 0-9, '-' or '_', starting with a letter or digit",
            "pattern": "^[a-z0-9][a-z0-9_-]{0,63}$"
        })
    }
}

/// Validate a project slug. [`Slug::new`] is the type-level form.
pub fn validate_slug(slug: &str) -> Result<(), crate::CoreError> {
    let invalid = || crate::CoreError::InvalidSlug(slug.to_string());
    if slug.is_empty() || slug.len() > MAX_SLUG_LEN {
        return Err(invalid());
    }
    let mut bytes = slug.bytes();
    let first = bytes.next().ok_or_else(invalid)?;
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(invalid());
    }
    for b in slug.bytes() {
        let ok = b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_';
        if !ok {
            return Err(invalid());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Stable across renames — the slug may change, this may not.
    pub id: ProjectId,
    /// The directory name under `projects/`. Unique workspace-wide.
    pub slug: Slug,
    pub name: String,
    pub root: ProjectRoot,
    pub vcs: Vcs,
    /// Where it was born — the workspace, a goal, or a workflow's step.
    /// History, never ownership: attachment stays the one relation, and this
    /// survives the deletion of whatever it names.
    pub origin: crate::origin::ProjectOrigin,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<Assignee>,
    #[serde(default)]
    pub publish: PublishPolicy,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    /// A person's grouping of projects in the rail — one level, free text.
    /// Display only: nothing resolves through it and the index does not hold it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// A picture for the project: an attachment by content hash, served by
    /// `GET /attachments/{sha256}?as=image`. Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<crate::attachment::AttachmentRef>,
    /// Put away: out of the rail and the pickers, refused for an attachment
    /// or an agent step's placement; the folder and every record stay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<crate::archive::Archived>,
    /// Monotonic snapshot revision, as on `Goal`.
    pub revision: u64,
    pub created_at: u64,
}

impl Project {
    pub fn is_archived(&self) -> bool {
        self.archived.is_some()
    }
}

/// The whole of the Goal ⇄ Project relationship: one row per attachment.
/// Symmetric, and neither side owns the other. Attaching and detaching move
/// no bytes (invariant I14).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub goal: GoalId,
    pub project: ProjectId,
    pub attached_at: u64,
    pub attached_by: PrincipalId,
}

/// Where the project's files live.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "type")]
pub enum ProjectRoot {
    /// `projects/<slug>/tree` — created and owned by Bisa.
    Managed,
    /// An existing folder anywhere on disk, adopted by reference. The platform
    /// never writes its own scratch into it.
    External { path: String },
}

impl ProjectRoot {
    pub fn is_external(&self) -> bool {
        matches!(self, ProjectRoot::External { .. })
    }

    /// The index column's spelling.
    pub fn kind(&self) -> &'static str {
        match self {
            ProjectRoot::Managed => "managed",
            ProjectRoot::External { .. } => "external",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "type")]
pub enum Vcs {
    /// A plain folder. Work in it isolates by copy, not by worktree.
    None,
    Git {
        default_branch: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remote: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code_host: Option<CodeHost>,
    },
}

impl Vcs {
    pub fn is_git(&self) -> bool {
        matches!(self, Vcs::Git { .. })
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Vcs::None => "none",
            Vcs::Git { .. } => "git",
        }
    }
}

/// The code host behind a repository's `origin`, recorded when the project is
/// made: which kind (`github` · `gitlab` · `bitbucket` — the code host crate's
/// words), which instance, and the repository's path on it. A record, not a
/// live fact: the Connection card reads the remote again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeHost {
    pub kind: String,
    /// The instance — `github.com`, or a host of the person's own.
    pub host: String,
    /// `owner/repo` — the whole namespace, then the name.
    pub owner_repo: String,
}

/// How a branch leaves this machine. Pushing, opening and merging a pull
/// request are outward actions. The default is `Auto` — a new project publishes
/// without a gate (the owner's chosen default); a person can require a
/// gate, or forbid publishing, per project in the repository panel.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PublishPolicy {
    /// No agent may publish; a person may.
    Manual,
    /// Allowed, through the `Publish` gate.
    Gated,
    /// Allowed without a gate. The default.
    #[default]
    Auto,
}

impl PublishPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            PublishPolicy::Manual => "manual",
            PublishPolicy::Gated => "gated",
            PublishPolicy::Auto => "auto",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_slugs() {
        for ok in ["storefront", "notes", "web-app", "api_v2", "a", "x1"] {
            Slug::new(ok).unwrap_or_else(|e| panic!("{ok:?} should be valid: {e}"));
        }
    }

    #[test]
    fn refuses_path_traversal_in_every_shape() {
        for bad in [
            "",
            ".",
            "..",
            "../",
            "a/../b",
            "/etc/passwd",
            "C:\\Windows",
            ".hidden",
            "trailing.",
            "with space",
            "nul\0byte",
            "-rf",
            "_leading",
            "UPPER",
            "a\u{ff0f}b",
            "\u{0430}pi",
            "café",
            "\u{202e}txt",
        ] {
            assert!(Slug::new(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(Slug::new("a".repeat(MAX_SLUG_LEN)).is_ok());
        assert!(Slug::new("a".repeat(MAX_SLUG_LEN + 1)).is_err());
    }

    #[test]
    fn a_slug_refuses_to_deserialize_when_invalid() {
        assert!(serde_json::from_str::<Slug>(r#""web-app""#).is_ok());
        assert!(serde_json::from_str::<Slug>(r#""../etc""#).is_err());
    }

    fn storefront(origin: crate::origin::ProjectOrigin) -> Project {
        Project {
            id: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            slug: Slug::new("storefront").unwrap(),
            name: "Storefront".into(),
            root: ProjectRoot::Managed,
            vcs: Vcs::Git {
                default_branch: "main".into(),
                remote: Some("/tmp/origin.git".into()),
                code_host: Some(CodeHost {
                    kind: "github".into(),
                    host: "github.com".into(),
                    owner_repo: "acme/storefront".into(),
                }),
            },
            origin,
            assignees: vec![Assignee::Agent("developer".into())],
            publish: PublishPolicy::Gated,
            tags: Tags::default(),
            group: None,
            photo: None,
            revision: 1,
            archived: None,
            created_at: 7,
        }
    }

    /// A project records where it was born and still owns nothing and is
    /// owned by nothing: attachment is the one relation, and `origin` is
    /// history a reader may only display.
    #[test]
    fn a_project_records_its_origin_and_no_owner() {
        use crate::origin::ProjectOrigin;
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(4, 1));
        for origin in [
            ProjectOrigin::Workspace,
            ProjectOrigin::from_goal(goal),
            ProjectOrigin::Step {
                goal: Some(goal),
                step: crate::origin::StepRef {
                    run: crate::RunId::from_ulid(ulid::Ulid::from_parts(4, 2)),
                    step: crate::StepId::new("implement").unwrap(),
                    workflow: crate::WorkflowId::from_ulid(ulid::Ulid::from_parts(4, 3)),
                },
            },
        ] {
            let p = storefront(origin);
            let json = serde_json::to_value(&p).unwrap();
            for key in json.as_object().unwrap().keys() {
                assert!(!key.contains("owner"), "a project has no owner: {key}");
            }
            assert_eq!(serde_json::from_value::<Project>(json).unwrap(), p);
        }
    }

    /// A record written before projects carried an origin — or carrying any
    /// field this shape does not — is refused, never read as something else
    /// (no backward compatibility).
    #[test]
    fn an_old_project_shape_is_refused() {
        let mut json =
            serde_json::to_value(storefront(crate::origin::ProjectOrigin::Workspace)).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("origin");
        assert!(serde_json::from_value::<Project>(json.clone()).is_err());
        json.as_object_mut()
            .unwrap()
            .insert("owner_goal".into(), serde_json::Value::Null);
        assert!(serde_json::from_value::<Project>(json).is_err());
    }

    #[test]
    fn attachment_roundtrip() {
        let a = Attachment {
            goal: GoalId::from_ulid(ulid::Ulid::from_parts(1, 1)),
            project: ProjectId::from_ulid(ulid::Ulid::from_parts(3, 1)),
            attached_at: 5,
            attached_by: PrincipalId::new("ab".repeat(32)).unwrap(),
        };
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<Attachment>(&json).unwrap(), a);
    }

    #[test]
    fn publish_defaults_to_auto_and_external_roots_roundtrip() {
        assert_eq!(PublishPolicy::default(), PublishPolicy::Auto);
        let r = ProjectRoot::External {
            path: "/srv/existing".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<ProjectRoot>(&json).unwrap(), r);
        assert!(r.is_external());
        assert_eq!(r.kind(), "external");
        assert!(!Vcs::None.is_git());
    }
}
