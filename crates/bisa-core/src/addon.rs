//! `Addon`: a community-built overlay widget — a folder of HTML, CSS and
//! JavaScript with an `addon.json` beside it — as the platform knows it
//! ([18 — Addons](../../../docs/architecture/18-addons.md)).
//!
//! The **manifest** is the developer's word: what the addon is, how its
//! window may be shaped and moved, and which capabilities it declares it
//! wants. The **record** is the workspace's word: the manifest as installed,
//! where it came from, whether it is enabled, and what the person granted —
//! always a subset of what was declared. The record is GEP kind 33407
//! ([`crate::kind::KIND_ADDON`]); the bundle's bytes never travel.
//!
//! Everything here is pure: the shapes, their validation as a list of
//! problems the way a connector's is ([`crate::connector::ConnectorProblem`]),
//! and the rules a bundle's file listing must keep. The store copies files;
//! the node serves them; the desktop runs them behind its walls. None of
//! those decisions is made here, but every rule they enforce is spelled here
//! once.

use crate::id::AddonId;
use crate::origin::Origin;
use crate::tags::{Tags, MAX_TAGS, MAX_TAG_LEN};
use crate::text::Text;
use serde::{Deserialize, Serialize};

/// The manifest's file name at the root of an addon folder.
pub const MANIFEST_FILE_NAME: &str = "addon.json";
/// The page a window shows when the manifest names no `entry`.
pub const DEFAULT_ENTRY: &str = "index.html";
/// The library's file name: served by the node at every addon's root, so a
/// bundle may not ship a file of that name.
pub const SDK_FILE_NAME: &str = "bisa-addon.js";
/// Most files a bundle may hold.
pub const MAX_ADDON_FILES: usize = 256;
/// Most bytes a bundle may hold, all files together.
pub const MAX_ADDON_BYTES: u64 = 8 * 1024 * 1024;
/// Most bytes one file of a bundle may hold.
pub const MAX_ADDON_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Most bytes a manifest may hold.
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// Deepest a bundle's folders may nest.
pub const MAX_BUNDLE_DEPTH: usize = 8;
/// The smallest and largest edge a window may have or ask for, in CSS px —
/// small enough for a slim read-out, large enough for a page.
pub const MIN_WINDOW_PX: u32 = 40;
pub const MAX_WINDOW_PX: u32 = 1600;
/// Longest a name may be, in characters.
pub const MAX_ADDON_NAME_CHARS: usize = 60;
/// Longest a description may be, in characters.
pub const MAX_ADDON_DESCRIPTION_CHARS: usize = 400;
/// Longest any other free-text field may be, in characters.
pub const MAX_ADDON_FIELD_CHARS: usize = 200;

/// The files a bundle may hold, by extension, and the type each is served
/// as. A file of any other extension is refused at install; one that
/// appears later is served as a download, never run.
pub const SERVED_EXTENSIONS: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("json", "application/json"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("ico", "image/x-icon"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("ttf", "font/ttf"),
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("wav", "audio/wav"),
    ("mp4", "video/mp4"),
    ("webm", "video/webm"),
    ("txt", "text/plain; charset=utf-8"),
    ("md", "text/markdown; charset=utf-8"),
];

/// The type a bundle file is served as, by its extension; `None` for a file
/// a bundle may not hold.
pub fn served_content_type(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    SERVED_EXTENSIONS
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, ty)| *ty)
}

/// Whether `rel` is a path a bundle may carry: `/`-joined plain components,
/// none empty, none `.` or `..`, none starting with a dot, no control
/// characters, no backslash, at most 255 bytes a component and 1024 in all.
pub fn is_bundle_path(rel: &str) -> bool {
    if rel.is_empty() || rel.len() > 1024 || rel.starts_with('/') || rel.ends_with('/') {
        return false;
    }
    if rel
        .bytes()
        .any(|b| b == 0 || b == b'\\' || b.is_ascii_control())
    {
        return false;
    }
    rel.split('/').all(|component| {
        !component.is_empty()
            && component.len() <= 255
            && !component.starts_with('.')
            && component != "."
            && component != ".."
    })
}

