//! One policy out of the built-ins and every settings layer.
//!
//! Settings resolve first-holder-wins, which is right for a number and wrong
//! for a list: a team's rules and this machine's rules should both apply. So
//! the engine hands this module one [`SettingsLayer`] per scope that holds a
//! value, in the order they should be tried after the built-ins, and gets one
//! [`SecurityPolicy`] back. A rule that cannot be read or compiled is a
//! [`Problem`] the settings panel shows — never a reason for the whole policy
//! to fail, because a policy that fails open is worse than one with a gap it
//! names.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::builtin;
use crate::guard::{Guard, GuardRule};
use crate::redact::{RedactRule, Redactor};

/// Which feature a problem belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "SecurityFeature")]
pub enum Feature {
    Redactor,
    Guard,
}

/// A rule the policy could not use, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "PolicyProblem")]
pub struct Problem {
    pub feature: Feature,
    /// The rule's id, or `#<index>` when it could not even be read.
    pub rule: String,
    pub reason: String,
}

/// The rule lists one settings scope holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsLayer {
    pub redact_rules: Vec<RedactRule>,
    pub redact_builtins_off: Vec<String>,
    pub guard_rules: Vec<GuardRule>,
    pub guard_builtins_off: Vec<String>,
}

impl SettingsLayer {
    /// Reads a layer from the four settings values as stored. A rule that is
    /// not a rule is reported by its index and skipped; a list that is not a
    /// list is empty.
    pub fn from_values(
        redact_rules: &Value,
        redact_builtins_off: &Value,
        guard_rules: &Value,
        guard_builtins_off: &Value,
    ) -> (Self, Vec<Problem>) {
        let mut problems = Vec::new();
        let redact = read_rules::<RedactRule>(redact_rules, Feature::Redactor, &mut problems);
        let guard = read_rules::<GuardRule>(guard_rules, Feature::Guard, &mut problems);
        (
            SettingsLayer {
                redact_rules: redact,
                redact_builtins_off: strings(redact_builtins_off),
                guard_rules: guard,
                guard_builtins_off: strings(guard_builtins_off),
            },
            problems,
        )
    }
}

fn read_rules<T: for<'de> Deserialize<'de>>(
    value: &Value,
    feature: Feature,
    problems: &mut Vec<Problem>,
) -> Vec<T> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(
            |(i, item)| match serde_json::from_value::<T>(item.clone()) {
                Ok(rule) => Some(rule),
                Err(e) => {
                    problems.push(Problem {
                        feature,
                        rule: format!("#{i}"),
                        reason: e.to_string(),
                    });
                    None
                }
            },
        )
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The effective policy: every rule, in order, and the compiled matchers.
pub struct SecurityPolicy {
    pub redact_rules: Vec<RedactRule>,
    pub guard_rules: Vec<GuardRule>,
    pub redactor: Redactor,
    pub guard: Guard,
    /// How many of the node's own environment variables are armed as
    /// detectors — a count, never a name or a value.
    pub env_detectors: usize,
}

impl std::fmt::Debug for SecurityPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityPolicy")
            .field("redact_rules", &self.redact_rules.len())
            .field("guard_rules", &self.guard_rules.len())
            .field("env_detectors", &self.env_detectors)
            .finish()
    }
}

