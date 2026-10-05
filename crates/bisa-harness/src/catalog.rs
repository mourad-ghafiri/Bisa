//! The three-tier harness catalog (a bring-your-own-harness model):
//!
//! 1. **Compiled-in** adapters, registered by `bisa-adapters` at engine
//!    startup.
//! 2. **PATH-probed presets**: known harnesses we can detect but ship no
//!    first-class adapter for; surfaced in `harness list` with install hints.
//! 3. **User custom descriptors**: JSON files in `~/.bisa/harnesses/`,
//!    security-constrained (no auto-install, reserved env keys stripped,
//!    reserved id namespace).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::traits::HarnessAdapter;
use crate::types::{InteractiveLaunch, ProbeResult};

/// Env-key prefix owned by the engine; user/custom descriptors must never
/// set these (credential brokering and identity would be spoofable).
pub const RESERVED_ENV_PREFIX: &str = "BISA_";

/// Ids reserved for compiled-in adapters; custom descriptors may not claim
/// them (or the `acp:` namespace, which tier-1 uses for ACP targets).
pub const BUILTIN_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "pi",
    "omp",
    "opencode",
    "copilot",
    "grok",
    "gemini",
    "acp",
    "custom",
];

/// Tier-2 preset: PATH-probed only, never auto-installed.
#[derive(Debug, Clone)]
pub struct PresetHarness {
    pub id: &'static str,
    pub label: &'static str,
    pub command: &'static str,
    /// What continues the tool's latest session in the directory it starts
    /// in — the tool's own words, never a guess (see [`InteractiveLaunch`]).
    pub resume_args: &'static [&'static str],
    pub install_hint: &'static str,
}

/// The tier-2 list, and the rule it is held to: **a preset is a harness we can
/// detect but ship no first-class adapter for.**
///
/// `preset:omp` and `preset:opencode` used to be here and were neither. Both
/// have compiled-in adapters, so `harness list` reported omp three times —
/// `omp`, `acp:omp` and `preset:omp` — and every picker in the app offered the
/// same tool under three names, two of which could not run: a preset has no
/// adapter object, so `HarnessCatalog::get("preset:omp")` has always answered
/// `None`. A test below keeps a preset from ever again shadowing an adapter.
pub const PRESET_HARNESSES: &[PresetHarness] = &[
    PresetHarness {
        id: "preset:goose",
        label: "Goose",
        command: "goose",
        // `goose session --resume`: "Resume a previous session" — with no
        // `--name` or `--session-id` given, the previous one.
        resume_args: &["session", "--resume"],
        install_hint: "https://block.github.io/goose/",
    },
    PresetHarness {
        id: "preset:cursor-agent",
        label: "Cursor Agent",
        command: "cursor-agent",
        // `--continue`: "Continue the previous session (alias for
        // `--resume=-1`)" in the Cursor CLI reference.
        resume_args: &["--continue"],
        install_hint: "https://cursor.com",
    },
];

/// Tier-3 user descriptor. Deliberately minimal (a strict tier-3 security
/// posture): no install commands, no icon URLs, env sanitized on load.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomHarnessSpec {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// What continues this command's latest session in the same directory, if
    /// it has such a form. Empty means it has none, and the terminal opens it
    /// fresh — the catalog has never had an opinion about what a tier-3 binary
    /// does, and it does not gain one here.
    #[serde(default)]
    pub resume_args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("custom harness id {0:?} collides with a reserved builtin id")]
    ReservedId(String),
    #[error("invalid descriptor {path}: {message}")]
    InvalidDescriptor { path: String, message: String },
    #[error("i/o error reading {path}: {message}")]
    Io { path: String, message: String },
}

/// Which tier a listing entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessTier {
    Builtin,
    Preset,
    Custom,
}

