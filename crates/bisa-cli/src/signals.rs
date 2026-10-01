//! `bisa signal`: named signals and the durable queue, from the terminal.
//!
//! A **signal** is the durable record of one occurrence — a schedule that
//! came due, a hook call, a message, a named signal a run raised. `emit`
//! raises a *named* one by hand: it is heard by the `signal` starts, waits
//! and boundary events that name it, and nothing listening is a normal, quiet
//! answer. `list` reads the queue's newest, queued or settled — never a
//! payload. `release` lets through a signal the content screen held for a
//! person to read.
//!
//! **An event never acts — it enqueues.** Nothing here starts a run. With a
//! daemon the node raises and releases (`POST /signals`,
//! `POST /signals/{id}/release`) and its worker starts runs from the queue;
//! without one an embedded engine that hears nothing
//! ([`crate::ctx::Ctx::quiet_engine`]) writes the signal down and leaves, so
//! this process never becomes the thing that starts runs on somebody's
//! terminal — the next node to open the workspace dispatches what is queued.
//!
//! Both paths answer in the node's shapes (`{signal, listeners}`,
//! `SignalView`), and one renderer prints them.

use crate::ctx::Ctx;
use crate::listening::rows;
use crate::output::Out;
use anyhow::Result;
use bisa_core::{GoalId, SignalScope};
use bisa_node::dto::SignalView;
use clap::Subcommand;
use serde_json::{json, Value};

/// What stands where a signal names nothing: no name, no listener.
const NONE: &str = "—";

