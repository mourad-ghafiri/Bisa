//! Value types shared across the harness boundary.

use bisa_core::{McpMount, ToolTier, WorkItemId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Host-level probe outcome for an adapter (mirrors `bisa-iso`'s
/// two-phase discipline: available here does not guarantee `launch` succeeds
/// for a specific spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Version string reported by the binary, when obtainable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl ProbeResult {
    pub fn available(version: Option<String>) -> Self {
        Self {
            available: true,
            reason: None,
            version,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
            version: None,
        }
    }
}

/// How to run a harness **interactively**, in a terminal a person is sitting at.
///
/// This is not the same thing as [`HarnessAdapter::launch`] and must never be
/// confused with it. Every adapter launches its binary in a *protocol* mode —
/// `claude -p --output-format stream-json`, `codex exec --json`,
/// `pi --mode rpc`, `opencode run --format json` — because the engine drives
/// those sessions and reads structured events back. Attaching a PTY to one of
/// those shows a person a stream of JSON. This is the other invocation: the
/// bare command, the one you would type yourself.
///
/// A harness that has no such form returns `None` rather than a guess, which is
/// why [`HarnessAdapter::interactive`] defaults to `None`. An ACP adapter's
/// whole purpose is to put a binary into protocol mode, and an A2A endpoint is
/// an HTTP URL with no local process at all.
///
/// **This describes a command; it never runs one.** It is served read-only over
/// the node's control plane so the desktop shell can offer what is installed,
/// and the shell is the only thing in the system that spawns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractiveLaunch {
    pub program: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// What continues the harness's most recent session **in the working
    /// directory it is started in** — `--continue`, `--resume`, whatever this
    /// tool calls it.
    ///
    /// Empty means the harness has no such form, and that is the honest answer
    /// for most of them rather than a guess: a flag invented here would be a
    /// command that fails in somebody's terminal. Nothing has to check for
    /// emptiness — appending nothing is opening fresh.
    ///
    /// **Directory scoping is the whole reason this is the harness's own flag
    /// rather than a session id.** These CLIs already file their history by
    /// working directory, and the shell is opened in the workstream, so "the
    /// latest session here" needs no bookkeeping on our side and cannot drift
    /// from what the tool itself believes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume_args: Vec<String>,
}

impl InteractiveLaunch {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            resume_args: Vec::new(),
        }
    }

    pub fn with_args(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            resume_args: Vec::new(),
        }
    }

    /// Name what continues this harness's latest session in the same directory.
    pub fn resumable_with(mut self, resume_args: Vec<String>) -> Self {
        self.resume_args = resume_args;
        self
    }
}

/// A file written for one interactive session before it launches — a settings
/// file, an extension — under the session's own run directory, never in a
/// project. Always private to the person (`0600`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFile {
    pub name: String,
    pub contents: String,
}

/// Where a harness's own events are read from, when its status is pulled
/// rather than reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PullSource {
    /// A server-sent event stream the harness serves itself.
    Sse { url: String },
}

/// What one interactive session needs in order to report what it is doing.
///
/// The engine asks the adapter for it once per launch
/// ([`crate::HarnessAdapter::interactive_reporting`]) and materialises it: the
/// files are written, the arguments are appended to the interactive command,
/// the pull source is read. The desktop shell that runs the command never
/// interprets a harness — it applies a plan. An empty plan means the harness
/// cannot say what it does, and it opens as a plain terminal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportingPlan {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<LaunchFile>,
    /// Appended to the interactive command, after its own arguments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// The embedded terminal should watch the harness's terminal
    /// notifications (OSC 9) for an approval prompt and report it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub intercept_approval_notifications: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<PullSource>,
}

impl ReportingPlan {
    /// Nothing to apply: the harness will not report.
    pub fn is_none(&self) -> bool {
        self.files.is_empty()
            && self.args.is_empty()
            && !self.intercept_approval_notifications
            && self.pull.is_none()
    }
}

