//! `bisa inbox`: what concerns the person, one row a thing — the rows the
//! node's `GET /inbox` holds, printed with the command that settles each
//! ask.
//!
//! The rows come from one builder (`bisa_engine::inbox`): asked of the node
//! when one runs, which knows the gates open right now, and built here from
//! the workspace alone otherwise — the same rows in the same order, minus
//! what only a running engine has (a live gate's id, a terminal waiting at
//! its prompt). Every durable ask, notice and read mark is in both.
//!
//! [`hint_of`] is the one place that decides which command is printed under
//! an ask, so the line a person copies is a line that works.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::target;
use anyhow::Result;
use serde_json::{json, Value};

/// `bisa inbox`.
pub async fn inbox(ctx: &Ctx, out: &Out) -> Result<()> {
    let rows: Vec<Value> = if let Some(client) = ctx.node_client().await {
        let v = client.get("/inbox").await?;
        v["rows"].as_array().cloned().unwrap_or_default()
    } else {
        offline_rows(&ctx.workspace()?)?
    };
    render(out, &rows);
    Ok(())
}

/// The rows with no node: the builder's, over nothing live.
fn offline_rows(ws: &bisa_store::Workspace) -> Result<Vec<Value>> {
    let live = bisa_engine::inbox::LiveView::none();
    let say = |text: &bisa_core::Text| bisa_i18n::say(text);
    let reader = bisa_engine::inbox::Reader {
        ws,
        live: &live,
        say: &say,
    };
    let rows = bisa_engine::inbox::rows(&reader)?;
    Ok(rows
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<Value>, _>>()?)
}

/// The command that settles one ask of a row, as a person types it.
///
/// - a held `wait` step is released, by its step: `approve --no` on one is
///   refused, so it is never offered;
/// - an answer-shaped ask is answered, a yes-or-no one approved;
/// - when the home owes several asks, this one is named: by `--step` for a
///   step's, by `--subject` for any other — an unnamed verb is refused there
///   rather than guessed;
/// - run inputs are offered for an adoption alone: approving it starts the
///   run with them, and nothing else takes any.
///
/// `siblings` is every ask of the row, this one among them.
pub fn hint_of(row_key: &str, ask: &Value, siblings: &[Value]) -> String {
    let id = target::hint_id(row_key, ask);
    let subject = ask["subject"].as_str().unwrap_or("");
    let step = ask["step"].as_str();
    if let (true, Some(step)) = (subject.starts_with("release:"), step) {
        // for the machine: a command to type, the same in every language
        return format!("bisa step release {id} {step}");
    }
    let several = siblings.iter().filter(|s| s["home"] == ask["home"]).count() > 1;
    let named = match step {
        _ if !several => String::new(),
        Some(step) => format!(" --step {step}"),
        None if subject.is_empty() => String::new(),
        None => format!(" --subject {subject}"),
    };
    if ask["expects"]["kind"].as_str() == Some("answer") {
        let offered = ask["expects"]["options"]
            .as_array()
            .is_some_and(|options| !options.is_empty());
        // The escape hatch is always shown, whether or not the asker
        // offered options: it is the platform's, so a narrower question
        // cannot take it away.
        return if offered {
            format!("bisa answer {id} -o <id> [\"…\"]{named}  |  --unsure")
        } else {
            // for the machine: a command to type, the same in every language
            format!("bisa answer {id} \"…\"{named}  |  --unsure")
        };
    }
    // An ask from a node that says no subject is an adoption when it is an
    // approval of no step — the rule before subjects were on the wire.
    let adoption = subject.starts_with("adopt:")
        || (subject.is_empty()
            && ask["gate_kind"].as_str() == Some("approval")
            && ask["step"].is_null());
    let inputs = if adoption { " [--input k=v]" } else { "" };
    // for the machine: a command to type, the same in every language
    format!("bisa approve {id} [--no]{named}{inputs}")
}

/// What a handled row's last decision was, in words: a question is
/// answered, a gate approved or declined.
fn decided_line(decided: &Value) -> String {
    let subject = decided["subject"].as_str().unwrap_or("");
    if subject.starts_with("ask_human:") || subject.starts_with("step:") {
        return bisa_i18n::say(&bisa_core::text!("cli-inbox-question-answered"));
    }
    format!(
        "{} {}",
        decided["gate_kind"].as_str().unwrap_or("gate"),
        if decided["approve"].as_bool().unwrap_or(false) {
            "approved"
        } else {
            "declined"
        }
    )
}

/// A row that waits and owes no ask — a step holding for the clock, a
/// signal, a message: where to look, since there is nothing to answer.
fn waiting_line(row: &Value) -> Option<String> {
    let owes = row["needs_action"]
        .as_array()
        .is_some_and(|asks| !asks.is_empty());
    (row["status"].as_str() == Some("waiting") && !owes).then(|| {
        format!(
            "   ⏸ waiting — see: bisa status {}",
            row["key"].as_str().unwrap_or("")
        )
    })
}