/// One row of `harness list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessListing {
    pub id: String,
    pub label: String,
    pub tier: HarnessTier,
    pub probe: ProbeResult,
    /// Where the binary resolved on `PATH`, when a bare lookup found one.
    ///
    /// This used to be smuggled into `ProbeResult::version`, whose doc says
    /// *version string reported by the binary* — so every preset and custom row
    /// rendered `/opt/homebrew/bin/goose` in a column labelled version, while
    /// compiled-in adapters (which really do run `--version`) rendered a
    /// version. Two different facts under one name is how a UI ends up lying
    /// without anybody writing a lie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// How to run this one in a terminal, or `None` when it has no interactive
    /// form. See [`InteractiveLaunch`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<InteractiveLaunch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
    /// How this harness is installed, in the official page's words — for
    /// the compiled-in harnesses the platform knows the page of
    /// (`install::install_hint`); the setup gate and Settings › Harnesses
    /// show it when the harness is missing.
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub install: Option<crate::install::InstallHint>,
    /// The Tool & Commands Guard can veto this harness's tool calls before
    /// they run (`HarnessCaps::TOOL_GUARD`). A preset or custom harness runs
    /// under its own sandbox; the platform only observes it.
    #[serde(default)]
    pub tool_guard: bool,
    /// The guard can hand this harness a rewritten input — a redacted
    /// placeholder restored right before execution (`HarnessCaps::INPUT_REWRITE`).
    #[serde(default)]
    pub input_rewrite: bool,
}

/// Fallback resolver for parameterized adapter ids (e.g. `a2a:<base-url>`),
/// consulted by [`HarnessCatalog::get`] only after exact-id lookup fails.
/// Returns `None` when the id is not in the resolver's namespace.
pub type AdapterResolver = Arc<dyn Fn(&str) -> Option<Arc<dyn HarnessAdapter>> + Send + Sync>;

/// The catalog. Compiled-in adapters are registered by the adapters crate;
/// presets are static; custom descriptors are loaded from a directory.
///
/// Its two caches are its own, not the process's: two catalogs in one
/// process — two engines in a test binary, a CLI's embedded engine beside a
/// node's — never read each other's probes.
pub struct HarnessCatalog {
    adapters: Vec<Arc<dyn HarnessAdapter>>,
    custom: Vec<CustomHarnessSpec>,
    resolvers: Vec<AdapterResolver>,
    /// The probed listing, shared by every `GET /harnesses` — which the
    /// desktop hits on every terminal open. An uncached listing spawns
    /// `--version` for every builtin plus a PATH lookup per preset (a
    /// subprocess storm), so it is served from here; the TTL comes from
    /// `cache.harness_listing.ttl_ms`, passed by the caller.
    /// `list_with_timeout` stays uncached for the callers (the CLI, tests)
    /// that want a live probe every time.
    listing: bisa_cache::TtlCell<Vec<HarnessListing>>,
    /// Per-adapter model lists, keyed by adapter id; TTL from
    /// `cache.harness_models.ttl_ms`.
    models: bisa_cache::TtlCache<String, Vec<crate::types::ModelInfo>>,
}

