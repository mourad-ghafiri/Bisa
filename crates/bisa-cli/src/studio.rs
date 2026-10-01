//! The Studio surface from the terminal: Channels (standing conversations),
//! Messages (DMs, including with your agents), and Pulse.
//!
//! Every scope — channel, goal, workstream — takes the same message commands;
//! a scope is just a ULID.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::tags::{keeps, TagFilterArgs, TagSetArgs};
use anyhow::{bail, Result};
use bisa_core::tags::TagEntity;
use bisa_core::{
    AgentId, ArtifactRef, AttachmentRef, Channel, ChannelId, ChannelKind, MessageBody, PrincipalId,
    RosterPolicy, ScopeKind,
};
use bisa_store::{PostOrigin, Workspace};
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand)]
pub enum ChannelCmd {
    /// List channels with unread counts
    List {
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Open a new standing conversation, when the catalog has no ready-made
    /// one that fits (`bisa catalog list --kind channel`)
    Create {
        /// What to call it
        name: String,
        /// One line saying what this conversation is for
        #[arg(long)]
        topic: Option<String>,
        /// Agent that belongs in this conversation (repeatable).
        ///
        /// A roster is a directory, not a subscription: it says who belongs
        /// here and it is what `--mention <channel-id>` expands to. Rostered
        /// agents still answer only when addressed — five agents in a channel
        /// must never mean five harness sessions per message.
        #[arg(long = "agent")]
        agents: Vec<String>,
        /// Team that belongs in this conversation (repeatable): its enabled
        /// agents are members, and leave with it when it is stood down
        #[arg(long = "team")]
        teams: Vec<String>,
        /// A person of this workspace, by pubkey, who reaches the channel
        /// (repeatable) — what a guest is let in through
        #[arg(long = "human")]
        humans: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
    },
    /// Change a channel's topic, roster or tags. The audience and the kind are
    /// fixed: changing an audience would change who past messages were
    /// encrypted to, which is not something an edit can do.
    Edit {
        id: String,
        /// Replace the topic
        #[arg(long)]
        topic: Option<String>,
        /// Replace the roster's agents with these ids (repeatable). A roster
        /// is a directory, not a subscription — see `channels create --help`.
        /// Each half of a roster — agents, teams, people — is replaced only
        /// when it is named, and kept as it is otherwise.
        #[arg(long = "agent")]
        agents: Vec<String>,
        /// Replace the roster's teams with these ids (repeatable)
        #[arg(long = "team")]
        teams: Vec<String>,
        /// Replace the roster's people with these pubkeys (repeatable)
        #[arg(long = "human")]
        humans: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
    },
}

/// The three halves of a roster as a person types them, each `None` when
/// the command line did not name it.
#[derive(Default)]
struct RosterWords {
    agents: Option<Vec<String>>,
    teams: Option<Vec<String>>,
    humans: Option<Vec<String>>,
}

impl RosterWords {
    fn of(agents: Vec<String>, teams: Vec<String>, humans: Vec<String>) -> Self {
        let named = |half: Vec<String>| (!half.is_empty()).then_some(half);
        Self {
            agents: named(agents),
            teams: named(teams),
            humans: named(humans),
        }
    }

    /// The roster these words make of `stored`: a half that was named is
    /// replaced, a half that was not is kept — so naming an agent never
    /// takes a team or a person off the roster.
    fn over(&self, stored: &RosterPolicy) -> Result<RosterPolicy> {
        let (kept_agents, kept_teams, kept_humans) = match stored {
            RosterPolicy::Listed {
                agents,
                teams,
                humans,
            } => (agents.clone(), teams.clone(), humans.clone()),
            RosterPolicy::Everyone => (vec![], vec![], vec![]),
        };
        let ids = |e: bisa_core::CoreError| anyhow::anyhow!("{e}");
        Ok(RosterPolicy::Listed {
            agents: match &self.agents {
                Some(named) => named
                    .iter()
                    .map(|a| AgentId::new(a).map_err(ids))
                    .collect::<Result<Vec<_>>>()?,
                None => kept_agents,
            },
            teams: match &self.teams {
                Some(named) => named
                    .iter()
                    .map(|t| bisa_core::TeamId::new(t).map_err(ids))
                    .collect::<Result<Vec<_>>>()?,
                None => kept_teams,
            },
            humans: match &self.humans {
                Some(named) => named
                    .iter()
                    .map(|p| PrincipalId::new(p.clone()).map_err(ids))
                    .collect::<Result<Vec<_>>>()?,
                None => kept_humans,
            },
        })
    }