/// What an adapter is told when asked how one session should report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportingContext {
    /// The session's handle, as the reporter will name it.
    pub session: String,
    /// The reporter's argv — the binary and its arguments — for a hook to run
    /// with the payload on stdin, or appended as one more argument.
    pub reporter: Vec<String>,
    /// Where the plan's files will be written; the adapter names them in its
    /// arguments as absolute paths under it.
    pub files_dir: PathBuf,
    /// A free loopback port, for a harness that serves its own events.
    pub port: Option<u16>,
    /// The guard's argv — the binary and its arguments — for a pre-execution
    /// hook that waits for the node's verdict; `None` when this machine does
    /// not guard terminal sessions.
    pub guard: Option<Vec<String>>,
    /// How long that hook may wait for the verdict before the harness's own
    /// prompt stands.
    pub guard_timeout_secs: u64,
}

/// Everything an adapter needs to launch one session. The orchestrator owns
/// identity (the session id is minted by the engine, not the harness); the
/// harness's own native id comes back inside [`ResumeToken`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSpec {
    /// The work-item this session executes, if any (None for ad-hoc runs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemId>,
    /// Working directory for the session.
    pub cwd: PathBuf,
    /// Optional launch prompt. Contract: when non-empty the adapter sends it
    /// as the first turn during `launch()`; when empty the adapter starts
    /// idle and the caller drives via `HarnessSession::prompt()`. Callers
    /// must not both set this and call `prompt()` for the same turn.
    pub prompt: String,
    /// Model override (harness-native model name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// How hard the model works, already resolved and clamped by the engine
    /// to a level this harness takes for this model: the adapter translates
    /// it into its own flag and never chooses one. `None` sends nothing — the
    /// harness runs at its own default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// MCP servers to inject into the session, each with where it came from
    /// (always includes the Bisa MCP server, a platform mount, when the
    /// adapter supports MCP_SERVERS). Only a platform mount's tools run
    /// without a judgement; an installed server's are judged like a command.
    #[serde(default)]
    pub mcp_servers: Vec<McpMount>,
    /// Extra environment for the harness process. Reserved `BISA_*`
    /// keys are stripped by the catalog/engine before reaching the adapter.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Names the harness process must not inherit — the proxy variables when
    /// the `network.proxy.mode` setting says `none`, or the pair the manual
    /// proxy leaves unnamed. Applied after `env`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_remove: Vec<String>,
    /// Highest tool tier the session may use without a gate decision.
    pub tier_ceiling: ToolTier,
    /// JSON Schema for the expected structured result, if contracted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Agent skills, resolved from the workspace's skill library and
    /// materialized at launch into the harness-native skills dir (or appended
    /// to the instructions) by [`crate::skills::materialize`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<SkillPayload>,
}

/// One skill, resolved and ready to deliver to a session.
///
/// The description is carried rather than scraped from the markdown: it is
/// what a model reads to decide whether to open the skill at all, so guessing
/// it from the first non-heading line — which is what this used to do — put a
/// sentence fragment in the one field that has to be a sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPayload {
    /// The library id. Becomes the skill directory name.
    pub id: String,
    /// Human title, used as the prompt-appendix heading.
    pub name: String,
    /// One line: when to reach for this skill.
    pub description: String,
    pub markdown: String,
}

/// A model a harness can run, as discovered from the harness itself.
/// An empty discovery result means "unknown" — never "unsupported"; callers
/// keep free-text entry available either way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Harness-native model id (what `SessionSpec.model` accepts).
    pub id: String,
    /// Human-facing label, when the harness distinguishes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The efforts this model takes on this harness, lowest first — what a
    /// picker offers beside it. Empty means the harness has no effort
    /// control for it, or learns the levels only from a live session.
    #[serde(default)]
    pub efforts: Vec<Effort>,
}

impl ModelInfo {
    /// A listed model and the efforts it takes.
    pub fn new(id: impl Into<String>, label: Option<&str>, efforts: Vec<Effort>) -> Self {
        Self {
            id: id.into(),
            label: label.map(str::to_string),
            efforts,
        }
    }
}