/// `MAJOR.MINOR.PATCH`, each a decimal number, optionally `-pre` and
/// `+build` made of `[0-9A-Za-z.-]` — the shape of a version, without the
/// ordering rules nothing here needs.
pub fn is_semver(version: &str) -> bool {
    let (core, build) = match version.split_once('+') {
        Some((c, b)) => (c, Some(b)),
        None => (version, None),
    };
    let (core, pre) = match core.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    if numbers.len() != 3 {
        return false;
    }
    let number = |n: &str| {
        !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && (n == "0" || !n.starts_with('0'))
    };
    if !numbers.iter().all(|n| number(n)) {
        return false;
    }
    let ident = |s: &str| {
        !s.is_empty()
            && s.split('.').all(|part| {
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
    };
    pre.is_none_or(ident) && build.is_none_or(ident)
}

fn default_entry() -> String {
    DEFAULT_ENTRY.to_string()
}

fn yes() -> bool {
    true
}

/// How a window's chrome is drawn: a slim bar with the name, the grip and
/// the close button, or nothing but a grip that appears on hover.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AddonFrame {
    #[default]
    Bar,
    None,
}

/// The corner a window starts in before anyone drags it.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AddonDock {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

/// How the developer shaped the window: its size, its bounds, and which
/// of the person's moves it allows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddonWindow {
    /// The size it opens at, in CSS px.
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u32>,
    /// Whether the person may drag a corner (and the addon ask to resize).
    #[serde(default = "yes")]
    pub resizable: bool,
    /// Whether the window has a close button (and the addon may ask to close).
    #[serde(default = "yes")]
    pub closable: bool,
    /// Whether the host paints nothing behind the page, so the page's own
    /// transparency shows the app through it.
    #[serde(default)]
    pub transparent: bool,
    #[serde(default)]
    pub frame: AddonFrame,
    #[serde(default)]
    pub default_dock: AddonDock,
}

impl Default for AddonWindow {
    fn default() -> Self {
        Self {
            width: 240,
            height: 160,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            resizable: true,
            closable: true,
            transparent: false,
            frame: AddonFrame::Bar,
            default_dock: AddonDock::BottomRight,
        }
    }
}

/// One capability an addon may declare and a person may grant. A closed
/// list: the bridge answers a method only when the record grants the
/// permission the registry names for it, and a word not here is refused
/// at install.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum AddonPermission {
    /// The platform's version and the locale.
    PlatformInfo,
    /// The theme's scheme, and a word when it changes.
    Theme,
    /// The machine's load — CPU, GPU, memory, disk — at the footer's cadence.
    SystemLoad,
    /// Counts of what needs the person, what is under review, what is running.
    WorkspaceSummary,
    /// A notice through the desktop, rate-limited.
    Notify,
    /// Writing text to the clipboard.
    ClipboardWrite,
    /// A small key-value store of the addon's own, kept by the host.
    Storage,
    /// Fetching from the internet — only these hosts, only through the node.
    Network { hosts: Vec<String> },
    /// Asking the person to open a link in their browser.
    OpenUrl,
    /// Taking the person to one of the app's screens.
    Navigate,
}

