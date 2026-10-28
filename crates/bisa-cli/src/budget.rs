//! `--budget-tokens`, `--budget-usd-cents`, `--budget-secs`: a ceiling named
//! on the command line — a goal's own at capture (`bisa new`), or the one
//! each run a workflow's events start spends against (`bisa workflow on`).
//! One set of flags, one reading, wherever a ceiling is decided by hand.

use bisa_core::Budget;
use clap::Args;

/// The ceiling the flags name. None of the three leaves the workspace
/// default (`budget.default.*`) in force; any of them makes a budget of its
/// own, the others unlimited — and a zero lifts a ceiling, as it does in the
/// settings.
#[derive(Args, Default)]
pub struct BudgetArgs {
    /// Token ceiling; 0 lifts it
    #[arg(long = "budget-tokens")]
    budget_tokens: Option<u64>,
    /// Cost ceiling in US cents; 0 lifts it
    #[arg(long = "budget-usd-cents")]
    budget_usd_cents: Option<u64>,
    /// Wall-clock ceiling in seconds; 0 lifts it
    #[arg(long = "budget-secs")]
    budget_secs: Option<u64>,
}

impl BudgetArgs {
    /// The budget these flags say: none when no flag was given, so the
    /// workspace default applies.
    pub fn budget(&self) -> Option<Budget> {
        let given = [self.budget_tokens, self.budget_usd_cents, self.budget_secs];
        if given.iter().all(Option::is_none) {
            return None;
        }
        let lifted = |ceiling: Option<u64>| ceiling.filter(|n| *n > 0);
        Some(Budget {
            max_tokens: lifted(self.budget_tokens),
            max_usd_cents: lifted(self.budget_usd_cents),
            max_wall_clock_secs: lifted(self.budget_secs),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_budget_is_none_the_workspace_s_or_the_flags() {
        assert_eq!(BudgetArgs::default().budget(), None);
        let tokens = BudgetArgs {
            budget_tokens: Some(10_000),
            ..BudgetArgs::default()
        };
        assert_eq!(
            tokens.budget(),
            Some(Budget {
                max_tokens: Some(10_000),
                ..Budget::default()
            })
        );
        // A zero lifts the ceiling: alone, it is a budget with none at all.
        let lifted = BudgetArgs {
            budget_usd_cents: Some(0),
            ..BudgetArgs::default()
        };
        assert_eq!(lifted.budget(), Some(Budget::default()));
        assert_eq!(
            serde_json::to_value(lifted.budget()).ok(),
            Some(json!({})),
            "`{{}}` is no ceiling, whatever the default says"
        );
        // Two named and one lifted: the named ones stand, the lifted one is gone.
        let mixed = BudgetArgs {
            budget_tokens: Some(7),
            budget_usd_cents: Some(50),
            budget_secs: Some(0),
        };
        assert_eq!(
            mixed.budget(),
            Some(Budget {
                max_tokens: Some(7),
                max_usd_cents: Some(50),
                max_wall_clock_secs: None,
            })
        );
    }
}