// ---------------------------------------------------------------------------
// Model plans and MCP transports
// ---------------------------------------------------------------------------

// Domain types, owned by the core and used here verbatim: a plan on an agent
// definition is the plan a launch walks, with nothing translated between the
// two. Re-exported so adapter code keeps one import path.
pub use bisa_core::{
    AllHealthy, Effort, EffortChoice, McpServerConfig, ModelChoice, ModelHealthView, ModelPlan,
    ModelStrategy,
};

/// Harness-native handle for re-attaching to a session later
/// (idle→parked→revived lifecycle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeToken {
    /// Which adapter minted this token.
    pub adapter_id: String,
    /// The harness's own session identifier (e.g. Claude Code session id,
    /// pi session path).
    pub native_id: String,
    /// Where the session was running. Required, and not derived on the way
    /// back in.
    ///
    /// Attaching used to resolve this from whatever was to hand — a
    /// transcript's parent directory, the daemon's own working directory, or
    /// `/` when even that failed — so a revived session ran somewhere nobody
    /// chose. The transcript's parent is the subtler of the two: it looks like
    /// a derivation and is not one, because a Claude Code transcript lives
    /// under the harness's own data directory rather than beside the work.
    ///
    /// A session that cannot say where it ran cannot be resumed, and that is a
    /// refusal rather than a guess.
    pub cwd: PathBuf,
    /// Local transcript path, when the adapter mirrors one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<PathBuf>,
    /// The model the session ran on, as it was launched. A revived session
    /// runs it again: a harness keeps a conversation, not the flags it was
    /// started with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The effort the session ran at, as it was launched — sent again when
    /// the session is revived, for the same reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
}

impl ResumeToken {
    /// A token for a session launched from `spec`: where it ran, and the
    /// model and the effort it ran with.
    pub fn of(
        adapter_id: impl Into<String>,
        native_id: impl Into<String>,
        spec: &SessionSpec,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            native_id: native_id.into(),
            cwd: spec.cwd.clone(),
            transcript_path: None,
            model: spec.model.clone(),
            effort: spec.effort,
        }
    }
}

/// Coarse phase of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Starting,
    Idle,
    /// A turn is streaming.
    Turn,
    /// Blocked on an [`crate::InputRequest`] the engine has not answered yet
    /// (INPUT_REQUESTS harnesses).
    AwaitingInput,
    /// Interrupted (crash/park) but resumable.
    Suspended,
    Ended,
}

/// Accumulated cost for a session.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct SessionCost {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub usd_cents: u64,
}

/// Authoritative session state. `revision` is monotonic; a snapshot with a
/// lower revision than one already seen MUST be discarded by consumers.
/// Progress events never mutate this — they are advisory hints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub revision: u64,
    pub phase: Phase,
    /// Short human-readable gist of current/last activity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    pub cost: SessionCost,
}

/// A file a turn carries.
///
/// **A path, always — and bytes only for the adapters that can use them.**
/// Every harness can read a file it is given the path to: `Read`/`Grep`/`Glob`
/// are in claude-code's allowlist at every tier, and the other adapters apply
/// no tool restriction at all. So the path in the prompt text is the universal
/// mechanism and the floor.
///
/// The ceiling is [`HarnessCaps::IMAGE_INPUT`]: an adapter that declares it
/// gets images as images rather than as a path to one, which is the difference
/// between a model looking at a screenshot and a model reading a PNG's bytes.
/// Only claude-code declares it, because only claude-code's wire has a content
/// array to put a block in — codex and opencode pass the prompt on **argv**.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    /// Absolute, and inside the workspace's content-addressed store.
    pub path: std::path::PathBuf,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        self.mime.starts_with("image/")
    }
}

/// Input for `prompt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInput {
    pub text: String,
    /// Files the message being answered carried. Named in `text` as absolute
    /// paths regardless; carried natively only by an adapter that can.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

