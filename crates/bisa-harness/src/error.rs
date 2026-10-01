//! Error taxonomy for the harness layer.

/// Errors returned by adapter/session methods.
///
/// # Two chains, two errors — the distinction this taxonomy exists for
///
/// Two variants tell the caller to *walk a chain* rather than fail the work,
/// and they are **not** interchangeable:
///
/// | Variant | What is broken | Which chain to walk |
/// |---|---|---|
/// | [`HarnessError::Unavailable`] | the **harness** cannot serve this host at all (binary missing, not logged in, version too old) | the work-item's `harness_candidates` — try Codex instead of Claude Code |
/// | [`HarnessError::ModelUnavailable`] | the harness is fine; **this model** is not (quota exhausted, rate-limited, retired id, org has no access) | the agent's [`crate::ModelPlan`] — try opus-5 instead of fable-5, same harness |
///
/// Collapsing them loses real information in both directions. Walking the
/// harness chain on a quota wall abandons a perfectly good harness and burns
/// the fallback harness's quota too; walking the model chain on a missing
/// binary retries the same absent program once per model. And treating either
/// as a plain failure — which is what shipped before this taxonomy — marks
/// finished work as failed: *"You've reached your Fable 5 limit"* arrived as
/// `Outcome::Failed { error }`, byte-identical in shape to a genuine defect,
/// and killed two agents mid-edit.
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    /// Fall back to the next candidate **harness**.
    #[error("harness unavailable: {0}")]
    Unavailable(String),
    /// Fall back to the next **model** in the agent's plan. The harness works;
    /// it will not run this model right now.
    ///
    /// `retry_after` is seconds, and only ever present when the harness said
    /// so itself (a `retry-after` header, a "try again in 12 minutes"). It is
    /// never guessed here — the engine's cooldown policy owns the guess.
    #[error("model unavailable: {model} ({reason})")]
    ModelUnavailable {
        model: String,
        reason: String,
        retry_after: Option<u64>,
    },
    /// The session is mid-operation and this call conflicts (e.g. `prompt`
    /// while a turn is streaming — use `steer`/`follow_up`).
    #[error("session busy")]
    Busy,
    /// The adapter does not advertise the capability this call requires.
    #[error("capability not supported: {0}")]
    NotSupported(&'static str),
    /// The harness broke its wire contract (bad frame, unexpected message).
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// The underlying process is gone; the session can only be disposed
    /// (or re-attached via its resume token, if the adapter supports RESUME).
    #[error("session terminated")]
    Terminated,
}

impl HarnessError {
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self::Unavailable(msg.into())
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(msg.into())
    }

    /// Build a [`HarnessError::ModelUnavailable`].
    pub fn model_unavailable(
        model: impl Into<String>,
        reason: impl Into<String>,
        retry_after: Option<u64>,
    ) -> Self {
        Self::ModelUnavailable {
            model: model.into(),
            reason: reason.into(),
            retry_after,
        }
    }

    /// Walk the **harness** chain.
    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    /// Walk the **model** chain. See the type docs: this is not
    /// [`HarnessError::is_unavailable`], and the two must not be merged.
    pub const fn is_model_unavailable(&self) -> bool {
        matches!(self, Self::ModelUnavailable { .. })
    }
}
