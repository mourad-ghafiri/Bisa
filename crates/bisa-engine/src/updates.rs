//! The newest release of the platform on GitHub, read when a person asks —
//! the desktop's *You › Update* — and held for `cache.updates.ttl_ms`.
//!
//! The node answers **facts, never a comparison**: the latest published
//! release as GitHub lists it (`GET /repos/{owner}/{repo}/releases/latest`
//! names the one marked *latest* — never a draft, never a prerelease), or
//! why there is none. Which version a person runs is the desktop's to know
//! (the app the person would download is the desktop, whose version is
//! baked in at build time), so the desktop decides whether what came back is
//! newer, in a model of its own.
//!
//! Nothing here polls: a request leaves the machine only when the dialog
//! asks, and the answer is held for the TTL unless it is a failure, which is
//! asked again next time; a refresh drops the held answer first. The source
//! is **`None` by default** — an engine nobody configured, every test
//! fixture, never dials GitHub and answers *off*; the node that serves a
//! person hands in the repository the build was made from
//! (`crates/bisa-cli/src/ctx.rs`), a test a stub on the loopback.
//!
//! The read is unsigned — a public endpoint, no token, nothing of the
//! person's in the request — through the one outbound client every crate
//! shares (`bisa_http::Clients`), so the `network.*` policy holds. GitHub
//! allows sixty unsigned requests an hour from one address; a limit is said
//! as such, with the wait when GitHub names one.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where GitHub's API lives.
pub const GITHUB_API: &str = "https://api.github.com";

/// How long one read may take before it is *unreachable*.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Where the latest release is read from: one URL, built from the
/// repository the build was made from, or handed in whole by a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatesSource {
    latest_url: String,
    /// Where a release's links must point — `https://github.com/<slug>/` for
    /// a repository source — so what the answer names is the repository's
    /// own page and assets, never a link an answer could redirect a person
    /// to; `None` for a source handed in whole, which is trusted as given.
    pinned: Option<String>,
}

impl UpdatesSource {
    /// GitHub's *latest release* endpoint for a repository URL of the
    /// `https://github.com/owner/repo` shape (a `.git` suffix and a trailing
    /// slash tolerated); `None` for any other URL, so a fork published
    /// elsewhere asks nobody.
    pub fn github(repository: &str) -> Option<Self> {
        let slug = repository_slug(repository)?;
        Some(Self {
            latest_url: format!("{GITHUB_API}/repos/{slug}/releases/latest"),
            pinned: Some(format!("https://github.com/{slug}/")),
        })
    }

    /// A source at exactly this URL — a test's stub on the loopback, or a
    /// rehearsal's (`BISA_RELEASES_API`) — whose links are trusted as given.
    pub fn at(latest_url: impl Into<String>) -> Self {
        Self {
            pinned: None,
            latest_url: latest_url.into(),
        }
    }

    /// The URL one read goes to.
    pub fn latest_url(&self) -> &str {
        &self.latest_url
    }
}

/// `owner/repo` out of `https://github.com/owner/repo`, with or without a
/// `.git` suffix or a trailing slash; `None` for a URL that is not a GitHub
/// repository. The same rule the release scripts read the slug by
/// (`scripts/release/releaseModel.mjs`).
pub fn repository_slug(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("http://github.com/"))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let owner = parts.next().filter(|p| !p.is_empty())?;
    let repo = parts.next().filter(|p| !p.is_empty())?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

/// What one check answered: the latest release, that there is none, that
/// nothing was asked, or why the read failed. `checked_at` is unix seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateCheck {
    /// The release GitHub marks *latest*.
    Latest {
        release: LatestRelease,
        checked_at: u64,
    },
    /// GitHub has no published release for the repository (a 404).
    NoRelease { checked_at: u64 },
    /// The engine was started without a source: nothing was asked.
    Off,
    /// The read failed; never held, so the next ask tries again.
    Failed {
        failure: UpdateFailure,
        checked_at: u64,
    },
}

