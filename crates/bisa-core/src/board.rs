//! The Board: every workstream a card in one of five columns, a rank within
//! the column, a due date — a *view* over workstreams, kept on the record
//! beside the name, the note and the pin (ide/16).
//!
//! **A column is not the state.** A workstream's lifecycle (`open`, `dirty`,
//! `committed`, …, `closed`) moves only through a `WorkstreamTransition`
//! (invariant I42); a card's column is where a person put it. Until a person
//! places a card, the column *follows* the lifecycle by [`BoardColumn::for_state`];
//! once placed, the choice stands — except a closed workstream, which is
//! always Archived: it is terminal and its checkout may be gone.
//!
//! Ranks are integers with room between them ([`RANK_STEP`]): a move takes
//! the midpoint of its neighbours, and when two neighbours touch the column
//! is renumbered ([`rank_at`]). The desktop never does that arithmetic — it
//! sends an index, the engine asks here.

use crate::workstream::WorkstreamState;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The five columns, in the order the Board shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BoardColumn {
    Backlog,
    Todo,
    Doing,
    Done,
    Archived,
}

impl BoardColumn {
    pub const ALL: [BoardColumn; 5] = [
        BoardColumn::Backlog,
        BoardColumn::Todo,
        BoardColumn::Doing,
        BoardColumn::Done,
        BoardColumn::Archived,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            BoardColumn::Backlog => "backlog",
            BoardColumn::Todo => "todo",
            BoardColumn::Doing => "doing",
            BoardColumn::Done => "done",
            BoardColumn::Archived => "archived",
        }
    }

    /// Where an **unplaced** card sits: the lifecycle read as a column. Work
    /// nobody touched is Backlog; anything with a change, a commit, a push or
    /// a pull request is Doing; merged is Done; closed is Archived.
    pub fn for_state(state: &WorkstreamState) -> BoardColumn {
        match state {
            WorkstreamState::Open => BoardColumn::Backlog,
            WorkstreamState::Dirty
            | WorkstreamState::Committed
            | WorkstreamState::Pushed
            | WorkstreamState::PrOpen { .. } => BoardColumn::Doing,
            WorkstreamState::Merged { .. } => BoardColumn::Done,
            WorkstreamState::Closed => BoardColumn::Archived,
        }
    }

    /// The column a card is shown in: the person's choice when there is one,
    /// else the lifecycle's — and Archived for a closed workstream whatever
    /// was chosen.
    pub fn shown(chosen: Option<BoardColumn>, state: &WorkstreamState) -> BoardColumn {
        if matches!(state, WorkstreamState::Closed) {
            return BoardColumn::Archived;
        }
        chosen.unwrap_or_else(|| BoardColumn::for_state(state))
    }
}

impl fmt::Display for BoardColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BoardColumn {
    type Err = BoardError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BoardColumn::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| BoardError::UnknownColumn(s.to_string()))
    }
}

/// A calendar day, `YYYY-MM-DD` — a due date is a day, not an instant, so it
/// carries no time zone and reads the same on every machine.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct DueDate(String);

impl DueDate {
    /// Accept exactly `YYYY-MM-DD` naming a real day of the calendar.
    pub fn parse(s: &str) -> Result<DueDate, BoardError> {
        let bytes = s.as_bytes();
        let shaped = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| (i == 4 || i == 7) || b.is_ascii_digit());
        if !shaped {
            return Err(BoardError::BadDate(s.to_string()));
        }
        let year: u32 = s[0..4]
            .parse()
            .map_err(|_| BoardError::BadDate(s.to_string()))?;
        let month: u32 = s[5..7]
            .parse()
            .map_err(|_| BoardError::BadDate(s.to_string()))?;
        let day: u32 = s[8..10]
            .parse()
            .map_err(|_| BoardError::BadDate(s.to_string()))?;
        if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return Err(BoardError::BadDate(s.to_string()));
        }
        Ok(DueDate(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap {
                29
            } else {
                28
            }
        }
    }
}

impl TryFrom<String> for DueDate {
    type Error = BoardError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        DueDate::parse(&s)
    }
}

impl From<DueDate> for String {
    fn from(d: DueDate) -> String {
        d.0
    }
}

impl fmt::Display for DueDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A workstream's place on the Board — nothing until a person sets something.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamBoard {
    /// The column the person chose; `None` follows the lifecycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<BoardColumn>,
    /// The order within the column, meaningful only beside other ranks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<DueDate>,
}

impl WorkstreamBoard {
    pub fn is_empty(&self) -> bool {
        self.column.is_none() && self.rank.is_none() && self.due.is_none()
    }
}

/// The room left between two ranks when a column is numbered afresh.
pub const RANK_STEP: u32 = 1024;

/// Where a moved card's rank comes from: a number between its neighbours, or
/// the whole column renumbered when the neighbours touch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placement {
    /// The moved card takes this rank; nothing else moves.
    Rank(u32),
    /// The column, with the moved card already inserted at its index, takes
    /// these ranks in order.
    Renumber(Vec<u32>),
}

/// The rank for a card dropped at `index` among `ranks` — the column's
/// other cards, ascending, the moved card not among them. Past the end
/// means last.
pub fn rank_at(ranks: &[u32], index: usize) -> Placement {
    let index = index.min(ranks.len());
    let before = index.checked_sub(1).map(|i| ranks[i]);
    let after = ranks.get(index).copied();
    match (before, after) {
        (None, None) => Placement::Rank(RANK_STEP),
        (None, Some(a)) if a > 1 => Placement::Rank(a / 2),
        (Some(b), None) => match b.checked_add(RANK_STEP) {
            Some(r) => Placement::Rank(r),
            None => Placement::Renumber(renumbered(ranks.len() + 1)),
        },
        (Some(b), Some(a)) if a - b > 1 => Placement::Rank(b + (a - b) / 2),
        _ => Placement::Renumber(renumbered(ranks.len() + 1)),
    }
}