/// The options a question offered, one bracketed id each.
fn offered_line(ask: &Value) -> Option<String> {
    let options = ask["expects"]["options"].as_array()?;
    if options.is_empty() {
        return None;
    }
    Some(
        options
            .iter()
            .map(|o| {
                format!(
                    "[{}] {}{}",
                    o["id"].as_str().unwrap_or(""),
                    o["label"].as_str().unwrap_or(""),
                    if o["recommended"].as_bool().unwrap_or(false) {
                        " (recommended)"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("  "),
    )
}

/// The rows, asks first under each. A question shows how to answer it; a
/// gate shows how to decide it.
fn render(out: &Out, rows: &[Value]) {
    if rows.is_empty() {
        out.say(&bisa_core::text!(
            "cli-main-inbox-empty-nothing-waiting-you"
        ));
    }
    for r in rows {
        let key = r["key"].as_str().unwrap_or("");
        let title = r["title"].as_str().unwrap_or("");
        let unread = r["unread_count"].as_u64().unwrap_or(0);
        let badge = if unread > 0 {
            format!("  {unread} unread")
        } else {
            String::new()
        };
        // Rows are kept once earned, so the list holds conversations you have
        // already dealt with. Without a mark for read the terminal has no way
        // to say so, and every row looks equally owed.
        let mark = if r["read"].as_bool().unwrap_or(false) {
            "·"
        } else {
            "●"
        };
        out.human(&format!(
            "{mark} {key}  {}  {title}{badge}",
            r["kind"].as_str().unwrap_or("")
        ));
        if r["handled"].as_bool().unwrap_or(false) {
            out.human(&format!("   ✓ {}", decided_line(&r["decided"])));
        }
        let asks = r["needs_action"].as_array().cloned().unwrap_or_default();
        for ask in &asks {
            let question = ask["question"].as_str().unwrap_or("");
            let hint = hint_of(key, ask, &asks);
            if ask["expects"]["kind"].as_str() == Some("answer") {
                out.human(&format!("   ? {question}"));
                if let Some(offered) = offered_line(ask) {
                    out.human(&format!("     {offered}"));
                }
            } else {
                out.human(&format!("   ⏸ {question}"));
            }
            out.human(&format!("     → {hint}"));
        }
        if let Some(line) = waiting_line(r) {
            out.human(&line); // LCOV_EXCL_LINE a row that waits and owes no ask, printed for a person: no journey stages one, and the line is `waiting_line`'s, held by its own test; out by decision
        }
        if let Some(rep) = r["representative"].as_object() {
            out.human(&format!(
                "   ✉ {}",
                rep.get("snippet").and_then(|s| s.as_str()).unwrap_or("")
            ));
        }
        // What happened to it, the unread ones only: a notice is read here
        // and opened there, and the row says how many the terminal did not print.
        let unread_notices = r["unread_notices"].as_u64().unwrap_or(0) as usize;
        let notices = r["notices"].as_array().cloned().unwrap_or_default();
        for n in notices.iter().take(unread_notices) {
            out.human(&format!(
                "   • {}{}",
                n["notice"].as_str().unwrap_or("").replace('_', " "),
                n["title"]
                    .as_str()
                    .map(|t| format!(" — {t}"))
                    .unwrap_or_default()
            ));
        }
        if notices.len() > unread_notices && unread_notices > 0 {
            // LCOV_EXCL_START the read notices counted, which no journey stages; out by decision
            out.say(&bisa_core::text!(
                "cli-main-you-have-read",
                a0 = (notices.len() - unread_notices).to_string()
            ));
            // LCOV_EXCL_STOP
        }
    }
    out.json_value(json!({"rows": rows}));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal_home() -> Value {
        json!({"home": "goal", "goal": "01GOAL"})
    }

    fn step_ask(step: &str, kind: &str, subject: &str) -> Value {
        json!({
            "home": goal_home(), "step": step, "subject": subject,
            "gate_kind": if kind == "answer" { "escalation" } else { "approval" },
            "expects": {"kind": kind, "options": []},
        })
    }

    /// A held `wait` is released by its step — `approve --no` on one is
    /// refused, so the line never offers it.
    #[test]
    fn a_held_wait_is_hinted_as_step_release() {
        let hold = step_ask("hold", "decision", "release:01RUN/hold");
        assert_eq!(
            hint_of("01GOAL", &hold, std::slice::from_ref(&hold)),
            "bisa step release 01GOAL hold"
        );
        // Under a workspace run's row — keyed by its workflow — the run.
        let mut on_run = hold.clone();
        on_run["home"] = json!({"home": "run", "run": "01RUN"});
        assert_eq!(
            hint_of("01WORKFLOW", &on_run, std::slice::from_ref(&on_run)),
            "bisa step release 01RUN hold"
        );
    }

    /// Several steps waiting: each line names its own, since an unnamed
    /// verb is refused there. One alone needs no name.
    #[test]
    fn several_waiting_steps_are_hinted_with_step() {
        let ship = step_ask("ship", "decision", "approval:01RUN/ship");
        let which = step_ask("which", "answer", "step:01RUN/which");
        let both = [ship.clone(), which.clone()];
        assert_eq!(
            hint_of("01GOAL", &ship, &both),
            "bisa approve 01GOAL [--no] --step ship"
        );
        assert_eq!(
            hint_of("01GOAL", &which, &both),
            "bisa answer 01GOAL \"…\" --step which  |  --unsure"
        );
        assert_eq!(
            hint_of("01GOAL", &ship, std::slice::from_ref(&ship)),
            "bisa approve 01GOAL [--no]"
        );
        // Asks of two homes on one row — two runs of one workflow — are
        // each the only one of theirs.
        let mut other = ship.clone();
        other["home"] = json!({"home": "run", "run": "01OTHER"});
        assert_eq!(
            hint_of("01GOAL", &ship, &[ship.clone(), other]),
            "bisa approve 01GOAL [--no]"
        );
    }

    /// Run inputs start an adopted workflow's run; an amendment takes none.
    #[test]
    fn an_amendment_offers_no_inputs() {
        let ask = |subject: &str| {
            json!({"home": goal_home(), "step": null, "subject": subject,
                   "gate_kind": "approval", "expects": {"kind": "decision"}})
        };
        let amend = ask("amend:01RUN@01HELD");
        assert_eq!(
            hint_of("01GOAL", &amend, std::slice::from_ref(&amend)),
            "bisa approve 01GOAL [--no]"
        );
        let adopt = ask("adopt:01WF@3");
        assert_eq!(
            hint_of("01GOAL", &adopt, std::slice::from_ref(&adopt)),
            "bisa approve 01GOAL [--no] [--input k=v]"
        );
        // A node that says no subject: an approval of no step is the
        // adoption it always was.
        let old = json!({"home": goal_home(), "step": null,
                         "gate_kind": "approval", "expects": {"kind": "decision"}});
        assert_eq!(
            hint_of("01GOAL", &old, std::slice::from_ref(&old)),
            "bisa approve 01GOAL [--no] [--input k=v]"
        );
    }

    /// A durable ask of no step among several is named by its subject.
    #[test]
    fn a_durable_ask_among_several_is_hinted_with_subject() {
        let question = json!({
            "home": goal_home(), "step": null, "subject": "ask_human:01ITEM",
            "gate_kind": "escalation",
            "expects": {"kind": "answer", "options": [{"id": "eu", "label": "EU"}]},
        });
        let amend = json!({"home": goal_home(), "step": null, "subject": "amend:01RUN@01HELD",
                           "gate_kind": "approval", "expects": {"kind": "decision"}});
        let both = [question.clone(), amend.clone()];
        assert_eq!(
            hint_of("01GOAL", &question, &both),
            "bisa answer 01GOAL -o <id> [\"…\"] --subject ask_human:01ITEM  |  --unsure"
        );
        assert_eq!(
            hint_of("01GOAL", &amend, &both),
            "bisa approve 01GOAL [--no] --subject amend:01RUN@01HELD"
        );
        assert_eq!(offered_line(&question).as_deref(), Some("[eu] EU"));
        assert_eq!(offered_line(&amend), None);
    }

    /// A row that waits on nobody says where to look; one with an ask, or
    /// one that does not wait, says nothing more.
    #[test]
    fn a_row_waiting_on_nobody_points_at_its_status() {
        let waiting = json!({"key": "01GOAL", "status": "waiting", "needs_action": []});
        assert_eq!(
            waiting_line(&waiting).as_deref(),
            Some("   ⏸ waiting — see: bisa status 01GOAL")
        );
        let asked = json!({"key": "01GOAL", "status": "waiting", "needs_action": [{}]});
        assert_eq!(waiting_line(&asked), None);
        let running = json!({"key": "01GOAL", "status": "running", "needs_action": []});
        assert_eq!(waiting_line(&running), None);
        assert_eq!(waiting_line(&json!({"key": "general"})), None);
    }

    /// An answered question reads as answered, a gate as decided.
    #[test]
    fn a_handled_question_reads_as_answered() {
        assert_eq!(
            decided_line(
                &json!({"gate_kind": "escalation", "approve": true, "subject": "ask_human:01ITEM"})
            ),
            "question answered"
        );
        assert_eq!(
            decided_line(
                &json!({"gate_kind": "escalation", "approve": true, "subject": "step:01RUN/which"})
            ),
            "question answered"
        );
        assert_eq!(
            decided_line(
                &json!({"gate_kind": "approval", "approve": false, "subject": "adopt:01WF@1"})
            ),
            "approval declined"
        );
        assert_eq!(
            decided_line(&json!({"approve": true, "subject": "approval:01RUN/ship"})),
            "gate approved"
        );
    }
}
