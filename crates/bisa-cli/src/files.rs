//! `bisa files`: the folders a goal, a project, a workstream or a work
//! item owns, from the terminal.
//!
//! `tree` lists, annotated — a person looking at a goal's folder sees its
//! life (what it plans against, where its work ran, what it produced) rather
//! than a directory dump. `show` prints one file, capped, and refuses to
//! spray a PNG across the terminal. `search` and `replace` read and rewrite
//! across a root; `new`, `mv`, `cp` and `rm` change what is in it.
//!
//! **The node first**, every verb: through a running node when there is one
//! — it holds the workspace, and with the desktop open one always runs — and
//! through the workspace, or an engine of this process's own for a write,
//! when there is none. Both ways call the same functions — [`bisa_store::tree`]
//! holds the resolution, the containment check and the bounds, the engine's
//! `ide::files` and `ide::search` the writes — so the two never disagree
//! about what is inside a root, and the answer is the wire's either way.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{Context as _, Result};
use bisa_store::{FileContent, FileScope, FileTree};
use clap::Subcommand;
use serde_json::json;
use std::str::FromStr;

#[derive(Subcommand)]
pub enum FilesCmd {
    /// List what a scope's folder holds, annotated by what each entry is
    Tree {
        /// `goal` | `project` | `workstream` | `work_item`
        scope: String,
        /// The goal, project, workstream or work item id
        id: String,
        /// Sub-path to list, relative to the scope's root
        #[arg(long)]
        path: Option<String>,
        /// How many levels to descend (default 1, capped)
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Print one file under a scope's root
    Show {
        scope: String,
        id: String,
        /// The file, relative to the scope's root
        #[arg(long)]
        path: String,
    },
    /// Search the scope's files: `.gitignore` honoured, binaries skipped, symlinks never followed
    Search {
        scope: String,
        id: String,
        query: String,
        #[command(flatten)]
        opts: SearchOpts,
    },
    /// Replace across the scope's files. Previews by default; `--apply` writes each file by
    /// compare-and-swap and reports any that changed since the preview.
    Replace {
        scope: String,
        id: String,
        query: String,
        replacement: String,
        #[arg(long)]
        apply: bool,
        #[command(flatten)]
        opts: SearchOpts,
    },
    /// Create an empty file (or, with `--dir`, a folder) under the scope's writable root
    New {
        scope: String,
        id: String,
        /// The path to create, relative to the root
        #[arg(long)]
        path: String,
        /// Create a folder rather than a file
        #[arg(long)]
        dir: bool,
    },
    /// Rename or move within the root; the target must not exist
    Mv {
        scope: String,
        id: String,
        from: String,
        to: String,
    },
    /// Duplicate a file or a folder within the root; the target must not exist
    Cp {
        scope: String,
        id: String,
        from: String,
        to: String,
    },
    /// Remove a file, or a folder with `--recursive` — to the OS trash when `editor.delete.trash`
    /// is on for the root's project (the default), which the answer says
    Rm {
        scope: String,
        id: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        recursive: bool,
    },
}

#[derive(clap::Args, Clone, Debug, Default)]
pub struct SearchOpts {
    /// The query is a regular expression (otherwise it is matched literally)
    #[arg(long)]
    pub regex: bool,
    /// `smart` (default: case-insensitive unless the query has an upper-case letter), `sensitive` or `insensitive`
    #[arg(long, default_value = "smart")]
    pub case: String,
    /// Whole words only
    #[arg(long)]
    pub word: bool,
    /// A glob a path must match (repeatable)
    #[arg(long = "include")]
    pub include: Vec<String>,
    /// A glob that excludes a path (repeatable)
    #[arg(long = "exclude")]
    pub exclude: Vec<String>,
    /// Stop after this many matches
    #[arg(long)]
    pub limit: Option<usize>,
}

impl SearchOpts {
    /// The search as the node's route reads it: a query string.
    fn address(&self, q: &str) -> String {
        let mut said = format!(
            "q={}&regex={}&case={}&word={}",
            query(q),
            self.regex,
            query(&self.case),
            self.word
        );
        for (name, globs) in [("include", &self.include), ("exclude", &self.exclude)] {
            if !globs.is_empty() {
                said.push_str(&format!("&{name}={}", query(&globs.join(","))));
            }
        }
        if let Some(limit) = self.limit {
            said.push_str(&format!("&limit={limit}"));
        }
        said
    }