impl From<&str> for PromptInput {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            attachments: Vec::new(),
        }
    }
}

impl From<String> for PromptInput {
    fn from(text: String) -> Self {
        Self {
            text,
            attachments: Vec::new(),
        }
    }
}

/// Input for `steer` / `follow_up`. Two-queue semantics (pi model):
/// steering is delivered before the next model call of the running turn;
/// follow-ups are delivered only when the agent would otherwise stop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Steer {
    pub text: String,
    /// Files the message being answered carried.
    ///
    /// Here as well as on [`PromptInput`] because a live session takes a
    /// follow-up rather than a fresh prompt — so without this, the second
    /// image in a conversation would be the first one the agent could not see.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

impl From<&str> for Steer {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            attachments: Vec::new(),
        }
    }
}

impl From<String> for Steer {
    fn from(text: String) -> Self {
        Self {
            text,
            attachments: Vec::new(),
        }
    }
}

#[cfg(test)]
mod model_plan_tests {
    use super::*;
    use std::collections::HashMap;

    /// A health view built from literals — the whole point of keeping
    /// [`ModelPlan::order`] pure.
    #[derive(Default)]
    struct Fake {
        cooldowns: HashMap<String, u64>,
        busy: HashMap<String, u32>,
    }

    impl Fake {
        fn cooling(pairs: &[(&str, u64)]) -> Self {
            Self {
                cooldowns: pairs.iter().map(|(m, t)| ((*m).to_string(), *t)).collect(),
                busy: HashMap::new(),
            }
        }

        fn busy(pairs: &[(&str, u32)]) -> Self {
            Self {
                cooldowns: HashMap::new(),
                busy: pairs.iter().map(|(m, n)| ((*m).to_string(), *n)).collect(),
            }
        }
    }

    impl ModelHealthView for Fake {
        fn cooldown_until(&self, model: &str) -> Option<u64> {
            self.cooldowns.get(model).copied()
        }

        fn in_flight(&self, model: &str) -> u32 {
            self.busy.get(model).copied().unwrap_or(0)
        }
    }

    fn plan(strategy: ModelStrategy, models: &[(&str, u32, bool)]) -> ModelPlan {
        ModelPlan {
            strategy,
            effort: None,
            models: models
                .iter()
                .map(|(m, w, e)| ModelChoice {
                    model: (*m).to_string(),
                    weight: *w,
                    enabled: *e,
                    suited_for: None,
                    effort: None,
                })
                .collect(),
        }
    }

    // --- degenerate shapes ---------------------------------------------------

    #[test]
    fn empty_plan_yields_nothing() {
        let p = ModelPlan::default();
        assert!(p.order(&AllHealthy, 0, None).is_empty());
        assert_eq!(p.first(&AllHealthy, 0), None);
        assert!(p.is_empty());
    }

    #[test]
    fn all_disabled_yields_nothing() {
        for strategy in [
            ModelStrategy::Fallback,
            ModelStrategy::Weighted,
            ModelStrategy::RoundRobin,
            ModelStrategy::LeastBusy,
        ] {
            let p = plan(strategy, &[("a", 1, false), ("b", 5, false)]);
            assert!(
                p.order(&AllHealthy, 7, None).is_empty(),
                "{strategy:?} yielded a disabled model"
            );
        }
    }

    #[test]
    fn blank_ids_are_dropped() {
        let p = plan(ModelStrategy::Fallback, &[("", 1, true), ("  ", 1, true)]);
        assert!(p.order(&AllHealthy, 0, None).is_empty());
    }

    #[test]
    fn single_model_is_stable_under_every_strategy_and_rotation() {
        for strategy in [
            ModelStrategy::Fallback,
            ModelStrategy::Weighted,
            ModelStrategy::RoundRobin,
            ModelStrategy::LeastBusy,
        ] {
            let p = plan(strategy, &[("solo", 3, true)]);
            for rotation in 0..5 {
                assert_eq!(p.order(&AllHealthy, rotation, None), vec!["solo"]);
            }
        }
    }