impl Default for HarnessCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl HarnessCatalog {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
            custom: Vec::new(),
            resolvers: Vec::new(),
            listing: bisa_cache::TtlCell::new("harness.listing"),
            models: bisa_cache::TtlCache::new("harness.models"),
        }
    }

    /// Register a compiled-in adapter. Last registration wins on id conflict
    /// (deliberate: tests can override).
    pub fn register(&mut self, adapter: Arc<dyn HarnessAdapter>) {
        self.adapters.retain(|a| a.id() != adapter.id());
        self.adapters.push(adapter);
    }

    pub fn adapters(&self) -> &[Arc<dyn HarnessAdapter>] {
        &self.adapters
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn HarnessAdapter>> {
        self.adapters
            .iter()
            .find(|a| a.id() == id)
            .cloned()
            .or_else(|| self.resolvers.iter().find_map(|r| r(id)))
    }

    /// Register a fallback resolver for a parameterized id namespace
    /// (e.g. every `a2a:<url>`). Exact registrations always win.
    pub fn register_resolver(&mut self, resolver: AdapterResolver) {
        self.resolvers.push(resolver);
    }

    /// Models the given harness reports, cached per adapter id for `ttl`
    /// (`cache.harness_models.ttl_ms`). Empty = unknown (probe failed,
    /// nothing advertised) — callers keep free-text model entry available.
    pub async fn models_for(
        &self,
        id: &str,
        ttl: std::time::Duration,
    ) -> Vec<crate::types::ModelInfo> {
        if let Some(models) = self.models.get(ttl, &id.to_string()) {
            return models;
        }
        let models = match self.get(id) {
            Some(adapter) => adapter.models().await,
            None => Vec::new(),
        };
        if !ttl.is_zero() {
            self.models.insert(id.to_string(), models.clone());
        }
        models
    }

    /// A harness's account usage, from its adapter — asked fresh every time;
    /// the engine caches it (`cache.harness_usage.ttl_ms`), because a
    /// provider's usage endpoint is not a thing to hit on every render. A
    /// preset or an unknown id has no adapter and reports nothing, in its
    /// own name.
    pub async fn usage_for(&self, id: &str) -> crate::usage::UsageState {
        match self.get(id) {
            Some(adapter) => adapter.usage().await,
            None => crate::usage::UsageState::unsupported(id),
        }
    }

    pub fn custom_specs(&self) -> &[CustomHarnessSpec] {
        &self.custom
    }

    /// Load every `*.json` descriptor in `dir`. One bad file never aborts
    /// discovery (omp's rule); errors are returned alongside the loaded set.
    pub fn load_custom_dir(&mut self, dir: &Path) -> Vec<CatalogError> {
        let mut errors = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    return errors; // no custom dir = nothing custom, not an error
                }
                errors.push(CatalogError::Io {
                    path: dir.display().to_string(),
                    message: err.to_string(),
                });
                return errors;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Err(err) => errors.push(CatalogError::Io {
                    path: path.display().to_string(),
                    message: err.to_string(),
                }),
                Ok(text) => match serde_json::from_str::<CustomHarnessSpec>(&text) {
                    Err(err) => errors.push(CatalogError::InvalidDescriptor {
                        path: path.display().to_string(),
                        message: err.to_string(),
                    }),
                    Ok(spec) => match sanitize_custom(spec) {
                        Ok(spec) => {
                            self.custom.retain(|c| c.id != spec.id);
                            self.custom.push(spec);
                        }
                        Err(e) => errors.push(e),
                    },
                },
            }
        }
        errors
    }

    /// One compiled-in adapter's row.
    fn adapter_row(adapter: &Arc<dyn HarnessAdapter>, probe: ProbeResult) -> HarnessListing {
        HarnessListing {
            id: adapter.id().to_string(),
            label: adapter.display_name().to_string(),
            tier: HarnessTier::Builtin,
            // A compiled-in adapter's probe runs `--version`, so the path is
            // not something a bare lookup has told us here.
            path: None,
            launch: adapter.interactive(),
            probe,
            install_hint: crate::install::install_hint(adapter.id()).map(|h| h.url.to_string()),
            install: crate::install::install_hint(adapter.id()),
            tool_guard: adapter.caps().contains(bisa_core::HarnessCaps::TOOL_GUARD),
            input_rewrite: adapter
                .caps()
                .contains(bisa_core::HarnessCaps::INPUT_REWRITE),
        }
    }

    /// Every row that is *not* a compiled-in adapter: the tier-2 presets and
    /// the tier-3 user descriptors.
    ///
    /// Shared by [`Self::list`] and [`Self::list_with_timeout`], which differ
    /// only in how they probe adapters. They used to carry a verbatim copy of
    /// these two loops each, so every field added to a listing had to be added
    /// twice — and a field added once is a listing that disagrees with itself
    /// depending on which caller asked.
    fn catalog_rows(&self) -> Vec<HarnessListing> {
        let mut out = Vec::with_capacity(PRESET_HARNESSES.len() + self.custom.len());
        for preset in PRESET_HARNESSES {
            let (probe, path) = probe_command(preset.command);
            out.push(HarnessListing {
                id: preset.id.to_string(),
                label: preset.label.to_string(),
                tier: HarnessTier::Preset,
                path,
                launch: Some(
                    InteractiveLaunch::new(preset.command)
                        .resumable_with(preset.resume_args.iter().map(|a| a.to_string()).collect()),
                ),
                probe,
                install_hint: Some(preset.install_hint.to_string()),
                install: None,
                tool_guard: false,
                input_rewrite: false,
            });
        }
        for custom in &self.custom {
            let (probe, path) = probe_command(&custom.command);
            out.push(HarnessListing {
                id: format!("custom:{}", custom.id),
                label: custom.label.clone(),
                tier: HarnessTier::Custom,
                path,
                // A custom descriptor's command *is* its interactive form: it
                // is whatever the user wrote, and this catalog has never had an
                // opinion about what that binary does.
                launch: Some(
                    InteractiveLaunch::with_args(custom.command.clone(), custom.args.clone())
                        .resumable_with(custom.resume_args.clone()),
                ),
                probe,
                install_hint: custom.install_hint.clone(),
                install: None,
                tool_guard: false,
                input_rewrite: false,
            });
        }
        out
    }

    /// The probed listing, cached for `ttl` (`cache.harness_listing.ttl_ms`).
    /// This is the one every `GET /harnesses` should call: it spares
    /// the per-request subprocess storm of re-probing every binary, and a fresh
    /// probe still runs at most once per TTL. `list_with_timeout` stays uncached
    /// for the callers (the CLI, tests) that want a live probe every time.
    ///
    /// A listing in which a probe **timed out** is answered but not kept: a
    /// cold `--version` on a machine's first launch is not a harness that is
    /// absent, and holding it as one for the TTL is what left a footer with
    /// nothing to show until a restart. The next ask probes again.
    pub async fn list_cached(
        &self,
        ttl: std::time::Duration,
        per_probe: std::time::Duration,
    ) -> Vec<HarnessListing> {
        if let Some(listings) = self.listing.get(ttl) {
            return listings;
        }
        let listings = self.list_with_timeout(per_probe).await;
        if !ttl.is_zero() && !listings.iter().any(|l| timed_out(&l.probe)) {
            self.listing.set(listings.clone());
        }
        listings
    }

    /// Whether `id` probed as installed the last time the listing was read —
    /// a look at the warm cache alone, never a probe: `None` while the cache
    /// is cold or the id is not a row. What a synchronous reader (the
    /// Decision-Making Agent's readiness) may ask without spawning anything.
    pub fn installed_cached(&self, id: &str, ttl: std::time::Duration) -> Option<bool> {
        self.listing
            .get(ttl)?
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.probe.available)
    }

    /// Like [`Self::list`], but adapter probes run concurrently, each under
    /// `per_probe` — one slow or wedged binary can't hang the whole listing
    /// (it reports as unavailable with a timeout reason instead). Preset and
    /// custom probes are bare PATH lookups and need no timeout.
    pub async fn list_with_timeout(&self, per_probe: std::time::Duration) -> Vec<HarnessListing> {
        let probes = self.adapters.iter().map(|adapter| {
            let adapter = Arc::clone(adapter);
            async move {
                let probe = match tokio::time::timeout(per_probe, adapter.probe()).await {
                    Ok(p) => p,
                    Err(_) => ProbeResult {
                        available: false,
                        reason: Some(format!("{TIMED_OUT} {per_probe:?}")),
                        version: None,
                    },
                };
                Self::adapter_row(&adapter, probe)
            }
        });
        let mut out: Vec<HarnessListing> = futures::future::join_all(probes).await;
        out.extend(self.catalog_rows());
        out
    }

    /// Everything, probed, for `harness list`.
    pub async fn list(&self) -> Vec<HarnessListing> {
        let mut out = Vec::new();
        for adapter in &self.adapters {
            let probe = adapter.probe().await;
            out.push(Self::adapter_row(adapter, probe));
        }
        out.extend(self.catalog_rows());
        out
    }
}