/// `len` ranks, one step apart, starting at one step.
pub fn renumbered(len: usize) -> Vec<u32> {
    (1..=len as u32).map(|i| i * RANK_STEP).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BoardError {
    #[error("unknown board column {0:?}: one of backlog, todo, doing, done, archived")]
    UnknownColumn(String),
    #[error("not a calendar day {0:?}: a due date is YYYY-MM-DD")]
    BadDate(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unplaced_card_follows_the_lifecycle_and_a_closed_one_is_always_archived() {
        assert_eq!(
            BoardColumn::for_state(&WorkstreamState::Open),
            BoardColumn::Backlog
        );
        for s in [
            WorkstreamState::Dirty,
            WorkstreamState::Committed,
            WorkstreamState::Pushed,
            WorkstreamState::PrOpen {
                number: 1,
                url: "u".into(),
            },
        ] {
            assert_eq!(BoardColumn::for_state(&s), BoardColumn::Doing, "{s:?}");
        }
        assert_eq!(
            BoardColumn::for_state(&WorkstreamState::Merged {
                number: 1,
                url: "u".into()
            }),
            BoardColumn::Done
        );
        assert_eq!(
            BoardColumn::for_state(&WorkstreamState::Closed),
            BoardColumn::Archived
        );
        assert_eq!(
            BoardColumn::shown(Some(BoardColumn::Todo), &WorkstreamState::Committed),
            BoardColumn::Todo,
            "a choice stands"
        );
        assert_eq!(
            BoardColumn::shown(None, &WorkstreamState::Committed),
            BoardColumn::Doing
        );
        assert_eq!(
            BoardColumn::shown(Some(BoardColumn::Todo), &WorkstreamState::Closed),
            BoardColumn::Archived,
            "closed is archived whatever was chosen"
        );
    }

    #[test]
    fn the_columns_round_trip_through_their_words() {
        for c in BoardColumn::ALL {
            assert_eq!(c.as_str().parse::<BoardColumn>().unwrap(), c);
            assert_eq!(
                serde_json::to_string(&c).unwrap(),
                format!("{:?}", c.as_str())
            );
        }
        assert!(matches!(
            "later".parse::<BoardColumn>(),
            Err(BoardError::UnknownColumn(_))
        ));
    }

    #[test]
    fn a_due_date_is_a_real_day() {
        for ok in ["2026-09-30", "2024-02-29", "2000-02-29", "0001-01-01"] {
            assert_eq!(DueDate::parse(ok).unwrap().as_str(), ok);
        }
        for bad in [
            "2026-9-3",
            "2026-13-01",
            "2026-04-31",
            "2023-02-29",
            "1900-02-29",
            "0000-01-01",
            "2026/09/30",
            "",
            "2026-09-30T00:00",
        ] {
            assert!(
                matches!(DueDate::parse(bad), Err(BoardError::BadDate(_))),
                "{bad}"
            );
        }
        let d: DueDate = serde_json::from_str("\"2026-09-30\"").unwrap();
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"2026-09-30\"");
        assert!(serde_json::from_str::<DueDate>("\"soon\"").is_err());
    }

    #[test]
    fn an_empty_board_slot_is_not_written() {
        assert!(WorkstreamBoard::default().is_empty());
        let b = WorkstreamBoard {
            column: Some(BoardColumn::Doing),
            rank: Some(2048),
            due: None,
        };
        assert_eq!(
            serde_json::to_string(&b).unwrap(),
            r#"{"column":"doing","rank":2048}"#
        );
    }

    #[test]
    fn a_rank_lands_between_its_neighbours_and_renumbers_when_they_touch() {
        assert_eq!(
            rank_at(&[], 0),
            Placement::Rank(RANK_STEP),
            "an empty column"
        );
        assert_eq!(
            rank_at(&[1024, 2048], 0),
            Placement::Rank(512),
            "before the first"
        );
        assert_eq!(
            rank_at(&[1024, 2048], 2),
            Placement::Rank(3072),
            "after the last"
        );
        assert_eq!(
            rank_at(&[1024, 2048], 9),
            Placement::Rank(3072),
            "past the end is last"
        );
        assert_eq!(rank_at(&[1024, 2048], 1), Placement::Rank(1536), "between");
        assert_eq!(
            rank_at(&[1024, 1025], 1),
            Placement::Renumber(vec![1024, 2048, 3072]),
            "no room"
        );
        assert_eq!(
            rank_at(&[1], 0),
            Placement::Renumber(vec![1024, 2048]),
            "no room before the first"
        );
        assert_eq!(
            rank_at(&[u32::MAX], 1),
            Placement::Renumber(vec![1024, 2048]),
            "no room after the last"
        );
    }

    // added by the coverage pass: board.rs

    #[test]
    fn a_due_date_prints_as_its_calendar_day() {
        assert_eq!(
            DueDate::parse("2026-10-08").unwrap().to_string(),
            "2026-10-08"
        );
    }

    // added by the coverage pass: b5-board.rs
    #[test]
    fn a_column_prints_as_its_word() {
        for column in BoardColumn::ALL {
            assert_eq!(column.to_string(), column.as_str());
        }
    }
}
