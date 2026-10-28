//! `bisa note`: the notes — a person's markdown scratchpads about the
//! workspace, the node, a goal, a project, a workflow or a channel — from the
//! terminal.
//!
//! Every verb goes through a running node when there is one (`/notes`, the
//! routes the desktop's overlay uses, so an open window hears a note made or
//! removed) and through an engine of this process's own otherwise, the rows
//! one shape either way (`NoteRow`). A note has two writers — you and any
//! agent you asked — so an edit of the body carries the hash of the body it
//! read: `--base-hash` as `show` printed it, or, left out, the hash the note
//! has as the edit begins; a body that moved since is refused in the store's
//! words, never written over.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Context as _, Result};
use bisa_core::{NoteId, OwnerScope};
use bisa_node::dto::NoteRow;
use bisa_store::{body_hash, NewNote, NotePatch, OwnerFilter};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum NoteCmd {
    /// Every note — one kind's with --scope, one record's with --scope and --id
    List {
        /// A kind: workspace, node, goal, project, workflow or channel
        #[arg(long)]
        scope: Option<String>,
        /// The record's id, for a kind that names one
        #[arg(long, requires = "scope")]
        id: Option<String>,
    },
    /// One note: its facts, the hash an edit of its body must carry, and the body
    Show { id: String },
    /// Make a note under a scope
    New {
        /// Where it belongs: workspace, node, goal, project, workflow or channel
        #[arg(long, default_value = "workspace")]
        scope: String,
        /// The record's id, for a kind that names one
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        title: String,
        /// The body, as Markdown
        #[arg(long, conflicts_with = "body_file")]
        body: Option<String>,
        /// The body, read from this file
        #[arg(long, value_name = "PATH")]
        body_file: Option<PathBuf>,
    },
    /// Edit a note: its title, its body — against the hash of the body it replaces — or its pin
    Edit {
        id: String,
        #[arg(long)]
        title: Option<String>,
        /// The new body, as Markdown
        #[arg(long, conflicts_with = "body_file")]
        body: Option<String>,
        /// The new body, read from this file
        #[arg(long, value_name = "PATH")]
        body_file: Option<PathBuf>,
        /// The hash of the body you read (`show` prints it); left out, the body as it stands when the edit begins
        #[arg(long, value_name = "SHA256")]
        base_hash: Option<String>,
        /// Pin it at the top of its list, or unpin it
        #[arg(long, value_name = "true|false")]
        pinned: Option<bool>,
    },
    /// Delete a note, and the conversations about it
    Rm { id: String },
}

pub async fn note(ctx: &Ctx, out: &Out, cmd: NoteCmd) -> Result<()> {
    match cmd {
        NoteCmd::List { scope, id } => list(ctx, out, scope.as_deref(), id.as_deref()).await,
        NoteCmd::Show { id } => show(ctx, out, &id).await,
        NoteCmd::New {
            scope,
            id,
            title,
            body,
            body_file,
        } => {
            let body = body_text(body, body_file)?.unwrap_or_default();
            new(ctx, out, &scope, id.as_deref(), &title, &body).await
        }
        NoteCmd::Edit {
            id,
            title,
            body,
            body_file,
            base_hash,
            pinned,
        } => {
            let body = body_text(body, body_file)?;
            edit(ctx, out, &id, title, body, base_hash, pinned).await
        }
        NoteCmd::Rm { id } => rm(ctx, out, &id).await,
    }
}

/// The body a verb was given: the words, the file's text, or neither.
fn body_text(body: Option<String>, file: Option<PathBuf>) -> Result<Option<String>> {
    match (body, file) {
        (Some(text), _) => Ok(Some(text)),
        (None, Some(path)) => std::fs::read_to_string(&path).map(Some).with_context(|| {
            bisa_core::text!("cli-notes-reading-file", path = path.display().to_string())
        }),
        (None, None) => Ok(None),
    }
}