impl AddonPermission {
    /// The permission's word on the wire and in a grant.
    pub fn word(&self) -> &'static str {
        match self {
            AddonPermission::PlatformInfo => "platform_info",
            AddonPermission::Theme => "theme",
            AddonPermission::SystemLoad => "system_load",
            AddonPermission::WorkspaceSummary => "workspace_summary",
            AddonPermission::Notify => "notify",
            AddonPermission::ClipboardWrite => "clipboard_write",
            AddonPermission::Storage => "storage",
            AddonPermission::Network { .. } => "network",
            AddonPermission::OpenUrl => "open_url",
            AddonPermission::Navigate => "navigate",
        }
    }

    /// Every word, in the order a review lists them.
    pub const WORDS: [&'static str; 10] = [
        "platform_info",
        "theme",
        "system_load",
        "workspace_summary",
        "notify",
        "clipboard_write",
        "storage",
        "network",
        "open_url",
        "navigate",
    ];

    /// The declared permission a word names — a grant copies the
    /// declaration, so a `network` grant carries the declared hosts and
    /// nothing a caller could widen.
    pub fn grant_by_word<'a>(word: &str, declared: &'a [AddonPermission]) -> Option<&'a Self> {
        declared.iter().find(|p| p.word() == word)
    }

    /// Whether every grant is one of the declarations, exactly.
    pub fn is_subset(granted: &[AddonPermission], declared: &[AddonPermission]) -> bool {
        granted.iter().all(|g| declared.contains(g))
    }
}

/// The developer's word: the folder's `addon.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddonManifest {
    /// Also the folder's name under `addons/` and the `d` tag of the record.
    /// A community addon should namespace it (`acme.weather-pro`); a
    /// built-in's is its catalog slug.
    pub id: AddonId,
    pub name: String,
    pub description: String,
    /// `MAJOR.MINOR.PATCH[-pre][+build]`.
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// `https://` only; shown, never fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// `https://` only; shown, never fetched or cloned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// An SPDX identifier or the licence's name — the developer's word.
    pub license: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub window: AddonWindow,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<AddonPermission>,
    /// The page the window shows, relative to the folder.
    #[serde(default = "default_entry")]
    pub entry: String,
}

/// One thing wrong with a manifest or a record, and the field it is about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddonProblem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The rule broken, as a message (`problem-addon-…` in `locales/en/problems.ftl`).
    pub text: Text,
}

impl AddonProblem {
    pub fn at(field: impl Into<String>, text: Text) -> Self {
        Self {
            field: Some(field.into()),
            text,
        }
    }
}

