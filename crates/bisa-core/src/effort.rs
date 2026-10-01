//! Effort: how hard a model works on a task. One vocabulary for every
//! harness — each adapter translates a level into its own flag, variant or
//! command — and one rule for who decides: the step, then the model, then the
//! agent's plan, then the `agents.effort` setting. Everything here is a pure
//! function: no harness, no registry, no clock.
//!
//! Two words are kept apart. An [`Effort`] is a level a model runs at. An
//! [`EffortChoice`] is what somebody asks for: a level, or `auto` — the
//! Decision-Making Agent reads the task and names the level. Thinking switched
//! off is not an effort, and has no word here.

use serde::{Deserialize, Serialize};

/// A level a model runs at, lowest first. The order is the enum's own, so
/// `Effort::Low < Effort::High`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub const ALL: [Effort; 6] = [
        Effort::Minimal,
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
    ];

    /// What runs when nobody says otherwise.
    pub const DEFAULT: Effort = Effort::High;

    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }

    /// The level to run at when only `available` can be taken: this one, else
    /// the nearest below it, else the lowest above it. `None` when nothing is
    /// available — the harness has no control, and nothing is sent.
    pub fn clamp_to(self, available: &[Effort]) -> Option<Effort> {
        let below = available.iter().copied().filter(|e| *e <= self).max();
        below.or_else(|| available.iter().copied().min())
    }
}

impl std::str::FromStr for Effort {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Effort::ALL
            .into_iter()
            .find(|e| e.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownEffort(s.to_string()))
    }
}

