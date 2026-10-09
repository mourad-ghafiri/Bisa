//! What a boot says while it works.
//!
//! Opening a workspace can take a while — an index rebuilt after an upgrade
//! or a damaged file replays every journal — and a process that waits on
//! the open (the desktop's shell, a person at a terminal) needs to know the
//! node is working, not wedged. The store says so through this observer; who
//! listens, and how the words reach a screen, is the caller's.

/// One phase of a boot, as the store and the engine pass through it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootPhase {
    /// The workspace's files are being opened and checked.
    OpeningWorkspace,
    /// The index is being rebuilt from the files: `done` of `of` goals
    /// walked so far. Said at the start (`0 of n`), along the way, and at
    /// the end (`n of n`).
    RebuildingIndex { done: usize, of: usize },
    /// The workspace is open; the engine is picking up what was running.
    StartingEngine,
}

impl BootPhase {
    /// The phase's one word, for a log line or a wire field.
    pub fn name(self) -> &'static str {
        match self {
            BootPhase::OpeningWorkspace => "opening_workspace",
            BootPhase::RebuildingIndex { .. } => "rebuilding_index",
            BootPhase::StartingEngine => "starting_engine",
        }
    }
}

/// Who hears the phases of a boot.
pub trait BootObserver: Send + Sync {
    fn phase(&self, phase: BootPhase);
}

/// An observer that hears nothing — every open that nobody waits on.
pub struct Quiet;

impl BootObserver for Quiet {
    fn phase(&self, _phase: BootPhase) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phase_has_its_word_and_the_quiet_observer_hears_nothing() {
        assert_eq!(BootPhase::OpeningWorkspace.name(), "opening_workspace");
        assert_eq!(
            BootPhase::RebuildingIndex { done: 1, of: 3 }.name(),
            "rebuilding_index"
        );
        assert_eq!(BootPhase::StartingEngine.name(), "starting_engine");
        Quiet.phase(BootPhase::StartingEngine);
    }
}