impl AddonManifest {
    /// Every rule a manifest breaks, none stopping the others: a developer
    /// fixes the whole list at once.
    pub fn validate(&self) -> Vec<AddonProblem> {
        let mut out = Vec::new();
        let mut push = |field: &str, text: Text| out.push(AddonProblem::at(field, text));

        if self.name.trim().is_empty() {
            push("name", crate::text!("problem-addon-needs-name"));
        } else if self.name.chars().count() > MAX_ADDON_NAME_CHARS {
            push(
                "name",
                crate::text!(
                    "problem-addon-name-too-long",
                    max = MAX_ADDON_NAME_CHARS.to_string()
                ),
            );
        }
        if self.description.trim().is_empty() {
            push(
                "description",
                crate::text!("problem-addon-needs-description"),
            );
        } else if self.description.chars().count() > MAX_ADDON_DESCRIPTION_CHARS {
            push(
                "description",
                crate::text!(
                    "problem-addon-description-too-long",
                    max = MAX_ADDON_DESCRIPTION_CHARS.to_string()
                ),
            );
        }
        if !is_semver(&self.version) {
            push(
                "version",
                crate::text!(
                    "problem-addon-version-not-semver",
                    version = format!("{:?}", self.version)
                ),
            );
        }
        if self.license.trim().is_empty() {
            push("license", crate::text!("problem-addon-license-needed"));
        }
        for (field, value) in [
            ("author", self.author.as_deref()),
            ("license", Some(self.license.as_str())),
            ("homepage", self.homepage.as_deref()),
            ("repo", self.repo.as_deref()),
        ] {
            if value.is_some_and(|v| v.chars().count() > MAX_ADDON_FIELD_CHARS) {
                push(
                    field,
                    crate::text!(
                        "problem-addon-field-too-long",
                        field = field.to_string(),
                        max = MAX_ADDON_FIELD_CHARS.to_string()
                    ),
                );
            }
        }
        for (field, url) in [("homepage", &self.homepage), ("repo", &self.repo)] {
            if let Some(url) = url {
                let https = crate::connector::url_head(url).is_some_and(|h| h.scheme == "https");
                if !https {
                    push(
                        field,
                        crate::text!(
                            "problem-addon-url-not-https",
                            field = field.to_string(),
                            url = format!("{url:?}")
                        ),
                    );
                }
            }
        }

        if self.tags.len() > MAX_TAGS {
            push(
                "tags",
                crate::text!(
                    "problem-addon-too-many-tags",
                    found = self.tags.len().to_string(),
                    max = MAX_TAGS.to_string()
                ),
            );
        }
        let mut seen_tags: Vec<&str> = Vec::new();
        for tag in &self.tags {
            let keeps_shape = tag.len() <= MAX_TAG_LEN
                && Tags::new([tag.as_str()])
                    .is_ok_and(|t| t.as_slice() == std::slice::from_ref(tag));
            if !keeps_shape {
                push(
                    "tags",
                    crate::text!("problem-addon-tag-shape", tag = format!("{tag:?}")),
                );
            }
            if seen_tags.contains(&tag.as_str()) {
                push(
                    "tags",
                    crate::text!("problem-addon-tag-declared-twice", tag = format!("{tag:?}")),
                );
            }
            seen_tags.push(tag);
        }

        let w = &self.window;
        for (field, value) in [
            ("window.width", Some(w.width)),
            ("window.height", Some(w.height)),
            ("window.min_width", w.min_width),
            ("window.min_height", w.min_height),
            ("window.max_width", w.max_width),
            ("window.max_height", w.max_height),
        ] {
            if let Some(v) = value {
                if !(MIN_WINDOW_PX..=MAX_WINDOW_PX).contains(&v) {
                    push(
                        field,
                        crate::text!(
                            "problem-addon-window-out-of-bounds",
                            field = field.to_string(),
                            value = v.to_string(),
                            min = MIN_WINDOW_PX.to_string(),
                            max = MAX_WINDOW_PX.to_string()
                        ),
                    );
                }
            }
        }
        for (axis, size, min, max) in [
            ("width", w.width, w.min_width, w.max_width),
            ("height", w.height, w.min_height, w.max_height),
        ] {
            if let (Some(lo), Some(hi)) = (min, max) {
                if lo > hi {
                    push(
                        &format!("window.min_{axis}"),
                        crate::text!(
                            "problem-addon-window-min-above-max",
                            axis = axis.to_string()
                        ),
                    );
                }
            }
            if min.is_some_and(|lo| size < lo) || max.is_some_and(|hi| size > hi) {
                push(
                    &format!("window.{axis}"),
                    crate::text!(
                        "problem-addon-window-size-outside-min-max",
                        axis = axis.to_string()
                    ),
                );
            }
        }

        if !is_bundle_path(&self.entry) {
            push(
                "entry",
                crate::text!(
                    "problem-addon-entry-not-bundle-path",
                    entry = format!("{:?}", self.entry)
                ),
            );
        } else if served_content_type(&self.entry) != served_content_type(DEFAULT_ENTRY) {
            push(
                "entry",
                crate::text!(
                    "problem-addon-entry-not-html",
                    entry = format!("{:?}", self.entry)
                ),
            );
        }

        let mut seen_words: Vec<&str> = Vec::new();
        for (i, permission) in self.permissions.iter().enumerate() {
            let field = format!("permissions.{i}");
            let word = permission.word();
            if seen_words.contains(&word) {
                push(
                    &field,
                    crate::text!(
                        "problem-addon-permission-declared-twice",
                        word = word.to_string()
                    ),
                );
            }
            seen_words.push(word);
            if let AddonPermission::Network { hosts } = permission {
                if hosts.is_empty() {
                    push(&field, crate::text!("problem-addon-network-needs-host"));
                }
                for host in hosts {
                    if !crate::connector::is_host_pattern(host) {
                        push(
                            &field,
                            crate::text!(
                                "problem-addon-network-host-not-pattern",
                                host = format!("{host:?}")
                            ),
                        );
                    } else if crate::connector::is_loopback(host.trim_start_matches("*.")) {
                        push(
                            &field,
                            crate::text!(
                                "problem-addon-network-loopback-refused",
                                host = format!("{host:?}")
                            ),
                        );
                    }
                }
            }
        }
        out
    }
}