    fn named(&self) -> bool {
        self.agents.is_some() || self.teams.is_some() || self.humans.is_some()
    }

    /// The halves that were named, as the node's bodies read them.
    fn into_body(self, mut body: serde_json::Value) -> serde_json::Value {
        for (key, half) in [
            ("agents", self.agents),
            ("teams", self.teams),
            ("humans", self.humans),
        ] {
            if let Some(half) = half {
                body[key] = json!(half);
            }
        }
        body
    }
}

/// A channel as the node answered it.
fn channel_said(answer: &serde_json::Value) -> Result<Channel> {
    Ok(serde_json::from_value(answer["channel"].clone())?)
}

/// Direct messages. Sending is a subcommand rather than the bare `dm <pubkey>`
/// it was, because a positional recipient list has no room beside a verb:
/// `bisa dm list` would have been read as a DM to a person called
/// "list".
#[derive(Subcommand)]
pub enum DmCmd {
    /// List direct conversations with unread counts
    List,
    /// Send a direct message (opens or reuses the conversation)
    Send {
        /// Recipient pubkeys (one or more)
        #[arg(required = true, num_args = 1..)]
        pubkeys: Vec<String>,
        #[arg(long)]
        text: String,
    },
}

/// A channel's roster: who belongs in the conversation, and what the channel
/// handle expands to. Shown wherever a channel is printed, because a directory
/// nobody can see is a directory nobody uses.
fn channel_id(s: &str) -> Result<ChannelId> {
    ChannelId::new(s).map_err(|e| anyhow::anyhow!("{e}"))
}

fn render_roster(out: &Out, c: &Channel) {
    match &c.roster {
        RosterPolicy::Everyone => out.say(&bisa_core::text!("cli-studio-roster-everyone")),
        RosterPolicy::Listed {
            agents,
            teams,
            humans,
        } => {
            if agents.is_empty() && teams.is_empty() && humans.is_empty() {
                return;
            }
            let mut names: Vec<String> = agents.iter().map(|a| a.to_string()).collect();
            names.extend(teams.iter().map(|t| format!("team:{t}")));
            names.extend(humans.iter().map(|p| format!("human:{}", &p.as_hex()[..8])));
            out.human(&format!("  roster: {}", names.join(", ")));
        }
    }
}

/// Reads go to the workspace directly; what writes a channel goes through
/// the running node when there is one — every open window is told — and
/// into the record when there is none.
pub async fn channels(ctx: &Ctx, out: &Out, cmd: ChannelCmd) -> Result<()> {
    let ws = ctx.workspace()?;
    match cmd {
        ChannelCmd::List { filter } => {
            let admitted = filter.admitted(&ws, TagEntity::Channel)?;
            let unread: std::collections::HashMap<String, u64> =
                ws.unread_counts()?.into_iter().collect();
            // Standing channels only — `bisa dm list` is where a DM
            // belongs, and printing one here rendered a `Dm` in the kind
            // column of a listing whose whole point is the rooms you can join.
            let channels: Vec<Channel> = ws
                .list_channels_of_kind(ChannelKind::Standing)?
                .into_iter()
                .filter(|c| keeps(&admitted, c.id.as_str()))
                .collect();
            if channels.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-studio-no-channels-match-bisa-catalog-list"
                ));
            }
            for c in &channels {
                let n = unread.get(c.id.as_str()).copied().unwrap_or(0);
                let badge = if n > 0 {
                    format!("  {n} unread")
                } else {
                    String::new()
                };
                out.human(&format!(
                    "{:<28} {:<20} {:?}  {}{badge}",
                    c.id,
                    c.name,
                    c.kind,
                    crate::tags::label(&c.tags)
                ));
                render_roster(out, c);
            }
            out.json_value(json!({"channels": channels}));
        }
        ChannelCmd::Create {
            name,
            topic,
            agents,
            teams,
            humans,
            tags,
        } => {
            // The roster is a directory, not a subscription: it says who
            // belongs, and nobody speaks until addressed.
            let words = RosterWords::of(agents, teams, humans);
            let roster = words.over(&RosterPolicy::Listed {
                agents: vec![],
                teams: vec![],
                humans: vec![],
            })?;
            let tags = tags.parse()?;
            let channel = match ctx.node_client().await {
                Some(node) => {
                    let body = words.into_body(json!({"name": name, "topic": topic, "tags": tags}));
                    channel_said(&node.post("/channels", body).await?)?
                }
                None => ws.create_channel(&name, topic.as_deref(), roster, tags)?,
            };
            out.say(&bisa_core::text!(
                "cli-studio-channel-created-post-with-bisa-msg",
                a0 = (channel.name).to_string(),
                a1 = (channel.id).to_string()
            ));
            render_roster(out, &channel);
            out.say(&bisa_core::text!(
                "cli-studio-address-whole-roster-with-bisa-msg",
                id = (channel.id).to_string()
            ));
            out.json_value(json!({"channel": channel}));
        }
        ChannelCmd::Edit {
            id,
            topic,
            agents,
            teams,
            humans,
            tags,
        } => {
            let cid = channel_id(&id)?;
            let current = ws.get_channel(&cid)?;
            // Each field is replaced only when named, so editing a topic does
            // not silently empty a roster somebody else set — and naming an
            // agent takes no team and no person off it.
            let words = RosterWords::of(agents, teams, humans);
            let roster = if words.named() {
                words.over(&current.roster)?
            } else {
                current.roster.clone()
            };
            let tags = tags.apply(current.tags)?;
            let channel = match ctx.node_client().await {
                Some(node) => {
                    let mut body = json!({"tags": tags});
                    if let Some(topic) = &topic {
                        body["topic"] = json!(topic);
                    }
                    let body = words.into_body(body);
                    channel_said(&node.patch(&format!("/channels/{cid}"), body).await?)?
                }
                None => {
                    let topic = topic.or(current.topic.clone());
                    ws.update_channel(&cid, topic.as_deref(), roster, tags)?
                }
            };
            out.human(&format!("{}  {}", channel.id, channel.name));
            render_roster(out, &channel);
            out.json_value(json!({"channel": channel}));
        }
    }
    Ok(())
}