/// The scope two words name, checked as the node checks them: a kind off the
/// list, a standalone kind with an id, a record kind without one, each a
/// refusal in a sentence.
fn scope_of(kind: &str, id: Option<&str>) -> Result<OwnerScope> {
    let id = id.map(str::trim).filter(|s| !s.is_empty());
    if !OwnerScope::KINDS.contains(&kind) {
        bail!(bisa_core::text!(
            "cli-notes-scope-unknown",
            kind = kind.to_string(),
            kinds = OwnerScope::KINDS.join(", ")
        ));
    }
    match (OwnerScope::kind_takes_id(kind), id) {
        (false, Some(_)) => bail!(bisa_core::text!(
            "cli-notes-scope-takes-no-id",
            kind = kind.to_string()
        )),
        (true, None) => bail!(bisa_core::text!(
            "cli-notes-scope-needs-id",
            kind = kind.to_string()
        )),
        (_, id) => OwnerScope::from_parts(kind, id).ok_or_else(|| {
            anyhow::anyhow!(bisa_core::text!(
                "cli-notes-scope-id",
                kind = kind.to_string(),
                id = id.unwrap_or("").to_string()
            ))
        }),
    }
}

/// What a listing lists: nothing asked is every note; a kind alone is every
/// note of that kind; a kind with an id is one scope's.
fn filter_of(kind: Option<&str>, id: Option<&str>) -> Result<OwnerFilter> {
    match (kind, id.map(str::trim).filter(|s| !s.is_empty())) {
        (None, _) => Ok(OwnerFilter::All),
        (Some(kind), None) if OwnerScope::kind_takes_id(kind) => OwnerScope::KINDS
            .iter()
            .find(|k| **k == kind)
            .map(|k| OwnerFilter::Kind(k))
            .ok_or_else(|| {
                anyhow::anyhow!(bisa_core::text!(
                    "cli-notes-scope-unknown",
                    kind = kind.to_string(),
                    kinds = OwnerScope::KINDS.join(", ")
                ))
            }),
        (Some(kind), id) => Ok(OwnerFilter::Scope(scope_of(kind, id)?)),
    }
}

fn parse_note_id(raw: &str) -> Result<NoteId> {
    raw.parse::<NoteId>().map_err(|e| {
        anyhow::anyhow!(bisa_core::text!(
            "cli-notes-note-id",
            id = raw.to_string(),
            e = e.to_string()
        ))
    })
}

/// A note as the wire draws it — the node's row, built here from the store's
/// record when no node runs, so both paths print the same thing.
fn row(note: bisa_core::Note) -> Value {
    serde_json::to_value(NoteRow {
        id: note.id.to_string(),
        scope: note.scope.kind().to_string(),
        scope_id: note.scope.id(),
        hash: body_hash(&note.body),
        title: note.title,
        body: note.body,
        created_at: note.created_at,
        updated_at: note.updated_at,
        pinned: note.pinned,
    })
    .unwrap_or(Value::Null)
}

/// One line of a listing: the id, where it belongs, the title, the pin.
fn list_line(note: &Value) -> String {
    let scope = match note["scope_id"].as_str() {
        Some(id) => format!("{}:{id}", note["scope"].as_str().unwrap_or("")),
        None => note["scope"].as_str().unwrap_or("").to_string(),
    };
    let pinned = if note["pinned"].as_bool().unwrap_or(false) {
        format!(
            "  {}",
            bisa_i18n::say(&bisa_core::text!("cli-notes-pinned"))
        )
    } else {
        String::new()
    };
    format!(
        "{}  {scope:<26}  {}{pinned}",
        note["id"].as_str().unwrap_or(""),
        note["title"].as_str().unwrap_or("")
    )
}