impl SecurityPolicy {
    /// Built-ins first — the shipped patterns, then one `EnvValue` detector
    /// per name in `env_names` that says *secret* ([`builtin::env_rules`]),
    /// switched off where any layer says so — then each layer's own rules in
    /// the order given. `env` answers the node's environment for `EnvValue`
    /// detectors; `env_names` is the node's variable names when the automatic
    /// detectors are on, and empty when they are off.
    pub fn from_layers(
        layers: &[SettingsLayer],
        env: &dyn Fn(&str) -> Option<String>,
        env_names: &[String],
    ) -> (Self, Vec<Problem>) {
        let mut redact_rules = builtin::redact_rules();
        redact_rules.extend(builtin::env_rules(env_names));
        let mut guard_rules = builtin::guard_rules();
        for rule in &mut redact_rules {
            if layers
                .iter()
                .any(|l| l.redact_builtins_off.iter().any(|id| id == &rule.id))
            {
                rule.enabled = false;
            }
        }
        for rule in &mut guard_rules {
            if layers
                .iter()
                .any(|l| l.guard_builtins_off.iter().any(|id| id == &rule.id))
            {
                rule.enabled = false;
            }
        }
        let mut problems = Vec::new();
        // A rule's id is its name in the status and in *builtins off*; two
        // rules under one id would leave the later one dead in silence, so
        // the later one is dropped and the collision is a problem on the
        // status — a policy with a gap it names, never a gap it hides.
        for layer in layers {
            for rule in layer.redact_rules.iter().cloned().map(user) {
                if redact_rules.iter().any(|r| r.id == rule.id) {
                    problems.push(Problem {
                        feature: Feature::Redactor,
                        rule: rule.id.clone(),
                        reason: "another rule already has this id; this one is not applied".into(),
                    });
                    continue;
                }
                redact_rules.push(rule);
            }
            for rule in layer.guard_rules.iter().cloned().map(user_guard) {
                if guard_rules.iter().any(|r| r.id == rule.id) {
                    problems.push(Problem {
                        feature: Feature::Guard,
                        rule: rule.id.clone(),
                        reason: "another rule already has this id; this one is not applied".into(),
                    });
                    continue;
                }
                guard_rules.push(rule);
            }
        }
        let env_detectors = redact_rules
            .iter()
            .filter(|r| r.enabled && r.origin == crate::redact::Origin::Builtin)
            .filter(|r| match &r.detector {
                crate::redact::Detector::EnvValue { name } => {
                    env(name).is_some_and(|v| v.trim().len() >= crate::redact::AUTO_ENV_MIN_LEN)
                }
                crate::redact::Detector::Pattern { .. } => false,
            })
            .count();
        let (redactor, bad) = Redactor::compile(&redact_rules, env);
        problems.extend(bad.into_iter().map(|b| Problem {
            feature: Feature::Redactor,
            rule: b.rule,
            reason: b.reason,
        }));
        let (guard, bad) = Guard::compile(&guard_rules);
        problems.extend(bad.into_iter().map(|b| Problem {
            feature: Feature::Guard,
            rule: b.rule,
            reason: b.reason,
        }));
        (
            SecurityPolicy {
                redact_rules,
                guard_rules,
                redactor,
                guard,
                env_detectors,
            },
            problems,
        )
    }

    /// The built-in patterns alone — what a node runs before anyone writes a
    /// setting, with no environment read.
    pub fn builtin(env: &dyn Fn(&str) -> Option<String>) -> Self {
        Self::from_layers(&[], env, &[]).0
    }
}

/// A person's rule is a person's rule whatever its JSON claimed.
fn user(mut rule: RedactRule) -> RedactRule {
    rule.origin = crate::redact::Origin::User;
    rule
}

