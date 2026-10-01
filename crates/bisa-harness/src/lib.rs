//! The harness abstraction layer.
//!
//! Bisa orchestrates *foreign* coding harnesses (Claude Code, Codex,
//! pi, omp, opencode, GitHub Copilot CLI, Grok Build, anything ACP-speaking)
//! behind two traits:
//! [`HarnessAdapter`] (how to find, probe, launch, and re-attach a harness)
//! and [`HarnessSession`] (one running session: prompt/steer/abort, and a
//! three-tier event stream). Concrete adapters live in `bisa-adapters`;
//! this crate owns the contracts, the event vocabulary, the shared subprocess
//! plumbing, and the three-tier harness catalog.
//!
//! Contract rules (ported from the pi/omp research):
//! - **Snapshots are authoritative, progress is advisory.** A
//!   [`SessionSnapshot`] carries a monotonic `revision`; [`SessionEvent`]s
//!   are transient hints and must never be reduced into authoritative state.
//! - **Errors are events, not panics.** A failing session emits
//!   `Lifecycle(Ended { outcome: Failed, .. })`; trait methods return
//!   [`HarnessError`] for caller mistakes (Busy, NotSupported, ...).
//! - **`Unavailable` means walk the fallback chain** — the same two-phase
//!   discipline as `bisa-iso`.
//! - **A dead model is not dead work.** [`HarnessError::ModelUnavailable`]
//!   and [`event::Outcome::ModelUnavailable`] say "this *model* will not run
//!   right now"; the caller walks the agent's [`ModelPlan`] instead of
//!   failing the work-item. See [`error::HarnessError`] for why that is a
//!   different chain from `Unavailable`.

pub mod catalog;
pub mod error;
pub mod error_text;
pub mod event;
pub mod install;
pub mod proc;
pub mod skills;
pub mod traits;
pub mod types;
pub mod usage;

#[cfg(any(test, feature = "mock"))]
pub mod mock;

pub use bisa_core::{McpMount, McpProvenance};
pub use catalog::{
    CustomHarnessSpec, HarnessCatalog, HarnessListing, HarnessTier, PRESET_HARNESSES,
};
pub use error::HarnessError;
pub use event::{
    InputAnswer, InputKind, InputRequest, LifecycleEvent, Outcome, ProgressEvent, SessionEvent,
    SubagentId,
};
pub use traits::{BoxEventStream, HarnessAdapter, HarnessSession};
pub use types::{
    AllHealthy, Attachment, Effort, EffortChoice, InteractiveLaunch, McpServerConfig, ModelChoice,
    ModelHealthView, ModelInfo, ModelPlan, ModelStrategy, Phase, ProbeResult, PromptInput,
    ResumeToken, SessionCost, SessionSnapshot, SessionSpec, SkillPayload, Steer,
};
pub use types::{LaunchFile, PullSource, ReportingContext, ReportingPlan};
pub use usage::{UsageAccount, UsageReport, UsageSource, UsageState, UsageWindow};