    // --- fallback ------------------------------------------------------------

    #[test]
    fn fallback_is_plan_order_and_ignores_rotation() {
        let p = plan(
            ModelStrategy::Fallback,
            &[("a", 9, true), ("b", 1, true), ("c", 4, true)],
        );
        for rotation in 0..7 {
            assert_eq!(p.order(&AllHealthy, rotation, None), vec!["a", "b", "c"]);
        }
    }

    #[test]
    fn fallback_skips_disabled_but_keeps_order() {
        let p = plan(
            ModelStrategy::Fallback,
            &[("a", 1, true), ("b", 1, false), ("c", 1, true)],
        );
        assert_eq!(p.order(&AllHealthy, 0, None), vec!["a", "c"]);
    }

    // --- round robin ---------------------------------------------------------

    #[test]
    fn round_robin_rotates_exhaustively() {
        let p = plan(
            ModelStrategy::RoundRobin,
            &[("a", 1, true), ("b", 1, true), ("c", 1, true)],
        );
        assert_eq!(p.order(&AllHealthy, 0, None), vec!["a", "b", "c"]);
        assert_eq!(p.order(&AllHealthy, 1, None), vec!["b", "c", "a"]);
        assert_eq!(p.order(&AllHealthy, 2, None), vec!["c", "a", "b"]);
        // wraps, and keeps wrapping
        assert_eq!(p.order(&AllHealthy, 3, None), vec!["a", "b", "c"]);
        assert_eq!(p.order(&AllHealthy, 100, None), vec!["b", "c", "a"]);
    }

    #[test]
    fn round_robin_rotates_over_the_ready_band_only() {
        // `b` is cooling, so rotation 1 must land on `c`, not on `b`.
        let p = plan(
            ModelStrategy::RoundRobin,
            &[("a", 1, true), ("b", 1, true), ("c", 1, true)],
        );
        let health = Fake::cooling(&[("b", 500)]);
        assert_eq!(p.order(&health, 0, None), vec!["a", "c", "b"]);
        assert_eq!(p.order(&health, 1, None), vec!["c", "a", "b"]);
    }

    // --- weighted ------------------------------------------------------------

    #[test]
    fn weighted_head_follows_the_weights_across_rotations() {
        let p = plan(ModelStrategy::Weighted, &[("a", 3, true), ("b", 1, true)]);
        // total weight 4: three rotations out of four lead with `a`.
        let heads: Vec<&str> = (0..8).map(|r| p.order(&AllHealthy, r, None)[0]).collect();
        assert_eq!(
            heads,
            vec!["a", "a", "a", "b", "a", "a", "a", "b"],
            "weighted head distribution must be deterministic and proportional"
        );
    }

    #[test]
    fn weighted_tail_is_descending_weight_then_plan_order() {
        let p = plan(
            ModelStrategy::Weighted,
            &[("a", 1, true), ("b", 5, true), ("c", 5, true)],
        );
        // rotation 0 lands in `a`'s band (cumulative 0..1).
        assert_eq!(p.order(&AllHealthy, 0, None), vec!["a", "b", "c"]);
        // rotation 1 lands in `b`'s band (1..6); tail is c (5) then a (1).
        assert_eq!(p.order(&AllHealthy, 1, None), vec!["b", "c", "a"]);
        // rotation 6 lands in `c`'s band (6..11); tail is b (5) then a (1).
        assert_eq!(p.order(&AllHealthy, 6, None), vec!["c", "b", "a"]);
    }

    #[test]
    fn weighted_treats_zero_weight_as_one() {
        let p = plan(ModelStrategy::Weighted, &[("a", 0, true), ("b", 0, true)]);
        assert_eq!(p.order(&AllHealthy, 0, None), vec!["a", "b"]);
        assert_eq!(p.order(&AllHealthy, 1, None), vec!["b", "a"]);
    }

    // --- least busy ----------------------------------------------------------

