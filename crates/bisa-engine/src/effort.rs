//! Effort at a launch: the engine's side of `bisa_core::effort`.
//!
//! The core says who decides — the step, the model, the agent's plan, the
//! `agents.effort` setting — and how a level is fitted to a list. What is
//! here is what a launch adds to that: the setting read for the project the
//! work stands in, the question the Decision-Making Agent is put when an
//! attempt comes to `auto`, its answer read back into a level, and the fit of
//! one attempt against what its harness takes for its model.
//!
//! Nothing here asks anybody. The question is built and the answer is read;
//! the asking is the decider's (`decider::effort`), once for a launch walk,
//! and the session the Decision-Making Agent itself runs in never comes
//! through it.

use crate::Inner;
use bisa_core::{DecisionQuestion, Effort, EffortChoice, ModelPlan, ProjectId};
use serde_json::Value;

/// The setting that says how hard a model works when nobody else did.
pub const SETTING: &str = "agents.effort";

/// What the Decision-Making Agent is asked, in words a model reads.
const INSTRUCTIONS: &str = "How hard should the model work on this task? Each level costs more \
     time and more tokens than the one below it. Prefer the lowest level that will do the job \
     well.";

/// A stored word as what somebody asks for. A word that is no choice — a
/// file edited by hand — reads as the default, never as a refusal at launch.
pub fn choice_of(value: &Value) -> EffortChoice {
    value
        .as_str()
        .and_then(|word| word.trim().parse().ok())
        .unwrap_or(EffortChoice::DEFAULT)
}

/// A stored word as a level, for the two settings that take no `auto`: the
/// Decision-Making Agent's own and the classifier's.
pub fn level_of(value: &Value) -> Effort {
    value
        .as_str()
        .and_then(|word| word.trim().parse().ok())
        .unwrap_or(Effort::DEFAULT)
}

/// `agents.effort` as it resolves for the project the work stands in: the
/// project's, then the workspace's, then `high`.
pub fn setting(inner: &Inner, project: Option<ProjectId>) -> EffortChoice {
    inner
        .ws
        .setting(SETTING, project)
        .map(|resolved| choice_of(&resolved.value))
        .unwrap_or(EffortChoice::DEFAULT)
}

/// One of the level settings (`decisions.harness.effort`,
/// `security.classifier.effort`), resolved for the node.
pub fn level_setting(inner: &Inner, key: &str) -> Effort {
    inner
        .ws
        .setting(key, None)
        .map(|resolved| level_of(&resolved.value))
        .unwrap_or(Effort::DEFAULT)
}

/// The efforts `harness` takes for a model it does not list — what a picker
/// offers beside a model id somebody typed. Empty for a harness with no
/// control, and for one this node cannot launch.
pub fn harness_efforts(inner: &Inner, harness: &str) -> Vec<Effort> {
    inner
        .catalog
        .get(harness)
        .map(|adapter| adapter.efforts(None))
        .unwrap_or_default()
}

/// Whether an attempt of a launch walk may come to `auto` — the one case
/// the Decision-Making Agent is asked. A step that pins a model launches
/// that model alone, so only its chain is read.
pub fn asks(
    plan: &ModelPlan,
    model_pin: Option<&str>,
    pin: Option<EffortChoice>,
    setting: EffortChoice,
) -> bool {
    match model_pin {
        Some(model) => plan.resolve_effort(Some(model), pin, setting).is_auto(),
        None => plan.asks_effort(pin, setting),
    }
}

/// The levels as they are offered: lowest first, each once.
fn offered(levels: &[Effort]) -> Vec<Effort> {
    let mut levels = levels.to_vec();
    levels.sort();
    levels.dedup();
    levels
}

/// What a level is for.
fn purpose(level: Effort) -> &'static str {
    match level {
        Effort::Minimal => "trivial changes and quick lookups",
        Effort::Low => "simple, well-specified, quick tasks",
        Effort::Medium => "ordinary work",
        Effort::High => "complex problems and difficult coding",
        Effort::Xhigh => "long agentic work across many steps",
        Effort::Max => "the hardest problems, where depth matters more than time",
    }
}

/// One option's sentence: its rank among the levels offered, then what it
/// is for. The rank is said in words because a question's options travel as
/// a map, and a map's order is the alphabet's.
fn sentence(level: Effort, rank: usize, of: usize) -> String {
    let edge = if rank == 1 {
        ", the lowest"
    } else if rank == of {
        ", the highest"
    } else {
        ""
    };
    format!("level {rank} of {of}{edge} — {}", purpose(level))
}

/// The options of the question over `levels`: each level's wire word, and
/// its sentence.
pub fn options(levels: &[Effort]) -> Vec<(String, String)> {
    let levels = offered(levels);
    let of = levels.len();
    levels
        .into_iter()
        .enumerate()
        .map(|(i, level)| (level.as_str().to_string(), sentence(level, i + 1, of)))
        .collect()
}