    fn query(&self, q: &str) -> Result<bisa_engine::ide::search::SearchQuery> {
        use bisa_engine::ide::search::CaseMode;
        let case = match self.case.as_str() {
            "smart" => CaseMode::Smart,
            "sensitive" => CaseMode::Sensitive,
            "insensitive" => CaseMode::Insensitive,
            other => anyhow::bail!(bisa_core::text!(
                "cli-files-case-smart-sensitive-insensitive",
                other = format!("{other:?}")
            )),
        };
        Ok(bisa_engine::ide::search::SearchQuery {
            q: q.to_string(),
            regex: self.regex,
            case,
            word: self.word,
            include: self.include.clone(),
            exclude: self.exclude.clone(),
            limit: self.limit,
        })
    }
}

pub async fn files(ctx: &Ctx, out: &Out, cmd: FilesCmd) -> Result<()> {
    match cmd {
        FilesCmd::Tree {
            scope,
            id,
            path,
            depth,
        } => tree(ctx, out, &scope, &id, path.as_deref().unwrap_or(""), depth).await,
        FilesCmd::Show { scope, id, path } => show(ctx, out, &scope, &id, &path).await,
        FilesCmd::Search {
            scope,
            id,
            query,
            opts,
        } => search(ctx, out, &scope, &id, &query, &opts).await,
        FilesCmd::Replace {
            scope,
            id,
            query,
            replacement,
            apply,
            opts,
        } => replace(ctx, out, &scope, &id, &query, &replacement, apply, &opts).await,
        FilesCmd::New {
            scope,
            id,
            path,
            dir,
        } => mutate(ctx, out, &scope, &id, Change::New { path, dir }).await,
        FilesCmd::Mv {
            scope,
            id,
            from,
            to,
        } => mutate(ctx, out, &scope, &id, Change::Move { from, to }).await,
        FilesCmd::Cp {
            scope,
            id,
            from,
            to,
        } => mutate(ctx, out, &scope, &id, Change::Copy { from, to }).await,
        FilesCmd::Rm {
            scope,
            id,
            path,
            recursive,
        } => mutate(ctx, out, &scope, &id, Change::Remove { path, recursive }).await,
    }
}

/// One change to what a root holds.
enum Change {
    New { path: String, dir: bool },
    Move { from: String, to: String },
    Copy { from: String, to: String },
    Remove { path: String, recursive: bool },
}

impl Change {
    /// Asked of the node, which answers what it did.
    async fn asked(
        &self,
        client: &crate::client::NodeClient,
        scope: FileScope,
        id: &str,
    ) -> Result<serde_json::Value> {
        let root = format!("/ide/files/{scope}/{id}");
        match self {
            Change::New { path, dir } => {
                let kind = if *dir { "dir" } else { "file" };
                client
                    .post(&root, json!({ "path": path, "kind": kind }))
                    .await
            }
            Change::Move { from, to } => {
                client
                    .post(&format!("{root}/move"), json!({ "from": from, "to": to }))
                    .await
            }
            Change::Copy { from, to } => {
                client
                    .post(&format!("{root}/copy"), json!({ "from": from, "to": to }))
                    .await
            }
            Change::Remove { path, recursive } => {
                client
                    .delete(&format!(
                        "{root}?path={}&recursive={recursive}",
                        query(path)
                    ))
                    .await
            }
        }
    }

    /// Made by an engine of this process's own, answered as the node would.
    fn made(
        &self,
        inner: &std::sync::Arc<bisa_engine::Inner>,
        scope: FileScope,
        id: &str,
    ) -> Result<serde_json::Value, bisa_engine::EngineError> {
        use bisa_engine::ide::files;
        Ok(match self {
            Change::New { path, dir } => {
                let kind = if *dir {
                    files::EntryKind::Dir
                } else {
                    files::EntryKind::File
                };
                let made = files::create_entry(inner, scope, id, path, kind)?;
                json!({"path": made, "kind": if *dir { "dir" } else { "file" }})
            }
            Change::Move { from, to } => {
                let moved = files::move_entry(inner, scope, id, from, to)?;
                json!({"from": from, "path": moved})
            }
            Change::Copy { from, to } => {
                let copied = files::copy_entry(inner, scope, id, from, to)?;
                json!({"from": from, "path": copied})
            }
            Change::Remove { path, recursive } => {
                let project = match scope {
                    FileScope::Workstream => id
                        .parse()
                        .ok()
                        .and_then(|wid| inner.ws.get_workstream(wid).ok())
                        .map(|w| w.project),
                    _ => None,
                };
                let disposal = files::Disposal::from_setting(files::setting_bool(
                    inner,
                    project,
                    "editor.delete.trash",
                    true,
                ));
                let gone = files::delete_entry(inner, scope, id, path, *recursive, disposal)?;
                json!({"path": gone.path, "disposal": gone.disposal})
            }
        })
    }

