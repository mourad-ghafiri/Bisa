//! Model plans: an ordered list of models plus a strategy for choosing among
//! them. `order` is a pure function of `(plan, health, rotation, lead)` — no
//! clock, no RNG, no registry — which is what makes the five strategies
//! exhaustively testable. The fifth, `AutoRoute`, is the one whose head somebody
//! else names: the Decision-Making Agent reads the task and the plan's models,
//! and the model it picks arrives here as `lead`. With no lead it is `Fallback`.
//!
//! A plan also says how hard its models work: an effort of its own, and one a
//! model may state for itself. They are the two middle links of the chain
//! `effort.rs` resolves — the step's pin before them, the `agents.effort`
//! setting after — and [`ModelPlan::resolve_effort`] is that chain for one
//! model of the plan. Nothing here knows which levels a harness takes: the
//! clamp is the launch's, against the adapter's own list.

use serde::{Deserialize, Serialize};

use crate::effort::{resolve, EffortChoice, Resolved};

/// What the engine's health ledger answers about a model. `order` reads no
/// clock, so an **expired** cooldown must be reported as `None`.
pub trait ModelHealthView {
    /// Unix seconds until which this model is in cooldown, or `None` when it is
    /// usable now.
    fn cooldown_until(&self, model: &str) -> Option<u64>;
    /// Sessions currently running on this model. Only `LeastBusy` reads it.
    fn in_flight(&self, model: &str) -> u32;
}

/// Every model healthy and idle — the identity health view.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllHealthy;

impl ModelHealthView for AllHealthy {
    fn cooldown_until(&self, _model: &str) -> Option<u64> {
        None
    }
    fn in_flight(&self, _model: &str) -> u32 {
        0
    }
}

/// How a [`ModelPlan`] picks which of its models goes first.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ModelStrategy {
    /// Plan order, unchanged: the first entry that launches and survives.
    #[default]
    Fallback,
    /// The head is chosen proportionally to `weight` across successive
    /// `rotation` values; the fall-through order after it is by descending
    /// weight. Deterministic.
    Weighted,
    /// Strict rotation: entry `rotation % n` leads, then wrap.
    RoundRobin,
    /// Fewest in-flight sessions first; plan order breaks ties.
    LeastBusy,
    /// The Decision-Making Agent picks the model that suits the task, and
    /// that model leads; plan order follows it. Plan order alone when it names
    /// none.
    AutoRoute,
}

impl ModelStrategy {
    /// Whether the head of the order is somebody else's to name.
    pub fn is_routed(self) -> bool {
        matches!(self, ModelStrategy::AutoRoute)
    }
}

/// One model in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelChoice {
    /// Harness-native model id.
    pub model: String,
    /// Relative share for `Weighted`. `0` is clamped to `1` so an enabled
    /// entry is never silently unreachable — use `enabled` to turn one off.
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Off without losing its place in the order.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// What this model is the right one for, in a sentence — what `AutoRoute`
    /// reads beside the model's id when it picks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suited_for: Option<String>,
    /// How hard this model works, when it says so itself: a level, or `auto`.
    /// Absent means the plan's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortChoice>,
}

fn default_weight() -> u32 {
    1
}

fn default_enabled() -> bool {
    true
}

impl ModelChoice {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            weight: 1,
            enabled: true,
            suited_for: None,
            effort: None,
        }
    }

    /// A model and what it is the right one for.
    pub fn suited(model: impl Into<String>, suited_for: impl Into<String>) -> Self {
        Self {
            suited_for: Some(suited_for.into()),
            ..Self::new(model)
        }
    }

    pub fn weighted(model: impl Into<String>, weight: u32) -> Self {
        Self {
            model: model.into(),
            weight,
            enabled: true,
            suited_for: None,
            effort: None,
        }
    }

    /// The same model, stating its own effort.
    pub fn at(self, effort: EffortChoice) -> Self {
        Self {
            effort: Some(effort),
            ..self
        }
    }
}

/// An **empty** plan is the honest way to say "whatever the harness runs by
/// default": `order` yields nothing and the caller pins no model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelPlan {
    #[serde(default)]
    pub strategy: ModelStrategy,
    /// How hard the plan's models work — a level, or `auto` — unless a model
    /// states its own. Absent means the `agents.effort` setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortChoice>,
    #[serde(default)]
    pub models: Vec<ModelChoice>,
}