#[derive(Subcommand)]
pub enum SignalCmd {
    /// Raise a named signal: heard by the starts, waits and boundary events
    /// that name it. Nothing listening is a normal, quiet answer
    Emit {
        /// The signal's name: dot-separated lowercase parts, e.g.
        /// `report.ready`
        name: String,
        /// Payload as JSON
        #[arg(long)]
        data: Option<String>,
        /// Raise it on this goal rather than in the workspace
        #[arg(long)]
        goal: Option<String>,
    },
    /// The newest signals, queued or settled — never a payload
    List {
        /// How many to list, newest first
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Let a held signal through: an outside payload the content screen
    /// would not pass, read by a person
    Release { id: String },
}

pub async fn signal(ctx: &Ctx, out: &Out, cmd: SignalCmd) -> Result<()> {
    match cmd {
        SignalCmd::Emit { name, data, goal } => {
            emit(ctx, out, &name, data.as_deref(), goal.as_deref()).await
        }
        SignalCmd::List { limit } => list(ctx, out, limit).await,
        SignalCmd::Release { id } => release(ctx, out, &id).await,
    }
}

// ---------------------------------------------------------------------------
// emit
// ---------------------------------------------------------------------------

async fn emit(
    ctx: &Ctx,
    out: &Out,
    name: &str,
    data: Option<&str>,
    goal: Option<&str>,
) -> Result<()> {
    let payload = data
        .map(crate::run::parse_data)
        .transpose()?
        .unwrap_or(Value::Null);
    let goal = goal.map(crate::parse_goal_id).transpose()?;
    let raised = raise(ctx, name, payload, goal).await?;
    out.human(&raised_line(name, &raised));
    out.json_value(json!({
        "name": name,
        "signal": raised["signal"],
        "listeners": raised["listeners"],
    }));
    Ok(())
}

/// Raise the signal — through the node, or through an engine that hears
/// nothing — and answer as the node does: `{signal, listeners}`, the record
/// kept for the waits that replay it and the signals written for the
/// listeners that heard it.
async fn raise(ctx: &Ctx, name: &str, payload: Value, goal: Option<GoalId>) -> Result<Value> {
    if let Some(client) = ctx.node_client().await {
        let mut body = json!({ "name": name });
        if !payload.is_null() {
            body["payload"] = payload;
        }
        if let Some(goal) = goal {
            body["goal"] = json!(goal);
        }
        return client.post("/signals", body).await;
    }
    let engine = ctx.quiet_engine().await?;
    let emitted = raise_embedded(&engine, name, payload, goal);
    engine.shutdown().await;
    let emitted = emitted?;
    Ok(json!({"signal": emitted.signal, "listeners": emitted.listeners}))
}

/// The signal raised on an embedded engine: on the goal when one is named —
/// a goal nobody has is refused in the store's words — in the workspace
/// otherwise.
fn raise_embedded(
    engine: &bisa_engine::Engine,
    name: &str,
    payload: Value,
    goal: Option<GoalId>,
) -> Result<bisa_engine::listen::emit::Emitted> {
    let scope = match goal {
        Some(goal) => {
            engine.workspace().get_goal(goal)?;
            SignalScope::Goal { goal }
        }
        None => SignalScope::Workspace,
    };
    Ok(engine.emit_signal(name, payload, scope)?)
}

/// What raising a signal did, in a line: its record, and who heard it.
fn raised_line(name: &str, raised: &Value) -> String {
    let heard: Vec<&str> = rows(&raised["listeners"])
        .iter()
        .filter_map(Value::as_str)
        .collect();
    bisa_i18n::say(&bisa_core::text!(
        "cli-signals-raised",
        name = name.to_string(),
        signal = raised["signal"].as_str().unwrap_or_default().to_string(),
        n = heard.len(),
        ids = heard.join(", ")
    ))
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

async fn list(ctx: &Ctx, out: &Out, limit: usize) -> Result<()> {
    let signals = newest(ctx, limit).await?;
    if signals.is_empty() {
        out.say(&bisa_core::text!("cli-signals-none"));
    }
    for view in &signals {
        out.human(&signal_line(view));
    }
    out.json_value(json!({ "signals": signals }));
    Ok(())
}

/// The newest signals as `GET /signals` lists them (`SignalView`): from the
/// node, or read off the workspace's own queue.
async fn newest(ctx: &Ctx, limit: usize) -> Result<Vec<Value>> {
    if let Some(client) = ctx.node_client().await {
        let listed = client.get(&format!("/signals?limit={limit}")).await?;
        return Ok(rows(&listed).to_vec());
    }
    let mut views = Vec::new();
    for queued in ctx.workspace()?.list_signals(None, limit)? {
        views.push(serde_json::to_value(SignalView::from(queued))?);
    }
    Ok(views)
}

/// One signal (`SignalView`) as a row: its id, when, its source, its name,
/// the listener it was raised for, where it stands — and why, when it says.
fn signal_line(view: &Value) -> String {
    let word = |key: &str| view[key].as_str().unwrap_or(NONE).to_string();
    let note = view["note"]
        .as_str()
        .map(|note| format!("  — {note}"))
        .unwrap_or_default();
    format!(
        "{}  {:<10}  {:<9} {:<24} {:<8} {}{note}",
        word("id"),
        view["at"].as_u64().unwrap_or(0),
        word("source"),
        word("name"),
        word("state"),
        word("listener"),
    )
}

// ---------------------------------------------------------------------------
// release
// ---------------------------------------------------------------------------

async fn release(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let released = let_through(ctx, id).await?;
    out.say(&bisa_core::text!(
        "cli-signals-released",
        id = id.to_string(),
        state = released["signal"]["state"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    ));
    out.json_value(released);
    Ok(())
}

/// Let the held signal through — by the node, or by an engine that hears
/// nothing — and answer as the node does: `{signal}`, queued again.
async fn let_through(ctx: &Ctx, id: &str) -> Result<Value> {
    if let Some(client) = ctx.node_client().await {
        return client
            .post(&format!("/signals/{id}/release"), json!({}))
            .await;
    }
    let engine = ctx.quiet_engine().await?;
    let released = engine.release_signal(id);
    let view = engine.workspace().signal(id);
    engine.shutdown().await;
    released?;
    Ok(json!({"signal": view?.map(SignalView::from)}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raising_says_the_record_and_who_heard_it() {
        let unheard = raised_line(
            "nobody.cares",
            &json!({"signal": "01RECORD", "listeners": []}),
        );
        assert!(
            unheard.contains("01RECORD")
                && unheard.contains("nobody.cares")
                && unheard.contains("nothing is listening"),
            "{unheard}"
        );
        let one = raised_line(
            "report.ready",
            &json!({"signal": "01RECORD", "listeners": ["01HEARD"]}),
        );
        assert!(
            one.contains("one listener") && one.contains("01HEARD"),
            "{one}"
        );
        let two = raised_line(
            "report.ready",
            &json!({"signal": "01RECORD", "listeners": ["01A", "01B"]}),
        );
        assert!(
            two.contains("2 listeners") && two.contains("01A, 01B"),
            "{two}"
        );
        // An answer with no list is nobody listening, never a failure.
        let bare = raised_line("report.ready", &json!({"signal": "01RECORD"}));
        assert!(bare.contains("nothing is listening"), "{bare}");
    }

    #[test]
    fn a_signal_is_a_row_of_what_it_is_and_where_it_stands() {
        let held = signal_line(&json!({
            "id": "01SIGNAL", "listener": "workspace:01WF/ticket", "source": "hook",
            "at": 1_727_563_600u64, "state": "held",
            "scope": {"scope": "workspace"},
            "note": "the content screen would not pass it",
        }));
        for word in [
            "01SIGNAL",
            "1727563600",
            "hook",
            "held",
            "workspace:01WF/ticket",
            "the content screen would not pass it",
        ] {
            assert!(held.contains(word), "{word}: {held}");
        }
        // A named signal kept for the waits names no listener.
        let kept = signal_line(&json!({
            "id": "01NAMED", "source": "signal", "name": "report.ready",
            "at": 7, "state": "done", "scope": {"scope": "workspace"},
        }));
        assert!(
            kept.contains("report.ready") && kept.trim_end().ends_with(NONE),
            "{kept}"
        );
        assert!(!kept.contains("  — "), "no note, no dash: {kept:?}");
    }
}