/// The workspace's word: the manifest as installed, where it came from,
/// whether it runs, and what the person granted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddonRecord {
    pub manifest: AddonManifest,
    /// Recorded at install, never accepted from a caller.
    #[serde(default)]
    pub origin: Origin,
    pub enabled: bool,
    /// A subset of `manifest.permissions`, exactly as declared.
    #[serde(default)]
    pub granted: Vec<AddonPermission>,
    pub installed_at: u64,
}

impl AddonRecord {
    /// The manifest's problems, and a grant that was never declared.
    pub fn validate(&self) -> Vec<AddonProblem> {
        let mut out = self.manifest.validate();
        for grant in &self.granted {
            if !self.manifest.permissions.contains(grant) {
                out.push(AddonProblem::at(
                    "granted",
                    crate::text!(
                        "problem-addon-grant-not-declared",
                        word = grant.word().to_string()
                    ),
                ));
            }
        }
        out
    }

    /// The grant under `word`, if the person gave it.
    pub fn grant(&self, word: &str) -> Option<&AddonPermission> {
        AddonPermission::grant_by_word(word, &self.granted)
    }

    /// The hosts a `network` grant names, if the person gave it.
    pub fn network_hosts(&self) -> Option<&[String]> {
        match self.grant("network") {
            Some(AddonPermission::Network { hosts }) => Some(hosts),
            _ => None,
        }
    }
}

/// What a bundle can get wrong, and what the platform refuses an addon for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddonError {
    #[error("the bundle has no {0}")]
    EntryMissing(String),
    #[error("{0} is a name the platform serves itself; the bundle may not carry one")]
    ReservedFile(String),
    #[error("{0} is not a file a bundle may carry")]
    NotServedFile(String),
    #[error("{0} is not a path a bundle may carry")]
    BadBundlePath(String),
    #[error("the bundle holds {found} files; the most is {max}", max = MAX_ADDON_FILES)]
    TooManyFiles { found: usize },
    #[error("{name} is {bytes} bytes; the most one file may be is {max}", max = MAX_ADDON_FILE_BYTES)]
    FileTooLarge { name: String, bytes: u64 },
    #[error("the bundle is {bytes} bytes; the most is {max}", max = MAX_ADDON_BYTES)]
    TooLarge { bytes: u64 },
    #[error("{0} was never declared by the addon, so it cannot be granted")]
    GrantNotDeclared(String),
    #[error("addon {0} is not enabled")]
    NotEnabled(String),
    #[error("addon {0} has no files on this machine")]
    FilesAbsent(String),
    #[error("addon {id} was not granted {word}")]
    NotGranted { id: String, word: String },
}