impl UpdateCheck {
    /// Whether the answer is worth holding for the TTL: a release or its
    /// absence is; a failure is asked again.
    fn cacheable(&self) -> bool {
        matches!(self, Self::Latest { .. } | Self::NoRelease { .. })
    }

    /// One word for the log.
    fn word(&self) -> &'static str {
        match self {
            Self::Latest { .. } => "latest",
            Self::NoRelease { .. } => "no_release",
            Self::Off => "off",
            Self::Failed { .. } => "failed",
        }
    }
}

/// Why a read failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateFailure {
    /// No answer came back — the network, a proxy, the deadline. The
    /// innermost cause, for the log; never a URL that could carry a login.
    Unreachable { reason: String },
    /// GitHub's rate limit on unsigned reads, with the wait when it named one.
    RateLimited { retry_in_secs: Option<u64> },
    /// An answer that was neither a release nor a known refusal.
    Unexpected { status: u16 },
}

/// A published release as the desktop reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LatestRelease {
    /// The tag as GitHub holds it — `v0.2.0`.
    pub tag: String,
    /// The tag without its leading `v` — `0.2.0`, the shape the desktop
    /// compares with its own version.
    pub version: String,
    /// The release's title, when it has one — *Bisa 0.2.0*.
    pub name: Option<String>,
    /// When it was published, unix seconds, when GitHub said.
    pub published_at: Option<u64>,
    /// The release page, where the notes and every asset are.
    pub url: String,
    /// The notes, Markdown as the release carries them.
    pub notes: Option<String>,
    /// Whether GitHub marks it a prerelease (never for *latest*, kept so the
    /// shape says what it is).
    pub prerelease: bool,
    /// What the release ships, as listed.
    pub assets: Vec<ReleaseAsset>,
}

/// One file a release ships.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// GitHub's release object, the fields read; every other field ignored.
#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// A link the answer gave, kept when it is under `pinned` — the
/// repository's own pages and assets — and dropped otherwise; every link is
/// kept when nothing is pinned.
fn pinned_link(url: &str, pinned: Option<&str>) -> Option<String> {
    match pinned {
        Some(prefix) if !url.starts_with(prefix) => None,
        _ => Some(url.to_string()),
    }
}