    #[test]
    fn least_busy_sorts_by_in_flight_and_breaks_ties_by_plan_order() {
        let p = plan(
            ModelStrategy::LeastBusy,
            &[("a", 1, true), ("b", 1, true), ("c", 1, true)],
        );
        let health = Fake::busy(&[("a", 4), ("b", 0), ("c", 4)]);
        assert_eq!(p.order(&health, 0, None), vec!["b", "a", "c"]);
        // rotation must not perturb it
        assert_eq!(p.order(&health, 99, None), vec!["b", "a", "c"]);
    }

    // --- cooldown ------------------------------------------------------------

    #[test]
    fn cooling_models_sink_below_ready_ones() {
        let p = plan(
            ModelStrategy::Fallback,
            &[("a", 1, true), ("b", 1, true), ("c", 1, true)],
        );
        let health = Fake::cooling(&[("a", 900)]);
        assert_eq!(p.order(&health, 0, None), vec!["b", "c", "a"]);
    }

    #[test]
    fn all_in_cooldown_still_yields_soonest_expiring_first() {
        // The documented choice: never return an empty list just because
        // everything is throttled — that is the M5 failure mode.
        let p = plan(
            ModelStrategy::Fallback,
            &[("a", 1, true), ("b", 1, true), ("c", 1, true)],
        );
        let health = Fake::cooling(&[("a", 900), ("b", 100), ("c", 500)]);
        assert_eq!(p.order(&health, 0, None), vec!["b", "c", "a"]);
        assert_eq!(p.first(&health, 0), Some("b"));
    }

    #[test]
    fn equal_cooldown_deadlines_keep_plan_order() {
        let p = plan(
            ModelStrategy::Fallback,
            &[("a", 1, true), ("b", 1, true), ("c", 1, true)],
        );
        let health = Fake::cooling(&[("a", 100), ("b", 100), ("c", 100)]);
        assert_eq!(p.order(&health, 0, None), vec!["a", "b", "c"]);
    }

    #[test]
    fn a_disabled_model_stays_out_even_when_everything_else_is_cooling() {
        let p = plan(
            ModelStrategy::Fallback,
            &[("a", 1, true), ("off", 1, false)],
        );
        let health = Fake::cooling(&[("a", 100)]);
        assert_eq!(p.order(&health, 0, None), vec!["a"]);
    }

    #[test]
    fn cooldown_band_is_ordered_by_deadline_under_every_strategy() {
        for strategy in [
            ModelStrategy::Fallback,
            ModelStrategy::Weighted,
            ModelStrategy::RoundRobin,
            ModelStrategy::LeastBusy,
        ] {
            let p = plan(strategy, &[("a", 1, true), ("b", 1, true)]);
            let health = Fake::cooling(&[("a", 200), ("b", 50)]);
            assert_eq!(
                p.order(&health, 3, None),
                vec!["b", "a"],
                "{strategy:?} mis-ordered the cooldown band"
            );
        }
    }

    // --- constructors + serde ------------------------------------------------

    #[test]
    fn pinned_and_fallback_constructors() {
        assert_eq!(
            ModelPlan::pinned("x").order(&AllHealthy, 0, None),
            vec!["x"]
        );
        assert_eq!(
            ModelPlan::fallback(["x", "y"]).order(&AllHealthy, 0, None),
            vec!["x", "y"]
        );
    }

    #[test]
    fn serde_defaults_fill_weight_and_enabled() {
        let p: ModelPlan = serde_json::from_str(r#"{"models":[{"model":"x"}]}"#).unwrap();
        assert_eq!(p.strategy, ModelStrategy::Fallback);
        assert_eq!(p.models[0].weight, 1);
        assert!(p.models[0].enabled);
        let round: ModelPlan = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(round, p);
    }

    #[test]
    fn strategy_wire_names_are_snake_case() {
        let json = serde_json::to_string(&ModelStrategy::RoundRobin).unwrap();
        assert_eq!(json, "\"round_robin\"");
    }
}

#[cfg(test)]
mod effort_tests {
    use super::*;

