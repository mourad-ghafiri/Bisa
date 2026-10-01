//! Listening, as the terminal shows it: who hears a workflow's start events,
//! where a host stands, and a public hook's secret — once.
//!
//! A **listener** is a start event armed for a **host**: a library workflow
//! a person turned On, or a goal whose workflow begins on events. The verbs
//! that show one — `bisa workflow on | off | listeners | hook-secret`,
//! `bisa run` when it arms a goal, `bisa status`, `bisa approve` — share the
//! lines built here, so a listener reads the same wherever it is printed.
//!
//! **The node answers, the CLI prints.** Every renderer takes the node's own
//! shapes as JSON (`ListenerView`, `HookSecret`, `Listening`) and decides
//! nothing. Without a daemon the same shapes are read off an embedded engine
//! that hears nothing ([`crate::ctx::Ctx::quiet_engine`]) by the node's own
//! readers (`bisa_node::listening::listeners_of`, `every_listener`): a view
//! is built in one place, whoever asks.
//!
//! **A secret is shown once.** The turn that mints it prints it
//! ([`secret_lines`]) and so does a rotation; no other line here carries one.

use crate::ctx::Ctx;
use anyhow::Result;
use bisa_core::{Budget, ListenerHost, Listening, PauseReason, Text};
use bisa_engine::Engine;
use bisa_node::dto::ListenerView;
use serde_json::Value;
use std::collections::BTreeMap;

/// What stands where a moment is not known yet, or an event never came.
const NEVER: &str = "—";

/// The kind of host a hook's secret belongs to, as `hook-secret` names it.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookHost {
    /// A library workflow that is On.
    Workflow,
    /// A goal that listens.
    Goal,
}

impl HookHost {
    pub fn as_str(self) -> &'static str {
        match self {
            HookHost::Workflow => "workflow",
            HookHost::Goal => "goal",
        }
    }

    /// The node's route prefix for a host of this kind: `/workflows/<id>` or
    /// `/goals/<id>`.
    pub fn route(self, id: &str) -> String {
        match self {
            HookHost::Workflow => format!("/workflows/{id}"),
            HookHost::Goal => format!("/goals/{id}"),
        }
    }
}

/// The rows of a JSON list; none for anything that is not one — an absent
/// `secrets` is no secret.
pub fn rows(list: &Value) -> &[Value] {
    match list.as_array() {
        Some(rows) => rows,
        None => &[],
    }
}

// ---------------------------------------------------------------------------
// Reading the listeners
// ---------------------------------------------------------------------------

/// The route a host's listeners are read from — every listener's, when no
/// host is named.
pub fn listeners_route(host: Option<&ListenerHost>) -> String {
    match host {
        Some(ListenerHost::Workspace { workflow }) => format!("/workflows/{workflow}/listeners"),
        Some(ListenerHost::Goal { goal }) => format!("/goals/{goal}/listeners"),
        None => "/listeners".to_string(),
    }
}

/// The listeners of `host` — of every host that listens, when none is named
/// — as the node lists them (`ListenerView`): from the node, or read off an
/// embedded engine that hears nothing.
pub async fn fetch(ctx: &Ctx, host: Option<&ListenerHost>) -> Result<Vec<Value>> {
    if let Some(client) = ctx.node_client().await {
        let listed = client.get(&listeners_route(host)).await?;
        return Ok(rows(&listed).to_vec());
    }
    let engine = ctx.quiet_engine().await?;
    let views = match host {
        Some(host) => listeners_of(&engine, host),
        None => every_listener(&engine),
    };
    engine.shutdown().await;
    views
}

/// The listeners of one host, as `GET …/listeners` lists them — the node's
/// own reading of an engine, as the rows every renderer here takes. A host
/// nobody has is refused in the store's words.
pub fn listeners_of(engine: &Engine, host: &ListenerHost) -> Result<Vec<Value>> {
    as_rows(&bisa_node::listening::listeners_of(engine, host)?)
}

/// Every listener of every host that listens, as `GET /listeners` lists them.
pub fn every_listener(engine: &Engine) -> Result<Vec<Value>> {
    as_rows(&bisa_node::listening::every_listener(engine)?)
}

