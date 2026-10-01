//! What the webview tells the menu bar icon, and how the icon paints it.
//!
//! The words are the webview's (`shell/trayModel.mjs`): it has the facts —
//! the connection, the Inbox, the sessions, the engine's pause — and one
//! vocabulary for every surface. This file only paints: a state is a dot's
//! colour or none, a count is the title beside the mark or none, and the
//! tooltip is the sentence and the count together.
//!
//! The colours are the operating system's own — the menu bar is the OS's
//! surface, not a theme's — so the dot reads like a native status light:
//! amber for something owed, green for work under way, red for a node that
//! cannot be reached, grey for a platform that is not doing anything yet or
//! on purpose. Calm is no dot at all: the mark alone means connected, idle
//! and nothing owed.

use serde::{Deserialize, Serialize};

/// One colour, as the icon paints it.
pub type Rgb = [u8; 3];

/// Something needs you.
pub const AMBER: Rgb = [255, 159, 10];
/// Agents are working.
pub const GREEN: Rgb = [52, 199, 89];
/// The node is unreachable.
pub const RED: Rgb = [255, 59, 48];
/// Connecting, or paused: nothing runs, by circumstance or by choice.
pub const GREY: Rgb = [142, 142, 147];

/// What the platform is doing, ranked by the webview: trouble first, then
/// what is owed, then work, then the two kinds of stillness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayState {
    /// The node has not answered yet.
    Connecting,
    /// The node answered once and cannot be reached now.
    Trouble,
    /// Something needs the person — an ask, a gate, a harness at its prompt, a join.
    Waiting,
    /// An agent is mid-turn or a harness session runs.
    Working,
    /// The engine is paused: nothing new starts.
    Paused,
    /// Connected, idle, nothing owed.
    Quiet,
}

impl TrayState {
    /// The dot this state wears, or none for calm.
    pub fn dot(self) -> Option<Rgb> {
        match self {
            TrayState::Waiting => Some(AMBER),
            TrayState::Working => Some(GREEN),
            TrayState::Trouble => Some(RED),
            TrayState::Connecting | TrayState::Paused => Some(GREY),
            TrayState::Quiet => None,
        }
    }
}

/// The webview's report, whole: the state, how many things need the person,
/// and the two sentences the menu shows for them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayReport {
    pub state: TrayState,
    /// Rows of the Inbox that need the person (`sidebarModel.inboxBadge(...).needs`).
    pub needs: u32,
    /// What the platform is doing, in one sentence.
    pub status: String,
    /// The count in words — *3 need you*, *Nothing needs you*.
    pub needs_words: String,
}

impl TrayReport {
    /// The text beside the mark: the count while something is owed, else nothing.
    pub fn title(&self) -> Option<String> {
        (self.needs > 0).then(|| self.needs.to_string())
    }

    /// The sentence on hover: the app's name, the status, and the count in words when owed.
    pub fn tooltip(&self) -> String {
        if self.needs > 0 {
            format!("Bisa — {} · {}", self.status, self.needs_words)
        } else {
            format!("Bisa — {}", self.status)
        }
    }

    /// Whether the needs line in the menu is a door: only while something is owed.
    pub fn needs_is_a_door(&self) -> bool {
        self.needs > 0
    }
}

impl Default for TrayReport {
    /// What the icon says before the webview has said anything.
    fn default() -> Self {
        Self {
            state: TrayState::Connecting,
            needs: 0,
            status: "Connecting to the node…".to_string(),
            needs_words: "Nothing needs you".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(state: TrayState, needs: u32) -> TrayReport {
        TrayReport {
            state,
            needs,
            status: "Working — 2 agents".to_string(),
            needs_words: if needs == 1 {
                "1 needs you".to_string()
            } else {
                format!("{needs} need you")
            },
        }
    }

    #[test]
    fn a_state_is_one_dot_or_none_and_calm_is_none() {
        assert_eq!(TrayState::Waiting.dot(), Some(AMBER));
        assert_eq!(TrayState::Working.dot(), Some(GREEN));
        assert_eq!(TrayState::Trouble.dot(), Some(RED));
        assert_eq!(TrayState::Connecting.dot(), Some(GREY));
        assert_eq!(TrayState::Paused.dot(), Some(GREY));
        assert_eq!(TrayState::Quiet.dot(), None, "the mark alone means calm");
    }

    #[test]
    fn the_title_is_the_count_while_owed_and_the_tooltip_carries_the_words() {
        let calm = report(TrayState::Working, 0);
        assert_eq!(calm.title(), None);
        assert_eq!(calm.tooltip(), "Bisa — Working — 2 agents");
        assert!(!calm.needs_is_a_door());
        let owed = report(TrayState::Waiting, 3);
        assert_eq!(owed.title(), Some("3".to_string()));
        assert_eq!(owed.tooltip(), "Bisa — Working — 2 agents · 3 need you");
        assert!(owed.needs_is_a_door());
    }

    #[test]
    fn the_first_word_is_connecting_and_the_wire_is_snake_case() {
        let first = TrayReport::default();
        assert_eq!(first.state, TrayState::Connecting);
        assert_eq!(first.title(), None);
        let json = serde_json::to_string(&report(TrayState::Trouble, 1)).unwrap();
        assert!(json.contains(r#""state":"trouble""#), "{json}");
        assert!(json.contains(r#""needs_words":"1 needs you""#), "{json}");
        let back: TrayReport =
            serde_json::from_str(r#"{"state":"quiet","needs":0,"status":"Quiet — nothing running","needs_words":"Nothing needs you"}"#)
                .unwrap();
        assert_eq!(back.state, TrayState::Quiet);
    }
}