    fn spec() -> SessionSpec {
        SessionSpec {
            work_item: None,
            cwd: PathBuf::from("/work/here"),
            prompt: String::new(),
            model: None,
            effort: None,
            mcp_servers: vec![],
            env: BTreeMap::new(),
            env_remove: vec![],
            tier_ceiling: ToolTier::Write,
            output_schema: None,
            skills: vec![],
        }
    }

    #[test]
    fn a_spec_carries_an_effort_only_when_one_was_resolved() {
        let bare = serde_json::to_value(spec()).unwrap();
        assert!(bare.get("effort").is_none());
        assert!(bare.get("model").is_none());
        let asked = SessionSpec {
            model: Some("claude-opus-5-5[1m]".into()),
            effort: Some(Effort::Xhigh),
            ..spec()
        };
        let wire = serde_json::to_value(&asked).unwrap();
        assert_eq!(wire["effort"], "xhigh");
        let back: SessionSpec = serde_json::from_value(wire).unwrap();
        assert_eq!(back.effort, Some(Effort::Xhigh));
        assert_eq!(back.model.as_deref(), Some("claude-opus-5-5[1m]"));
        // `auto` is what somebody asks for, never what a session is sent.
        let mut auto = serde_json::to_value(spec()).unwrap();
        auto["effort"] = serde_json::json!("auto");
        assert!(serde_json::from_value::<SessionSpec>(auto).is_err());
    }

    #[test]
    fn a_listed_model_always_says_its_efforts_even_when_it_has_none() {
        let none = ModelInfo::new("claude-haiku-4-5", Some("Claude Haiku 4.5"), vec![]);
        let wire = serde_json::to_value(&none).unwrap();
        assert_eq!(wire["efforts"], serde_json::json!([]));
        let some = ModelInfo::new("opus", None, vec![Effort::Low, Effort::Xhigh, Effort::Max]);
        let wire = serde_json::to_value(&some).unwrap();
        assert_eq!(wire["efforts"], serde_json::json!(["low", "xhigh", "max"]));
        assert!(wire.get("label").is_none());
        assert_eq!(serde_json::from_value::<ModelInfo>(wire).unwrap(), some);
        // A listing written before efforts were known reads as none.
        let old: ModelInfo = serde_json::from_str(r#"{"id":"x"}"#).unwrap();
        assert!(old.efforts.is_empty());
    }

    #[test]
    fn a_resume_token_remembers_the_model_and_the_effort_it_ran_with() {
        let launched = SessionSpec {
            model: Some("claude-sonnet-5-5[1m]".into()),
            effort: Some(Effort::High),
            ..spec()
        };
        let token = ResumeToken::of("claude-code", "sess-1", &launched);
        assert_eq!(token.adapter_id, "claude-code");
        assert_eq!(token.native_id, "sess-1");
        assert_eq!(token.cwd, PathBuf::from("/work/here"));
        assert_eq!(token.transcript_path, None);
        assert_eq!(token.model.as_deref(), Some("claude-sonnet-5-5[1m]"));
        assert_eq!(token.effort, Some(Effort::High));
        let wire = serde_json::to_value(&token).unwrap();
        assert_eq!(wire["model"], "claude-sonnet-5-5[1m]");
        assert_eq!(wire["effort"], "high");
        assert_eq!(serde_json::from_value::<ResumeToken>(wire).unwrap(), token);

        // A session launched with neither says neither, and a token stored
        // before either was kept still reads.
        let plain = ResumeToken::of("pi", "s", &spec());
        let wire = serde_json::to_value(&plain).unwrap();
        assert!(wire.get("model").is_none());
        assert!(wire.get("effort").is_none());
        let stored: ResumeToken =
            serde_json::from_str(r#"{"adapter_id":"pi","native_id":"s","cwd":"/w"}"#).unwrap();
        assert_eq!(stored.model, None);
        assert_eq!(stored.effort, None);
    }
}