/// What a person says from the command line, ready to be posted.
struct Words<'a> {
    text: &'a str,
    reply_to: Option<String>,
    /// Who it addresses, as typed — what the node resolves, as the store does.
    addressed: &'a [String],
    /// The same, resolved: what the store is handed.
    mentions: &'a [PrincipalId],
    files: &'a [AttachmentRef],
    artifacts: Vec<ArtifactRef>,
}

/// The node's route a scope is posted to: one per kind of scope.
fn post_route(kind: ScopeKind, scope: &str) -> String {
    match kind {
        ScopeKind::Channel => format!("/channels/{scope}/messages"),
        ScopeKind::Goal => format!("/goals/{scope}/messages"),
        ScopeKind::Conversation => format!("/conversations/{scope}/messages"),
    }
}

/// **The one door a person's words go through from the command line.**
/// Through the running node when there is one: its engine hears them — the
/// agents they address are woken, the starts, waits and boundary events that
/// listen for a message hear it, and a desktop that is open draws it. With
/// no node they are written into the store, where they wait to be read: no
/// engine is there to hear them, and none is started to.
async fn say(ctx: &Ctx, ws: &Workspace, scope: &str, words: Words<'_>) -> Result<String> {
    if let Some(client) = ctx.node_client().await {
        let kind = ws.resolve_scope(scope)?.kind;
        let posted = client
            .post(
                &post_route(kind, scope),
                json!({
                    "content": words.text,
                    "reply_to": words.reply_to,
                    "mentions": words.addressed,
                    "attachments": words.files,
                    "artifacts": words.artifacts,
                }),
            )
            .await?;
        return posted["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!(bisa_core::text!("cli-studio-node-posted-no-id")));
    }
    Ok(ws.post_message(
        scope,
        MessageBody::Post {
            text: words.text.to_string(),
            context: vec![],
            artifacts: words.artifacts,
            thinking: None,
            said: None,
        },
        words.reply_to,
        words.mentions,
        words.files,
        None,
        // `bisa msg` is a person at a terminal.
        PostOrigin::Asked,
    )?)
}

/// Post to any scope: a channel, a goal thread, or a conversation.
///
/// Mentions resolve through the store's one resolver, so `@`-tokens mean the
/// same thing here, in the node and in the MCP tool — including a channel's own
/// id, which expands to that channel's roster.
#[allow(clippy::too_many_arguments)]
pub async fn post_message(
    ctx: &Ctx,
    out: &Out,
    scope: &str,
    content: &str,
    reply_to: Option<String>,
    mentions: &[String],
    attachments: &[std::path::PathBuf],
    artifacts: &[String],
) -> Result<()> {
    let ws = ctx.workspace()?;
    let addressed = mentions;
    let mentions = ws.resolve_mentions(scope, addressed)?;
    // A file is read into the content-addressed store first, then named by
    // its descriptor — the same two steps the desktop takes over HTTP.
    let store = |path: &std::path::Path| -> Result<bisa_core::AttachmentRef> {
        let bytes = std::fs::read(path).map_err(|e| {
            anyhow::anyhow!(bisa_core::text!(
                "cli-studio-cannot-read",
                a0 = (path.display()).to_string(),
                e = e.to_string()
            ))
        })?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        Ok(ws.put_attachment(&bytes, &name, bisa_core::mime_of_name(&name))?)
    };
    let files = attachments
        .iter()
        .map(|p| store(p))
        .collect::<Result<Vec<_>>>()?;
    let artifacts = artifacts
        .iter()
        .map(|spec| {
            let (path, title) = split_artifact_spec(spec);
            let file = store(std::path::Path::new(path))?;
            Ok(bisa_core::ArtifactRef::from_attachment(
                file,
                title.map(str::to_string),
                None,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let attached = files.len();
    let shared = artifacts.len();
    let id = say(
        ctx,
        &ws,
        scope,
        Words {
            text: content,
            reply_to,
            addressed,
            mentions: &mentions,
            files: &files,
            artifacts,
        },
    )
    .await?;
    out.say(&bisa_core::text!(
        "cli-studio-posted",
        scope = scope.to_string(),
        a0 = (if mentions.is_empty() {
            String::new()
        } else {
            bisa_i18n::say(&bisa_core::text!(
                "cli-studio-mentioned",
                a0 = (mentions.len()).to_string()
            ))
        })
        .to_string()
    ));
    out.json_value(json!({
        "scope": scope,
        "id": id,
        "mentions": mentions.iter().map(|p| p.as_hex().to_string()).collect::<Vec<_>>(),
        "attachments": attached,
        "artifacts": shared,
    }));
    Ok(())
}

/// `<path>` or `<path>:<title>`: the split is on the last `:` and only when
/// the left half is a file — a path with a colon in it and no title stays
/// whole.
fn split_artifact_spec(spec: &str) -> (&str, Option<&str>) {
    if std::path::Path::new(spec).is_file() {
        return (spec, None);
    }
    match spec.rsplit_once(':') {
        Some((path, title)) if !title.trim().is_empty() && std::path::Path::new(path).is_file() => {
            (path, Some(title.trim()))
        }
        _ => (spec, None),
    }
}

/// A scope's read mark moved: by the node when one runs — the unread a
/// window shows follows at once — and in the record otherwise.
async fn mark(ctx: &Ctx, ws: &Workspace, scope: &str, read: bool) -> Result<()> {
    match ctx.node_client().await {
        Some(node) => {
            let door = if read { "/read" } else { "/unread" };
            node.post(door, json!({"scope": scope})).await?;
        }
        None if read => ws.mark_read(scope)?,
        None => ws.mark_unread(scope)?,
    }
    Ok(())
}

/// A scope's messages read and printed for a person — reading is
/// acknowledging, so the read mark moves — and handed back for the one JSON
/// answer the verb prints: `msgs` prints them as they are, a `conversation
/// show` beside the conversation's row.
pub async fn read_messages(
    ctx: &Ctx,
    out: &Out,
    scope: &str,
    limit: usize,
    before: Option<u64>,
) -> Result<Vec<bisa_store::MessageRow>> {
    let ws = ctx.workspace()?;
    let rows = ws.messages(scope, before.map(bisa_store::PageBefore::at), limit)?;
    if rows.is_empty() {
        out.say(&bisa_core::text!("cli-studio-no-messages-scope-yet"));
    }
    for m in &rows {
        if m.retracted {
            continue;
        }
        let who = &m.author[..m.author.len().min(8)];
        out.human(&format!("{:>10}  {who}  {}", m.created_at, m.content));
    }
    mark(ctx, &ws, scope, true).await?;
    Ok(rows)
}

pub async fn messages(
    ctx: &Ctx,
    out: &Out,
    scope: &str,
    limit: usize,
    before: Option<u64>,
) -> Result<()> {
    let rows = read_messages(ctx, out, scope, limit, before).await?;
    out.json_value(json!({"scope": scope, "messages": rows}));
    Ok(())
}

pub async fn dm(ctx: &Ctx, out: &Out, cmd: DmCmd) -> Result<()> {
    match cmd {
        DmCmd::List => list_dms(ctx, out),
        DmCmd::Send { pubkeys, text } => send_dm(ctx, out, &pubkeys, &text).await,
    }
}

/// The listing `channels list` sends people to and that did not exist.
///
/// M12 narrowed `channels list` to standing channels — correctly: a DM is a
/// conversation, not a room you join — and the comment there promised
/// `bisa dm list` as the place a DM belongs. It was not written, so
/// between M12 and here the CLI could open a DM, post to one and read one,
/// and could not tell you which ones existed.
///
/// No tag filter, unlike `channels list`: [`bisa_store::Workspace::open_dm`]
/// writes an empty tag set and nothing can add to it, so a `--tag` here would
/// be a control that always returns nothing.
fn list_dms(ctx: &Ctx, out: &Out) -> Result<()> {
    let ws = ctx.workspace()?;
    let unread: std::collections::HashMap<String, u64> = ws.unread_counts()?.into_iter().collect();
    let dms = ws.list_channels_of_kind(ChannelKind::Direct)?;
    if dms.is_empty() {
        out.say(&bisa_core::text!(
            "cli-studio-no-direct-messages-yet-bisa-dm"
        ));
    }
    for c in &dms {
        let n = unread.get(c.id.as_str()).copied().unwrap_or(0);
        let badge = if n > 0 {
            format!("  {n} unread")
        } else {
            String::new()
        };
        // The audience *is* the DM: a DM's name is derived from it and there
        // is no roster to print, so the participant count is what tells two
        // conversations with the same short prefixes apart.
        out.human(&format!(
            "{:<28} {:<28} {} participants{badge}",
            c.id,
            c.name,
            c.audience.principals().len()
        ));
    }
    out.json_value(json!({"dms": dms}));
    Ok(())
}

/// Open (or reuse) a DM with the given participants and post to it.
async fn send_dm(ctx: &Ctx, out: &Out, pubkeys: &[String], content: &str) -> Result<()> {
    let ws = ctx.workspace()?;
    if pubkeys.is_empty() {
        bail!(bisa_core::text!(
            "cli-studio-message-needs-least-one-recipient"
        ));
    }
    let members: Vec<PrincipalId> = pubkeys
        .iter()
        .map(|p| PrincipalId::new(p.clone()).map_err(|e| anyhow::anyhow!("{e}")))
        .collect::<Result<_>>()?;
    // Opened through the node when there is one, as its words then go.
    let channel: Channel = match ctx.node_client().await {
        Some(client) => {
            let opened = client.post("/dms", json!({ "members": pubkeys })).await?;
            serde_json::from_value(opened["channel"].clone())
                .map_err(|_| anyhow::anyhow!(bisa_core::text!("cli-studio-node-opened-no-dm")))?
        }
        None => ws.open_dm(&members)?,
    };
    let id = say(
        ctx,
        &ws,
        channel.id.as_str(),
        Words {
            text: content,
            reply_to: None,
            addressed: pubkeys,
            mentions: &members,
            files: &[],
            artifacts: vec![],
        },
    )
    .await?;
    out.say(&bisa_core::text!(
        "cli-studio-sent",
        a0 = (channel.name).to_string(),
        a1 = (channel.id).to_string()
    ));
    out.json_value(json!({"channel": channel.id, "id": id}));
    Ok(())
}

/// One `/pulse` row → the words for it.
///
/// The route ships the event, not a sentence, so the same prose the journal
/// prints is reached by parsing the payload back — the alternative is a
/// second vocabulary for the same facts, which is how the desktop ended up
/// showing one thing live and another after a reload.
fn pulse_line(row: &serde_json::Value) -> String {
    let title = row["title"].as_str().unwrap_or_default();
    let body =
        match serde_json::from_value::<bisa_core::event::JournalPayload>(row["event"].clone()) {
            Ok(payload) => crate::activity::payload_line(&payload),
            // Not a journal fact: the two conversation sources name
            // themselves, and an engine's fact — what a listener heard,
            // started or could not start — is said as the timeline says it.
            Err(_) => match row["event"]["type"].as_str() {
                Some("message") => {
                    format!("“{}”", row["event"]["snippet"].as_str().unwrap_or_default())
                }
                other => crate::activity::fact_line(&row["event"])
                    .unwrap_or_else(|| other.unwrap_or("?").replace('_', " ")),
            },
        };
    if title.is_empty() {
        body
    } else {
        format!("{title}: {body}")
    }
}

pub async fn pulse(ctx: &Ctx, out: &Out, limit: usize, concept: &str) -> Result<()> {
    if let Some(client) = ctx.node_client().await {
        let v = client
            .get(&format!("/pulse?limit={limit}&concept={concept}"))
            .await?;
        let rows = v["rows"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            out.say(&bisa_core::text!("cli-studio-nothing-has-happened-yet"));
        }
        for r in &rows {
            out.human(&format!(
                "{:>10}  {:<10} {}",
                r["at"].as_u64().unwrap_or(0),
                r["concept"].as_str().unwrap_or(""),
                pulse_line(r)
            ));
        }
        out.json_value(json!({"rows": rows, "next": v["next"]}));
        return Ok(());
    }
    // Embedded: the workspace journal across goals is the timeline — the
    // goals' story alone, since the feed's index is the node's to serve.
    if concept != "all" && concept != "goals" {
        out.say(&bisa_core::text!(
            "cli-studio-without-node-feed-goals-journals-concept"
        ));
    }
    let ws = ctx.workspace()?;
    let mut lines: Vec<(u64, String)> = Vec::new();
    for goal in ws.list_goals(None)? {
        for e in ws
            .journal(&bisa_core::Home::Goal { goal: goal.id })?
            .into_iter()
            .rev()
            .take(limit)
        {
            lines.push((e.at, crate::activity::journal_line(&e)));
        }
    }
    lines.sort_by_key(|(at, _)| *at);
    let shown: Vec<_> = lines.into_iter().rev().take(limit).rev().collect();
    if shown.is_empty() {
        out.say(&bisa_core::text!("cli-studio-nothing-has-happened-yet"));
    }
    for (_, line) in &shown {
        out.human(line);
    }
    out.json_value(json!({"rows": shown.iter().map(|(at, t)| json!({"at": at, "text": t})).collect::<Vec<_>>()}));
    Ok(())
}

pub async fn read_marker(ctx: &Ctx, out: &Out, scope: &str, read: bool) -> Result<()> {
    let ws = ctx.workspace()?;
    mark(ctx, &ws, scope, read).await?;
    if read {
        out.say(&bisa_core::text!(
            "cli-studio-marked-read",
            scope = scope.to_string()
        ));
    } else {
        out.say(&bisa_core::text!(
            "cli-studio-marked-unread",
            scope = scope.to_string()
        ));
    }
    out.json_value(json!({"scope": scope, "read": read}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::pulse_line;
    use serde_json::json;

    /// `/pulse` ships the event, so the CLI parses it back rather than
    /// printing a sentence the node composed. Both arms matter: a journal
    /// payload must reach `activity::payload_line`, and a conversation row — which is
    /// *not* a journal payload — must not fall through to a bare tag.
    #[test]
    fn a_pulse_row_is_rendered_from_its_event() {
        assert_eq!(
            pulse_line(&json!({
                "at": 1, "title": "ship it",
                "event": {"type": "note", "text": "the total is wrong"}
            })),
            "ship it: noted “the total is wrong”"
        );
        assert_eq!(
            pulse_line(&json!({
                "at": 1, "title": "watercooler",
                "event": {"type": "message", "snippet": "hello everyone"}
            })),
            "watercooler: “hello everyone”"
        );
        // A row with no title is still a row, not a line beginning ": ".
        assert_eq!(
            pulse_line(&json!({"at": 1, "event": {"type": "invented_later"}})),
            "invented later"
        );
        // What a listener did is an engine's fact, filed under its host: it
        // is said in words, never as its bare tag.
        let fired = pulse_line(&json!({
            "at": 1, "title": "Nightly report",
            "event": {
                "type": "listener_fired", "listener": "workspace:01WF/nightly",
                "signal": "01S", "outcome": {"outcome": "started", "run": "01RUN"}
            }
        }));
        assert!(
            fired.starts_with("Nightly report: ")
                && fired.contains("workspace:01WF/nightly")
                && fired.contains("01RUN"),
            "{fired}"
        );
    }
}
