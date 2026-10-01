//! Conversations: a saved exchange with agents, with an origin — listed,
//! started, read, spoken into, titled, put away, deleted.
//!
//! The record, the log and the index are the workspace's own: no command
//! over them needs an agent's turn, so none starts an engine. **What
//! changes a conversation goes through the node when one runs** — a start,
//! a title, putting it away, deleting it, its mode — because a change is
//! more than the record there: the turns that were running are stopped, the
//! grants given under the old mode end, and every open window is told. With
//! no node there is no turn, no grant and no window, and the record is the
//! whole of it. Either way the answer is the wire's
//! (`bisa_node::dto::ConversationView`). A review of what an agent changed
//! (`changes`, `keep`, `undo`, `restore` — ide/20) is the running node's
//! alone: it writes the checkout.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Context, Result};
use bisa_core::{AgentId, ConversationId, ConversationOrigin};
use bisa_store::{ConversationFilter, NewConversation};
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand, Debug)]
pub enum ConversationCmd {
    /// List conversations, the most recently moved first
    List {
        /// What they are about: node, workspace, goal, workflow, project,
        /// workstream, drawing or note
        #[arg(long)]
        origin: Option<String>,
        /// The goal, workflow, project, workstream, drawing or note id, with
        /// `--origin`
        #[arg(long)]
        id: Option<String>,
        /// Every conversation standing in one project — its own and its
        /// checkouts' — instead of `--origin`
        #[arg(long, conflicts_with_all = ["origin", "id"])]
        project: Option<String>,
        /// Those an agent spoke in or was addressed in
        #[arg(long)]
        agent: Option<String>,
        /// Words to find in the messages or the titles
        #[arg(long)]
        q: Option<String>,
        /// The archived ones instead of the live ones
        #[arg(long)]
        archived: bool,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Start a conversation about something
    New {
        /// node, workspace, goal, workflow, project, workstream, drawing or note
        origin: String,
        /// The goal, workflow, project, workstream, drawing or note id — none
        /// for the node or the workspace
        id: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
    /// Read a conversation's messages (marks it read)
    Show {
        id: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Post into a conversation
    Post {
        id: String,
        text: String,
        /// Address an agent id or a pubkey (repeatable); an agent answers
        /// when it is addressed, and the default agent when nobody is
        #[arg(long = "mention")]
        mentions: Vec<String>,
    },
    /// Give a conversation a title, or take it away with no title
    Rename { id: String, title: Option<String> },
    /// Put a conversation away
    Archive { id: String },
    /// Take a conversation back out
    Unarchive { id: String },
    /// Delete a conversation: its record, its messages and its rows
    Delete { id: String },
    /// How far an agent goes on its own in a project's or a workstream's
    /// conversation: show it, or set it to manual, auto or plan
    Mode {
        id: String,
        /// manual | auto | plan
        mode: Option<String>,
    },
    /// What the conversation's agent changed in its checkout, turn by turn
    Changes { id: String },
    /// Keep an agent's changes: all of them, one turn's, or one file's
    Keep {
        id: String,
        #[command(flatten)]
        which: Which,
    },
    /// Undo an agent's changes: all of them, one turn's, or one file's.
    /// Somebody else's edit is never discarded: a file it would conflict
    /// with is left alone and named
    Undo {
        id: String,
        #[command(flatten)]
        which: Which,
        /// Undo a file somebody else edited on the same lines too
        #[arg(long)]
        force: bool,
    },
    /// Go back to before the message that woke a turn
    Restore { id: String, turn: String },
}

/// What a keep or an undo is about; neither flag is all of it.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct Which {
    /// One turn's changes (the id `changes` prints)
    #[arg(long, conflicts_with = "path")]
    turn: Option<String>,
    /// One file's, by its path in the checkout
    #[arg(long)]
    path: Option<String>,
}

impl Which {
    fn target(&self) -> serde_json::Value {
        match (&self.turn, &self.path) {
            (Some(turn), _) => json!({"grain": "turn", "turn": turn}),
            (None, Some(path)) => json!({"grain": "file", "path": path}),
            (None, None) => json!({"grain": "all"}),
        }
    }
}

/// One line per file of a turn: its state, what happened to it, how much.
fn change_lines(view: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();
    for turn in view["turns"].as_array().into_iter().flatten() {
        lines.push(format!(
            "{}  {} · {}",
            turn["turn"].as_str().unwrap_or_default(),
            turn["agent"].as_str().unwrap_or_default(),
            turn["mode"].as_str().unwrap_or_default(),
        ));
        for file in turn["files"].as_array().into_iter().flatten() {
            lines.push(format!(
                "  {:<8} {:<9} +{} -{}  {}{}",
                file["state"].as_str().unwrap_or_default(),
                file["kind"].as_str().unwrap_or_default(),
                file["added"].as_u64().unwrap_or(0),
                file["removed"].as_u64().unwrap_or(0),
                file["path"].as_str().unwrap_or_default(),
                if file["overlapped"].as_bool().unwrap_or(false) {
                    bisa_i18n::say(&bisa_core::text!(
                        "cli-conversations-also-edited-someone-else"
                    ))
                } else {
                    String::new()
                },
            ));
        }
    }
    lines
}

/// What a settle or a restore did, in a sentence, then what it left alone.
fn settled_lines(act: &str, settled: &serde_json::Value) -> Vec<String> {
    let mut lines = vec![format!(
        "{act}: {} file(s); {} still waiting",
        settled["files"].as_u64().unwrap_or(0),
        settled["pending"].as_u64().unwrap_or(0),
    )];
    for skipped in settled["skipped"].as_array().into_iter().flatten() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-conversations-left-alone",
            a0 = (skipped["path"].as_str().unwrap_or_default()).to_string(),
            a1 = (skipped["why"].as_str().unwrap_or_default()).to_string()
        )));
    }
    lines
}