/// Enforce the tier-3 security posture on a loaded descriptor.
fn sanitize_custom(mut spec: CustomHarnessSpec) -> Result<CustomHarnessSpec, CatalogError> {
    let id_lower = spec.id.to_ascii_lowercase();
    if BUILTIN_IDS.contains(&id_lower.as_str())
        || id_lower.starts_with("acp:")
        || id_lower.starts_with("preset:")
        || id_lower.starts_with("custom:")
    {
        return Err(CatalogError::ReservedId(spec.id));
    }
    // Reserved env keys are stripped, not rejected: a descriptor written for
    // an older version keeps working minus the forbidden keys.
    spec.env
        .retain(|k, _| !k.to_ascii_uppercase().starts_with(RESERVED_ENV_PREFIX));
    Ok(spec)
}

/// Host-level probe of a bare command: PATH lookup only (cheap, no spawn).
///
/// Returns the probe **and** the resolved path separately. They used to be one
/// value, with the path stuffed into `ProbeResult::version` — which is how a
/// column labelled *version* came to show a filesystem path for every preset.
/// A lookup that found a binary has learned where it is, not what version it
/// is; saying so takes a second return value and no more.
/// How a probe that outstayed its budget is worded — the one reason a listing
/// is not cached on (`list_cached`).
const TIMED_OUT: &str = "probe timed out after";