impl ModelPlan {
    /// A one-model plan.
    pub fn pinned(model: impl Into<String>) -> Self {
        Self {
            strategy: ModelStrategy::Fallback,
            effort: None,
            models: vec![ModelChoice::new(model)],
        }
    }

    /// An ordered fallback chain.
    pub fn fallback<I, S>(models: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            strategy: ModelStrategy::Fallback,
            effort: None,
            models: models.into_iter().map(ModelChoice::new).collect(),
        }
    }

    /// The same plan, stating the effort its models work at.
    pub fn at(self, effort: EffortChoice) -> Self {
        Self {
            effort: Some(effort),
            ..self
        }
    }

    pub fn is_empty(&self) -> bool {
        self.models.iter().all(|c| c.model.trim().is_empty())
    }

    /// The models to try, best-first: ready models ordered by strategy, then
    /// cooling models soonest-expiring first. **All models in cooldown still
    /// yields the whole list** — a cooldown is a guess, and returning nothing
    /// would turn "everything is briefly throttled" into "this work item
    /// failed".
    ///
    /// `lead` is read by `AutoRoute` alone, and only when it names a ready model
    /// of this plan: a pick of a model that is cooling, switched off or not here
    /// leads nothing.
    pub fn order(
        &self,
        health: &dyn ModelHealthView,
        rotation: u64,
        lead: Option<&str>,
    ) -> Vec<&str> {
        let mut ready: Vec<&ModelChoice> = Vec::new();
        let mut cooling: Vec<(u64, &ModelChoice)> = Vec::new();
        for choice in self
            .models
            .iter()
            .filter(|c| c.enabled && !c.model.trim().is_empty())
        {
            match health.cooldown_until(&choice.model) {
                Some(until) => cooling.push((until, choice)),
                None => ready.push(choice),
            }
        }
        let mut out: Vec<&str> = match self.strategy {
            ModelStrategy::Fallback => ready.iter().map(|c| c.model.as_str()).collect(),
            ModelStrategy::RoundRobin => rotate(&ready, rotation),
            ModelStrategy::Weighted => weighted(&ready, rotation),
            ModelStrategy::LeastBusy => least_busy(&ready, health),
            ModelStrategy::AutoRoute => led(&ready, lead),
        };
        cooling.sort_by_key(|(until, _)| *until);
        out.extend(cooling.into_iter().map(|(_, c)| c.model.as_str()));
        out
    }

    /// The head of [`Self::order`], for a caller that only needs to *say*
    /// which model a plan leads with.
    pub fn first(&self, health: &dyn ModelHealthView, rotation: u64) -> Option<&str> {
        self.order(health, rotation, None).into_iter().next()
    }

    /// The models `AutoRoute` may be asked to pick among, with what each is
    /// suited for: the ready ones, in plan order. Fewer than two is no question.
    pub fn routable(&self, health: &dyn ModelHealthView) -> Vec<(&str, Option<&str>)> {
        if !self.strategy.is_routed() {
            return Vec::new();
        }
        let ready: Vec<(&str, Option<&str>)> = self
            .models
            .iter()
            .filter(|c| c.enabled && !c.model.trim().is_empty())
            .filter(|c| health.cooldown_until(&c.model).is_none())
            .map(|c| (c.model.as_str(), c.suited_for.as_deref()))
            .collect();
        if ready.len() < 2 {
            Vec::new()
        } else {
            ready
        }
    }

    /// What `model` states for itself: the effort of the plan's first entry
    /// of that id. `None` when it states none, and for a model the plan does
    /// not hold — a step's pin may name one.
    pub fn effort_of(&self, model: &str) -> Option<EffortChoice> {
        self.models
            .iter()
            .find(|c| c.model == model)
            .and_then(|c| c.effort)
    }

    /// The effort chain for one attempt on `model`: the step's pin, then the
    /// model's own, then the plan's, then the setting. No model is the
    /// harness's default one, which states nothing. The level that comes out
    /// is still to be clamped to what the harness and the model take.
    pub fn resolve_effort(
        &self,
        model: Option<&str>,
        step: Option<EffortChoice>,
        setting: EffortChoice,
    ) -> Resolved {
        let own = model.and_then(|m| self.effort_of(m));
        resolve(step, own, self.effort, setting)
    }

    /// Whether an attempt on any model this plan may launch comes to `auto`
    /// — the one case the Decision-Making Agent is asked for a level. An
    /// empty plan launches the harness's default model, and is judged as
    /// that one attempt. A step that pins a model launches that model alone:
    /// its caller asks [`Self::resolve_effort`] about the pin instead.
    pub fn asks_effort(&self, step: Option<EffortChoice>, setting: EffortChoice) -> bool {
        let mut launchable = self
            .models
            .iter()
            .filter(|c| c.enabled && !c.model.trim().is_empty())
            .peekable();
        if launchable.peek().is_none() {
            return self.resolve_effort(None, step, setting).is_auto();
        }
        launchable.any(|c| resolve(step, c.effort, self.effort, setting).is_auto())
    }
}

