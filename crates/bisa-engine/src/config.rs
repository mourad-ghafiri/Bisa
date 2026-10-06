//! Engine configuration.

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Max work-items executing at once, across all adapters.
    pub global_concurrency: usize,
    /// Max concurrent sessions per adapter.
    pub per_adapter_concurrency: usize,
    /// Harness ids the operator has disabled.
    pub disabled_harnesses: Vec<String>,
    /// Wall-clock ceiling for one work-item run when its budget sets none.
    pub default_wall_clock_secs: u64,
    /// Idle TTL before a live session is parked (disk transcript retained).
    pub idle_ttl: Duration,
    /// How long a finished session stays in the roster (`GET /sessions`, the
    /// rail) so its `done` or `failed` is seen before it leaves.
    pub retain_ended_secs: u64,
    /// Result-intake socket path override. Defaults to
    /// `<workspace>/run/engine.sock`, falling back to a short temp path when
    /// that exceeds the platform's unix-socket path limit.
    pub socket_path: Option<PathBuf>,
    /// Max schema-validation attempts per work-item result.
    pub max_result_attempts: u8,
    /// How many times one goal may be re-asked after the human answered
    /// "I'm not sure"; the *not sure* past them fails a `human` step with the
    /// person's words.
    ///
    /// **3**, mirroring [`Self::max_result_attempts`]. "I'm not sure" is a
    /// real answer, not a decline, so it sends the asker back to narrow the
    /// question — but a question that can be re-asked forever is a way to
    /// never finish, and the person who did not know the first time is the
    /// one who pays for each retry. On the last round the asker is told to
    /// proceed on its own recommendation and journal the assumption, which
    /// leaves a reviewable fact rather than an open loop.
    pub max_clarify_rounds: u8,
    /// How many `(harness, model)` launches one work item may spend before it
    /// settles as a plain failure — the bound on the two-dimensional launch
    /// walk *plus* every mid-run relaunch after an
    /// [`bisa_harness::Outcome::ModelUnavailable`], which share one
    /// budget so a failover can never reset the count.
    ///
    /// **6**, because a realistic worst case is a three-model plan across two
    /// harness candidates, and one full sweep of that is exactly six launches.
    /// The walk normally stops long before: it never revisits a pair this run
    /// already buried, so a two-model plan on one harness gives up after two.
    /// The number exists for the case the product bound cannot cover — a model
    /// that dies *after* the session started, where a fresh model can die the
    /// same way — and it is deliberately small because every attempt is a
    /// process spawn against a service that has just said no.
    pub max_model_attempts: usize,
    /// Spawn the listening runtime's ticker and signal worker at all. `false`
    /// leaves the listening records and their queue exactly where they are
    /// and drives them only through [`crate::Engine::tick_listeners_at`] /
    /// [`crate::Engine::drain_signals`] — which is what the tests use, and
    /// what a one-shot CLI command wants. The ear runs either way: a run's
    /// waits and boundary events are the run's. A person's switch is the
    /// `events.enabled` setting, read live by a spawned runtime; its tick is
    /// `events.tick_secs`.
    pub events_enabled: bool,
    /// How often the wait ticker looks for a `delay`, a moment, a `schedule`
    /// or a boundary timer that has come due (seconds). Its own clock, always
    /// running: a run's timers do not depend on listening being on.
    pub wait_tick_secs: u64,
    /// How often the signal worker looks for queued signals (seconds).
    pub signal_poll_secs: u64,
    /// Wall-clock ceiling for one signal's dispatch.
    ///
    /// **300 s.** Dispatching a signal is a local operation — render the
    /// inputs, find or create the goal, start the run — and everything the
    /// run then does (agent steps, checks, waits) has budgets of its own
    /// rather than blocking the dispatch. Five minutes is generous and, being
    /// well under [`Self::signal_stale_secs`], guarantees a live dispatch is
    /// never mistaken for a dead one.
    pub signal_action_timeout_secs: u64,
    /// How long a `running` signal may sit before crash recovery returns it
    /// to the queue.
    ///
    /// **900 s**, deliberately 3× [`Self::signal_action_timeout_secs`]: a
    /// dispatch that is still alive cannot reach this age, so a requeue is
    /// proof the worker that held it is gone. Shorter would risk running an
    /// action twice concurrently (at-least-once turning into
    /// at-least-twice-at-once); much longer would leave a crashed node's work
    /// undone for no gain, since the sweep is cheap.
    pub signal_stale_secs: u64,
    /// Drive guided goals automatically: the Workflow Agent designs a
    /// workflow for a guided goal captured without one, and proposes the
    /// repair when a run fails.
    ///
    /// **Which** agent drives them is not configurable: it is
    /// `workflow-agent`, one of the two core agents that hold a record in every workspace.
    /// There used to be a `guided_agent` knob whose default id named an agent
    /// that was never seeded, so the lookup silently fell through to a
    /// hardcoded prompt with no model plan. This flag keeps only the question
    /// a node genuinely answers differently: whether it drives guided goals
    /// at all, or just runs workers.
    pub design_enabled: bool,
    /// Wall-clock ceiling for one guided wake.
    pub guided_wake_timeout_secs: u64,
    /// Wall-clock ceiling for one `check` step: the command it runs, or the
    /// schema validation. **120 s** — a check is a judgement, not the work,
    /// and one that hangs is a failed check rather than a stalled run.
    pub check_timeout_secs: u64,
    /// Wall-clock ceiling for one `connector` step: the whole call, retries
    /// included. **60 s** — an outside platform that has not answered in a
    /// minute is not going to, and a step that waits on it holds the run for
    /// nothing; the step fails with the timeout and `retries` may try again.
    pub connector_timeout_secs: u64,
    /// The code_hosts this engine talks to. `None` is the real set —
    /// GitHub, with the credential chain rooted at the workspace's `identity/`
    /// directory; a test hands in an in-memory code host so the pull-request path
    /// is exercised without a network.
    pub code_hosts: Option<bisa_codehost::CodeHostRegistry>,
    /// The `git` the engine reads identities and commits with. `None` is the
    /// process's git; a test hands in a handle carrying `GIT_CONFIG_GLOBAL`
    /// and `GIT_CONFIG_NOSYSTEM` so the developer's own identity cannot reach
    /// the engine and "nobody is set to commit" can be produced on purpose.
    pub git: Option<bisa_vcs::Git>,
    /// SSH for git hosts (ide/04 §Profiles by organization, Settings › Git &
    /// GitHub › SSH keys): the person's `~/.ssh` public material and the
    /// OpenSSH programs. **`None` by default**, so an engine nobody configured
    /// — every test fixture — answers `SshUnavailable` and never spawns `ssh`
    /// against a developer's directory; the node that serves a person hands in
    /// `Ssh::new(Cli, ~/.ssh, home)`, a test a `FakeSsh` on a temp dir.
    pub ssh: Option<bisa_ssh::Ssh>,
    /// The runner the code host CLIs (`gh`, `glab`) are asked through
    /// (ide/08 §CLI first). **`None` by default**, like `ssh`: an engine nobody
    /// configured has no CLI layer and never spawns a program against the
    /// developer's machine — every test fixture; the node that serves a person
    /// hands in `Cli::default()`, the real programs on `PATH`, and a test that
    /// wants the CLI path hands in a `FakeCli`.
    pub cli: Option<std::sync::Arc<dyn bisa_codehost::cli::CliRunner>>,
    /// The mobile tools on this machine (ide/19): Flutter, the simulators,
    /// `adb` and the emulators, probed and driven through `bisa-mobile-development`.
    /// **`None` by default**, like `ssh` and `cli`: an engine nobody
    /// configured answers that the mobile tools are not available and never
    /// spawns a program against the developer's machine — every test
    /// fixture; the node that serves a person hands in `Real::default()`,
    /// a test that wants the mobile path hands in a `FakeMobileDevelopment`.
    pub mobile_development:
        Option<std::sync::Arc<dyn bisa_mobile_development::MobileDevelopmentTools>>,
    /// What dials an installed MCP server to check it (06 § MCP servers):
    /// `None` is the real client (`bisa_mcp_probe::RmcpProbe`) — a probe
    /// runs only when a person asks for one, so the default spawns nothing
    /// on its own; a test hands in a `FakeProbe`.
    pub mcp_probe: Option<std::sync::Arc<dyn bisa_mcp_probe::McpProbe>>,
    /// The process's diagnostic log (`bisa-log`), when the process
    /// installed one: the engine applies the machine's `logging.*` settings
    /// to it at start and on every write of one. **`None` by default** —
    /// every test fixture — and the engine then reads the settings for
    /// nothing; the `node` command hands in the handle it installed.
    pub log: Option<bisa_log::Handle>,
    /// The clients every crate reaches the network through
    /// (`bisa-http`), built from the `network.*` settings. **`None` by
    /// default** — the engine then builds its own from the workspace's
    /// settings; the node's CLI hands in the handle it gave the adapters, so
    /// a harness's usage read and the engine's code host calls share one
    /// policy, swapped together on a write.
    pub http: Option<std::sync::Arc<bisa_http::Clients>>,
    /// Where the latest release of the platform is read from, for the
    /// desktop's *You › Update* (`updates.rs`). **`None` by default**, like
    /// `ssh` and `cli`: an engine nobody configured — every test fixture —
    /// never dials GitHub and answers *off*; the node that serves a person
    /// hands in the repository this build was made from, a test a stub on
    /// the loopback.
    pub updates: Option<crate::updates::UpdatesSource>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            global_concurrency: 4,
            per_adapter_concurrency: 2,
            disabled_harnesses: vec![],
            default_wall_clock_secs: 7200,
            idle_ttl: Duration::from_secs(300),
            retain_ended_secs: 60,
            socket_path: None,
            max_result_attempts: 3,
            max_clarify_rounds: 3,
            max_model_attempts: 6,
            events_enabled: true,
            wait_tick_secs: 5,
            signal_poll_secs: 2,
            signal_action_timeout_secs: 300,
            signal_stale_secs: 900,
            design_enabled: true,
            guided_wake_timeout_secs: 600,
            check_timeout_secs: 120,
            connector_timeout_secs: 60,
            code_hosts: None,
            git: None,
            ssh: None,
            cli: None,
            mobile_development: None,
            mcp_probe: None,
            log: None,
            http: None,
            updates: None,
        }
    }
}
