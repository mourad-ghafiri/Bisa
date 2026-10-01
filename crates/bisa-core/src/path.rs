//! `RelPath`: a path relative to a scope's root, and nothing else.
//!
//! Parse, do not validate. A `RelPath` cannot be absolute, cannot contain a
//! `..` component, cannot carry a NUL, and is normalised to forward slashes —
//! so a function that joins one onto a root cannot be handed an escape. The
//! *containment* check (canonicalise, then compare) still happens at the store
//! edge, because a symlink is a third way to ask the same question and only
//! the filesystem can answer it.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RelPath(String);

impl RelPath {
    pub fn new(raw: impl Into<String>) -> Result<Self, crate::CoreError> {
        let raw: String = raw.into();
        let invalid = || crate::CoreError::InvalidRelPath(raw.clone());
        let normalised = raw.replace('\\', "/");
        let trimmed = normalised.trim_matches('/');
        if trimmed.is_empty() || normalised.starts_with('/') || normalised.contains('\0') {
            return Err(invalid());
        }
        // A Windows drive letter is absolute too.
        if trimmed.len() >= 2
            && trimmed.as_bytes()[1] == b':'
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
        {
            return Err(invalid());
        }
        let mut parts = Vec::new();
        for part in trimmed.split('/') {
            match part {
                "" | "." => continue,
                ".." => return Err(invalid()),
                p => parts.push(p),
            }
        }
        if parts.is_empty() {
            return Err(invalid());
        }
        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last component.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// The parent, or `None` at the root.
    pub fn parent(&self) -> Option<RelPath> {
        let (dir, _) = self.0.rsplit_once('/')?;
        Some(RelPath(dir.to_string()))
    }

    /// The extension, lowercase, without the dot.
    pub fn extension(&self) -> Option<String> {
        let name = self.file_name();
        let (_, ext) = name.rsplit_once('.')?;
        if ext.is_empty() || name.starts_with('.') && !name[1..].contains('.') {
            return None;
        }
        Some(ext.to_ascii_lowercase())
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RelPath({:?})", self.0)
    }
}

impl FromStr for RelPath {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<std::path::Path> for RelPath {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for RelPath {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("RelPath")
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A path relative to a scope's root: no leading slash, no '..' component, forward slashes"
        })
    }
}

/// Which rooted thing a relative path is read under. The four roots the
/// desktop may name; every file route resolves one of them and checks
/// containment by canonicalisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileScope {
    Goal,
    /// A checkout: the project's own root (its primary workstream) or a
    /// worktree or copy under it. There is no separate project
    /// scope — a project's root *is* a workstream.
    Workstream,
    /// One unit of work, wherever it actually ran — its workstream checkout when
    /// it has one, its home's scratch when it does not. The others name
    /// a directory directly; this one names a *thing* and asks the workspace
    /// where that thing's work happens.
    WorkItem,
    /// A run of the workspace's own folder — the home a goal is for a goal's
    /// run; its scratch is where a step with no project leaves what it made.
    Run,
}

impl FileScope {
    /// Every scope, and the order a surface lists them in. The 400 for an
    /// unknown scope is built from this, so a fifth scope cannot leave the
    /// message behind.
    pub const ALL: &'static [FileScope] = &[
        FileScope::Goal,
        FileScope::Workstream,
        FileScope::WorkItem,
        FileScope::Run,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FileScope::Goal => "goal",
            FileScope::Workstream => "workstream",
            FileScope::WorkItem => "work_item",
            FileScope::Run => "run",
        }
    }
}

impl fmt::Display for FileScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FileScope {
    type Err = crate::CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        FileScope::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownFileScope(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_scope_parses_its_four_names() {
        for s in FileScope::ALL {
            assert_eq!(s.as_str().parse::<FileScope>().unwrap(), *s);
            assert_eq!(serde_json::to_value(s).unwrap(), s.as_str());
        }
        assert_eq!(FileScope::ALL.len(), 4);
        assert!("project".parse::<FileScope>().is_err());
        assert_eq!(FileScope::WorkItem.to_string(), "work_item");
        assert_eq!(FileScope::Run.to_string(), "run");
    }

    #[test]
    fn accepts_and_normalises_ordinary_paths() {
        assert_eq!(RelPath::new("src/main.rs").unwrap().as_str(), "src/main.rs");
        assert_eq!(
            RelPath::new("./src//main.rs").unwrap().as_str(),
            "src/main.rs"
        );
        assert_eq!(RelPath::new("src\\lib.rs").unwrap().as_str(), "src/lib.rs");
        assert_eq!(RelPath::new("docs/").unwrap().as_str(), "docs");
    }

    #[test]
    fn refuses_every_way_out_of_the_root() {
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a/..",
            "C:\\x",
            "nul\0",
            ".",
            "./",
            "//",
        ] {
            assert!(RelPath::new(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn components() {
        let p = RelPath::new("src/ui/Editor.tsx").unwrap();
        assert_eq!(p.file_name(), "Editor.tsx");
        assert_eq!(p.parent().unwrap().as_str(), "src/ui");
        assert_eq!(p.extension().as_deref(), Some("tsx"));
        assert_eq!(RelPath::new("README").unwrap().extension(), None);
        assert_eq!(RelPath::new(".gitignore").unwrap().extension(), None);
        assert_eq!(RelPath::new("x").unwrap().parent(), None);
    }

    #[test]
    fn refuses_to_deserialize_an_escape() {
        assert!(serde_json::from_str::<RelPath>(r#""../etc/passwd""#).is_err());
        assert!(serde_json::from_str::<RelPath>(r#""src/x.rs""#).is_ok());
    }
}
