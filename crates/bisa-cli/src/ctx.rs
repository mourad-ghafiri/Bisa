//! Shared command context: workspace/catalog/engine construction.

use anyhow::{Context as _, Result};
use bisa_adapters::register_all;
use bisa_engine::{Engine, EngineConfig};
use bisa_harness::HarnessCatalog;
use bisa_store::{FileKeyStore, Workspace};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Ctx {
    pub data_dir: PathBuf,
    pub file_keys: bool,
    pub no_node: bool,
    /// The process's diagnostic log: the workspace's `logging.*` settings
    /// reach it once the workspace is open, and the engine keeps it in step
    /// after that.
    pub log: bisa_log::Handle,
    /// The language the CLI speaks and asks the node for.
    pub locale: bisa_i18n::Locale,
}

impl Ctx {
    pub fn new(
        data_dir: Option<PathBuf>,
        file_keys: bool,
        no_node: bool,
        log: bisa_log::Handle,
        locale: bisa_i18n::Locale,
    ) -> Self {
        let data_dir = data_dir.unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".bisa")
        });
        Self {
            data_dir,
            file_keys,
            no_node,
            log,
            locale,
        }
    }

    /// A healthy running daemon for this workspace, unless `--no-node`.
    /// `None` falls back to embedded mode silently.
    pub async fn node_client(&self) -> Option<crate::client::NodeClient> {
        if self.no_node {
            return None;
        }
        crate::client::NodeClient::discover(&self.data_dir, self.locale.clone()).await
    }

    pub fn workspace(&self) -> Result<Workspace> {
        let ws = if self.file_keys {
            Workspace::open_with_keystore(
                &self.data_dir,
                Box::new(FileKeyStore::new(
                    bisa_store::Paths::new(&self.data_dir).identity_dir(),
                )),
            )
        } else {
            Workspace::open(&self.data_dir)
        };
        let ws = ws.with_context(|| {
            bisa_core::text!(
                "cli-ctx-opening-workspace",
                a0 = (self.data_dir.display()).to_string()
            )
        })?;
        // The machine's `logging.*` settings, now that they can be read: a
        // one-shot command that opened the workspace logs as the node would,
        // and one whose person switched the log off writes nothing.
        match ws.settings(None) {
            Ok(resolved) => {
                if let Err(e) = self.log.apply(bisa_engine::logging::config_from(&resolved)) {
                    tracing::warn!("logging settings not applied: {e}");
                }
            }
            Err(e) => tracing::warn!("logging settings not read: {e}"),
        }
        Ok(ws)
    }

    /// The harness catalog over the process's shared HTTP clients — for a
    /// command with no engine behind it. An engine's catalog takes the
    /// engine's own handle: [`Self::catalog_with`].
    pub fn catalog(&self) -> HarnessCatalog {
        self.catalog_with(bisa_http::Clients::shared())
    }

    /// The harness catalog, every network-reaching adapter over `http`.
    pub fn catalog_with(&self, http: Arc<bisa_http::Clients>) -> HarnessCatalog {
        let mut catalog = HarnessCatalog::new();
        // Custom descriptors load first so `register_all` can wrap them in
        // adapters; a missing dir is not an error.
        let errors = catalog.load_custom_dir(&self.data_dir.join("harnesses"));
        for e in errors {
            // The process's log is installed by now: a descriptor that does
            // not load is an absorbed fault, kept where troubleshooting looks.
            tracing::warn!(target: "bisa_cli", "custom harness descriptor skipped: {e}");
        }
        register_all(&mut catalog, http);
        catalog
    }

    /// Start an embedded engine for one command (spawns the intake socket
    /// task). It **hears no start event**: a command does what it was asked
    /// and leaves, so it never starts a run from the queue that would be left
    /// mid-flight when it exits. What an event wrote down waits, durable, for
    /// the node ([`Self::node_engine`]). A run's waits and boundary events
    /// are heard either way: they are the run's.
    ///
    /// Refuses when another engine holds the workspace. The daemon probe in
    /// `node_client` gives up after half a second, and a daemon busy with a
    /// long operation routinely misses that — so "no answer" must fail
    /// loudly here rather than start a second scheduler on the same work.
    pub async fn engine(&self) -> Result<(Engine, ())> {
        let engine = self.start(EngineConfig {
            events_enabled: false,
            ..EngineConfig::default()
        })?;
        Ok((engine, ()))
    }

    /// The engine the daemon runs: the listening runtime with it — the
    /// ticker and the signal worker, under the person's `events.enabled`.
    /// The one engine that starts runs from events.
    pub async fn node_engine(&self) -> Result<Engine> {
        self.start(EngineConfig::default())
    }

    /// An embedded engine that hears no start event and designs nothing: what
    /// a one-shot command wants when it turns a host on or off, raises a
    /// signal or lets one through. It writes what it was asked to and
    /// leaves — it never becomes the thing that starts runs on somebody's
    /// terminal. A run's waits and boundary events are heard either way.
    pub async fn quiet_engine(&self) -> Result<Engine> {
        self.start(EngineConfig {
            events_enabled: false,
            design_enabled: false,
            ..EngineConfig::default()
        })
    }

    /// An embedded engine on this workspace, `base` deciding what it runs.
    fn start(&self, base: EngineConfig) -> Result<Engine> {
        let ws = self.workspace()?;
        // One set of HTTP clients under the `network.*` settings, made before
        // the engine so the adapters — a harness's usage endpoint, an A2A
        // peer — the code host CLIs and the engine's own calls share it and
        // are swapped together on a write.
        let http = Arc::new(bisa_engine::network::clients_from(&ws));
        // A person's CLI, like their node, reaches their `~/.ssh` public
        // material for the `git ssh` and `git connection` verbs.
        let config = EngineConfig {
            ssh: bisa_engine::ssh::default_ssh(),
            // The code host CLIs on this machine (`gh`, `glab`), asked first
            // for every pull-request verb (ide/08 §CLI first), handed the
            // same proxy the API path uses.
            cli: Some(bisa_engine::codehost::default_cli(Arc::clone(&http))),
            // The mobile tools on this machine (ide/19): Flutter, the
            // simulators, adb and the emulators, probed and driven by the node.
            mobile_development: Some(Arc::new(bisa_mobile_development::Real::default())),
            // The log this process installed: the engine applies the
            // machine's `logging.*` settings to it and follows every write.
            log: Some(self.log.clone()),
            http: Some(Arc::clone(&http)),
            ..base
        };
        match Engine::start(ws, self.catalog_with(http), config) {
            Ok(engine) => Ok(engine),
            Err(bisa_engine::EngineError::Locked { path, holder }) => {
                let who = holder.map(|h| format!(" — {h}")).unwrap_or_default();
                anyhow::bail!(bisa_core::text!(
                    "cli-ctx-another-engine-holds-workspace-did-not",
                    path = path.to_string(),
                    who = who.to_string()
                ));
            }
            Err(e) => Err(e).context(bisa_core::text!("cli-ctx-starting-engine")),
        }
    }

    pub fn npub(&self, ws: &Workspace) -> Result<String> {
        use nostr::nips::nip19::ToBech32;
        Ok(ws.owner_keys().public_key().to_bech32()?)
    }
}