fn user_guard(mut rule: GuardRule) -> GuardRule {
    rule.origin = crate::redact::Origin::User;
    rule
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::{Action, Host, Matcher, ToolCall, Verdict};
    use crate::redact::{Detector, Origin, Vault};
    use serde_json::json;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn the_builtins_alone_are_a_policy() {
        let policy = SecurityPolicy::builtin(&no_env);
        assert!(!policy.redact_rules.is_empty() && !policy.guard_rules.is_empty());
        assert!(policy
            .redact_rules
            .iter()
            .all(|r| r.origin == Origin::Builtin));
        assert!(!policy.redactor.is_empty() && !policy.guard.is_empty());
    }

    #[test]
    fn layers_switch_builtins_off_and_add_rules_in_order() {
        let (team, problems) = SettingsLayer::from_values(
            &json!([{ "id": "team_key", "label": "Team key", "detector": { "kind": "pattern", "regex": "TEAMKEY-[0-9]{6}" }, "origin": "builtin" }]),
            &json!(["github_token"]),
            &json!([{ "id": "ask_push", "label": "Ask before pushing", "action": "ask", "matcher": { "kind": "command", "regex": "^git push" } }]),
            &json!([]),
        );
        assert!(problems.is_empty(), "{problems:?}");
        let (machine, _) = SettingsLayer::from_values(
            &json!([]),
            &json!([]),
            &json!([{ "id": "allow_push_here", "label": "This machine may push", "action": "allow", "matcher": { "kind": "command", "regex": "^git push origin" } }]),
            &json!(["publishing"]),
        );
        let (policy, problems) = SecurityPolicy::from_layers(&[team, machine], &no_env, &[]);
        assert!(problems.is_empty(), "{problems:?}");
        let gh = policy
            .redact_rules
            .iter()
            .find(|r| r.id == "github_token")
            .unwrap();
        assert!(!gh.enabled, "a layer switched the built-in off");
        let team_key = policy
            .redact_rules
            .iter()
            .find(|r| r.id == "team_key")
            .unwrap();
        assert_eq!(
            team_key.origin,
            Origin::User,
            "a person's rule cannot claim to be built in"
        );
        assert!(policy
            .guard_rules
            .iter()
            .find(|r| r.id == "publishing")
            .is_some_and(|r| !r.enabled));
        let ids: Vec<_> = policy
            .guard_rules
            .iter()
            .filter(|r| r.origin == Origin::User)
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["ask_push", "allow_push_here"],
            "team rules come before the machine's"
        );
        let input = json!({ "command": "git push origin main" });
        let call = ToolCall {
            tool: "Bash",
            input: &input,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical: None,
        };
        assert_eq!(
            policy.guard.evaluate(&call),
            Verdict::Ask {
                rule: "ask_push".into()
            }
        );
        let out = policy.redactor.redact(
            &Vault::new("n"),
            "TEAMKEY-123456 and ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        );
        assert_eq!(
            out.kinds,
            vec!["team_key"],
            "the switched-off built-in redacts nothing"
        );
    }

    #[test]
    fn what_cannot_be_read_or_compiled_is_a_named_problem_not_a_failure() {
        let (layer, problems) = SettingsLayer::from_values(
            &json!([{ "id": "ok", "label": "ok", "detector": { "kind": "pattern", "regex": "(" } }, 42, { "label": "no id" }]),
            &json!("not a list"),
            &json!([{ "id": "g", "label": "g", "action": "deny", "matcher": { "kind": "path", "glob": "[" } }]),
            &json!(null),
        );
        assert_eq!(problems.len(), 2);
        assert!(problems
            .iter()
            .all(|p| p.feature == Feature::Redactor && p.rule.starts_with('#')));
        assert!(layer.redact_builtins_off.is_empty());
        let (policy, problems) = SecurityPolicy::from_layers(&[layer], &no_env, &[]);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems
            .iter()
            .any(|p| p.feature == Feature::Redactor && p.rule == "ok"));
        assert!(problems
            .iter()
            .any(|p| p.feature == Feature::Guard && p.rule == "g"));
        assert!(!policy.guard.is_empty(), "the built-ins still stand");
    }

    #[test]
    fn the_environment_names_become_switchable_builtin_detectors() {
        let names: Vec<String> = ["PATH", "FAKE_API_KEY", "FAKE_TOKEN", "SHORT_SECRET"]
            .into_iter()
            .map(String::from)
            .collect();
        let env = |name: &str| match name {
            "FAKE_API_KEY" => Some("fake-api-key-value".to_string()),
            "FAKE_TOKEN" => Some("fake-token-value".to_string()),
            "SHORT_SECRET" => Some("abc".to_string()),
            _ => None,
        };
        let (policy, problems) = SecurityPolicy::from_layers(&[], &env, &names);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(policy.env_detectors, 2, "the short value arms nothing");
        assert!(policy
            .redact_rules
            .iter()
            .any(|r| r.id == "env:FAKE_API_KEY" && r.origin == Origin::Builtin));
        let out = policy
            .redactor
            .redact(&Vault::new("n"), "fake-api-key-value fake-token-value abc");
        assert_eq!(out.count, 2);
        assert!(out.text.ends_with(" abc"));

        let (layer, _) = SettingsLayer::from_values(
            &json!([]),
            &json!(["env:FAKE_TOKEN"]),
            &json!([]),
            &json!([]),
        );
        let (policy, _) = SecurityPolicy::from_layers(&[layer], &env, &names);
        assert_eq!(policy.env_detectors, 1, "a layer switched one off by name");
        let out = policy
            .redactor
            .redact(&Vault::new("n"), "fake-api-key-value fake-token-value");
        assert_eq!(out.kinds, vec!["env:FAKE_API_KEY"]);

        let (policy, _) = SecurityPolicy::from_layers(&[], &env, &[]);
        assert_eq!(policy.env_detectors, 0, "no names, no detectors");
    }

    #[test]
    fn a_layer_rule_round_trips_through_json() {
        let rule = GuardRule {
            id: "x".into(),
            label: "x".into(),
            enabled: true,
            action: Action::Classify,
            matcher: Matcher::Tool {
                name: "WebFetch".into(),
            },
            hint: None,
            applies_to: None,
            origin: Origin::User,
        };
        let json = serde_json::to_value(&rule).unwrap();
        assert!(
            json.get("applies_to").is_none(),
            "a rule that applies everywhere is stored as it always was: {json}"
        );
        let back: GuardRule = serde_json::from_value(json).unwrap();
        assert_eq!(back, rule);
        let scoped = GuardRule {
            applies_to: Some(Host::Terminal),
            ..rule.clone()
        };
        let json = serde_json::to_value(&scoped).unwrap();
        assert_eq!(json["applies_to"], serde_json::json!("terminal"));
        let back: GuardRule = serde_json::from_value(json).unwrap();
        assert_eq!(back, scoped);
        let stored_before: GuardRule = serde_json::from_value(serde_json::json!({
            "id": "old", "label": "old", "action": "deny",
            "matcher": {"kind": "command", "regex": "^x"}
        }))
        .unwrap();
        assert_eq!(
            stored_before.applies_to, None,
            "a rule written before the field reads as everywhere"
        );
        let rule = RedactRule {
            id: "e".into(),
            label: "e".into(),
            enabled: false,
            detector: Detector::EnvValue {
                name: "FAKE".into(),
            },
            origin: Origin::User,
        };
        let back: RedactRule =
            serde_json::from_value(serde_json::to_value(&rule).unwrap()).unwrap();
        assert_eq!(back, rule);
    }

    /// A rule under an id already taken — a built-in's, or another scope's
    /// — is not applied, and the status names it: the later one would
    /// otherwise be dead policy nobody is told about.
    #[test]
    fn a_rule_whose_id_is_taken_is_dropped_and_named() {
        let mine = |id: &str, tool: &str| GuardRule {
            id: id.into(),
            label: id.into(),
            enabled: true,
            action: Action::Deny,
            matcher: Matcher::Tool { name: tool.into() },
            hint: None,
            applies_to: None,
            origin: Origin::User,
        };
        let workspace = SettingsLayer {
            guard_rules: vec![mine("publishing", "WebFetch"), mine("mine", "Bash")],
            ..Default::default()
        };
        let machine = SettingsLayer {
            guard_rules: vec![mine("mine", "Read")],
            redact_rules: vec![RedactRule {
                id: "github_token".into(),
                label: "mine too".into(),
                enabled: true,
                detector: Detector::Pattern { regex: "x+".into() },
                origin: Origin::User,
            }],
            ..Default::default()
        };
        let (policy, problems) = SecurityPolicy::from_layers(&[workspace, machine], &|_| None, &[]);
        let mut named: Vec<(Feature, String)> = problems
            .iter()
            .map(|p| (p.feature, p.rule.clone()))
            .collect();
        named.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(
            named,
            vec![
                (Feature::Redactor, "github_token".to_string()),
                (Feature::Guard, "mine".to_string()),
                (Feature::Guard, "publishing".to_string()),
            ],
            "{problems:?}"
        );
        assert!(problems
            .iter()
            .all(|p| p.reason.contains("already has this id")));
        // One rule per id stands: the built-in, and the workspace's `mine`.
        assert_eq!(
            policy
                .guard_rules
                .iter()
                .filter(|r| r.id == "publishing")
                .count(),
            1
        );
        assert_eq!(
            policy.guard_rules.iter().filter(|r| r.id == "mine").count(),
            1
        );
        let kept = policy.guard_rules.iter().find(|r| r.id == "mine").unwrap();
        assert!(
            matches!(&kept.matcher, Matcher::Tool { name } if name == "Bash"),
            "the first scope's"
        );
        assert_eq!(
            policy
                .redact_rules
                .iter()
                .filter(|r| r.id == "github_token")
                .count(),
            1
        );
        assert!(
            policy
                .redact_rules
                .iter()
                .any(|r| r.id == "github_token" && r.origin == Origin::Builtin),
            "the built-in stands, not the user's copy"
        );
    }

    // added by the coverage pass: policy.rs

    #[test]
    fn a_policy_debugs_as_counts_and_never_as_a_rule() {
        let policy = SecurityPolicy::builtin(&no_env);
        let debug = format!("{policy:?}");
        assert!(debug.contains("redact_rules:"), "{debug}");
        assert!(debug.contains("guard_rules:"), "{debug}");
        assert!(debug.contains("env_detectors:"), "{debug}");
        assert!(!debug.contains("ghp_"), "{debug}");
    }

    // added by the coverage pass: policy2.rs

    #[test]
    fn a_rule_list_that_is_not_a_list_is_empty_and_no_problem() {
        let (layer, problems) =
            SettingsLayer::from_values(&json!("not a list"), &json!(null), &json!(7), &json!(null));
        assert!(problems.is_empty(), "{problems:?}");
        assert!(layer.redact_rules.is_empty());
        assert!(layer.guard_rules.is_empty());
    }
}