/// The rules a bundle's listing keeps, judged before a byte is copied: the
/// entry is there, nothing wears the library's name, the manifest is not
/// among the files, every path and every extension is one a bundle may
/// carry, and the caps hold. `files` is `(path, bytes)` with `/`-joined
/// paths relative to the folder.
pub fn check_bundle_listing(files: &[(String, u64)], entry: &str) -> Result<(), AddonError> {
    if files.len() > MAX_ADDON_FILES {
        return Err(AddonError::TooManyFiles { found: files.len() });
    }
    let mut total: u64 = 0;
    for (path, bytes) in files {
        if !is_bundle_path(path) {
            return Err(AddonError::BadBundlePath(path.clone()));
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        if name == SDK_FILE_NAME {
            return Err(AddonError::ReservedFile(path.clone()));
        }
        if name == MANIFEST_FILE_NAME {
            return Err(AddonError::ReservedFile(path.clone()));
        }
        if served_content_type(name).is_none() {
            return Err(AddonError::NotServedFile(path.clone()));
        }
        if *bytes > MAX_ADDON_FILE_BYTES {
            return Err(AddonError::FileTooLarge {
                name: path.clone(),
                bytes: *bytes,
            });
        }
        total = total.saturating_add(*bytes);
    }
    if total > MAX_ADDON_BYTES {
        return Err(AddonError::TooLarge { bytes: total });
    }
    if !files.iter().any(|(p, _)| p == entry) {
        return Err(AddonError::EntryMissing(entry.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> AddonManifest {
        serde_json::from_str(
            r#"{
              "id": "acme.clock",
              "name": "Clock",
              "description": "The time, analog or digital.",
              "version": "1.2.0",
              "author": "Acme",
              "homepage": "https://example.com/clock",
              "repo": "https://example.com/acme/clock",
              "license": "MIT",
              "tags": ["time", "widgets"],
              "window": { "width": 200, "height": 200, "min_width": 120, "min_height": 120, "max_width": 400, "max_height": 400, "frame": "none", "transparent": true, "default_dock": "top_right" },
              "permissions": ["storage", "theme", { "network": { "hosts": ["api.example.com", "*.example.org:8443"] } }]
            }"#,
        )
        .expect("a well-formed manifest")
    }

    fn ids(problems: &[AddonProblem]) -> Vec<(Option<String>, String)> {
        problems
            .iter()
            .map(|p| (p.field.clone(), p.text.id.to_string()))
            .collect()
    }

    fn has(problems: &[AddonProblem], field: &str, id: &str) -> bool {
        problems
            .iter()
            .any(|p| p.field.as_deref() == Some(field) && p.text.id == id)
    }

    #[test]
    fn a_well_formed_manifest_has_no_problems_and_round_trips() {
        let m = manifest();
        assert!(m.validate().is_empty(), "{:?}", ids(&m.validate()));
        assert_eq!(m.entry, DEFAULT_ENTRY);
        assert_eq!(m.window.frame, AddonFrame::None);
        assert_eq!(m.window.default_dock, AddonDock::TopRight);
        let json = serde_json::to_string(&m).unwrap();
        let back: AddonManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn a_manifest_with_only_the_required_fields_takes_the_defaults() {
        let m: AddonManifest = serde_json::from_str(
            r#"{"id":"x","name":"X","description":"d","version":"0.1.0","license":"MIT"}"#,
        )
        .unwrap();
        assert_eq!(m.window, AddonWindow::default());
        assert!(m.permissions.is_empty());
        assert!(m.validate().is_empty());
    }

    #[test]
    fn a_misspelled_key_is_refused_by_name() {
        let err = serde_json::from_str::<AddonManifest>(
            r#"{"id":"x","name":"X","description":"d","version":"0.1.0","license":"MIT","permisions":[]}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("permisions"), "{err}");
    }

    #[test]
    fn permissions_keep_their_wire_shape() {
        let json = serde_json::to_string(&manifest().permissions).unwrap();
        assert_eq!(
            json,
            r#"["storage","theme",{"network":{"hosts":["api.example.com","*.example.org:8443"]}}]"#
        );
        assert_eq!(AddonPermission::WORDS.len(), 10);
        for word in AddonPermission::WORDS {
            let value = if word == "network" {
                r#"{"network":{"hosts":["a.example"]}}"#.to_string()
            } else {
                format!("{word:?}")
            };
            let p: AddonPermission = serde_json::from_str(&value).unwrap();
            assert_eq!(p.word(), word);
        }
    }

    #[test]
    fn a_grant_copies_the_declaration_and_a_subset_is_exact() {
        let m = manifest();
        let net = AddonPermission::grant_by_word("network", &m.permissions).unwrap();
        assert!(matches!(net, AddonPermission::Network { hosts } if hosts.len() == 2));
        assert!(AddonPermission::grant_by_word("notify", &m.permissions).is_none());
        let widened = AddonPermission::Network {
            hosts: vec!["evil.example".into()],
        };
        assert!(!AddonPermission::is_subset(
            std::slice::from_ref(&widened),
            &m.permissions
        ));
        assert!(AddonPermission::is_subset(
            &[AddonPermission::Storage, net.clone()],
            &m.permissions
        ));
    }

    #[test]
    fn every_manifest_rule_refuses_by_field() {
        let mut m = manifest();
        m.name = "  ".into();
        m.description = String::new();
        m.version = "1.2".into();
        m.license = String::new();
        m.homepage = Some("http://example.com".into());
        m.repo = Some("git@github.com:a/b".into());
        m.tags = vec!["Widgets".into(), "ok".into(), "ok".into()];
        m.window.width = 20;
        m.window.min_height = Some(500);
        m.window.max_height = Some(300);
        m.entry = "../index.html".into();
        m.permissions = vec![
            AddonPermission::Storage,
            AddonPermission::Storage,
            AddonPermission::Network { hosts: vec![] },
        ];
        let problems = m.validate();
        for (field, id) in [
            ("name", "problem-addon-needs-name"),
            ("description", "problem-addon-needs-description"),
            ("version", "problem-addon-version-not-semver"),
            ("license", "problem-addon-license-needed"),
            ("homepage", "problem-addon-url-not-https"),
            ("repo", "problem-addon-url-not-https"),
            ("tags", "problem-addon-tag-shape"),
            ("tags", "problem-addon-tag-declared-twice"),
            ("window.width", "problem-addon-window-out-of-bounds"),
            ("window.min_height", "problem-addon-window-min-above-max"),
            ("window.height", "problem-addon-window-size-outside-min-max"),
            ("entry", "problem-addon-entry-not-bundle-path"),
            ("permissions.1", "problem-addon-permission-declared-twice"),
            ("permissions.2", "problem-addon-network-needs-host"),
        ] {
            assert!(
                has(&problems, field, id),
                "{field} {id}: {:?}",
                ids(&problems)
            );
        }
    }

    #[test]
    fn the_other_manifest_rules_refuse_too() {
        let mut m = manifest();
        m.name = "n".repeat(MAX_ADDON_NAME_CHARS + 1);
        m.description = "d".repeat(MAX_ADDON_DESCRIPTION_CHARS + 1);
        m.author = Some("a".repeat(MAX_ADDON_FIELD_CHARS + 1));
        m.tags = (0..MAX_TAGS + 1).map(|i| format!("t{i}")).collect();
        m.entry = "main.js".into();
        m.permissions = vec![AddonPermission::Network {
            hosts: vec![
                "Bad Host".into(),
                "localhost:4477".into(),
                "*.127.0.0.1".into(),
            ],
        }];
        let problems = m.validate();
        for (field, id) in [
            ("name", "problem-addon-name-too-long"),
            ("description", "problem-addon-description-too-long"),
            ("author", "problem-addon-field-too-long"),
            ("tags", "problem-addon-too-many-tags"),
            ("entry", "problem-addon-entry-not-html"),
            ("permissions.0", "problem-addon-network-host-not-pattern"),
            ("permissions.0", "problem-addon-network-loopback-refused"),
        ] {
            assert!(
                has(&problems, field, id),
                "{field} {id}: {:?}",
                ids(&problems)
            );
        }
    }

    #[test]
    fn a_record_refuses_a_grant_it_never_declared() {
        let record = AddonRecord {
            manifest: manifest(),
            origin: Origin::Local,
            enabled: true,
            granted: vec![AddonPermission::Notify],
            installed_at: 1,
        };
        let problems = record.validate();
        assert!(has(
            &problems,
            "granted",
            "problem-addon-grant-not-declared"
        ));
        assert!(record.network_hosts().is_none());
        let granted = AddonRecord {
            granted: record.manifest.permissions.clone(),
            ..record
        };
        assert!(granted.validate().is_empty());
        assert_eq!(granted.network_hosts().unwrap().len(), 2);
        let json = serde_json::to_string(&granted).unwrap();
        assert!(json.contains(r#""origin":"local""#), "{json}");
    }

    #[test]
    fn semver_is_three_numbers_with_an_optional_tail() {
        for ok in [
            "0.1.0",
            "1.2.3",
            "10.0.0-beta.1",
            "1.0.0+build.5",
            "1.0.0-rc.1+sha",
        ] {
            assert!(is_semver(ok), "{ok}");
        }
        for bad in ["1.2", "v1.2.3", "01.2.3", "1.2.3.4", "1.2.3-", "a.b.c", ""] {
            assert!(!is_semver(bad), "{bad}");
        }
    }

    #[test]
    fn bundle_paths_are_plain_relative_and_never_hidden() {
        for ok in [
            "index.html",
            "css/style.css",
            "img/a-b_c.png",
            "deep/er/file.js",
        ] {
            assert!(is_bundle_path(ok), "{ok}");
        }
        for bad in [
            "",
            "/index.html",
            "../index.html",
            "a/../b.js",
            ".git/config",
            "a/.hidden.css",
            "a\\b.js",
            "a\0b.js",
            "dir/",
            "./x.js",
        ] {
            assert!(!is_bundle_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn served_types_come_from_the_allowlist() {
        assert_eq!(
            served_content_type("INDEX.HTML"),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(served_content_type("a.woff2"), Some("font/woff2"));
        assert_eq!(served_content_type("x.exe"), None);
        assert_eq!(served_content_type("noext"), None);
        assert_eq!(served_content_type("a.wasm"), None);
    }

    #[test]
    fn a_bundle_listing_keeps_every_rule() {
        let ok = vec![
            ("index.html".to_string(), 10u64),
            ("main.js".to_string(), 20),
            ("img/icon.png".to_string(), 30),
        ];
        assert_eq!(check_bundle_listing(&ok, "index.html"), Ok(()));
        assert_eq!(
            check_bundle_listing(&ok, "app.html"),
            Err(AddonError::EntryMissing("app.html".into()))
        );
        let with = |extra: (&str, u64)| {
            let mut v = ok.clone();
            v.push((extra.0.to_string(), extra.1));
            v
        };
        assert_eq!(
            check_bundle_listing(&with(("lib/bisa-addon.js", 1)), "index.html"),
            Err(AddonError::ReservedFile("lib/bisa-addon.js".into()))
        );
        assert_eq!(
            check_bundle_listing(&with(("addon.json", 1)), "index.html"),
            Err(AddonError::ReservedFile("addon.json".into()))
        );
        assert_eq!(
            check_bundle_listing(&with(("tool.exe", 1)), "index.html"),
            Err(AddonError::NotServedFile("tool.exe".into()))
        );
        assert_eq!(
            check_bundle_listing(&with(("../x.js", 1)), "index.html"),
            Err(AddonError::BadBundlePath("../x.js".into()))
        );
        assert_eq!(
            check_bundle_listing(&with(("big.png", MAX_ADDON_FILE_BYTES + 1)), "index.html"),
            Err(AddonError::FileTooLarge {
                name: "big.png".into(),
                bytes: MAX_ADDON_FILE_BYTES + 1
            })
        );
        let many: Vec<(String, u64)> = (0..MAX_ADDON_FILES + 1)
            .map(|i| (format!("f{i}.js"), 1))
            .collect();
        assert_eq!(
            check_bundle_listing(&many, "f0.js"),
            Err(AddonError::TooManyFiles {
                found: MAX_ADDON_FILES + 1
            })
        );
        let heavy: Vec<(String, u64)> = (0..5)
            .map(|i| (format!("f{i}.png"), MAX_ADDON_FILE_BYTES))
            .collect();
        assert!(matches!(
            check_bundle_listing(&heavy, "f0.png"),
            Err(AddonError::TooLarge { .. })
        ));
    }
}