/// The node's views, as it writes them on the wire.
fn as_rows(views: &[ListenerView]) -> Result<Vec<Value>> {
    views
        .iter()
        .map(|view| Ok(serde_json::to_value(view)?))
        .collect()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// A moment as the terminal prints one — unix seconds, as every other verb
/// does — or a dash where there is none.
fn moment(at: &Value) -> String {
    at.as_u64()
        .map_or_else(|| NEVER.to_string(), |at| at.to_string())
}

/// A sentence the node sent as data (`Text`), said in the person's language;
/// nothing for a value that is none.
fn said(text: &Value) -> String {
    serde_json::from_value::<Text>(text.clone())
        .map(|text| bisa_i18n::say(&text))
        .unwrap_or_default()
}

/// One listener (`ListenerView`) in lines: what it is and where it stands,
/// then how its hook is called when it is one, then what is wrong with it
/// when something is.
pub fn listener_block(view: &Value) -> Vec<String> {
    let word = |key: &str| view[key].as_str().unwrap_or_default().to_string();
    let count = |key: &str| view[key].as_u64().unwrap_or(0).to_string();
    let mut lines = vec![bisa_i18n::say(&bisa_core::text!(
        "cli-listening-row",
        listener = word("listener"),
        event = word("event"),
        summary = said(&view["summary"]),
        next = moment(&view["next_due"]),
        last = moment(&view["last_fired_at"]),
        backlog = count("backlog"),
        live = count("live_runs")
    ))];
    if let Some(path) = view["local_hook"].as_str() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-listening-hook-local",
            path = path.to_string()
        )));
    }
    if let Some(path) = view["public_hook"]["path"].as_str() {
        let minted = view["public_hook"]["has_secret"].as_bool().unwrap_or(false);
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-listening-hook-public",
            path = path.to_string(),
            secret = secret_state(minted)
        )));
    }
    if let Some(why) = view["failed"].as_str() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-listening-failed",
            why = why.to_string()
        )));
    }
    lines
}

/// Whether a public hook's secret is minted — never the secret.
fn secret_state(minted: bool) -> String {
    if minted {
        bisa_i18n::say(&bisa_core::text!("cli-listening-secret-minted"))
    } else {
        bisa_i18n::say(&bisa_core::text!("cli-listening-secret-none"))
    }
}

/// A table of listeners: one block each, in the order they were answered.
pub fn listener_lines(views: &[Value]) -> Vec<String> {
    views.iter().flat_map(listener_block).collect()
}

/// A public hook's secret, as it is shown — once — with its path, how it
/// authenticates a call and how a lost one is replaced. `id` is the host's.
pub fn secret_lines(host: HookHost, id: &str, secrets: &[Value]) -> Vec<String> {
    secrets
        .iter()
        .map(|secret| {
            let word = |key: &str| secret[key].as_str().unwrap_or_default().to_string();
            bisa_i18n::say(&bisa_core::text!(
                "cli-listening-hook-secret",
                step = word("step"),
                path = word("path"),
                secret = word("secret"),
                host = host.as_str(),
                id = id.to_string()
            ))
        })
        .collect()
}

/// Where a host stands (`Listening`): listening since when — or paused, since
/// when and why — and what it listens with. Nothing for a host that does not
/// listen.
pub fn standing_lines(listening: Option<&Listening>) -> Vec<String> {
    let Some(listening) = listening else {
        return Vec::new();
    };
    let mut lines = vec![match &listening.paused {
        None => bisa_i18n::say(&bisa_core::text!(
            "cli-listening-since",
            since = listening.since.to_string()
        )),
        Some(paused) => bisa_i18n::say(&bisa_core::text!(
            "cli-listening-paused",
            at = paused.at.to_string(),
            why = pause_words(&paused.reason)
        )),
    }];
    if !listening.inputs.is_empty() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-listening-with",
            inputs = inputs_words(&listening.inputs)
        )));
    }
    if let Some(budget) = &listening.budget {
        lines.push(budget_words(budget));
    }
    lines
}