impl std::fmt::Display for Effort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What somebody asks for: a level, or `auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EffortChoice {
    Auto,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl EffortChoice {
    pub const ALL: [EffortChoice; 7] = [
        EffortChoice::Auto,
        EffortChoice::Minimal,
        EffortChoice::Low,
        EffortChoice::Medium,
        EffortChoice::High,
        EffortChoice::Xhigh,
        EffortChoice::Max,
    ];

    /// What the `agents.effort` setting says when nobody set it.
    pub const DEFAULT: EffortChoice = EffortChoice::High;

    pub fn as_str(self) -> &'static str {
        match self.level() {
            Some(level) => level.as_str(),
            None => "auto",
        }
    }

    /// The level asked for, or `None` when it is the judge's to name.
    pub fn level(self) -> Option<Effort> {
        match self {
            EffortChoice::Auto => None,
            EffortChoice::Minimal => Some(Effort::Minimal),
            EffortChoice::Low => Some(Effort::Low),
            EffortChoice::Medium => Some(Effort::Medium),
            EffortChoice::High => Some(Effort::High),
            EffortChoice::Xhigh => Some(Effort::Xhigh),
            EffortChoice::Max => Some(Effort::Max),
        }
    }

    pub fn is_auto(self) -> bool {
        self.level().is_none()
    }
}

impl From<Effort> for EffortChoice {
    fn from(level: Effort) -> Self {
        match level {
            Effort::Minimal => EffortChoice::Minimal,
            Effort::Low => EffortChoice::Low,
            Effort::Medium => EffortChoice::Medium,
            Effort::High => EffortChoice::High,
            Effort::Xhigh => EffortChoice::Xhigh,
            Effort::Max => EffortChoice::Max,
        }
    }
}

impl std::str::FromStr for EffortChoice {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        EffortChoice::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownEffort(s.to_string()))
    }
}

impl std::fmt::Display for EffortChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the chain comes to for one attempt, before the model's own levels
/// clamp it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// Somebody named the level.
    Level(Effort),
    /// The judge names it; `fallback` runs when the judge is off, unsure or
    /// fails.
    Auto { fallback: Effort },
}

impl Resolved {
    pub fn is_auto(self) -> bool {
        matches!(self, Resolved::Auto { .. })
    }

    /// The level to run at, given what the judge said — nothing, when it was
    /// not asked or did not answer.
    pub fn level(self, judged: Option<Effort>) -> Effort {
        match self {
            Resolved::Level(level) => level,
            Resolved::Auto { fallback } => judged.unwrap_or(fallback),
        }
    }
}

/// Who decides, first that is set: the step's pin, the model's own, the
/// agent's plan, the setting. `auto` falls back to the next level further
/// down the chain that is a level, and to [`Effort::DEFAULT`] when there is
/// none.
pub fn resolve(
    step: Option<EffortChoice>,
    model: Option<EffortChoice>,
    plan: Option<EffortChoice>,
    setting: EffortChoice,
) -> Resolved {
    let mut chain = [step, model, plan, Some(setting)].into_iter().flatten();
    let first = chain.next().unwrap_or(setting);
    match first.level() {
        Some(level) => Resolved::Level(level),
        None => Resolved::Auto {
            fallback: chain
                .find_map(EffortChoice::level)
                .unwrap_or(Effort::DEFAULT),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const AUTO: Option<EffortChoice> = Some(EffortChoice::Auto);

    fn asked(level: Effort) -> Option<EffortChoice> {
        Some(level.into())
    }

    #[test]
    fn the_levels_are_ordered_lowest_first() {
        let mut sorted = Effort::ALL;
        sorted.sort();
        assert_eq!(sorted, Effort::ALL);
        assert!(Effort::Minimal < Effort::Low);
        assert!(Effort::High < Effort::Xhigh);
        assert!(Effort::Xhigh < Effort::Max);
        assert_eq!(Effort::DEFAULT, Effort::High);
        assert_eq!(EffortChoice::DEFAULT.level(), Some(Effort::DEFAULT));
    }

    #[test]
    fn every_word_reads_back_and_is_what_the_wire_says() {
        for level in Effort::ALL {
            assert_eq!(Effort::from_str(level.as_str()).unwrap(), level);
            assert_eq!(level.to_string(), level.as_str());
            let wire = serde_json::to_value(level).unwrap();
            assert_eq!(wire, serde_json::json!(level.as_str()));
            assert_eq!(serde_json::from_value::<Effort>(wire).unwrap(), level);
        }
        for choice in EffortChoice::ALL {
            assert_eq!(EffortChoice::from_str(choice.as_str()).unwrap(), choice);
            assert_eq!(choice.to_string(), choice.as_str());
            let wire = serde_json::to_value(choice).unwrap();
            assert_eq!(wire, serde_json::json!(choice.as_str()));
            assert_eq!(
                serde_json::from_value::<EffortChoice>(wire).unwrap(),
                choice
            );
        }
        // One word, no underscore: a harness reads `xhigh`.
        assert_eq!(Effort::Xhigh.as_str(), "xhigh");
        assert_eq!(
            serde_json::to_value(EffortChoice::Xhigh).unwrap(),
            serde_json::json!("xhigh")
        );
    }

    #[test]
    fn a_choice_is_auto_or_one_of_the_levels() {
        assert_eq!(EffortChoice::ALL[0], EffortChoice::Auto);
        assert!(EffortChoice::Auto.is_auto());
        assert_eq!(EffortChoice::Auto.level(), None);
        assert_eq!(EffortChoice::Auto.as_str(), "auto");
        let levels: Vec<Effort> = EffortChoice::ALL
            .into_iter()
            .filter_map(EffortChoice::level)
            .collect();
        assert_eq!(levels, Effort::ALL);
        for level in Effort::ALL {
            let choice = EffortChoice::from(level);
            assert_eq!(choice.level(), Some(level));
            assert!(!choice.is_auto());
            assert_eq!(choice.as_str(), level.as_str());
        }
    }

    #[test]
    fn a_word_that_is_no_level_is_refused() {
        for odd in [
            "", "High", "HIGH", " high", "x_high", "off", "none", "ultra",
        ] {
            assert!(Effort::from_str(odd).is_err(), "{odd:?}");
            assert!(EffortChoice::from_str(odd).is_err(), "{odd:?}");
            let wire = serde_json::json!(odd);
            assert!(serde_json::from_value::<Effort>(wire.clone()).is_err());
            assert!(serde_json::from_value::<EffortChoice>(wire).is_err());
        }
        // `auto` is a choice and never a level.
        assert!(Effort::from_str("auto").is_err());
        assert!(matches!(
            Effort::from_str("ultra"),
            Err(crate::CoreError::UnknownEffort(word)) if word == "ultra"
        ));
    }

    #[test]
    fn a_level_is_clamped_to_itself_then_below_then_above() {
        let five = [
            Effort::Low,
            Effort::Medium,
            Effort::High,
            Effort::Xhigh,
            Effort::Max,
        ];
        assert_eq!(Effort::High.clamp_to(&five), Some(Effort::High));
        // The nearest below.
        let four = [Effort::Low, Effort::Medium, Effort::High, Effort::Max];
        assert_eq!(Effort::Xhigh.clamp_to(&four), Some(Effort::High));
        assert_eq!(Effort::Max.clamp_to(&four), Some(Effort::Max));
        // Nothing below: the lowest above.
        assert_eq!(Effort::Minimal.clamp_to(&four), Some(Effort::Low));
        let top = [Effort::Max, Effort::High];
        assert_eq!(Effort::Low.clamp_to(&top), Some(Effort::High));
        assert_eq!(Effort::Xhigh.clamp_to(&top), Some(Effort::High));
        // The order it is listed in does not matter, and neither does a repeat.
        let mixed = [Effort::Max, Effort::Low, Effort::Low, Effort::Medium];
        assert_eq!(Effort::High.clamp_to(&mixed), Some(Effort::Medium));
        // No control: nothing is sent.
        for level in Effort::ALL {
            assert_eq!(level.clamp_to(&[]), None);
            assert_eq!(level.clamp_to(&Effort::ALL), Some(level));
            assert_eq!(level.clamp_to(&[Effort::Medium]), Some(Effort::Medium));
        }
    }

    #[test]
    fn the_first_that_is_set_decides() {
        let setting = EffortChoice::High;
        assert_eq!(
            resolve(None, None, None, setting),
            Resolved::Level(Effort::High)
        );
        assert_eq!(
            resolve(None, None, asked(Effort::Max), setting),
            Resolved::Level(Effort::Max)
        );
        assert_eq!(
            resolve(None, asked(Effort::Low), asked(Effort::Max), setting),
            Resolved::Level(Effort::Low)
        );
        assert_eq!(
            resolve(
                asked(Effort::Xhigh),
                asked(Effort::Low),
                asked(Effort::Max),
                setting
            ),
            Resolved::Level(Effort::Xhigh)
        );
        // A level above an `auto` below it: the level, and nobody is asked.
        assert_eq!(
            resolve(asked(Effort::Medium), AUTO, AUTO, EffortChoice::Auto),
            Resolved::Level(Effort::Medium)
        );
    }

    #[test]
    fn auto_falls_back_to_the_next_level_down_the_chain() {
        assert_eq!(
            resolve(None, None, None, EffortChoice::Auto),
            Resolved::Auto {
                fallback: Effort::High
            }
        );
        assert_eq!(
            resolve(None, None, AUTO, EffortChoice::Low),
            Resolved::Auto {
                fallback: Effort::Low
            }
        );
        assert_eq!(
            resolve(
                AUTO,
                asked(Effort::Max),
                asked(Effort::Low),
                EffortChoice::High
            ),
            Resolved::Auto {
                fallback: Effort::Max
            }
        );
        // An `auto` further down is passed over, never the fallback.
        assert_eq!(
            resolve(AUTO, AUTO, asked(Effort::Medium), EffortChoice::High),
            Resolved::Auto {
                fallback: Effort::Medium
            }
        );
        assert_eq!(
            resolve(AUTO, AUTO, AUTO, EffortChoice::Auto),
            Resolved::Auto {
                fallback: Effort::DEFAULT
            }
        );
        // What is above the `auto` is never its fallback.
        assert_eq!(
            resolve(None, AUTO, None, EffortChoice::Minimal),
            Resolved::Auto {
                fallback: Effort::Minimal
            }
        );
    }

    #[test]
    fn a_resolved_level_ignores_the_judge_and_auto_takes_it() {
        let named = Resolved::Level(Effort::Low);
        assert!(!named.is_auto());
        assert_eq!(named.level(None), Effort::Low);
        assert_eq!(named.level(Some(Effort::Max)), Effort::Low);
        let auto = Resolved::Auto {
            fallback: Effort::Medium,
        };
        assert!(auto.is_auto());
        assert_eq!(auto.level(Some(Effort::Max)), Effort::Max);
        assert_eq!(auto.level(None), Effort::Medium);
    }
}