/// Whether a probe's answer is the budget's, not the binary's.
fn timed_out(probe: &ProbeResult) -> bool {
    probe
        .reason
        .as_deref()
        .is_some_and(|r| r.starts_with(TIMED_OUT))
}

pub fn probe_command(command: &str) -> (ProbeResult, Option<String>) {
    match which::which(command) {
        Ok(path) => {
            let path = path.display().to_string();
            (ProbeResult::available(None), Some(path))
        }
        Err(_) => (
            ProbeResult::unavailable(format!("{command} not found on PATH")),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_names_its_own_way_to_continue_the_latest_session() {
        let forms: Vec<(&str, &[&str])> = PRESET_HARNESSES
            .iter()
            .map(|p| (p.id, p.resume_args))
            .collect();
        assert_eq!(
            forms,
            vec![
                ("preset:goose", &["session", "--resume"][..]),
                ("preset:cursor-agent", &["--continue"][..]),
            ]
        );
        let catalog = HarnessCatalog::new();
        let rows = catalog.catalog_rows();
        let goose = rows.iter().find(|r| r.id == "preset:goose").unwrap();
        assert_eq!(
            goose.launch.as_ref().unwrap().resume_args,
            vec!["session".to_string(), "--resume".to_string()]
        );
    }

    #[test]
    fn sanitize_strips_reserved_env_and_rejects_reserved_ids() {
        let mut env = BTreeMap::new();
        env.insert("BISA_PRIVATE_KEY".to_string(), "sneaky".to_string());
        env.insert("bisa_relay".to_string(), "sneaky".to_string());
        env.insert("MY_AGENT_MODE".to_string(), "acp".to_string());
        let spec = CustomHarnessSpec {
            id: "my-agent".into(),
            label: "My Agent".into(),
            command: "my-agent-bin".into(),
            args: vec!["acp".into()],
            resume_args: vec![],
            env,
            install_hint: None,
        };
        let clean = sanitize_custom(spec).unwrap();
        assert_eq!(clean.env.len(), 1);
        assert!(clean.env.contains_key("MY_AGENT_MODE"));

        for bad in [
            "claude-code",
            "ACP:thing",
            "custom:x",
            "preset:goose",
            "codex",
            "copilot",
            "Grok",
            "gemini",
        ] {
            let spec = CustomHarnessSpec {
                id: bad.into(),
                label: "x".into(),
                command: "x".into(),
                args: vec![],
                resume_args: vec![],
                env: BTreeMap::new(),
                install_hint: None,
            };
            assert!(
                matches!(sanitize_custom(spec), Err(CatalogError::ReservedId(_))),
                "{bad}"
            );
        }
    }
}