/// One conversation as the wire says it, from the workspace's own row.
fn view_of(ws: &bisa_store::Workspace, id: ConversationId) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(
        bisa_node::dto::ConversationView::from(ws.conversation_row(id)?),
    )?)
}

/// A conversation changed through the node, as the node answers it.
async fn patched(
    client: &crate::client::NodeClient,
    id: ConversationId,
    change: serde_json::Value,
) -> Result<serde_json::Value> {
    let answer = client
        .patch(&format!("/conversations/{id}"), change)
        .await?;
    Ok(answer["conversation"].clone())
}

/// A review writes files and announces them: the running node's work.
async fn review_client(ctx: &Ctx) -> Result<crate::client::NodeClient> {
    ctx.node_client().await.context(bisa_core::text!(
        "cli-conversations-review-agent-s-changes-runs-node"
    ))
}

fn parse_id(s: &str) -> Result<ConversationId> {
    s.parse::<ConversationId>().map_err(|_| {
        anyhow::anyhow!(bisa_core::text!(
            "cli-conversations-not-conversation-id",
            s = format!("{s:?}")
        ))
    })
}

/// The origin words and id as a person types them, resolved against the
/// workspace: a workstream carries its project from its own record.
fn origin_of(
    ws: &bisa_store::Workspace,
    kind: &str,
    id: Option<&str>,
) -> Result<ConversationOrigin> {
    if !ConversationOrigin::KINDS.contains(&kind) {
        bail!(bisa_core::text!(
            "cli-conversations-unknown-origin-one",
            kind = format!("{kind:?}"),
            a0 = (ConversationOrigin::KINDS.join(", ")).to_string()
        ));
    }
    if ConversationOrigin::kind_takes_id(kind) != id.is_some() {
        bail!(bisa_core::text!(
            "cli-conversations-origin-id",
            kind = format!("{kind:?}"),
            a0 = (if id.is_some() {
                bisa_i18n::say(&bisa_core::text!("cli-conversations-takes-no"))
            } else {
                bisa_i18n::say(&bisa_core::text!("cli-conversations-needs"))
            })
            .to_string()
        ));
    }
    let project = match (kind, id) {
        ("workstream", Some(id)) => {
            let wid = id.parse::<bisa_core::WorkstreamId>().map_err(|_| {
                anyhow::anyhow!(bisa_core::text!(
                    "cli-conversations-not-workstream-id",
                    id = format!("{id:?}")
                ))
            })?;
            Some(ws.get_workstream(wid)?.project.to_string())
        }
        _ => None,
    };
    ConversationOrigin::from_parts(kind, id, project.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(bisa_core::text!(
            "cli-conversations-not-id",
            id = format!("{id:?}"),
            kind = kind.to_string()
        ))
    })
}