/// The release the desktop reads, from GitHub's object — its links held to
/// `pinned`: a page off the repository is the tag's own page instead, an
/// asset off it is left out.
fn release_of(gh: GhRelease, pinned: Option<&str>) -> LatestRelease {
    let version = gh
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&gh.tag_name)
        .to_string();
    let published_at = gh
        .published_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp())
        .filter(|t| *t >= 0)
        .map(|t| t as u64);
    let url = pinned_link(&gh.html_url, pinned)
        .unwrap_or_else(|| format!("{}releases/tag/{}", pinned.unwrap_or_default(), gh.tag_name));
    LatestRelease {
        version,
        tag: gh.tag_name,
        name: gh.name.filter(|n| !n.trim().is_empty()),
        published_at,
        url,
        notes: gh.body.filter(|b| !b.trim().is_empty()),
        prerelease: gh.prerelease,
        assets: gh
            .assets
            .into_iter()
            .filter_map(|a| {
                Some(ReleaseAsset {
                    name: a.name,
                    url: pinned_link(&a.browser_download_url, pinned)?,
                    size: a.size,
                })
            })
            .collect(),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The latest release: from the cache, else from the source — or *off* when
/// the engine has none.
pub async fn check(inner: &crate::Inner, refresh: bool) -> UpdateCheck {
    let Some(source) = inner.updates.as_ref() else {
        return UpdateCheck::Off;
    };
    let ttl = inner.cache.settings().updates_ttl();
    let cell = inner.cache.updates();
    if refresh {
        cell.clear();
    } else if let Some(hit) = cell.get(ttl) {
        return hit;
    }
    let checked_at = now_secs();
    let answer = inner
        .http
        .outbound()
        .get(source.latest_url())
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .timeout(READ_TIMEOUT)
        .send()
        .await;
    let check = match answer {
        Err(e) => UpdateCheck::Failed {
            failure: UpdateFailure::Unreachable {
                reason: crate::network::cause_of(&e),
            },
            checked_at,
        },
        Ok(resp) => {
            let status = resp.status();
            if status.as_u16() == 404 {
                UpdateCheck::NoRelease { checked_at }
            } else if let Some(retry_in_secs) =
                bisa_codehost::github::rate_limit_secs(status, resp.headers())
            {
                UpdateCheck::Failed {
                    failure: UpdateFailure::RateLimited { retry_in_secs },
                    checked_at,
                }
            } else if status.is_success() {
                match resp.json::<GhRelease>().await {
                    Ok(gh) => UpdateCheck::Latest {
                        release: release_of(gh, source.pinned.as_deref()),
                        checked_at,
                    },
                    Err(_) => UpdateCheck::Failed {
                        failure: UpdateFailure::Unexpected {
                            status: status.as_u16(),
                        },
                        checked_at,
                    },
                }
            } else {
                UpdateCheck::Failed {
                    failure: UpdateFailure::Unexpected {
                        status: status.as_u16(),
                    },
                    checked_at,
                }
            }
        }
    };
    if !ttl.is_zero() && check.cacheable() {
        cell.set(check.clone());
    }
    tracing::info!(
        target: "bisa_engine::updates",
        outcome = check.word(), // LCOV_EXCL_LINE a tracing event's field, evaluated under a subscriber only
        refresh,
        "the latest release was asked for"
    );
    check
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_repository_url_is_a_slug_and_anything_else_is_none() {
        for url in [
            "https://github.com/mourad-ghafiri/Bisa",
            "https://github.com/mourad-ghafiri/Bisa.git",
            "https://github.com/mourad-ghafiri/Bisa/",
            " https://github.com/mourad-ghafiri/Bisa ",
        ] {
            assert_eq!(
                repository_slug(url).as_deref(),
                Some("mourad-ghafiri/Bisa"),
                "{url}"
            );
        }
        for url in [
            "https://gitlab.com/mourad-ghafiri/Bisa",
            "https://github.com/mourad-ghafiri",
            "https://github.com/mourad-ghafiri/Bisa/issues",
            "",
            "github.com/mourad-ghafiri/Bisa",
        ] {
            assert_eq!(repository_slug(url), None, "{url}");
        }
        assert_eq!(
            UpdatesSource::github("https://github.com/mourad-ghafiri/Bisa")
                .unwrap()
                .latest_url(),
            "https://api.github.com/repos/mourad-ghafiri/Bisa/releases/latest"
        );
        assert_eq!(UpdatesSource::github("https://example.com/x/y"), None);
    }

    #[test]
    fn a_release_is_read_with_its_version_date_notes_and_assets() {
        let gh: GhRelease = serde_json::from_value(serde_json::json!({
            "tag_name": "v0.3.0",
            "name": "Bisa 0.3.0",
            "html_url": "https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.3.0",
            "body": "### Added\n\n- A thing.\n",
            "published_at": "2026-10-03T17:53:35Z",
            "prerelease": false,
            "draft": false,
            "assets": [{"name": "Bisa-0.3.0-macos-universal.dmg", "browser_download_url": "https://github.com/mourad-ghafiri/Bisa/releases/download/v0.3.0/Bisa-0.3.0-macos-universal.dmg", "size": 124000000, "content_type": "application/x-apple-diskimage"}],
            "something_new": {"ignored": true}
        }))
        .unwrap();
        let release = release_of(gh, None);
        assert_eq!(release.version, "0.3.0");
        assert_eq!(release.tag, "v0.3.0");
        assert_eq!(release.name.as_deref(), Some("Bisa 0.3.0"));
        assert_eq!(release.published_at, Some(1_791_050_015));
        assert_eq!(release.notes.as_deref(), Some("### Added\n\n- A thing.\n"));
        assert!(!release.prerelease);
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "Bisa-0.3.0-macos-universal.dmg");
        assert_eq!(release.assets[0].size, 124_000_000);
        // A tag without the `v`, no notes, no date: the shape still holds.
        let bare: GhRelease = serde_json::from_value(serde_json::json!({
            "tag_name": "0.4.0", "html_url": "https://example.test/r", "body": "  "
        }))
        .unwrap();
        let release = release_of(bare, None);
        assert_eq!(release.version, "0.4.0");
        assert_eq!(release.notes, None);
        assert_eq!(release.published_at, None);
        assert!(release.assets.is_empty());
    }

    /// A repository source pins every link to the repository: a page
    /// elsewhere becomes the tag's own page, an asset elsewhere is left out,
    /// and a source handed in whole keeps what it was given.
    #[test]
    fn a_link_off_the_pinned_repository_is_replaced_or_dropped() {
        let gh = || -> GhRelease {
            serde_json::from_value(serde_json::json!({
                "tag_name": "v0.3.0",
                "html_url": "https://evil.test/claims/v0.3.0",
                "assets": [
                    {"name": "Bisa-0.3.0-macos-universal.dmg", "browser_download_url": "https://github.com/o/r/releases/download/v0.3.0/Bisa-0.3.0-macos-universal.dmg", "size": 1},
                    {"name": "elsewhere.dmg", "browser_download_url": "https://github.com/other/repo/releases/download/v0.3.0/elsewhere.dmg", "size": 1},
                    {"name": "plain.dmg", "browser_download_url": "http://github.com/o/r/releases/download/v0.3.0/plain.dmg", "size": 1},
                    {"name": "lookalike.dmg", "browser_download_url": "https://github.com.evil.test/o/r/releases/download/v0.3.0/lookalike.dmg", "size": 1}
                ]
            }))
            .unwrap()
        };
        let source = UpdatesSource::github("https://github.com/o/r").unwrap();
        let pinned = release_of(gh(), source.pinned.as_deref());
        assert_eq!(pinned.url, "https://github.com/o/r/releases/tag/v0.3.0");
        assert_eq!(pinned.assets.len(), 1, "{:?}", pinned.assets);
        assert_eq!(pinned.assets[0].name, "Bisa-0.3.0-macos-universal.dmg");
        let trusted = release_of(
            gh(),
            UpdatesSource::at("http://127.0.0.1:1/x").pinned.as_deref(),
        );
        assert_eq!(trusted.url, "https://evil.test/claims/v0.3.0");
        assert_eq!(trusted.assets.len(), 4);
    }

    #[test]
    fn the_wire_names_the_state_and_the_failure_kind() {
        let v = serde_json::to_value(UpdateCheck::Failed {
            failure: UpdateFailure::RateLimited {
                retry_in_secs: Some(42),
            },
            checked_at: 7,
        })
        .unwrap();
        assert_eq!(v["state"], "failed");
        assert_eq!(v["failure"]["kind"], "rate_limited");
        assert_eq!(v["failure"]["retry_in_secs"], 42);
        assert_eq!(
            serde_json::to_value(UpdateCheck::Off).unwrap()["state"],
            "off"
        );
        assert!(UpdateCheck::NoRelease { checked_at: 1 }.cacheable());
        assert!(!UpdateCheck::Failed {
            failure: UpdateFailure::Unexpected { status: 500 },
            checked_at: 1
        }
        .cacheable());
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_outcome_has_one_word_for_the_log() {
        let gh: GhRelease = serde_json::from_value(json!({
            "tag_name": "v0.3.0",
            "name": "Bisa 0.3.0",
            "html_url": "https://github.com/o/r/releases/tag/v0.3.0",
            "body": "notes",
            "published_at": "2026-10-10T09:00:00Z",
            "prerelease": false,
            "draft": false,
            "assets": []
        }))
        .unwrap();
        let latest = UpdateCheck::Latest {
            release: release_of(gh, None),
            checked_at: 1,
        };
        assert_eq!(latest.word(), "latest");
        assert_eq!(
            UpdateCheck::NoRelease { checked_at: 1 }.word(),
            "no_release"
        );
        assert_eq!(UpdateCheck::Off.word(), "off");
        let failed = UpdateCheck::Failed {
            failure: UpdateFailure::Unexpected { status: 500 },
            checked_at: 1,
        };
        assert_eq!(failed.word(), "failed");
    }
}