async fn list(ctx: &Ctx, out: &Out, scope: Option<&str>, id: Option<&str>) -> Result<()> {
    // The filter is checked here before either path, so a wrong word is one
    // sentence whether or not a node runs.
    let filter = filter_of(scope, id)?;
    let notes: Vec<Value> = if let Some(node) = ctx.node_client().await {
        let mut q = String::new();
        if let Some(scope) = scope {
            q.push_str(&format!("?scope={scope}"));
            if let Some(id) = id {
                q.push_str(&format!("&id={id}"));
            }
        }
        node.get(&format!("/notes{q}")).await?["notes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    } else {
        ctx.workspace()?
            .list_notes(filter)?
            .into_iter()
            .map(row)
            .collect()
    };
    if notes.is_empty() {
        out.say(&bisa_core::text!("cli-notes-none"));
    }
    for note in &notes {
        out.human(&list_line(note));
    }
    out.json_value(json!({"notes": notes}));
    Ok(())
}

async fn show(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let note_id = parse_note_id(id)?;
    let note = if let Some(node) = ctx.node_client().await {
        node.get(&format!("/notes/{note_id}")).await?["note"].clone()
    } else {
        row(ctx.workspace()?.get_note(note_id)?)
    };
    print_note(out, &note);
    out.json_value(json!({"note": note}));
    Ok(())
}

/// A note in full: the facts a listing shows, the hash an edit carries, a
/// blank line, the body as written.
fn print_note(out: &Out, note: &Value) {
    out.human(&list_line(note));
    out.human(&format!(
        "{}: {}",
        bisa_i18n::say(&bisa_core::text!("cli-notes-hash")),
        note["hash"].as_str().unwrap_or("")
    ));
    out.human("");
    out.human(note["body"].as_str().unwrap_or(""));
}

async fn new(
    ctx: &Ctx,
    out: &Out,
    scope: &str,
    id: Option<&str>,
    title: &str,
    body: &str,
) -> Result<()> {
    let owner = scope_of(scope, id)?;
    let note = if let Some(node) = ctx.node_client().await {
        let mut payload = json!({"scope": scope, "title": title, "body": body});
        if let Some(id) = owner.id() {
            payload["id"] = json!(id);
        }
        node.post("/notes", payload).await?["note"].clone()
    } else {
        let engine = ctx.quiet_engine().await?;
        let made = bisa_engine::notes::create(
            engine.inner(),
            NewNote {
                scope: owner,
                title: title.to_string(),
                body: body.to_string(),
            },
        );
        engine.shutdown().await;
        row(made?)
    };
    out.say(&bisa_core::text!(
        "cli-notes-created",
        id = note["id"].as_str().unwrap_or("").to_string()
    ));
    out.json_value(json!({"note": note}));
    Ok(())
}

async fn edit(
    ctx: &Ctx,
    out: &Out,
    id: &str,
    title: Option<String>,
    body: Option<String>,
    base_hash: Option<String>,
    pinned: Option<bool>,
) -> Result<()> {
    let note_id = parse_note_id(id)?;
    if title.is_none() && body.is_none() && pinned.is_none() {
        bail!(bisa_core::text!("cli-notes-nothing-to-edit"));
    }
    let note = if let Some(node) = ctx.node_client().await {
        // The hash the edit carries: the one given, else the body's as it
        // stands now — read first, so the write names what it replaces.
        let base_hash = match (&body, base_hash) {
            (Some(_), None) => node.get(&format!("/notes/{note_id}")).await?["note"]["hash"]
                .as_str()
                .map(str::to_string),
            (_, given) => given,
        };
        let mut payload = json!({});
        if let Some(title) = title {
            payload["title"] = json!(title);
        }
        if let Some(body) = body {
            payload["body"] = json!(body);
        }
        if let Some(pinned) = pinned {
            payload["pinned"] = json!(pinned);
        }
        if let Some(hash) = base_hash {
            payload["base_hash"] = json!(hash);
        }
        node.patch(&format!("/notes/{note_id}"), payload).await?["note"].clone()
    } else {
        let engine = ctx.quiet_engine().await?;
        let base_hash = match (&body, base_hash) {
            (Some(_), None) => engine
                .workspace()
                .get_note(note_id)
                .map(|n| body_hash(&n.body))
                .ok(),
            (_, given) => given,
        };
        let edited = bisa_engine::notes::update(
            engine.inner(),
            note_id,
            NotePatch {
                title,
                body,
                pinned,
            },
            base_hash.as_deref(),
        );
        engine.shutdown().await;
        row(edited?)
    };
    out.say(&bisa_core::text!(
        "cli-notes-edited",
        id = note_id.to_string()
    ));
    out.json_value(json!({"note": note}));
    Ok(())
}

async fn rm(ctx: &Ctx, out: &Out, id: &str) -> Result<()> {
    let note_id = parse_note_id(id)?;
    if let Some(node) = ctx.node_client().await {
        node.delete(&format!("/notes/{note_id}")).await?;
    } else {
        let engine = ctx.quiet_engine().await?;
        let removed = bisa_engine::notes::delete(engine.inner(), note_id);
        engine.shutdown().await;
        removed?;
    }
    out.say(&bisa_core::text!(
        "cli-notes-removed",
        id = note_id.to_string()
    ));
    out.json_value(json!({"ok": true, "id": note_id.to_string()}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_two_words_checked_as_the_node_checks_them() {
        assert_eq!(scope_of("workspace", None).unwrap(), OwnerScope::Workspace);
        assert_eq!(
            scope_of("node", Some(" ")).unwrap(),
            OwnerScope::Node,
            "a blank id is none"
        );
        assert!(
            scope_of("workspace", Some("x")).is_err(),
            "a standalone kind takes no id"
        );
        assert!(scope_of("goal", None).is_err(), "a record kind needs one");
        assert!(scope_of("goal", Some("not-an-id")).is_err());
        assert!(scope_of("pet", None).is_err(), "a kind off the list");
        let goal = "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string();
        assert!(matches!(
            scope_of("goal", Some(&goal)).unwrap(),
            OwnerScope::Goal { .. }
        ));
        assert!(matches!(filter_of(None, None).unwrap(), OwnerFilter::All));
        assert!(
            matches!(filter_of(None, Some("x")).unwrap(), OwnerFilter::All),
            "an id with no kind narrows nothing"
        );
        assert!(matches!(
            filter_of(Some("project"), None).unwrap(),
            OwnerFilter::Kind("project")
        ));
        assert!(matches!(
            filter_of(Some("workspace"), None).unwrap(),
            OwnerFilter::Scope(OwnerScope::Workspace)
        ));
        assert!(filter_of(Some("pet"), None).is_err());
        assert!(matches!(
            filter_of(Some("goal"), Some(&goal)).unwrap(),
            OwnerFilter::Scope(OwnerScope::Goal { .. })
        ));
    }

    #[test]
    fn a_body_is_the_words_or_the_files_text() {
        assert_eq!(
            body_text(Some("a".into()), None).unwrap().as_deref(),
            Some("a")
        );
        assert_eq!(body_text(None, None).unwrap(), None);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("body.md");
        std::fs::write(&file, "# from a file\n").unwrap();
        assert_eq!(
            body_text(None, Some(file)).unwrap().as_deref(),
            Some("# from a file\n")
        );
        assert!(body_text(None, Some(dir.path().join("missing.md"))).is_err());
    }

    #[test]
    fn a_listing_line_names_the_place_the_title_and_the_pin() {
        let plain =
            json!({"id": "01J0NOTE", "scope": "workspace", "title": "Why", "pinned": false});
        assert_eq!(
            list_line(&plain),
            format!("01J0NOTE  {:<26}  Why", "workspace")
        );
        let pinned = json!({"id": "01J0NOTE", "scope": "goal", "scope_id": "01J0GOAL", "title": "Why", "pinned": true});
        assert!(
            list_line(&pinned).starts_with(&format!("01J0NOTE  {:<26}  Why  ", "goal:01J0GOAL"))
        );
        assert!(list_line(&pinned).ends_with("pinned"));
    }
}