/// The question the Decision-Making Agent is put about `levels`. `None`
/// when fewer than two are offered: one level is no choice, and none is a
/// harness without the control.
pub fn question(levels: &[Effort]) -> Option<DecisionQuestion> {
    let options = options(levels);
    (options.len() >= 2).then(|| DecisionQuestion::choice(INSTRUCTIONS, options))
}

/// The level an answer names. A word that is no level, or a level the
/// question never offered, is no judged level: the attempt's own fallback
/// runs.
pub fn judged_level(word: &str, levels: &[Effort]) -> Option<Effort> {
    let level: Effort = word.trim().parse().ok()?;
    levels.contains(&level).then_some(level)
}

/// The effort one attempt is sent: who decides for `model` — the step's
/// pin, the model's own, the plan's, the setting, with `judged` standing in
/// where that comes to `auto` — fitted to what the harness `takes` for it.
/// `None` when it takes none: nothing is sent, and the session has no
/// effort.
pub fn fitted(
    plan: &ModelPlan,
    model: Option<&str>,
    pin: Option<EffortChoice>,
    setting: EffortChoice,
    judged: Option<Effort>,
    takes: &[Effort],
) -> Option<Effort> {
    plan.resolve_effort(model, pin, setting)
        .level(judged)
        .clamp_to(takes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::ModelChoice;
    use serde_json::json;

    const FIVE: [Effort; 5] = [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
    ];
    const FOUR: [Effort; 4] = [Effort::Low, Effort::Medium, Effort::High, Effort::Max];

    fn criteria(question: DecisionQuestion) -> (String, Vec<(String, String)>) {
        match question {
            DecisionQuestion::Choice {
                instructions,
                criteria,
            } => (instructions, criteria.into_iter().collect()),
            other => panic!("an effort is a choice, not a {}", other.type_str()),
        }
    }

    #[test]
    fn five_levels_are_five_options_each_saying_its_rank_and_what_it_is_for() {
        assert_eq!(
            options(&FIVE),
            [
                (
                    "low".to_string(),
                    "level 1 of 5, the lowest — simple, well-specified, quick tasks".to_string()
                ),
                (
                    "medium".to_string(),
                    "level 2 of 5 — ordinary work".to_string()
                ),
                (
                    "high".to_string(),
                    "level 3 of 5 — complex problems and difficult coding".to_string()
                ),
                (
                    "xhigh".to_string(),
                    "level 4 of 5 — long agentic work across many steps".to_string()
                ),
                (
                    "max".to_string(),
                    "level 5 of 5, the highest — the hardest problems, where depth matters more \
                     than time"
                        .to_string()
                ),
            ]
        );
        let (instructions, criteria) = criteria(question(&FIVE).unwrap());
        assert!(instructions.contains("Prefer the lowest level that will do the job well"));
        // A map keeps the alphabet's order; the sentences keep the levels'.
        let ids: Vec<&str> = criteria.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["high", "low", "max", "medium", "xhigh"]);
        assert!(criteria
            .iter()
            .any(|(id, says)| id == "xhigh" && says.starts_with("level 4 of 5")));
    }

    #[test]
    fn four_levels_rank_among_four_and_the_order_given_does_not_matter() {
        let shuffled = [
            Effort::Max,
            Effort::Low,
            Effort::High,
            Effort::Medium,
            Effort::Low,
        ];
        assert_eq!(options(&shuffled), options(&FOUR));
        let said: Vec<String> = options(&FOUR).into_iter().map(|(_, says)| says).collect();
        assert_eq!(
            said,
            [
                "level 1 of 4, the lowest — simple, well-specified, quick tasks",
                "level 2 of 4 — ordinary work",
                "level 3 of 4 — complex problems and difficult coding",
                "level 4 of 4, the highest — the hardest problems, where depth matters more \
                 than time",
            ]
        );
    }

    #[test]
    fn two_levels_are_the_lowest_and_the_highest_and_fewer_is_no_question() {
        assert_eq!(
            options(&[Effort::High, Effort::Max]),
            [
                (
                    "high".to_string(),
                    "level 1 of 2, the lowest — complex problems and difficult coding".to_string()
                ),
                (
                    "max".to_string(),
                    "level 2 of 2, the highest — the hardest problems, where depth matters more \
                     than time"
                        .to_string()
                ),
            ]
        );
        // The two lowest levels are for the same work: the rank tells them apart.
        let lowest = options(&[Effort::Minimal, Effort::Low]);
        assert_ne!(lowest[0].1, lowest[1].1);
        assert!(question(&[Effort::High, Effort::Max]).is_some());
        assert!(question(&[Effort::High]).is_none());
        assert!(question(&[Effort::High, Effort::High]).is_none());
        assert!(question(&[]).is_none());
        assert_eq!(criteria(question(&Effort::ALL).unwrap()).1.len(), 6);
    }

    #[test]
    fn an_answer_is_a_level_only_when_it_is_one_the_question_offered() {
        assert_eq!(judged_level("xhigh", &FIVE), Some(Effort::Xhigh));
        assert_eq!(judged_level(" low ", &FIVE), Some(Effort::Low));
        // A level the model does not take was never offered.
        assert_eq!(judged_level("xhigh", &FOUR), None);
        assert_eq!(judged_level("minimal", &FIVE), None);
        // A word that is no level at all.
        for odd in ["", "auto", "High", "x_high", "ultra", "level 3 of 5"] {
            assert_eq!(judged_level(odd, &FIVE), None, "{odd:?}");
        }
        assert_eq!(judged_level("high", &[]), None);
    }

    #[test]
    fn a_stored_word_that_is_no_effort_reads_as_the_default() {
        assert_eq!(choice_of(&json!("auto")), EffortChoice::Auto);
        assert_eq!(choice_of(&json!("xhigh")), EffortChoice::Xhigh);
        for odd in [json!("ultra"), json!(""), json!(3), json!(null)] {
            assert_eq!(choice_of(&odd), EffortChoice::DEFAULT, "{odd}");
            assert_eq!(level_of(&odd), Effort::DEFAULT, "{odd}");
        }
        assert_eq!(level_of(&json!("max")), Effort::Max);
        // `auto` is a choice and never a level.
        assert_eq!(level_of(&json!("auto")), Effort::DEFAULT);
    }

    #[test]
    fn an_attempt_is_fitted_to_what_its_own_model_takes() {
        let plan = ModelPlan::fallback(["opus", "sonnet"]).at(EffortChoice::Xhigh);
        let high = EffortChoice::High;
        assert_eq!(
            fitted(&plan, Some("opus"), None, high, None, &FIVE),
            Some(Effort::Xhigh)
        );
        // The nearest below, on a model that does not take the level.
        assert_eq!(
            fitted(&plan, Some("sonnet"), None, high, None, &FOUR),
            Some(Effort::High)
        );
        // The step's pin is over the plan's.
        assert_eq!(
            fitted(
                &plan,
                Some("opus"),
                Some(EffortChoice::Low),
                high,
                None,
                &FIVE
            ),
            Some(Effort::Low)
        );
        // No list: nothing is sent, whatever was asked for.
        assert_eq!(fitted(&plan, Some("opus"), None, high, None, &[]), None);
        assert_eq!(
            fitted(
                &plan,
                None,
                Some(EffortChoice::Max),
                high,
                Some(Effort::Low),
                &[]
            ),
            None
        );
    }

    #[test]
    fn a_judged_level_stands_in_only_where_the_chain_comes_to_auto() {
        let mut plan = ModelPlan::fallback(["opus", "sonnet"]).at(EffortChoice::Auto);
        plan.models[1] = ModelChoice::new("sonnet").at(EffortChoice::Low);
        let high = EffortChoice::High;
        let judged = Some(Effort::Max);
        assert_eq!(
            fitted(&plan, Some("opus"), None, high, judged, &FIVE),
            Some(Effort::Max)
        );
        // The same judged level, fitted to another model's list.
        assert_eq!(
            fitted(
                &plan,
                Some("opus"),
                None,
                high,
                judged,
                &[Effort::Low, Effort::High]
            ),
            Some(Effort::High)
        );
        // A model that names its own level is not the judge's.
        assert_eq!(
            fitted(&plan, Some("sonnet"), None, high, judged, &FIVE),
            Some(Effort::Low)
        );
        // Nobody answered: the next level down the chain.
        assert_eq!(
            fitted(&plan, Some("opus"), None, EffortChoice::Medium, None, &FIVE),
            Some(Effort::Medium)
        );
    }

    #[test]
    fn a_walk_asks_when_an_attempt_may_come_to_auto_and_a_pinned_model_is_read_alone() {
        let high = EffortChoice::High;
        let auto = EffortChoice::Auto;
        let mut plan = ModelPlan::fallback(["opus", "sonnet"]);
        assert!(!asks(&plan, None, None, high));
        assert!(asks(&plan, None, None, auto));
        assert!(asks(&plan, None, Some(auto), high));
        assert!(!asks(&plan, None, Some(EffortChoice::Low), auto));
        // One model of the plan says `auto`: the walk may reach it.
        plan.models[1].effort = Some(auto);
        assert!(asks(&plan, None, None, high));
        // A pin launches that model alone.
        assert!(!asks(&plan, Some("opus"), None, high));
        assert!(asks(&plan, Some("sonnet"), None, high));
        // A pinned model the plan does not hold states nothing: the plan's,
        // then the setting.
        assert!(!asks(&plan, Some("elsewhere"), None, high));
        assert!(asks(&plan, Some("elsewhere"), None, auto));
    }
}