fn led<'a>(ready: &[&'a ModelChoice], lead: Option<&str>) -> Vec<&'a str> {
    let head = lead.and_then(|l| ready.iter().position(|c| c.model == l));
    let rest = ready
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != head)
        .map(|(_, c)| c.model.as_str());
    head.map(|i| ready[i].model.as_str())
        .into_iter()
        .chain(rest)
        .collect()
}

fn rotate<'a>(ready: &[&'a ModelChoice], rotation: u64) -> Vec<&'a str> {
    let n = ready.len();
    if n == 0 {
        return Vec::new();
    }
    let start = (rotation % n as u64) as usize;
    (0..n)
        .map(|i| ready[(start + i) % n].model.as_str())
        .collect()
}

fn weighted<'a>(ready: &[&'a ModelChoice], rotation: u64) -> Vec<&'a str> {
    if ready.is_empty() {
        return Vec::new();
    }
    let total: u64 = ready.iter().map(|c| c.weight.max(1) as u64).sum();
    let mut pick = rotation % total;
    let mut head = 0usize;
    for (i, choice) in ready.iter().enumerate() {
        let w = choice.weight.max(1) as u64;
        if pick < w {
            head = i;
            break;
        }
        pick -= w;
    }
    let mut rest: Vec<(usize, &&ModelChoice)> = ready
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != head)
        .collect();
    rest.sort_by(|a, b| b.1.weight.cmp(&a.1.weight).then(a.0.cmp(&b.0)));
    std::iter::once(ready[head].model.as_str())
        .chain(rest.into_iter().map(|(_, c)| c.model.as_str()))
        .collect()
}