    /// What was done, in a sentence, from the answer.
    fn sentence(&self, answer: &serde_json::Value) -> String {
        let path = answer["path"].as_str().unwrap_or_default();
        match self {
            Change::New { .. } => format!("created {path}"),
            Change::Move { from, .. } => bisa_i18n::say(&bisa_core::text!(
                "cli-files-moved",
                from = from.to_string(),
                moved = path.to_string()
            )),
            Change::Copy { from, .. } => bisa_i18n::say(&bisa_core::text!(
                "cli-files-copied",
                from = from.to_string(),
                copied = path.to_string()
            )),
            Change::Remove { .. } => {
                let how = if answer["disposal"] == "trash" {
                    bisa_i18n::say(&bisa_core::text!("cli-files-moved-to-trash"))
                } else {
                    bisa_i18n::say(&bisa_core::text!("cli-files-removed"))
                };
                format!("{how}: {path}")
            }
        }
    }

    /// The answer as the verb prints it: what the change is about, and
    /// nothing of the wire's own.
    fn printed(&self, answer: &serde_json::Value) -> serde_json::Value {
        match self {
            Change::New { .. } => json!({"path": answer["path"], "kind": answer["kind"]}),
            Change::Move { .. } | Change::Copy { .. } => {
                json!({"from": answer["from"], "path": answer["path"]})
            }
            Change::Remove { .. } => {
                json!({"path": answer["path"], "disposal": answer["disposal"]})
            }
        }
    }
}

/// One write under a root — the confinement check, the announcement and the
/// settings are the engine's: the node's when one runs, this process's own
/// when none does.
async fn mutate(ctx: &Ctx, out: &Out, scope: &str, id: &str, change: Change) -> Result<()> {
    let scope = parse_scope(scope)?;
    let answer = if let Some(client) = ctx.node_client().await {
        change.asked(&client, scope, id).await?
    } else {
        let (engine, _) = ctx.engine().await?;
        let made = change.made(engine.inner(), scope, id);
        engine.shutdown().await;
        made?
    };
    out.human(&change.sentence(&answer));
    out.json_value(change.printed(&answer));
    Ok(())
}

async fn search(
    ctx: &Ctx,
    out: &Out,
    scope: &str,
    id: &str,
    q: &str,
    opts: &SearchOpts,
) -> Result<()> {
    use bisa_engine::ide::search;
    let scope = parse_scope(scope)?;
    let asked = opts.query(q)?;
    let mut hits = Vec::new();
    let mut found = |hit: search::SearchHit| {
        out.human(&format!(
            "{}:{}:{}: {}",
            hit.path, hit.line, hit.column, hit.text
        ));
        hits.push(hit);
    };
    let summary = if let Some(client) = ctx.node_client().await {
        let mut frames = client
            .stream(&format!("/ide/search/{scope}/{id}?{}", opts.address(q)))
            .await?;
        let mut summary = None;
        while let Some(frame) = frames.recv().await {
            match frame["type"].as_str() {
                Some("hit") => found(
                    serde_json::from_value(frame["hit"].clone())
                        .context(bisa_core::text!("cli-files-bad-search-payload-from-node"))?,
                ),
                Some("done") => {
                    summary = Some(
                        serde_json::from_value::<search::SearchSummary>(frame)
                            .context(bisa_core::text!("cli-files-bad-search-payload-from-node"))?,
                    );
                    break;
                }
                Some("error") => {
                    anyhow::bail!(frame["error"].as_str().unwrap_or_default().to_string())
                }
                _ => {}
            }
        }
        // A stream that ended with no last frame is a search nobody finished.
        summary.context(bisa_core::text!("cli-files-search-ended-early"))?
    } else {
        let (engine, _) = ctx.engine().await?;
        let done = search::search(engine.inner(), scope, id, &asked, |hit| {
            found(hit);
            true
        });
        engine.shutdown().await;
        done?
    };
    out.say(&bisa_core::text!(
        "cli-files-matches-files-scanned",
        a0 = (summary.matches).to_string(),
        a1 = (summary.files_with_matches).to_string(),
        a2 = (summary.files_scanned).to_string(),
        a3 = (if summary.truncated { ", truncated" } else { "" }).to_string()
    ));
    out.json_value(json!({"hits": hits, "summary": summary}));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn replace(
    ctx: &Ctx,
    out: &Out,
    scope: &str,
    id: &str,
    q: &str,
    replacement: &str,
    apply: bool,
    opts: &SearchOpts,
) -> Result<()> {
    use bisa_engine::ide::search;
    let scope = parse_scope(scope)?;
    let query = opts.query(q)?;
    if let Some(client) = ctx.node_client().await {
        return replace_via_node(&client, out, scope, id, q, replacement, apply, opts).await;
    }
    let (engine, _) = ctx.engine().await?;
    let previews = search::preview_replace(engine.inner(), scope, id, &query, replacement);
    let previews = match previews {
        Ok(previews) => previews,
        Err(e) => {
            engine.shutdown().await;
            return Err(e.into());
        }
    };
    say_previews(out, &previews);
    if !apply {
        out.say(&bisa_core::text!(
            "cli-files-files-would-change-pass-apply-write",
            a0 = (previews.len()).to_string()
        ));
        out.json_value(json!({"applied": false, "files": previews}));
        engine.shutdown().await;
        return Ok(());
    }
    let files: Vec<(String, String)> = previews
        .iter()
        .map(|p| (p.path.clone(), p.base_hash.clone()))
        .collect();
    let outcomes = search::apply_replace(engine.inner(), scope, id, &query, replacement, &files);
    engine.shutdown().await;
    let outcomes = outcomes?;
    say_outcomes(out, &outcomes);
    out.json_value(json!({"applied": true, "files": outcomes}));
    Ok(())
}

/// What a replacement would change, a line before and a line after.
fn say_previews(out: &Out, previews: &[bisa_engine::ide::search::ReplacePreview]) {
    for p in previews {
        for c in &p.changes {
            out.human(&format!(
                "{}:{}: - {}\n{}:{}: + {}",
                p.path, c.line, c.before, p.path, c.line, c.after
            ));
        }
    }
}

/// What became of each file a replacement was applied to.
fn say_outcomes(out: &Out, outcomes: &[bisa_engine::ide::search::ReplaceOutcome]) {
    for o in outcomes {
        out.human(&format!("{o:?}"));
    }
}

/// The replacement through the node: its preview, and with `apply` the
/// write of what was previewed, each file under the hash its preview read.
#[allow(clippy::too_many_arguments)]
async fn replace_via_node(
    client: &crate::client::NodeClient,
    out: &Out,
    scope: FileScope,
    id: &str,
    q: &str,
    replacement: &str,
    apply: bool,
    opts: &SearchOpts,
) -> Result<()> {
    use bisa_engine::ide::search;
    let route = format!("/ide/replace/{scope}/{id}");
    let body = |files: Option<serde_json::Value>| {
        let mut body = json!({
            "q": q,
            "regex": opts.regex,
            "case": opts.case,
            "word": opts.word,
            "include": opts.include,
            "exclude": opts.exclude,
            "replacement": replacement,
            "apply": files.is_some(),
        });
        if let Some(files) = files {
            body["files"] = files;
        }
        body
    };
    let answer = client.post(&route, body(None)).await?;
    let previews: Vec<search::ReplacePreview> = serde_json::from_value(answer["files"].clone())
        .context(bisa_core::text!("cli-files-bad-search-payload-from-node"))?;
    say_previews(out, &previews);
    if !apply {
        out.say(&bisa_core::text!(
            "cli-files-files-would-change-pass-apply-write",
            a0 = (previews.len()).to_string()
        ));
        out.json_value(json!({"applied": false, "files": previews}));
        return Ok(());
    }
    if previews.is_empty() {
        // Nothing would change: there is nothing to write, and nothing to ask.
        out.json_value(json!({"applied": true, "files": []}));
        return Ok(());
    }
    let files: Vec<serde_json::Value> = previews
        .iter()
        .map(|p| json!({"path": p.path, "base_hash": p.base_hash}))
        .collect();
    let answer = client.post(&route, body(Some(json!(files)))).await?;
    let outcomes: Vec<search::ReplaceOutcome> = serde_json::from_value(answer["files"].clone())
        .context(bisa_core::text!("cli-files-bad-search-payload-from-node"))?;
    say_outcomes(out, &outcomes);
    out.json_value(json!({"applied": true, "files": outcomes}));
    Ok(())
}

/// One of the three, refused here rather than after a round trip — and with
/// the same sentence the node would have sent back.
fn parse_scope(raw: &str) -> Result<FileScope> {
    FileScope::from_str(raw).map_err(|_| {
        let valid: Vec<&str> = FileScope::ALL.iter().map(|s| s.as_str()).collect();
        anyhow::anyhow!(bisa_core::text!(
            "cli-files-unknown-file-scope-use-one",
            raw = format!("{raw:?}"),
            a0 = (valid.join(", ")).to_string()
        ))
    })
}

async fn tree(
    ctx: &Ctx,
    out: &Out,
    scope: &str,
    id: &str,
    path: &str,
    depth: Option<usize>,
) -> Result<()> {
    let scope = parse_scope(scope)?;
    let tree: FileTree = if let Some(client) = ctx.node_client().await {
        let mut url = format!("/tree/{scope}/{id}?path={}", query(path));
        if let Some(d) = depth {
            url.push_str(&format!("&depth={d}"));
        }
        serde_json::from_value(client.get(&url).await?)
            .context(bisa_core::text!("cli-files-bad-tree-payload-from-node"))?
    } else {
        ctx.workspace()?.list_tree(scope, id, path, depth)?
    };

    out.human(&format!("{}", tree.root.display()));
    for e in &tree.entries {
        let name = match (e.dir, e.symlink) {
            (_, true) => format!("{} ->", e.path),
            (true, false) => format!("{}/", e.path),
            (false, false) => e.path.clone(),
        };
        out.human(&format!(
            "  {:<9} {:<52} {:>9}",
            e.kind.as_str(),
            name,
            e.size.map(bytes).unwrap_or_default()
        ));
    }
    if tree.entries.is_empty() {
        out.say(&bisa_core::text!("cli-files-empty"));
    }
    // Two bounds, two sentences, so the reader knows which knob to turn.
    if tree.truncated {
        out.say(&bisa_core::text!("cli-files-listing-hit-entry-cap"));
    }
    if tree.deeper {
        out.say(&bisa_core::text!(
            "cli-files-more-below-depth",
            depth = format!("{}", tree.depth)
        ));
    }
    out.json_value(json!({"scope": scope.as_str(), "id": id, "tree": tree}));
    Ok(())
}

async fn show(ctx: &Ctx, out: &Out, scope: &str, id: &str, path: &str) -> Result<()> {
    let scope = parse_scope(scope)?;
    let content: FileContent = if let Some(client) = ctx.node_client().await {
        let url = format!("/file/{scope}/{id}?path={}", query(path));
        serde_json::from_value(client.get(&url).await?)
            .context(bisa_core::text!("cli-files-bad-file-payload-from-node"))?
    } else {
        ctx.workspace()?.read_file(scope, id, path)?
    };

    match (&content.text, content.binary) {
        (_, true) => out.say(&bisa_core::text!(
            "cli-files-binary-not-shown",
            a0 = (content.path).to_string(),
            a1 = (bytes(content.size)).to_string()
        )),
        (Some(text), false) => {
            out.human(text.trim_end_matches('\n'));
            if content.truncated {
                out.say(&bisa_core::text!(
                    "cli-files-truncated-first-part",
                    size = bytes(content.size)
                ));
            }
        }
        (None, false) => out.say(&bisa_core::text!("cli-files-nothing-show")),
    }
    out.json_value(json!({"scope": scope.as_str(), "id": id, "file": content}));
    Ok(())
}

/// Percent-encode one query value.
///
/// A path can hold a space, a `#` or an `&`, and each of those would otherwise
/// reach the daemon as a different path — or, for `&`, as a second parameter
/// entirely. RFC 3986's unreserved set plus `/`, which a path separator needs
/// and no query parser treats specially. A dependency for this would be six
/// lines of code traded for a crate.
fn query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A size a person reads, not a byte count they have to divide.
fn bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut size = n as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_value_survives_the_characters_a_path_can_hold() {
        assert_eq!(query("work/notes.md"), "work/notes.md");
        assert_eq!(query("a b&c=d"), "a%20b%26c%3Dd");
        // Not a boundary check — `..` travels intact and is refused where
        // every other path is, by `resolve_within` inside the store.
        assert_eq!(query("../secret"), "../secret");
    }

    #[test]
    fn a_size_reads_as_a_size() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1536), "1.5 KiB");
    }

    #[test]
    fn an_unknown_scope_names_the_three_that_work() {
        let err = parse_scope("agent").unwrap_err().to_string();
        // A project's root is a workstream: there is no project scope.
        assert!(err.contains("goal"), "{err}");
        assert!(err.contains("workstream"), "{err}");
        assert!(err.contains("work_item"), "{err}");
        assert!(!err.contains("project"), "{err}");
    }
}