/// Why a host stopped hearing its events without being turned off.
fn pause_words(reason: &PauseReason) -> String {
    match reason {
        PauseReason::RunFailed { run } => bisa_i18n::say(&bisa_core::text!(
            "cli-listening-paused-run-failed",
            run = run.to_string()
        )),
        PauseReason::BudgetSpent => {
            bisa_i18n::say(&bisa_core::text!("cli-listening-paused-budget-spent"))
        }
    }
}

/// `name=value` pairs, as `--input` takes them.
fn inputs_words(inputs: &BTreeMap<String, Value>) -> String {
    inputs
        .iter()
        .map(|(name, value)| match value {
            Value::String(text) => format!("{name}={text}"),
            other => format!("{name}={other}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A listening's own budget, in words: each ceiling it sets on a run — or
/// that it sets none, whatever the workspace's default says.
fn budget_words(budget: &Budget) -> String {
    let ceilings: Vec<String> = [
        budget.max_tokens.map(|n| {
            bisa_i18n::say(&bisa_core::text!(
                "cli-listening-budget-tokens",
                n = n.to_string()
            ))
        }),
        budget.max_usd_cents.map(|n| {
            bisa_i18n::say(&bisa_core::text!(
                "cli-listening-budget-cents",
                n = n.to_string()
            ))
        }),
        budget.max_wall_clock_secs.map(|n| {
            bisa_i18n::say(&bisa_core::text!(
                "cli-listening-budget-secs",
                n = n.to_string()
            ))
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    if ceilings.is_empty() {
        return bisa_i18n::say(&bisa_core::text!("cli-listening-budget-none"));
    }
    bisa_i18n::say(&bisa_core::text!(
        "cli-listening-budget",
        ceilings = ceilings.join(", ")
    ))
}

/// One way a workflow begins (`StartSummary`: `{step, event, summary}`), in a
/// line.
pub fn start_line(start: &Value) -> String {
    bisa_i18n::say(&bisa_core::text!(
        "cli-listening-start",
        step = start["step"].as_str().unwrap_or_default().to_string(),
        event = start["event"].as_str().unwrap_or_default().to_string(),
        summary = said(&start["summary"])
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{Paused, RunId};
    use serde_json::json;

    /// A line the catalog has: a miss renders as the message's id.
    fn sentence(line: &str) {
        assert!(
            !line.is_empty() && !line.contains("cli-listening"),
            "a line the catalog does not say: {line:?}"
        );
    }

    fn hourly() -> Value {
        json!({
            "listener": "workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/hourly",
            "host": "workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "step": "hourly",
            "event": "schedule",
            "summary": {"id": "step-summary-start-every", "args": {"secs": "3600"}},
            "next_due": 1_727_563_600u64,
            "backlog": 0,
            "live_runs": 1,
        })
    }

    #[test]
    fn a_listener_is_one_row_and_its_hook_and_its_trouble_follow() {
        let block = listener_block(&hourly());
        assert_eq!(block.len(), 1, "{block:?}");
        sentence(&block[0]);
        for word in [
            "workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/hourly",
            "schedule",
            "begins every 3600 seconds",
            "1727563600",
        ] {
            assert!(block[0].contains(word), "{word}: {}", block[0]);
        }
        assert!(
            block[0].contains(NEVER),
            "it never fired, and says so: {}",
            block[0]
        );

        let hook = json!({
            "listener": "goal:01DX5ZZKBKACTAV9WEVGEMMVRZ/ticket",
            "host": "goal:01DX5ZZKBKACTAV9WEVGEMMVRZ",
            "step": "ticket",
            "event": "hook",
            "summary": {"id": "step-summary-start-hook-public"},
            "backlog": 2,
            "live_runs": 0,
            "local_hook": "/goals/01DX5ZZKBKACTAV9WEVGEMMVRZ/hooks/ticket",
            "public_hook": {
                "path": "/hooks/goal:01DX5ZZKBKACTAV9WEVGEMMVRZ/ticket",
                "has_secret": true
            },
            "failed": "not armed: input `who` should hold text; it holds nothing",
        });
        let block = listener_block(&hook);
        assert_eq!(block.len(), 4, "{block:?}");
        block.iter().for_each(|line| sentence(line));
        assert!(
            block[1].contains("/goals/01DX5ZZKBKACTAV9WEVGEMMVRZ/hooks/ticket"),
            "{}",
            block[1]
        );
        assert!(
            block[2].contains("/hooks/goal:01DX5ZZKBKACTAV9WEVGEMMVRZ/ticket")
                && block[2].contains("minted"),
            "{}",
            block[2]
        );
        assert!(block[3].contains("not armed"), "{}", block[3]);
        assert_eq!(listener_lines(&[hourly(), hook]).len(), 5);
        assert!(listener_lines(&[]).is_empty());
    }

    #[test]
    fn a_secret_is_said_with_its_path_and_the_way_back() {
        let secrets = [json!({
            "step": "ticket",
            "path": "/hooks/workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket",
            "secret": "ab".repeat(32),
        })];
        let lines = secret_lines(HookHost::Workflow, "01ARZ3NDEKTSV4RRFFQ69G5FAV", &secrets);
        assert_eq!(lines.len(), 1);
        sentence(&lines[0]);
        for word in [
            "/hooks/workspace:01ARZ3NDEKTSV4RRFFQ69G5FAV/ticket",
            &"ab".repeat(32),
            "will not be shown again",
            "bisa workflow hook-secret workflow 01ARZ3NDEKTSV4RRFFQ69G5FAV ticket --rotate",
            "X-Bisa-Token",
            "X-Hub-Signature-256",
            "events.public_hooks",
        ] {
            assert!(lines[0].contains(word), "{word}: {}", lines[0]);
        }
        assert!(secret_lines(HookHost::Goal, "01G", rows(&Value::Null)).is_empty());
        assert_eq!(HookHost::Goal.route("01G"), "/goals/01G");
        assert_eq!(HookHost::Workflow.route("01W"), "/workflows/01W");
    }

    #[test]
    fn a_standing_says_since_when_with_what_and_why_it_paused() {
        assert!(standing_lines(None).is_empty());
        let on = Listening {
            inputs: BTreeMap::from([
                ("who".to_string(), json!("the team")),
                ("every".to_string(), json!(3600)),
            ]),
            budget: Some(Budget {
                max_tokens: Some(10_000),
                ..Budget::default()
            }),
            since: 7,
            paused: None,
        };
        let lines = standing_lines(Some(&on));
        assert_eq!(lines.len(), 3, "{lines:?}");
        lines.iter().for_each(|line| sentence(line));
        assert!(lines[0].contains('7'), "{}", lines[0]);
        assert!(
            lines[1].contains("every=3600") && lines[1].contains("who=the team"),
            "{}",
            lines[1]
        );
        assert_eq!(lines[2], "each run may spend at most 10000 tokens");
        assert_eq!(
            budget_words(&Budget {
                max_tokens: Some(10_000),
                max_usd_cents: Some(500),
                max_wall_clock_secs: Some(3_600),
            }),
            "each run may spend at most 10000 tokens, 500¢, 3600s"
        );
        // A budget that sets nothing is no ceiling, whatever the default says.
        assert_eq!(
            budget_words(&Budget::default()),
            "each run may spend without a ceiling"
        );

        let run = RunId::from_ulid(ulid::Ulid::from_parts(3, 3));
        let paused = Listening {
            inputs: BTreeMap::new(),
            budget: None,
            since: 7,
            paused: Some(Paused {
                reason: PauseReason::RunFailed { run },
                at: 9,
            }),
        };
        let lines = standing_lines(Some(&paused));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("paused") && lines[0].contains(&run.to_string()),
            "{}",
            lines[0]
        );
        let spent = Listening {
            paused: Some(Paused {
                reason: PauseReason::BudgetSpent,
                at: 9,
            }),
            ..paused
        };
        assert!(
            standing_lines(Some(&spent))[0].contains("budget"),
            "{:?}",
            standing_lines(Some(&spent))
        );
    }

    #[test]
    fn a_start_is_said_by_its_event_and_its_summary() {
        let line = start_line(&json!({
            "step": "raised", "event": "signal",
            "summary": {"id": "step-summary-start-signal", "args": {"name": "report.ready"}}
        }));
        sentence(&line);
        assert!(
            line.contains("raised") && line.contains("report.ready"),
            "{line}"
        );
    }
}