fn least_busy<'a>(ready: &[&'a ModelChoice], health: &dyn ModelHealthView) -> Vec<&'a str> {
    let mut v: Vec<&&ModelChoice> = ready.iter().collect();
    v.sort_by_key(|c| health.in_flight(&c.model));
    v.into_iter().map(|c| c.model.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Health(
        HashMap<&'static str, Option<u64>>,
        HashMap<&'static str, u32>,
    );

    impl ModelHealthView for Health {
        fn cooldown_until(&self, model: &str) -> Option<u64> {
            self.0.get(model).copied().flatten()
        }
        fn in_flight(&self, model: &str) -> u32 {
            self.1.get(model).copied().unwrap_or(0)
        }
    }

    #[test]
    fn fallback_keeps_plan_order_and_demotes_cooling() {
        let plan = ModelPlan::fallback(["a", "b", "c"]);
        let h = Health(
            HashMap::from([("a", Some(200)), ("b", Some(100))]),
            HashMap::new(),
        );
        assert_eq!(plan.order(&h, 0, None), vec!["c", "b", "a"]);
        assert_eq!(plan.order(&AllHealthy, 0, None), vec!["a", "b", "c"]);
        assert_eq!(plan.first(&AllHealthy, 0), Some("a"));
    }

    #[test]
    fn round_robin_rotates_and_weighted_is_deterministic() {
        let mut plan = ModelPlan::fallback(["a", "b", "c"]);
        plan.strategy = ModelStrategy::RoundRobin;
        assert_eq!(plan.order(&AllHealthy, 1, None), vec!["b", "c", "a"]);
        let w = ModelPlan {
            strategy: ModelStrategy::Weighted,
            effort: None,
            models: vec![ModelChoice::weighted("a", 3), ModelChoice::weighted("b", 1)],
        };
        assert_eq!(w.order(&AllHealthy, 0, None)[0], "a");
        assert_eq!(w.order(&AllHealthy, 3, None)[0], "b");
        assert_eq!(w.order(&AllHealthy, 3, None), vec!["b", "a"]);
    }

    #[test]
    fn least_busy_reads_load_and_disabled_entries_vanish() {
        let mut plan = ModelPlan::fallback(["a", "b"]);
        plan.strategy = ModelStrategy::LeastBusy;
        let h = Health(HashMap::new(), HashMap::from([("a", 3), ("b", 1)]));
        assert_eq!(plan.order(&h, 0, None), vec!["b", "a"]);
        plan.models[1].enabled = false;
        assert_eq!(plan.order(&h, 0, None), vec!["a"]);
        assert!(ModelPlan::default().is_empty());
        assert!(ModelPlan::default().order(&AllHealthy, 0, None).is_empty());
    }

    #[test]
    fn auto_route_leads_with_the_pick_and_is_fallback_without_one() {
        let mut plan = ModelPlan::fallback(["a", "b", "c"]);
        plan.strategy = ModelStrategy::AutoRoute;
        assert!(plan.strategy.is_routed());
        assert_eq!(plan.order(&AllHealthy, 7, None), vec!["a", "b", "c"]);
        assert_eq!(plan.order(&AllHealthy, 7, Some("c")), vec!["c", "a", "b"]);
        assert_eq!(plan.order(&AllHealthy, 7, Some("b")), vec!["b", "a", "c"]);
        // A pick that is not a ready model of this plan leads nothing.
        assert_eq!(
            plan.order(&AllHealthy, 0, Some("elsewhere")),
            vec!["a", "b", "c"]
        );
        let cooling = Health(HashMap::from([("c", Some(100))]), HashMap::new());
        assert_eq!(plan.order(&cooling, 0, Some("c")), vec!["a", "b", "c"]);
        plan.models[1].enabled = false;
        assert_eq!(plan.order(&AllHealthy, 0, Some("b")), vec!["a", "c"]);
        // No other strategy reads the lead.
        let fallback = ModelPlan::fallback(["a", "b"]);
        assert_eq!(fallback.order(&AllHealthy, 0, Some("b")), vec!["a", "b"]);
        assert!(!fallback.strategy.is_routed());
    }

    #[test]
    fn only_a_routed_plan_with_two_ready_models_is_a_question() {
        let mut plan = ModelPlan {
            strategy: ModelStrategy::AutoRoute,
            effort: None,
            models: vec![
                ModelChoice::suited("haiku", "quick edits"),
                ModelChoice::new("opus"),
                ModelChoice::new("  "),
            ],
        };
        assert_eq!(
            plan.routable(&AllHealthy),
            vec![("haiku", Some("quick edits")), ("opus", None)]
        );
        let cooling = Health(HashMap::from([("opus", Some(100))]), HashMap::new());
        assert!(plan.routable(&cooling).is_empty(), "one ready model");
        plan.strategy = ModelStrategy::Fallback;
        assert!(plan.routable(&AllHealthy).is_empty(), "not routed");
        let wire = serde_json::to_value(ModelStrategy::AutoRoute).unwrap();
        assert_eq!(wire, serde_json::json!("auto_route"));
    }

    #[test]
    fn everything_cooling_still_yields_the_whole_list() {
        let plan = ModelPlan::fallback(["a", "b"]);
        let h = Health(
            HashMap::from([("a", Some(50)), ("b", Some(10))]),
            HashMap::new(),
        );
        assert_eq!(plan.order(&h, 0, None), vec!["b", "a"]);
    }

    #[test]
    fn a_pinned_plan_is_one_model_and_an_empty_plan_pins_nothing() {
        let pinned = ModelPlan::pinned("sonnet");
        assert_eq!(pinned.models.len(), 1);
        assert_eq!(pinned.strategy, ModelStrategy::Fallback);
        assert!(!pinned.is_empty());
        assert_eq!(pinned.first(&AllHealthy, 0), Some("sonnet"));
        assert!(ModelPlan::default().is_empty());
        assert_eq!(
            ModelPlan::default().order(&AllHealthy, 0, None),
            Vec::<&str>::new()
        );
        assert_eq!(ModelPlan::default().first(&AllHealthy, 3), None);
        // Blank names are nothing: a plan of them is empty and orders nothing.
        let blank = ModelPlan::fallback(["", "   "]);
        assert!(blank.is_empty());
        assert_eq!(blank.order(&AllHealthy, 0, None), Vec::<&str>::new());
        // A weighted choice keeps its weight; `new` weighs one.
        assert_eq!(ModelChoice::weighted("x", 4).weight, 4);
        assert_eq!(ModelChoice::new("x").weight, 1);
        assert!(ModelChoice::new("x").enabled);
    }

    #[test]
    fn the_wire_fills_weight_and_enabled_and_the_plan_round_trips() {
        let c: ModelChoice = serde_json::from_str(r#"{"model":"opus"}"#).unwrap();
        assert_eq!(c, ModelChoice::new("opus"));
        let plan = ModelPlan {
            strategy: ModelStrategy::Weighted,
            effort: None,
            models: vec![
                ModelChoice::weighted("a", 3),
                ModelChoice {
                    enabled: false,
                    ..ModelChoice::new("b")
                },
            ],
        };
        let json = serde_json::to_string(&plan).unwrap();
        assert_eq!(serde_json::from_str::<ModelPlan>(&json).unwrap(), plan);
        let empty: ModelPlan = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, ModelPlan::default());
    }

    #[test]
    fn an_effort_is_on_the_wire_only_when_it_is_stated() {
        // Nobody stated one: the wire is what it was.
        let silent = ModelPlan::fallback(["opus", "sonnet"]);
        assert_eq!(silent.effort, None);
        assert!(silent.models.iter().all(|c| c.effort.is_none()));
        assert_eq!(
            serde_json::to_value(&silent).unwrap(),
            serde_json::json!({
                "strategy": "fallback",
                "models": [
                    { "model": "opus", "weight": 1, "enabled": true },
                    { "model": "sonnet", "weight": 1, "enabled": true },
                ],
            })
        );
        assert_eq!(ModelPlan::pinned("opus").effort, None);
        assert_eq!(ModelChoice::weighted("x", 2).effort, None);
        assert_eq!(ModelChoice::suited("x", "design work").effort, None);

        // The plan's and a model's, each in its own word.
        let stated = ModelPlan {
            strategy: ModelStrategy::Fallback,
            effort: Some(EffortChoice::High),
            models: vec![
                ModelChoice::new("opus").at(EffortChoice::Max),
                ModelChoice::new("sonnet").at(EffortChoice::Xhigh),
                ModelChoice::new("haiku").at(EffortChoice::Auto),
            ],
        };
        let wire = serde_json::to_value(&stated).unwrap();
        assert_eq!(wire["effort"], serde_json::json!("high"));
        assert_eq!(wire["models"][0]["effort"], serde_json::json!("max"));
        assert_eq!(wire["models"][1]["effort"], serde_json::json!("xhigh"));
        assert_eq!(wire["models"][2]["effort"], serde_json::json!("auto"));
        assert_eq!(serde_json::from_value::<ModelPlan>(wire).unwrap(), stated);
        assert_eq!(
            ModelPlan::fallback(["opus"]).at(EffortChoice::Low).effort,
            Some(EffortChoice::Low)
        );

        // A word that is no effort is refused, on the plan and on a model.
        assert!(serde_json::from_str::<ModelPlan>(r#"{"effort":"ultra"}"#).is_err());
        assert!(
            serde_json::from_str::<ModelChoice>(r#"{"model":"opus","effort":"x_high"}"#).is_err()
        );
    }

    #[test]
    fn a_models_own_effort_is_read_by_its_id() {
        let plan = ModelPlan {
            strategy: ModelStrategy::Fallback,
            effort: Some(EffortChoice::Medium),
            models: vec![
                ModelChoice::new("opus").at(EffortChoice::Max),
                ModelChoice::new("sonnet"),
                // The same id twice: the first entry answers.
                ModelChoice::new("opus").at(EffortChoice::Low),
            ],
        };
        assert_eq!(plan.effort_of("opus"), Some(EffortChoice::Max));
        assert_eq!(plan.effort_of("sonnet"), None);
        // A model the plan does not hold states nothing.
        assert_eq!(plan.effort_of("elsewhere"), None);
        assert_eq!(ModelPlan::default().effort_of("opus"), None);
    }

    #[test]
    fn the_step_then_the_model_then_the_plan_then_the_setting_decide() {
        use crate::effort::Effort;
        let setting = EffortChoice::High;
        let mut plan = ModelPlan::fallback(["opus", "sonnet"]);
        // Nobody said: the setting.
        assert_eq!(
            plan.resolve_effort(Some("opus"), None, setting),
            Resolved::Level(Effort::High)
        );
        // The plan's wins over the setting.
        plan.effort = Some(EffortChoice::Max);
        assert_eq!(
            plan.resolve_effort(Some("opus"), None, setting),
            Resolved::Level(Effort::Max)
        );
        // A model's own wins over the plan's, for that model alone.
        plan.models[0].effort = Some(EffortChoice::Low);
        assert_eq!(
            plan.resolve_effort(Some("opus"), None, setting),
            Resolved::Level(Effort::Low)
        );
        assert_eq!(
            plan.resolve_effort(Some("sonnet"), None, setting),
            Resolved::Level(Effort::Max)
        );
        // The step's pin wins over all of them.
        assert_eq!(
            plan.resolve_effort(Some("opus"), Some(EffortChoice::Xhigh), setting),
            Resolved::Level(Effort::Xhigh)
        );
        // A pinned model the plan does not hold, and the harness's default
        // model, state nothing: the plan's.
        assert_eq!(
            plan.resolve_effort(Some("elsewhere"), None, setting),
            Resolved::Level(Effort::Max)
        );
        assert_eq!(
            plan.resolve_effort(None, None, setting),
            Resolved::Level(Effort::Max)
        );
        // `auto` on the model falls back to the plan's level.
        plan.models[1].effort = Some(EffortChoice::Auto);
        assert_eq!(
            plan.resolve_effort(Some("sonnet"), None, setting),
            Resolved::Auto {
                fallback: Effort::Max
            }
        );
    }

    #[test]
    fn the_judge_is_asked_only_when_a_model_that_may_launch_comes_to_auto() {
        let high = EffortChoice::High;
        let auto = EffortChoice::Auto;
        let mut plan = ModelPlan::fallback(["opus", "sonnet"]);
        // Every link a level.
        assert!(!plan.asks_effort(None, high));
        assert!(!plan.asks_effort(Some(EffortChoice::Max), auto));
        // The setting alone says `auto`.
        assert!(plan.asks_effort(None, auto));
        // The plan names a level over it.
        plan.effort = Some(EffortChoice::Low);
        assert!(!plan.asks_effort(None, auto));
        // One model says `auto`: that attempt may ask.
        plan.models[1].effort = Some(auto);
        assert!(plan.asks_effort(None, high));
        // A step's level is over every model's.
        assert!(!plan.asks_effort(Some(EffortChoice::Medium), high));
        // A step's `auto` is over every model's level.
        plan.models[1].effort = Some(EffortChoice::Max);
        assert!(plan.asks_effort(Some(auto), high));
        // A model switched off, or with no name, launches nothing.
        let mut off = ModelPlan::fallback(["opus", "sonnet", "  "]);
        off.models[1].effort = Some(auto);
        off.models[1].enabled = false;
        off.models[2].effort = Some(auto);
        assert!(!off.asks_effort(None, high));
        // An empty plan is the harness's default model: the plan's, then the
        // setting.
        let mut empty = ModelPlan::default();
        assert!(!empty.asks_effort(None, high));
        assert!(empty.asks_effort(None, auto));
        assert!(empty.asks_effort(Some(auto), high));
        empty.effort = Some(auto);
        assert!(empty.asks_effort(None, high));
        assert!(!empty.asks_effort(Some(EffortChoice::Low), high));
        // A plan whose every model is switched off launches the default too.
        let mut none_on = ModelPlan::fallback(["opus"]).at(auto);
        none_on.models[0].enabled = false;
        assert!(none_on.asks_effort(None, high));
    }

    // added by the coverage pass: model_plan.rs

    #[test]
    fn the_identity_health_view_counts_nothing_in_flight_and_cools_nothing_down() {
        assert_eq!(AllHealthy.in_flight("claude-opus-5-5"), 0);
        assert_eq!(AllHealthy.cooldown_until("claude-opus-5-5"), None);
    }

    // added by the coverage pass: b5-model_plan.rs
    #[test]
    fn nothing_ready_rotates_and_weighs_to_nothing() {
        let none: [&ModelChoice; 0] = [];
        assert!(rotate(&none, 3).is_empty());
        assert!(weighted(&none, 3).is_empty());
    }
}