/// A conversation put away, or taken back out: through the node when one
/// runs, which stops the turn that may be running first.
async fn archived(ctx: &Ctx, id: ConversationId, away: bool) -> Result<serde_json::Value> {
    match ctx.node_client().await {
        Some(client) => patched(&client, id, json!({"archived": away})).await,
        None => {
            let ws = ctx.workspace()?;
            ws.set_conversation_archived(id, away)?;
            view_of(&ws, id)
        }
    }
}

pub async fn conversation(ctx: &Ctx, out: &Out, cmd: ConversationCmd) -> Result<()> {
    match cmd {
        ConversationCmd::List {
            origin,
            id,
            project,
            agent,
            q,
            archived,
            limit,
        } => {
            let ws = ctx.workspace()?;
            if let Some(kind) = origin.as_deref() {
                origin_of(&ws, kind, id.as_deref())?;
            } else if id.is_some() {
                bail!(bisa_core::text!("cli-conversations-id-needs-origin"));
            }
            let project = project
                .as_deref()
                .map(|p| {
                    p.parse::<bisa_core::ProjectId>().map_err(|_| {
                        anyhow::anyhow!(bisa_core::text!(
                            "cli-conversations-not-project-id",
                            p = format!("{p:?}")
                        ))
                    })
                })
                .transpose()?;
            let rows = ws.list_conversations(&ConversationFilter {
                origin_kind: origin,
                origin_id: id,
                project,
                agent: agent.as_deref().map(AgentId::new).transpose()?,
                archived: Some(archived),
                query: q,
                before: None,
                limit,
            })?;
            if rows.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-conversations-no-conversations-yet-bisa-conversation-new"
                ));
            }
            for r in &rows {
                let title = r
                    .title
                    .clone()
                    .or_else(|| r.first_line.clone())
                    .unwrap_or_else(|| {
                        bisa_i18n::say(&bisa_core::text!("cli-conversations-new-conversation"))
                    });
                let about = match &r.origin_id {
                    Some(id) => format!("{} {}", r.origin_kind, &id[id.len().saturating_sub(6)..]),
                    None => r.origin_kind.clone(),
                };
                out.human(&format!(
                    "{}  {:<24}  {:>4} msgs  {}{}",
                    r.id,
                    about,
                    r.message_count,
                    title,
                    if r.archived { "  (archived)" } else { "" }
                ));
            }
            out.json_value(json!({"conversations": rows}));
        }
        ConversationCmd::New { origin, id, title } => {
            let ws = ctx.workspace()?;
            let origin = origin_of(&ws, &origin, id.as_deref())?;
            let view = match ctx.node_client().await {
                Some(client) => {
                    let answer = client
                        .post("/conversations", json!({"origin": origin, "title": title}))
                        .await?;
                    answer["conversation"].clone()
                }
                None => {
                    let mode = ws
                        .settings(origin.project())?
                        .into_iter()
                        .find(|r| r.key == "agents.conversation.mode")
                        .and_then(|r| {
                            r.value
                                .as_str()
                                .and_then(bisa_core::ConversationMode::parse)
                        })
                        .unwrap_or_default();
                    let made = ws.create_conversation(NewConversation {
                        origin,
                        title,
                        mode,
                    })?;
                    view_of(&ws, made.id)?
                }
            };
            out.say(&bisa_core::text!(
                "cli-conversations-started-conversation",
                a0 = view["id"].as_str().unwrap_or_default().to_string()
            ));
            out.json_value(json!({"conversation": view}));
        }
        ConversationCmd::Show { id, limit } => {
            let cid = parse_id(&id)?;
            let ws = ctx.workspace()?;
            let row = ws.conversation_row(cid)?;
            // One answer: the conversation and its messages, not two documents.
            let messages =
                crate::studio::read_messages(ctx, out, &cid.to_string(), limit, None).await?;
            out.json_value(json!({"conversation": row, "messages": messages}));
        }
        ConversationCmd::Post { id, text, mentions } => {
            let cid = parse_id(&id)?;
            let ws = ctx.workspace()?;
            if ws.get_conversation(cid)?.archived {
                bail!(bisa_core::text!(
                    "cli-conversations-conversation-archived-take-back-out-continue"
                ));
            }
            crate::studio::post_message(
                ctx,
                out,
                &cid.to_string(),
                &text,
                None,
                &mentions,
                &[],
                &[],
            )
            .await?;
        }
        ConversationCmd::Rename { id, title } => {
            let cid = parse_id(&id)?;
            let view = match ctx.node_client().await {
                Some(client) => patched(&client, cid, json!({"title": title})).await?,
                None => {
                    let ws = ctx.workspace()?;
                    ws.rename_conversation(cid, title.as_deref())?;
                    view_of(&ws, cid)?
                }
            };
            out.human(&match view["title"].as_str() {
                Some(t) => bisa_i18n::say(&bisa_core::text!(
                    "cli-conversations-now",
                    a0 = cid.to_string(),
                    t = format!("{t:?}")
                )),
                None => bisa_i18n::say(&bisa_core::text!(
                    "cli-conversations-has-no-title",
                    a0 = cid.to_string()
                )),
            });
            out.json_value(json!({"conversation": view}));
        }
        ConversationCmd::Archive { id } => {
            let view = archived(ctx, parse_id(&id)?, true).await?;
            out.human(&format!("{id} archived"));
            out.json_value(json!({"conversation": view}));
        }
        ConversationCmd::Unarchive { id } => {
            let view = archived(ctx, parse_id(&id)?, false).await?;
            out.human(&format!("{id} unarchived"));
            out.json_value(json!({"conversation": view}));
        }
        ConversationCmd::Delete { id } => {
            let cid = parse_id(&id)?;
            match ctx.node_client().await {
                // Its turns are stopped first: the node's to do.
                Some(client) => {
                    client.delete(&format!("/conversations/{cid}")).await?;
                }
                None => ctx.workspace()?.delete_conversation(cid)?,
            }
            out.human(&format!("{cid} deleted"));
            out.json_value(json!({"deleted": cid}));
        }
        ConversationCmd::Mode { id, mode } => {
            let cid = parse_id(&id)?;
            let ws = ctx.workspace()?;
            let view = match mode {
                None => view_of(&ws, cid)?,
                Some(word) => {
                    let mode = bisa_core::ConversationMode::parse(&word).ok_or_else(|| {
                        anyhow::anyhow!(bisa_core::text!(
                            "cli-conversations-unknown-mode-one-manual-auto-plan",
                            word = format!("{word:?}")
                        ))
                    })?;
                    if !ws.get_conversation(cid)?.origin.is_checkout() {
                        bail!(bisa_core::text!(
                            "cli-conversations-only-conversation-about-project-workstream-has"
                        ));
                    }
                    match ctx.node_client().await {
                        // What was allowed under the old mode ends with it,
                        // and a turn under way reads the new one at its
                        // next call: the engine's to do.
                        Some(client) => patched(&client, cid, json!({"mode": mode})).await?,
                        None => {
                            ws.set_conversation_mode(cid, mode)?;
                            view_of(&ws, cid)?
                        }
                    }
                }
            };
            out.say(&bisa_core::text!(
                "cli-conversations-mode",
                a0 = cid.to_string(),
                a1 = view["mode"].as_str().unwrap_or_default().to_string()
            ));
            out.json_value(json!({"conversation": view}));
        }
        ConversationCmd::Changes { id } => {
            let cid = parse_id(&id)?;
            let view = review_client(ctx)
                .await?
                .get(&format!("/conversations/{cid}/changes"))
                .await?;
            let lines = change_lines(&view);
            if lines.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-conversations-nothing-was-changed-conversation"
                ));
            }
            for line in lines {
                out.human(&line);
            }
            out.json_value(view);
        }
        ConversationCmd::Keep { id, which } => {
            let cid = parse_id(&id)?;
            let settled = review_client(ctx)
                .await?
                .post(
                    &format!("/conversations/{cid}/changes/settle"),
                    json!({"verdict": "keep", "target": which.target()}),
                )
                .await?;
            for line in settled_lines("kept", &settled) {
                out.human(&line);
            }
            out.json_value(settled);
        }
        ConversationCmd::Undo { id, which, force } => {
            let cid = parse_id(&id)?;
            let settled = review_client(ctx)
                .await?
                .post(
                    &format!("/conversations/{cid}/changes/settle"),
                    json!({"verdict": "undo", "target": which.target(), "force": force}),
                )
                .await?;
            for line in settled_lines("undone", &settled) {
                out.human(&line);
            }
            out.json_value(settled);
        }
        ConversationCmd::Restore { id, turn } => {
            let cid = parse_id(&id)?;
            let settled = review_client(ctx)
                .await?
                .post(
                    &format!("/conversations/{cid}/changes/restore"),
                    json!({"turn": turn}),
                )
                .await?;
            for line in settled_lines("restored", &settled) {
                out.human(&line);
            }
            out.json_value(settled);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keep_or_an_undo_is_about_all_a_turn_or_a_file() {
        assert_eq!(Which::default().target(), json!({"grain": "all"}));
        let turn = Which {
            turn: Some("01J".into()),
            path: None,
        };
        assert_eq!(turn.target(), json!({"grain": "turn", "turn": "01J"}));
        let file = Which {
            turn: None,
            path: Some("src/lib.rs".into()),
        };
        assert_eq!(
            file.target(),
            json!({"grain": "file", "path": "src/lib.rs"})
        );
    }

    #[test]
    fn the_changes_read_turn_by_turn_and_say_when_somebody_else_edited_too() {
        let view = json!({"turns": [{
            "turn": "01T", "agent": "developer", "mode": "manual",
            "files": [
                {"path": "src/lib.rs", "kind": "modified", "state": "pending",
                 "added": 4, "removed": 1, "overlapped": true},
                {"path": "notes.md", "kind": "created", "state": "kept",
                 "added": 9, "removed": 0, "overlapped": false},
            ],
        }]});
        let lines = change_lines(&view);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("developer · manual"));
        assert!(lines[1].contains("pending") && lines[1].contains("+4 -1"));
        assert!(lines[1].ends_with("(also edited by someone else)"));
        assert!(lines[2].ends_with("notes.md"));
        assert!(change_lines(&json!({"turns": []})).is_empty());
    }

    #[test]
    fn a_settle_names_what_it_left_alone_and_why() {
        let settled = json!({"files": 2, "pending": 1, "skipped": [
            {"path": "a.rs", "why": "the file changed on the same lines since"},
        ]});
        let lines = settled_lines("undone", &settled);
        assert_eq!(lines[0], "undone: 2 file(s); 1 still waiting");
        assert!(lines[1].starts_with("  left alone: a.rs — "));
    }
}
