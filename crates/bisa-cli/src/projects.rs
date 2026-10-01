//! `bisa project` and `bisa workstream`: the folders a goal owns
//! and the checkouts work happens in, from the terminal.
//!
//! **The node first.** A project made, attached, carried or forgotten and a
//! workstream opened or closed go through the node when one runs — it holds
//! the workspace, and with the desktop open one always runs — and through an
//! engine of this process's own when none does. Either way the answer is
//! the wire's, and the verb says it in the same words.
//!
//! So do the git verbs — what is staged, committed, who commits and the
//! repository's config. The pull request verbs and `push` still run against
//! an **embedded engine**, and are refused while a node holds the workspace.
//! What that changes is the `Publish` gate — pushing a branch and opening a
//! pull request open one, and with no daemon to hold it there is nobody but
//! you to decide it:
//!
//! * on a terminal, you are asked right there;
//! * `--yes` decides it up front, which is a person approving on the command
//!   line rather than a bypass — the decision is recorded and signed exactly
//!   like one taken from the inbox;
//! * with neither (a script, a CI job), the gate id is printed and **nothing
//!   is pushed**. The engine dies with the process, so an undecided gate
//!   cannot leak a push after the fact.
//!
//! Commands that only read records (`list`, `show`) skip the engine
//! entirely and read the workspace, because nothing about them touches a
//! working tree.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::tags::{keeps, TagFilterArgs, TagSetArgs};
use anyhow::{anyhow, bail, Context as _, Result};
use bisa_core::tags::TagEntity;
use bisa_core::{
    Assignee, GoalId, Project, ProjectId, ProjectRoot, PublishPolicy, Slug, Vcs, Workstream,
    WorkstreamId, WorkstreamKind, WorkstreamState,
};
use bisa_engine::{projects as eng, Engine, EnginePayload};
use bisa_store::{NewProject, Workspace, WorkstreamFilter};
use clap::Subcommand;
use serde_json::json;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// Create a project folder, initialised as a git repository. There is
    /// no flag for that: the folder is one Bisa makes, so nothing that
    /// was already somebody's gets written into. A project belongs to the
    /// workspace; `--goal` attaches it to a goal as well.
    New {
        /// Attach the new project to this goal
        #[arg(long)]
        goal: Option<String>,
        /// Folder name (lowercase letters, digits, `-`, `_`)
        slug: String,
        #[arg(long)]
        name: Option<String>,
        /// How pushes leave the machine: auto (default) | gated | manual
        #[arg(long)]
        publish: Option<String>,
        /// Who carries it: `agent:<id>` / `human:<hex>` / `team:<id>`
        #[arg(long = "assignee")]
        assignees: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
        /// Who commits in the new repository, as `"Name <email>"` — two local
        /// git config keys. Omitted, the repository inherits the
        /// global config and asks only when no identity resolves.
        #[arg(long, value_name = "NAME <EMAIL>")]
        committer: Option<String>,
        /// Local git config for the new repository, `key=value`, repeatable
        /// (schema keys: user.name, user.email, user.useConfigOnly,
        /// user.signingkey, commit.gpgsign, pull.rebase, core.autocrlf).
        #[arg(long = "git-config", value_name = "KEY=VALUE")]
        git_config: Vec<String>,
    },
    /// Who authors commits in this project's repository — or, with both
    /// `--name` and `--email`, set it. Written to the repository's **local**
    /// git config, shared by every workstream of the project; never global.
    Identity {
        project: String,
        #[arg(long, requires = "email")]
        name: Option<String>,
        #[arg(long, requires = "name")]
        email: Option<String>,
    },
    /// The git config the platform knows, at one layer: a project's local
    /// layer, or — with `--global` — your global git config, which Bisa
    /// writes here and nowhere else. Without `--set`/`--unset` it
    /// only reads.
    GitConfig {
        /// The project whose local layer to read or write (omit with --global)
        project: Option<String>,
        /// Your global git config instead of a project's
        #[arg(long, conflicts_with = "project")]
        global: bool,
        /// Set a key, `key=value`, repeatable
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Unset a key so it falls through to the layer beneath, repeatable
        #[arg(long = "unset", value_name = "KEY")]
        unset: Vec<String>,
    },
    /// Turn a plain-folder project into a git repository — an adopted folder
    /// too, the one write adopt allows, because this is the ask. `git init`,
    /// the who-commits policy, an empty root commit when someone can commit;
    /// existing copy workstreams stay copies.
    GitInit {
        project: String,
        /// Local git config for the new repository, `key=value`, repeatable
        #[arg(long = "git-config", value_name = "KEY=VALUE")]
        git_config: Vec<String>,
    },
    /// Clone a repository into a new project folder
    Clone {
        /// Attach the new project to this goal
        #[arg(long)]
        goal: Option<String>,
        url: String,
        /// Folder name (default: derived from the url)
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        name: Option<String>,
        /// Shallow clone depth
        #[arg(long)]
        depth: Option<u32>,
        #[arg(long)]
        publish: Option<String>,
        #[arg(long = "assignee")]
        assignees: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
        /// Who commits in the new repository, as `"Name <email>"` — two local
        /// git config keys. Omitted, the repository inherits the
        /// global config and asks only when no identity resolves.
        #[arg(long, value_name = "NAME <EMAIL>")]
        committer: Option<String>,
        /// Local git config for the new repository, `key=value`, repeatable
        /// (schema keys: user.name, user.email, user.useConfigOnly,
        /// user.signingkey, commit.gpgsign, pull.rebase, core.autocrlf).
        #[arg(long = "git-config", value_name = "KEY=VALUE")]
        git_config: Vec<String>,
    },
    /// Import an existing folder: copy it into the workspace's projects
    /// directory. The source folder is read and left exactly as it was.
    Import {
        /// Attach the new project to this goal
        #[arg(long)]
        goal: Option<String>,
        /// Absolute path to the folder to copy in
        path: PathBuf,
        /// Folder name to know it by (default: derived from the path)
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        publish: Option<String>,
        #[arg(long = "assignee")]
        assignees: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
        /// Who commits in the new repository, as `"Name <email>"` — two local
        /// git config keys. Omitted, the repository inherits the
        /// global config and asks only when no identity resolves.
        #[arg(long, value_name = "NAME <EMAIL>")]
        committer: Option<String>,
        /// Local git config for the new repository, `key=value`, repeatable
        /// (schema keys: user.name, user.email, user.useConfigOnly,
        /// user.signingkey, commit.gpgsign, pull.rebase, core.autocrlf).
        #[arg(long = "git-config", value_name = "KEY=VALUE")]
        git_config: Vec<String>,
    },
    /// Adopt an existing folder anywhere on disk, by reference. Bisa
    /// writes nothing into it — workstreams live beside it, under the project's
    /// own record.
    Adopt {
        /// Attach the new project to this goal
        #[arg(long)]
        goal: Option<String>,
        /// Absolute path to the existing folder
        path: PathBuf,
        /// Folder name to know it by (default: derived from the path)
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        publish: Option<String>,
        #[arg(long = "assignee")]
        assignees: Vec<String>,
        #[command(flatten)]
        tags: TagSetArgs,
        /// Who commits in the new repository, as `"Name <email>"` — two local
        /// git config keys. Omitted, the repository inherits the
        /// global config and asks only when no identity resolves.
        #[arg(long, value_name = "NAME <EMAIL>")]
        committer: Option<String>,
        /// Local git config for the new repository, `key=value`, repeatable
        /// (schema keys: user.name, user.email, user.useConfigOnly,
        /// user.signingkey, commit.gpgsign, pull.rebase, core.autocrlf).
        #[arg(long = "git-config", value_name = "KEY=VALUE")]
        git_config: Vec<String>,
    },
    /// Attach a project to a goal (moves no bytes)
    Attach { project: String, goal: String },
    /// Detach a project from a goal (moves no bytes)
    Detach { project: String, goal: String },
    /// List projects. `--goal` shows what one goal is attached to; without
    /// it, every project in the workspace.
    List {
        #[arg(long)]
        goal: Option<String>,
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Show a project: root, git status, assignees, workstreams
    Show { project: String },
    /// Forget a project. The folder stays on disk unless `--tree`.
    Rm {
        project: String,
        /// Also delete the working tree (managed projects only)
        #[arg(long)]
        tree: bool,
    },
    /// List what git has to say about each file in the project's folder
    Files { project: String },
    /// Put files in the index. Index-only: nothing in the folder changes.
    Stage {
        project: String,
        /// Paths as `project files` reports them (relative to the repo root)
        paths: Vec<String>,
    },
    /// Take files back out of the index. **The files themselves are left
    /// exactly as they are.**
    Unstage { project: String, paths: Vec<String> },
    /// One file's patch
    Diff {
        project: String,
        path: String,
        /// The index against HEAD, instead of the folder against the index
        #[arg(long)]
        staged: bool,
    },
    /// Commit the project's folder. Named paths are staged first; with none,
    /// what is already staged is committed — never "everything".
    Commit {
        project: String,
        #[arg(long, short = 'm')]
        message: String,
        /// Stage this path first (repeatable)
        #[arg(long = "path")]
        paths: Vec<String>,
    },
    /// Ask the core agent to draft a commit message for what is staged. It
    /// suggests; nothing about this commits.
    Message { project: String },
    /// Set who carries a project: its agents take work that runs here, its
    /// humans may decide its gates
    Assign {
        project: String,
        /// `agent:<id>` / `human:<hex>` / `team:<id>` (repeatable)
        assignees: Vec<String>,
        /// Replace the list instead of adding to it
        #[arg(long)]
        replace: bool,
    },
}

#[derive(Subcommand)]
pub enum WorkstreamCmd {
    /// List workstreams, optionally filtered
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        goal: Option<String>,
    },
    /// Open a checkout on its own branch
    Open {
        project: String,
        /// Whose folder holds the checkout (default: the project's owner)
        #[arg(long)]
        goal: Option<String>,
        /// Readable half of a derived branch name
        #[arg(long)]
        label: Option<String>,
        /// Agent this workstream belongs to
        #[arg(long)]
        agent: Option<String>,
        /// Where the checkout starts: a new branch name, or `new:<name>@<ref>`,
        /// `branch:<name>`, `remote:<remote>/<name>`, `tag:<name>`,
        /// `new-tag:<name>@<ref>`, `pr:<number>` (default: a derived new branch)
        #[arg(long)]
        from: Option<String>,
        /// The branch the work goes back to (default: the project's default branch)
        #[arg(long)]
        base: Option<String>,
    },
    /// Show a workstream: branch, distance from base, changed files
    Show { workstream: String },
    /// The patch a single commit would record right now
    Diff { workstream: String },
    /// Stage everything and commit
    Commit {
        workstream: String,
        #[arg(long, short = 'm')]
        message: String,
    },
    /// Push the branch to origin — through the Publish gate
    Push {
        workstream: String,
        /// Approve the Publish gate up front (you are the human deciding it)
        #[arg(long)]
        yes: bool,
    },
    /// Open a pull request on the code host behind `origin` — through the Publish gate.
    /// A branch not on the remote yet is pushed first, under the same gate.
    Pr {
        workstream: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        /// A draft, where the code host has them
        #[arg(long)]
        draft: bool,
        /// Approve the Publish gate up front
        #[arg(long)]
        yes: bool,
    },
    /// The workstream's pull request, read fresh from the code host
    PrView { workstream: String },
    /// Check runs on the workstream's pull request
    PrChecks { workstream: String },
    /// Review the workstream's pull request
    PrReview {
        workstream: String,
        /// approve | request-changes | comment
        #[arg(long, default_value = "comment")]
        event: String,
        #[arg(long, default_value = "")]
        body: String,
    },
    /// Merge the workstream's pull request — through the Publish gate
    PrMerge {
        workstream: String,
        /// merge | squash | rebase
        #[arg(long, default_value = "merge")]
        strategy: String,
        /// Leave the branch on the code host after the merge (it is deleted by default)
        #[arg(long)]
        keep_remote_branch: bool,
        /// Approve the Publish gate up front
        #[arg(long)]
        yes: bool,
    },
    /// Close a workstream. The checkout stays on disk unless `--tree`.
    Close {
        workstream: String,
        #[arg(long)]
        tree: bool,
    },
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse the wire form, refusing the whole list on the first bad entry: a
/// half-applied assignment is worse than a rejected one.
pub fn parse_assignees(raw: &[String]) -> Result<Vec<Assignee>> {
    raw.iter()
        .map(|s| {
            s.parse::<Assignee>().map_err(|_| {
                anyhow!(bisa_core::text!(
                    "cli-projects-not-assignee-write-agent-id-human",
                    s = format!("{s:?}")
                ))
            })
        })
        .collect()
}

pub(crate) fn parse_project(s: &str) -> Result<ProjectId> {
    ProjectId::from_str(s).with_context(|| {
        bisa_core::text!(
            "cli-projects-not-project-id-expected-ulid",
            s = format!("{s:?}")
        )
    })
}

fn parse_workstream(s: &str) -> Result<WorkstreamId> {
    WorkstreamId::from_str(s).with_context(|| {
        bisa_core::text!(
            "cli-projects-not-workstream-id-expected-ulid",
            s = format!("{s:?}")
        )
    })
}

fn parse_publish(s: Option<&str>) -> Result<PublishPolicy> {
    // Omitted → the core default (`auto`), read from one place so it
    // never drifts from the domain.
    match s {
        None => Ok(PublishPolicy::default()),
        Some("gated") => Ok(PublishPolicy::Gated),
        Some("manual") => Ok(PublishPolicy::Manual),
        Some("auto") => Ok(PublishPolicy::Auto),
        Some(other) => bail!(bisa_core::text!(
            "cli-projects-unknown-publish-policy-use-gated-manual",
            other = format!("{other:?}")
        )),
    }
}

/// Turn a URL tail or a folder name into something [`validate_slug`] accepts.
///
/// Deriving rather than refusing is the friendly half of `clone` and `adopt`:
/// nobody wants to spell out `--slug my-repo` for `.../my-repo.git`. The
/// result still goes through the validator, so a name that sanitises to
/// nothing is a clear error rather than a surprising directory.
fn derive_slug(raw: &str) -> String {
    let tail = raw
        .trim_end_matches('/')
        .rsplit(['/', '\\', ':'])
        .find(|s| !s.is_empty())
        .unwrap_or(raw);
    let tail = tail.strip_suffix(".git").unwrap_or(tail);
    let mut out = String::new();
    for ch in tail.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches(['-', '_']).to_string()
}

fn slug_of(slug: &str) -> Result<Slug> {
    Slug::new(slug).map_err(|e| {
        anyhow!(bisa_core::text!(
            "cli-projects-slug-lowercase-letters-digits-starting-with",
            e = e.to_string()
        ))
    })
}

fn goal_arg(goal: &Option<String>) -> Result<Option<GoalId>> {
    goal.as_deref().map(crate::parse_goal_id).transpose()
}

fn branch_of(w: &Workstream) -> Option<&str> {
    w.branch()
}

fn base_of(w: &Workstream) -> Option<&str> {
    w.base()
}

/// What the row says where a worktree would say its branch.
fn kind_word(w: &Workstream) -> &str {
    match &w.kind {
        WorkstreamKind::Worktree { branch, .. } => branch,
        WorkstreamKind::Primary => "(primary)",
        WorkstreamKind::Copy => "(copy)",
    }
}

fn state_word(s: &WorkstreamState) -> String {
    match s {
        WorkstreamState::PrOpen { number, .. } => format!("pr #{number}"),
        other => serde_json::to_value(other)
            .ok()
            .and_then(|v| v.get("state").and_then(|s| s.as_str()).map(str::to_string))
            .unwrap_or_else(|| "?".into()),
    }
}

fn vcs_word(v: &Vcs) -> String {
    match v {
        Vcs::None => "plain".into(),
        Vcs::Git { default_branch, .. } => bisa_i18n::say(&bisa_core::text!(
            "cli-projects-git",
            default_branch = default_branch.to_string()
        )),
    }
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

fn render_project(out: &Out, ws: &Workspace, p: &Project) {
    let path = ws.project_root_path(p);
    let root = match &p.root {
        ProjectRoot::Managed => "managed",
        ProjectRoot::External { .. } => "adopted",
    };
    out.human(&format!(
        "{:<28} {:<16} {:<12} {root:<8} {:<20} {}",
        p.id,
        p.slug,
        vcs_word(&p.vcs),
        crate::tags::label(&p.tags),
        path.display()
    ));
}

/// Who commits here, in one line: the pair and where it comes from, or the
/// instruction that sets one.
fn identity_line(id: &bisa_engine::ide::git::GitIdentity) -> String {
    use bisa_engine::ide::git::IdentitySource;
    match (&id.name, &id.email, id.source) {
        (Some(n), Some(e), IdentitySource::Local) => {
            format!("{n} <{e}> (local — set in this repository)")
        }
        (Some(n), Some(e), IdentitySource::Global) => {
            format!("{n} <{e}> (global — pin it to this repository with --name/--email)")
        }
        _ => bisa_i18n::say(&bisa_core::text!(
            "cli-projects-nobody-set-who-commits-here-with"
        )),
    }
}

/// Where a CLI-made project is born: the named goal, or the workspace.
fn origin_for(goal: Option<bisa_core::GoalId>) -> bisa_core::ProjectOrigin {
    match goal {
        Some(goal) => bisa_core::ProjectOrigin::from_goal(goal),
        None => bisa_core::ProjectOrigin::Workspace,
    }
}

pub async fn project(ctx: &Ctx, out: &Out, cmd: ProjectCmd) -> Result<()> {
    match cmd {
        ProjectCmd::New {
            goal,
            slug,
            name,
            publish,
            assignees,
            tags,
            committer,
            git_config,
        } => {
            let goal = goal_arg(&goal)?;
            let new = NewProject {
                origin: origin_for(goal),
                slug: slug_of(&slug)?,
                name,
                root: ProjectRoot::Managed,
                vcs: Vcs::None,
                assignees: parse_assignees(&assignees)?,
                publish: parse_publish(publish.as_deref())?,
                tags: tags.parse()?,
            };
            let source = bisa_engine::ProjectSource::New;
            let made = created(ctx, goal, new, source, committer, git_config).await?;
            out.say(&bisa_core::text!(
                "cli-projects-project-created-git-initialised",
                a0 = (made.project.slug).to_string(),
                a1 = made.path.clone()
            ));
            out.json_value(json!({"project": made.project, "path": made.path}));
            Ok(())
        }
        ProjectCmd::Clone {
            goal,
            url,
            slug,
            name,
            depth,
            publish,
            assignees,
            tags,
            committer,
            git_config,
        } => {
            let goal = goal_arg(&goal)?;
            let slug = slug.unwrap_or_else(|| derive_slug(&url));
            let new = NewProject {
                origin: origin_for(goal),
                slug: slug_of(&slug)?,
                name,
                root: ProjectRoot::Managed,
                vcs: Vcs::None,
                assignees: parse_assignees(&assignees)?,
                publish: parse_publish(publish.as_deref())?,
                tags: tags.parse()?,
            };
            let source = bisa_engine::ProjectSource::Clone {
                url: url.clone(),
                depth,
            };
            let made = created(ctx, goal, new, source, committer, git_config).await?;
            out.say(&bisa_core::text!(
                "cli-projects-cloned-into",
                url = url.to_string(),
                a0 = made.path.clone(),
                a1 = (vcs_word(&made.project.vcs)).to_string()
            ));
            out.json_value(json!({"project": made.project, "path": made.path}));
            Ok(())
        }
        ProjectCmd::Import {
            goal,
            path,
            slug,
            name,
            publish,
            assignees,
            tags,
            committer,
            git_config,
        } => {
            let goal = goal_arg(&goal)?;
            let ws = ctx.workspace()?;
            // The identical containment rule `adopt` and the HTTP route
            // enforce: one implementation, three surfaces.
            let real = bisa_node::projects::resolve_source_path(
                ws.root(),
                &path.to_string_lossy(),
                "import",
            )
            .map_err(|e| anyhow!(e))?;
            let slug = slug.unwrap_or_else(|| derive_slug(&real));
            let new = NewProject {
                origin: origin_for(goal),
                slug: slug_of(&slug)?,
                name,
                // Managed: an import's whole point is that the copy belongs to
                // the workspace.
                root: ProjectRoot::Managed,
                vcs: Vcs::None,
                assignees: parse_assignees(&assignees)?,
                publish: parse_publish(publish.as_deref())?,
                tags: tags.parse()?,
            };
            drop(ws);
            let source = bisa_engine::ProjectSource::Import {
                source: PathBuf::from(&real),
            };
            let made = created(ctx, goal, new, source, committer, git_config).await?;
            let stats = &made.imported;
            if stats.is_null() {
                bail!(bisa_core::text!(
                    "cli-projects-import-reported-no-statistics"
                ));
            }
            out.say(&bisa_core::text!(
                "cli-projects-imported-files-from-into-source-folder",
                a0 = (stats["files"].as_u64().unwrap_or(0)).to_string(),
                real = real.to_string(),
                a1 = made.path.clone(),
                a2 = (vcs_word(&made.project.vcs)).to_string()
            ));
            let skipped = stats["skipped_symlinks"].as_u64().unwrap_or(0);
            if skipped > 0 {
                out.say(&bisa_core::text!(
                    "cli-projects-symlink-s-were-skipped-following-one",
                    a0 = skipped.to_string()
                ));
            }
            out.json_value(
                json!({"project": made.project, "path": made.path, "source": real,
                                  "imported": stats}),
            );
            Ok(())
        }
        ProjectCmd::Adopt {
            goal,
            path,
            slug,
            name,
            publish,
            assignees,
            tags,
            committer,
            git_config,
        } => {
            let goal = goal_arg(&goal)?;
            let ws = ctx.workspace()?;
            // The identical containment rule the HTTP route enforces: one
            // implementation, two surfaces.
            let real = bisa_node::projects::resolve_adopt_path(ws.root(), &path.to_string_lossy())
                .map_err(|e| anyhow!(e))?;
            let slug = slug.unwrap_or_else(|| derive_slug(&real));
            let new = NewProject {
                origin: origin_for(goal),
                slug: slug_of(&slug)?,
                name,
                root: ProjectRoot::External { path: real.clone() },
                vcs: Vcs::None,
                assignees: parse_assignees(&assignees)?,
                publish: parse_publish(publish.as_deref())?,
                tags: tags.parse()?,
            };
            drop(ws);
            // What git the folder already has is read by the engine, never
            // created — and a committer is written into it only when this
            // request names one.
            let source = bisa_engine::ProjectSource::Adopt;
            let made = created(ctx, goal, new, source, committer, git_config).await?;
            out.say(&bisa_core::text!(
                "cli-projects-adopted-as-nothing-was-written-into",
                real = real.to_string(),
                a0 = (made.project.slug).to_string(),
                a1 = (vcs_word(&made.project.vcs)).to_string()
            ));
            out.json_value(json!({"project": made.project, "path": real}));
            Ok(())
        }
        ProjectCmd::Attach { project, goal } => {
            let pid = parse_project(&project)?;
            let goal = crate::parse_goal_id(&goal)?;
            let p = if let Some(client) = ctx.node_client().await {
                let answer = client
                    .post(
                        &format!("/projects/{pid}/attach"),
                        json!({ "goal": goal.to_string() }),
                    )
                    .await?;
                project_in(&answer)?
            } else {
                let (engine, _) = ctx.engine().await?;
                let attached = engine.attach_project(goal, pid);
                let p = engine.workspace().get_project(pid);
                engine.shutdown().await;
                attached?;
                p?
            };
            out.say(&bisa_core::text!(
                "cli-projects-project-attached",
                a0 = (p.slug).to_string(),
                goal = goal.to_string()
            ));
            out.json_value(json!({"project": p, "attached_to": goal.to_string()}));
            Ok(())
        }
        ProjectCmd::Detach { project, goal } => {
            let pid = parse_project(&project)?;
            let goal = crate::parse_goal_id(&goal)?;
            if let Some(client) = ctx.node_client().await {
                client
                    .delete_with(
                        &format!("/projects/{pid}/attach"),
                        json!({ "goal": goal.to_string() }),
                    )
                    .await?;
            } else {
                let (engine, _) = ctx.engine().await?;
                let detached = engine.detach_project(goal, pid);
                engine.shutdown().await;
                detached?;
            }
            out.say(&bisa_core::text!(
                "cli-projects-project-detached-from-nothing-disk-moved",
                pid = pid.to_string(),
                goal = goal.to_string()
            ));
            out.json_value(json!({"project": pid.to_string(), "detached_from": goal.to_string()}));
            Ok(())
        }
        ProjectCmd::List { goal, filter } => {
            let ws = ctx.workspace()?;
            let admitted = filter.admitted(&ws, TagEntity::Project)?;
            let projects: Vec<Project> = match &goal {
                Some(i) => ws.projects_for(crate::parse_goal_id(i)?)?,
                None => ws.list_projects()?,
            }
            .into_iter()
            .filter(|p| keeps(&admitted, &p.id.to_string()))
            .collect();
            if projects.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-projects-no-projects-match-bisa-project-new"
                ));
            }
            for p in &projects {
                render_project(out, &ws, p);
            }
            out.json_value(json!({"projects": projects}));
            Ok(())
        }
        ProjectCmd::Show { project } => {
            let pid = parse_project(&project)?;
            let ws = ctx.workspace()?;
            let p = ws.get_project(pid)?;
            let path = ws.project_root_path(&p);
            let workstreams = ws.list_workstreams(WorkstreamFilter::Project(pid))?;
            let goals: Vec<String> = ws
                .goals_of_project(pid)?
                .iter()
                .map(|g| g.to_string())
                .collect();
            out.say(&bisa_core::text!(
                "cli-projects-id-goals-root-vcs-publish",
                a0 = (p.name).to_string(),
                a1 = (p.id).to_string(),
                a2 = (if goals.is_empty() {
                    bisa_i18n::say(&bisa_core::text!("cli-projects-attached-none"))
                } else {
                    goals.join(", ")
                })
                .to_string(),
                a3 = (path.display()).to_string(),
                a4 = (if path.is_dir() { "present" } else { "MISSING" }).to_string(),
                a5 = (vcs_word(&p.vcs)).to_string(),
                a6 = format!("{:?}", p.publish)
            ));
            out.human(&format!("  tags      {}", crate::tags::label(&p.tags)));
            if !p.assignees.is_empty() {
                let who: Vec<String> = p.assignees.iter().map(|a| a.to_string()).collect();
                out.human(&format!("  assignees {}", who.join(", ")));
            }
            for w in &workstreams {
                out.human(&format!(
                    "  workstream  {} {:<28} {}",
                    w.id,
                    branch_of(w).unwrap_or("(copy)"),
                    state_word(&w.state)
                ));
            }
            out.json_value(json!({"project": p, "path": path, "workstreams": workstreams}));
            Ok(())
        }
        ProjectCmd::Rm { project, tree } => {
            let pid = parse_project(&project)?;
            let answer = if let Some(client) = ctx.node_client().await {
                client
                    .delete(&format!(
                        "/projects/{pid}{}",
                        if tree { "?tree=true" } else { "" }
                    ))
                    .await?
            } else {
                let (engine, _) = ctx.engine().await?;
                let deleted = engine.delete_project(pid, tree).await?;
                engine.shutdown().await;
                serde_json::to_value(deleted)?
            };
            out.say(&bisa_core::text!(
                "cli-projects-project-forgotten",
                pid = pid.to_string(),
                a0 = (if answer["removed_tree"].as_bool().unwrap_or(false) {
                    bisa_i18n::say(&bisa_core::text!(
                        "cli-projects-moved-trash",
                        a0 = (answer["path"].as_str().unwrap_or("")).to_string()
                    ))
                } else if let Some(why) = answer["kept"].as_str() {
                    bisa_i18n::say(&bisa_core::text!(
                        "cli-projects-could-not-be-removed-still-disk",
                        a0 = (answer["path"].as_str().unwrap_or("")).to_string(),
                        why = why.to_string()
                    ))
                } else {
                    bisa_i18n::say(&bisa_core::text!(
                        "cli-projects-left-disk",
                        a0 = (answer["path"].as_str().unwrap_or("")).to_string()
                    ))
                })
                .to_string()
            ));
            out.json_value(json!({"project": pid.to_string(), "removed_tree": answer["removed_tree"],
                                  "path": answer["path"], "workstreams_forgotten": answer["workstreams_forgotten"],
                                  "kept": answer["kept"]}));
            Ok(())
        }
        ProjectCmd::Files { project } => {
            let pid = parse_project(&project)?;
            let ws = ctx.workspace()?;
            let root = ws.project_root_path(&ws.get_project(pid)?);
            let files = blocking_git(move || bisa_vcs::git::status_files(&root)).await?;
            if files.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-projects-nothing-changed-folder-matches-head"
                ));
            }
            for f in &files {
                out.human(&format!(
                    "{}{} {}{}",
                    f.index,
                    f.worktree,
                    f.path.display(),
                    match &f.old_path {
                        Some(old) => bisa_i18n::say(&bisa_core::text!(
                            "cli-projects-was",
                            a0 = (old.display()).to_string()
                        )),
                        None => String::new(),
                    }
                ));
            }
            out.json_value(json!({"project": pid.to_string(), "files": files}));
            Ok(())
        }
        ProjectCmd::Stage { project, paths } => stage_or_not(ctx, out, &project, paths, true).await,
        ProjectCmd::Unstage { project, paths } => {
            stage_or_not(ctx, out, &project, paths, false).await
        }
        ProjectCmd::Diff {
            project,
            path,
            staged,
        } => {
            let pid = parse_project(&project)?;
            let ws = ctx.workspace()?;
            let root = ws.project_root_path(&ws.get_project(pid)?);
            let spec = path.clone();
            let diff = blocking_git(move || bisa_vcs::git::diff_file(&root, &spec, staged)).await?;
            if diff.trim().is_empty() {
                // A file git has never seen has no patch until it is staged,
                // and reading a diff must not stage anything.
                out.say(&bisa_core::text!(
                    "cli-projects-no-patch-brand-new-file-has",
                    a0 = (if staged { "staged" } else { "unstaged" }).to_string(),
                    path = path.to_string()
                ));
            } else {
                out.human(diff.trim_end());
            }
            out.json_value(
                json!({"project": pid.to_string(), "path": path, "staged": staged, "diff": diff}),
            );
            Ok(())
        }
        ProjectCmd::GitInit {
            project,
            git_config,
        } => {
            let pid = parse_project(&project)?;
            // Checked before anything starts: a malformed key is a usage error.
            let git_config = git_config
                .iter()
                .map(|s| parse_config_arg(s))
                .collect::<Result<Vec<_>>>()?;
            let answer = if let Some(client) = ctx.node_client().await {
                client
                    .post(
                        &format!("/projects/{pid}/git/init"),
                        serde_json::json!({ "git_config": git_config }),
                    )
                    .await?
            } else {
                let (engine, _) = ctx.engine().await?;
                let done = engine.init_repository(pid, git_config).await;
                engine.shutdown().await;
                let done = done?;
                serde_json::json!({
                    "project": done.project,
                    "committer": done.committer.sentence(),
                })
            };
            // Through the node the project comes as a row; embedded, as itself.
            let project = answer
                .get("project")
                .and_then(|p| p.get("project").or(Some(p)))
                .cloned()
                .unwrap_or_default();
            let slug = project["slug"].as_str().unwrap_or("?");
            let branch = project["vcs"]["default_branch"].as_str().unwrap_or("?");
            out.say(&bisa_core::text!(
                "cli-projects-project-now-repository",
                slug = slug.to_string(),
                branch = branch.to_string(),
                a0 = (match answer["committer"].as_str() {
                    Some(words) => format!("; {words}"),
                    None => String::new(),
                })
                .to_string()
            ));
            out.json_value(answer);
            Ok(())
        }
        ProjectCmd::Identity {
            project,
            name,
            email,
        } => {
            let pid = parse_project(&project)?;
            let wid = WorkstreamId::primary_of(pid);
            let pair = name.zip(email);
            let identity: bisa_engine::ide::git::GitIdentity =
                if let Some(client) = ctx.node_client().await {
                    // Who commits is two keys of the repository's own config:
                    // the one door the node has for them.
                    if let Some((name, email)) = &pair {
                        client
                            .put(
                                &format!("/workstreams/{wid}/git/config"),
                                json!({ "set": {
                                    "user.name": name.trim(),
                                    "user.email": email.trim(),
                                } }),
                            )
                            .await?;
                    }
                    let answer = client
                        .get(&format!("/workstreams/{wid}/git/identity"))
                        .await?;
                    serde_json::from_value(json!({
                        "name": answer["name"],
                        "email": answer["email"],
                        "source": answer["source"],
                        "global": answer["global"],
                    }))
                    .context(bisa_core::text!(
                        "cli-projects-unexpected-identity-payload-from-node"
                    ))?
                } else {
                    let (engine, _) = ctx.engine().await?;
                    let done = match &pair {
                        Some((name, email)) => {
                            bisa_engine::ide::git::set_identity(
                                engine.inner(),
                                wid,
                                name.trim(),
                                email.trim(),
                            )
                            .await
                        }
                        None => bisa_engine::ide::git::identity(engine.inner(), wid).await,
                    };
                    engine.shutdown().await;
                    done?
                };
            out.human(&identity_line(&identity));
            out.json_value(serde_json::to_value(&identity)?);
            Ok(())
        }
        ProjectCmd::GitConfig {
            project,
            global,
            set,
            unset,
        } => {
            // Checked before the engine starts: a malformed key is a usage error.
            let set = set
                .iter()
                .map(|s| parse_config_arg(s))
                .collect::<Result<Vec<_>>>()?;
            for key in &unset {
                bisa_vcs::key_def(key).ok_or_else(|| {
                    anyhow!(bisa_core::text!(
                        "cli-projects-not-git-config-key-platform-writes",
                        key = format!("{key:?}")
                    ))
                })?;
            }
            let writing = !set.is_empty() || !unset.is_empty();
            if let Some(client) = ctx.node_client().await {
                let route = match (&project, global) {
                    (_, true) => "/git/config".to_string(),
                    (Some(project), false) => format!(
                        "/workstreams/{}/git/config",
                        WorkstreamId::primary_of(parse_project(project)?)
                    ),
                    (None, false) => {
                        return Err(anyhow!(bisa_core::text!(
                            "cli-projects-name-project-pass-global-your-global"
                        )))
                    }
                };
                let answer = if writing {
                    let set: std::collections::BTreeMap<String, String> = set.into_iter().collect();
                    client
                        .put(&route, json!({ "set": set, "unset": unset }))
                        .await?
                } else {
                    client.get(&route).await?
                };
                for line in config_lines_of(&answer) {
                    out.human(&line);
                }
                out.json_value(answer);
                return Ok(());
            }
            let (engine, _) = ctx.engine().await?;
            let done = if global {
                if writing {
                    bisa_engine::identity::set_global_config(engine.inner(), set, unset).await
                } else {
                    bisa_engine::identity::global_config(engine.inner()).await
                }
            } else {
                match project {
                    None => {
                        engine.shutdown().await;
                        return Err(anyhow!(bisa_core::text!(
                            "cli-projects-name-project-pass-global-your-global"
                        )));
                    }
                    Some(project) => {
                        let wid = WorkstreamId::primary_of(parse_project(&project)?);
                        if writing {
                            bisa_engine::ide::git::set_local_config(engine.inner(), wid, set, unset)
                                .await
                        } else {
                            bisa_engine::ide::git::local_config(engine.inner(), wid).await
                        }
                    }
                }
            };
            engine.shutdown().await;
            let view = done?;
            for line in config_lines(&view) {
                out.human(&line);
            }
            out.json_value(serde_json::to_value(&view)?);
            Ok(())
        }
        ProjectCmd::Commit {
            project,
            message,
            paths,
        } => {
            let pid = parse_project(&project)?;
            if message.trim().is_empty() {
                bail!(bisa_core::text!("cli-projects-commit-needs-message"));
            }
            let primary = WorkstreamId::primary_of(pid);
            let (commit, short) = if let Some(client) = ctx.node_client().await {
                let answer = client
                    .post(
                        &format!("/workstreams/{primary}/git/commit"),
                        json!({ "message": message.trim(), "paths": paths }),
                    )
                    .await?;
                commit_in(&answer)
            } else {
                let (engine, _) = ctx.engine().await?;
                let done = eng::commit_in(engine.inner(), primary, message.trim(), paths).await;
                engine.shutdown().await;
                let commit = done?;
                (commit.as_str().to_string(), commit.short().to_string())
            };
            out.say(&bisa_core::text!(
                "cli-projects-committed-project",
                a0 = short.clone(),
                pid = pid.to_string()
            ));
            out.json_value(json!({"project": pid.to_string(), "commit": commit, "short": short}));
            Ok(())
        }
        ProjectCmd::Message { project } => {
            let pid = parse_project(&project)?;
            let primary = WorkstreamId::primary_of(pid);
            let asked: std::result::Result<String, String> =
                if let Some(client) = ctx.node_client().await {
                    let answer = client
                        .post(&format!("/workstreams/{primary}/git/message"), json!({}))
                        .await?;
                    match answer["suggested"].as_bool() {
                        Some(true) => Ok(answer["message"].as_str().unwrap_or_default().into()),
                        _ => Err(answer["error"].as_str().unwrap_or_default().into()),
                    }
                } else {
                    let (engine, _) = ctx.engine().await?;
                    let asked = eng::suggest_commit_message(engine.inner(), primary).await;
                    engine.shutdown().await;
                    asked.map_err(|e| e.to_string())
                };
            // A failed suggestion is an empty draft and a sentence saying
            // why, never a message the agent did not write.
            match asked {
                Ok(message) => {
                    out.human(&message);
                    out.json_value(json!({"project": pid.to_string(), "suggested": true,
                               "message": message}));
                }
                Err(e) => {
                    out.error(&bisa_i18n::say(&bisa_core::text!(
                        "cli-projects-no-suggestion",
                        e = e.clone()
                    )));
                    out.json_value(json!({"project": pid.to_string(), "suggested": false,
                                          "message": "", "error": e}));
                }
            }
            Ok(())
        }
        ProjectCmd::Assign {
            project,
            assignees,
            replace,
        } => {
            let pid = parse_project(&project)?;
            let mut p = ctx.workspace()?.get_project(pid)?;
            let wanted = parse_assignees(&assignees)?;
            p.assignees = merge_assignees(p.assignees, wanted, replace);
            let p = if let Some(client) = ctx.node_client().await {
                let carried: Vec<String> = p.assignees.iter().map(|a| a.to_string()).collect();
                let answer = client
                    .put(
                        &format!("/projects/{pid}/assignees"),
                        json!({ "assignees": carried }),
                    )
                    .await?;
                project_in(&answer)?
            } else {
                let (engine, _) = ctx.engine().await?;
                let carried = eng::update(engine.inner(), p);
                engine.shutdown().await;
                carried?
            };
            let who: Vec<String> = p.assignees.iter().map(|a| a.to_string()).collect();
            out.say(&bisa_core::text!(
                "cli-projects-project-carried",
                a0 = (p.slug).to_string(),
                a1 = (if who.is_empty() {
                    "nobody".to_string()
                } else {
                    who.join(", ")
                })
                .to_string()
            ));
            out.json_value(json!({"project": p, "assignees": who}));
            Ok(())
        }
    }
}

/// `bisa-vcs` is synchronous by design; the CLI drives it the same way
/// the engine does.
async fn blocking_git<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> std::result::Result<T, bisa_vcs::VcsError> + Send + 'static,
    T: Send + 'static,
{
    Ok(tokio::task::spawn_blocking(f).await.map_err(|e| {
        anyhow!(bisa_core::text!(
            "cli-projects-git-task-did-not-finish",
            e = e.to_string()
        ))
    })??)
}

/// Staging and unstaging differ by one word to the user and by one flag to
/// git, so they share a body — and both report the resulting rows, because
/// the only reason to stage is to see what happened.
async fn stage_or_not(
    ctx: &Ctx,
    out: &Out,
    project: &str,
    paths: Vec<String>,
    stage: bool,
) -> Result<()> {
    let pid = parse_project(project)?;
    if paths.is_empty() {
        bail!(bisa_core::text!(
            "cli-projects-name-paths-bisa-project-files-lists",
            project = project.to_string()
        ));
    }
    // The project's own tree is its primary workstream.
    let primary = WorkstreamId::primary_of(pid);
    let files: serde_json::Value = if let Some(client) = ctx.node_client().await {
        let verb = if stage { "stage" } else { "unstage" };
        client
            .post(
                &format!("/workstreams/{primary}/git/{verb}"),
                json!({ "paths": paths }),
            )
            .await?["files"]
            .clone()
    } else {
        let (engine, _) = ctx.engine().await?;
        let done = if stage {
            eng::stage_in(engine.inner(), primary, paths.clone()).await
        } else {
            eng::unstage_in(engine.inner(), primary, paths.clone()).await
        };
        engine.shutdown().await;
        serde_json::to_value(bisa_node::projects::file_rows(done?))?
    };
    let staged = files
        .as_array()
        .into_iter()
        .flatten()
        .filter(|file| file["staged"] == true)
        .count();
    out.say(&bisa_core::text!(
        "cli-projects-path-s-staged-total",
        a0 = (if stage { "staged" } else { "unstaged" }).to_string(),
        a1 = (paths.len()).to_string(),
        staged = staged.to_string(),
        a2 = (if stage {
            String::new()
        } else {
            bisa_i18n::say(&bisa_core::text!(
                "cli-projects-files-themselves-were-not-touched"
            ))
        })
        .to_string()
    ));
    out.json_value(json!({"project": pid.to_string(), "files": files}));
    Ok(())
}

/// Adding is the default because a project usually gains a second owner
/// rather than swapping its first; `--replace` is the explicit "exactly
/// these".
fn merge_assignees(current: Vec<Assignee>, wanted: Vec<Assignee>, replace: bool) -> Vec<Assignee> {
    if replace {
        return dedup(wanted);
    }
    let mut out = current;
    for a in wanted {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

fn dedup(v: Vec<Assignee>) -> Vec<Assignee> {
    let mut out: Vec<Assignee> = Vec::with_capacity(v.len());
    for a in v {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// A managed root is a folder we just made, so it is initialised as a
/// repository rather than left plain and branchless.
/// Attach a freshly made project to the goal that asked for it, when one did.
fn attach_if_asked(engine: &Engine, goal: Option<GoalId>, project: &Project) -> Result<()> {
    if let Some(goal) = goal {
        engine.attach_project(goal, project.id)?;
    }
    Ok(())
}

/// Every creation is one engine operation: record, primary
/// workstream and tree together, or nothing. The CLI only says which.
async fn create_via_engine(
    engine: &Engine,
    goal: Option<GoalId>,
    new: NewProject,
    source: bisa_engine::ProjectSource,
    git_config: Vec<(String, String)>,
) -> Result<bisa_engine::Created> {
    let created = bisa_engine::projects::create(
        engine.inner(),
        bisa_engine::NewProjectRequest {
            new,
            source,
            git_config,
        },
    )
    .await?;
    attach_if_asked(engine, goal, &created.project)?;
    Ok(created)
}

/// `--committer "Name <email>"` and `--git-config key=value`, read before
/// anything is created so a malformed one is a usage error and not a
/// rolled-back project. The committer is two config keys; every key is checked
/// against the vcs crate's schema, whose rule the engine applies again.
fn local_config_args(
    committer: Option<String>,
    git_config: &[String],
) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Some(text) = committer {
        let ident = bisa_vcs::git::parse_ident(&text).map_err(|_| {
            anyhow!(bisa_core::text!(
                "cli-projects-committer-wants-name-email-name-then",
                text = format!("{text:?}")
            ))
        })?;
        out.push(("user.name".to_string(), ident.name));
        out.push(("user.email".to_string(), ident.email));
    }
    for raw in git_config {
        out.push(parse_config_arg(raw)?);
    }
    Ok(out)
}

/// One `key=value`, the key one the platform knows.
fn parse_config_arg(raw: &str) -> Result<(String, String)> {
    let (key, value) = raw.split_once('=').ok_or_else(|| {
        anyhow!(bisa_core::text!(
            "cli-projects-git-config-wants-key-value-got",
            raw = format!("{raw:?}")
        ))
    })?;
    let key = key.trim();
    bisa_vcs::key_def(key).ok_or_else(|| {
        anyhow!(bisa_core::text!(
            "cli-projects-not-git-config-key-platform-writes-2",
            key = format!("{key:?}"),
            a0 = (bisa_vcs::GIT_CONFIG_KEYS
                .iter()
                .map(|d| d.key)
                .collect::<Vec<_>>()
                .join(", "))
            .to_string()
        ))
    })?;
    Ok((key.to_string(), value.trim().to_string()))
}

/// The commit in an answer of the node's: its id whole, and short.
fn commit_in(answer: &serde_json::Value) -> (String, String) {
    (
        answer["commit"].as_str().unwrap_or_default().to_string(),
        answer["short"].as_str().unwrap_or_default().to_string(),
    )
}

/// [`config_lines`], from the view as the node answers it.
fn config_lines_of(view: &serde_json::Value) -> Vec<String> {
    view["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            let key = entry["key"].as_str().unwrap_or_default();
            match (entry["local"].as_str(), entry["global"].as_str()) {
                (Some(v), _) => format!("{key} = {v} (local)"),
                (None, Some(v)) => format!("{key} = {v} (global)"),
                (None, None) => format!("{key} unset"),
            }
        })
        .collect()
}

/// One line per schema key: `key = value (local)` / `(global)` / `unset`.
fn config_lines(view: &bisa_vcs::GitConfigView) -> Vec<String> {
    view.entries
        .iter()
        .map(|e| match (&e.local, &e.global) {
            (Some(v), _) => format!("{} = {v} (local)", e.key),
            (None, Some(v)) => format!("{} = {v} (global)", e.key),
            (None, None) => format!("{} unset", e.key),
        })
        .collect()
}

/// A project as a creation answers it, whoever made it.
struct Made {
    project: Project,
    /// Where its files are.
    path: String,
    /// What an import copied and what it left behind; null for the others.
    imported: serde_json::Value,
}

/// The project in an answer of the node's, which carries it under `project`.
fn project_in(answer: &serde_json::Value) -> Result<Project> {
    serde_json::from_value(answer["project"].clone()).context(bisa_core::text!(
        "cli-projects-unexpected-project-payload-from-node"
    ))
}

/// What the node's creation route reads, from what the verb was told.
fn creation_body(
    new: &NewProject,
    source: &bisa_engine::ProjectSource,
    git_config: &[(String, String)],
) -> Result<serde_json::Value> {
    use bisa_engine::ProjectSource as Source;
    let mut body = json!({
        "slug": new.slug,
        "name": new.name,
        "assignees": new.assignees.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        "publish": new.publish,
        "tags": new.tags,
        "git_config": git_config
            .iter()
            .cloned()
            .collect::<std::collections::BTreeMap<_, _>>(),
    });
    match (source, &new.root) {
        (Source::New, _) => body["kind"] = json!("new"),
        (Source::Clone { url, depth }, _) => {
            body["kind"] = json!("clone");
            body["url"] = json!(url);
            body["depth"] = json!(depth);
        }
        (Source::Import { source }, _) => {
            body["kind"] = json!("import");
            body["path"] = json!(source);
        }
        (Source::Adopt, ProjectRoot::External { path }) => {
            body["kind"] = json!("adopt");
            body["path"] = json!(path);
        }
        (Source::Adopt, ProjectRoot::Managed) => {
            bail!(bisa_core::text!("cli-projects-adopt-needs-folder"))
        }
    }
    Ok(body)
}

/// Make a project — through the node when one runs, by an engine of this
/// process's own when none does — attached to `goal` when one asked.
async fn created(
    ctx: &Ctx,
    goal: Option<GoalId>,
    new: NewProject,
    source: bisa_engine::ProjectSource,
    committer: Option<String>,
    git_config: Vec<String>,
) -> Result<Made> {
    // Read before anything is asked: a malformed key is a usage error.
    let git_config = local_config_args(committer, &git_config)?;
    if let Some(client) = ctx.node_client().await {
        let route = match goal {
            Some(goal) => format!("/goals/{goal}/projects"),
            None => "/projects".to_string(),
        };
        let answer = client
            .post(&route, creation_body(&new, &source, &git_config)?)
            .await?;
        return Ok(Made {
            project: project_in(&answer)?,
            path: answer["path"].as_str().unwrap_or_default().to_string(),
            imported: answer["imported"].clone(),
        });
    }
    let (engine, _) = ctx.engine().await?;
    let made = create_via_engine(&engine, goal, new, source, git_config).await;
    let made = made.and_then(|created| {
        Ok(Made {
            path: engine
                .workspace()
                .project_root_path(&created.project)
                .display()
                .to_string(),
            imported: serde_json::to_value(created.imported)?,
            project: created.project,
        })
    });
    engine.shutdown().await;
    made
}

// ---------------------------------------------------------------------------
// Workstreams
// ---------------------------------------------------------------------------

pub async fn workstream(ctx: &Ctx, out: &Out, cmd: WorkstreamCmd) -> Result<()> {
    match cmd {
        WorkstreamCmd::List { project, goal } => {
            let ws = ctx.workspace()?;
            let filter = match (&project, &goal) {
                (Some(p), _) => WorkstreamFilter::Project(parse_project(p)?),
                (None, Some(i)) => WorkstreamFilter::Goal(crate::parse_goal_id(i)?),
                (None, None) => WorkstreamFilter::All,
            };
            let workstreams = ws.list_workstreams(filter)?;
            if workstreams.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-projects-no-workstreams-bisa-workstream-open-project"
                ));
            }
            for w in &workstreams {
                out.human(&format!(
                    "{:<28} {:<28} {:<10} {}",
                    w.id,
                    kind_word(w),
                    state_word(&w.state),
                    ws.workstream_checkout(w)
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                ));
            }
            out.json_value(json!({"workstreams": workstreams}));
            Ok(())
        }
        WorkstreamCmd::Open {
            project,
            goal,
            label,
            agent,
            from,
            base,
        } => {
            let pid = parse_project(&project)?;
            let ws = ctx.workspace()?;
            let p = ws.get_project(pid)?;
            // The goal the workstream is *for*, when there is one; a workstream
            // opened by hand may be for none.
            let host = match &goal {
                Some(i) => {
                    let id = crate::parse_goal_id(i)?;
                    if !ws.is_attached(id, pid)? {
                        bail!(bisa_core::text!(
                            "cli-projects-goal-not-attached-project-attach-first",
                            id = id.to_string(),
                            a0 = (p.slug).to_string()
                        ));
                    }
                    Some(id)
                }
                None => None,
            };
            let source = match from.as_deref() {
                Some(spec) => spec
                    .parse::<bisa_core::WorkstreamSource>()
                    .map_err(|e| anyhow::anyhow!("--from {e}"))?,
                None => bisa_core::WorkstreamSource::default(),
            };
            let (w, checkout) = if let Some(client) = ctx.node_client().await {
                let answer = client
                    .post(
                        &format!("/projects/{pid}/workstreams"),
                        json!({
                            "goal": host.map(|goal| goal.to_string()),
                            "label": label,
                            "agent": agent,
                            "source": source,
                            "base": base,
                        }),
                    )
                    .await?;
                let w: Workstream = serde_json::from_value(answer["workstream"].clone()).context(
                    bisa_core::text!("cli-projects-unexpected-workstream-payload-from-node"),
                )?;
                (
                    w,
                    PathBuf::from(answer["path"].as_str().unwrap_or_default()),
                )
            } else {
                let (engine, _) = ctx.engine().await?;
                let opened =
                    open_one(&engine, host, &p, label.as_deref(), agent, source, base).await;
                engine.shutdown().await;
                let w = opened?;
                let checkout = ws.workstream_checkout(&w)?;
                (w, checkout)
            };
            out.say(&bisa_core::text!(
                "cli-projects-workstream",
                a0 = (w.id).to_string(),
                a1 = (kind_word(&w)).to_string(),
                a2 = (checkout.display()).to_string()
            ));
            out.json_value(json!({"workstream": w, "path": checkout}));
            Ok(())
        }
        WorkstreamCmd::Show { workstream } => {
            let id = parse_workstream(&workstream)?;
            let ws = ctx.workspace()?;
            let w = ws.get_workstream(id)?;
            let checkout = ws.workstream_checkout(&w)?;
            let status = git_view(&w, &checkout).await;
            out.say(&bisa_core::text!(
                "cli-projects-workstream-project-kind-goal-branch-base",
                a0 = (w.id).to_string(),
                a1 = (w.project).to_string(),
                a2 = (w.kind.as_str()).to_string(),
                a3 = (w
                    .goal
                    .map(|g| g.to_string())
                    .unwrap_or_else(|| "-".to_string()))
                .to_string(),
                a4 = (kind_word(&w)).to_string(),
                a5 = (base_of(&w).unwrap_or("-")).to_string(),
                a6 = (state_word(&w.state)).to_string(),
                a7 = (checkout.display()).to_string()
            ));
            if let Some(s) = &status {
                out.say(&bisa_core::text!(
                    "cli-projects-changes-staged-unstaged-untracked-ahead-base",
                    a0 = (s.staged).to_string(),
                    a1 = (s.unstaged).to_string(),
                    a2 = (s.untracked).to_string(),
                    a3 = (s.ahead_of_base).to_string()
                ));
            }
            out.json_value(json!({"workstream": w, "status": status}));
            Ok(())
        }
        WorkstreamCmd::Diff { workstream } => {
            let id = parse_workstream(&workstream)?;
            let ws = ctx.workspace()?;
            let w = ws.get_workstream(id)?;
            require_git(&w)?;
            let path = ws.workstream_checkout(&w)?;
            // `git diff` cannot show a brand-new file without staging it, and
            // showing a diff must not stage anything — so new files are listed
            // separately rather than quietly missing from the answer.
            let (diff, untracked) = tokio::task::spawn_blocking(move || {
                use bisa_vcs::git;
                let diff = git::diff_head(&path)?;
                let untracked: Vec<String> = git::list_untracked(&path)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.display().to_string())
                    .collect();
                Ok::<_, bisa_vcs::VcsError>((diff, untracked))
            })
            .await
            .map_err(|e| {
                anyhow!(bisa_core::text!(
                    "cli-projects-git-task-did-not-finish",
                    e = e.to_string()
                ))
            })??;
            let clean = diff.trim().is_empty() && untracked.is_empty();
            if clean {
                out.say(&bisa_core::text!(
                    "cli-projects-workstream-clean-nothing-commit"
                ));
            } else {
                if !diff.trim().is_empty() {
                    out.human(diff.trim_end());
                }
                for path in &untracked {
                    out.human(&format!("untracked: {path}"));
                }
            }
            out.json_value(json!({"workstream": id.to_string(), "clean": clean,
                                  "diff": diff, "untracked": untracked}));
            Ok(())
        }
        WorkstreamCmd::Commit {
            workstream,
            message,
        } => {
            let id = parse_workstream(&workstream)?;
            if message.trim().is_empty() {
                bail!(bisa_core::text!("cli-projects-commit-needs-message"));
            }
            let (commit, short) = if let Some(client) = ctx.node_client().await {
                let answer = client
                    .post(
                        &format!("/workstreams/{id}/commit"),
                        json!({ "message": message.trim() }),
                    )
                    .await?;
                commit_in(&answer)
            } else {
                let (engine, _) = ctx.engine().await?;
                let done = eng::commit_workstream(engine.inner(), id, message.trim()).await;
                engine.shutdown().await;
                let commit = done?;
                (commit.as_str().to_string(), commit.short().to_string())
            };
            out.say(&bisa_core::text!(
                "cli-projects-committed-workstream",
                a0 = short.clone(),
                id = id.to_string()
            ));
            out.json_value(json!({"workstream": id.to_string(), "commit": commit, "short": short}));
            Ok(())
        }
        WorkstreamCmd::Push { workstream, yes } => {
            let id = parse_workstream(&workstream)?;
            let outcome = if let Some(client) = ctx.node_client().await {
                let act = Act {
                    route: format!("/workstreams/{id}/push"),
                    body: json!({}),
                    lands_as: &["pushed"],
                };
                published_by_node(ctx, &client, out, id, act, yes)
                    .await?
                    .map(|_| ())
            } else {
                let (engine, _) = ctx.engine().await?;
                let engine = Arc::new(engine);
                let inner = Arc::clone(engine.inner());
                let outcome = publish(&engine, out, id, yes, async move {
                    eng::push_workstream(&inner, id).await
                })
                .await;
                shutdown(engine).await;
                outcome?
            };
            match outcome {
                Some(()) => {
                    let ws = ctx.workspace()?;
                    let w = ws.get_workstream(id)?;
                    out.say(&bisa_core::text!(
                        "cli-projects-pushed-origin",
                        a0 = branch_of(&w)
                            .map(str::to_string)
                            .unwrap_or_else(|| bisa_i18n::say(&bisa_core::text!(
                                "cli-projects-the-branch"
                            )))
                    ));
                    out.json_value(json!({"workstream": w, "pushed": true}));
                }
                None => out.json_value(json!({"workstream": id.to_string(), "pushed": false})),
            }
            Ok(())
        }
        WorkstreamCmd::PrView { workstream } => {
            let id = parse_workstream(&workstream)?;
            let pr: Option<bisa_engine::codehost::PullRequest> =
                if let Some(client) = ctx.node_client().await {
                    let answer = client.get(&format!("/workstreams/{id}/pr")).await?;
                    serde_json::from_value(answer["pr"].clone()).context(bisa_core::text!(
                        "cli-projects-unexpected-pull-request-payload-from-node"
                    ))?
                } else {
                    let (engine, _) = ctx.engine().await?;
                    let pr = bisa_engine::codehost::linked_pr(engine.inner(), id).await;
                    engine.shutdown().await;
                    pr?
                };
            match pr {
                Some(pr) => {
                    out.human(&format!(
                        "#{} {} — {:?}{} — {}",
                        pr.number,
                        pr.title,
                        pr.state,
                        if pr.is_draft { " (draft)" } else { "" },
                        pr.url
                    ));
                    out.json_value(json!({"workstream": id.to_string(), "pr": pr}));
                }
                None => {
                    out.say(&bisa_core::text!(
                        "cli-projects-no-pull-request-open-workstream"
                    ));
                    out.json_value(json!({"workstream": id.to_string(), "pr": null}));
                }
            }
            Ok(())
        }
        WorkstreamCmd::PrChecks { workstream } => {
            let id = parse_workstream(&workstream)?;
            let checks: Vec<bisa_engine::codehost::CheckRun> =
                if let Some(client) = ctx.node_client().await {
                    let answer = client.get(&format!("/workstreams/{id}/pr/checks")).await?;
                    serde_json::from_value(answer["checks"].clone()).context(bisa_core::text!(
                        "cli-projects-unexpected-pull-request-payload-from-node"
                    ))?
                } else {
                    let (engine, _) = ctx.engine().await?;
                    let checks = bisa_engine::codehost::checks(engine.inner(), id).await;
                    engine.shutdown().await;
                    checks?
                };
            for c in &checks {
                out.human(&format!(
                    "{:<12} {:<10} {}",
                    c.conclusion.as_deref().unwrap_or(&c.status),
                    c.status,
                    c.name
                ));
            }
            if checks.is_empty() {
                out.say(&bisa_core::text!("cli-projects-no-check-runs"));
            }
            out.json_value(json!({"workstream": id.to_string(), "checks": checks}));
            Ok(())
        }
        WorkstreamCmd::PrReview {
            workstream,
            event,
            body,
        } => {
            let id = parse_workstream(&workstream)?;
            let event = match event.as_str() {
                "approve" => bisa_engine::codehost::ReviewEvent::Approve,
                "request-changes" | "request_changes" => {
                    bisa_engine::codehost::ReviewEvent::RequestChanges
                }
                "comment" => bisa_engine::codehost::ReviewEvent::Comment,
                other => {
                    bail!(bisa_core::text!(
                        "cli-projects-unknown-review-event-approve-request-changes",
                        other = format!("{other:?}")
                    ))
                }
            };
            let words = Some(body.clone()).filter(|b| !b.trim().is_empty());
            if let Some(client) = ctx.node_client().await {
                client
                    .post(
                        &format!("/workstreams/{id}/pr/review"),
                        json!({"event": event, "body": words}),
                    )
                    .await?;
            } else {
                let (engine, _) = ctx.engine().await?;
                let done = bisa_engine::codehost::review(
                    engine.inner(),
                    id,
                    bisa_engine::codehost::Review {
                        event,
                        body: words,
                        comments: vec![],
                    },
                    // The CLI is the person's hand: an agent reviews through the MCP tool.
                    bisa_engine::codehost::Reviewer::Person,
                )
                .await;
                engine.shutdown().await;
                done?;
            }
            out.say(&bisa_core::text!("cli-projects-review-submitted"));
            out.json_value(json!({"workstream": id.to_string(), "reviewed": true}));
            Ok(())
        }
        WorkstreamCmd::PrMerge {
            workstream,
            strategy,
            keep_remote_branch,
            yes,
        } => {
            let id = parse_workstream(&workstream)?;
            let strategy = match strategy.as_str() {
                "merge" => bisa_engine::codehost::MergeStrategy::Merge,
                "squash" => bisa_engine::codehost::MergeStrategy::Squash,
                "rebase" => bisa_engine::codehost::MergeStrategy::Rebase,
                other => bail!(bisa_core::text!(
                    "cli-projects-unknown-merge-strategy-merge-squash-rebase",
                    other = format!("{other:?}")
                )),
            };
            let delete_branch = !keep_remote_branch;
            let outcome = if let Some(client) = ctx.node_client().await {
                let act = Act {
                    route: format!("/workstreams/{id}/pr/merge"),
                    body: json!({"strategy": strategy, "delete_branch": delete_branch}),
                    lands_as: &["merged"],
                };
                published_by_node(ctx, &client, out, id, act, yes)
                    .await?
                    .map(|answer| merged_as_the_node_said(&answer))
            } else {
                let (engine, _) = ctx.engine().await?;
                let engine = Arc::new(engine);
                let inner = Arc::clone(engine.inner());
                let outcome = publish(&engine, out, id, yes, async move {
                    bisa_engine::codehost::merge(&inner, id, strategy, delete_branch).await
                })
                .await;
                shutdown(engine).await;
                outcome?
            };
            match outcome {
                Some(m) => {
                    out.human(&format!(
                        "{} — {}",
                        if m.merged {
                            bisa_i18n::say(&bisa_core::text!("cli-projects-merged"))
                        } else {
                            bisa_i18n::say(&bisa_core::text!("cli-projects-not-merged"))
                        },
                        m.message
                    ));
                    out.json_value(
                        json!({"workstream": id.to_string(), "merged": m.merged, "sha": m.sha}),
                    );
                }
                None => out.json_value(json!({"workstream": id.to_string(), "merged": false})),
            }
            Ok(())
        }
        WorkstreamCmd::Pr {
            workstream,
            title,
            body,
            draft,
            yes,
        } => {
            let id = parse_workstream(&workstream)?;
            if title.trim().is_empty() {
                bail!(bisa_core::text!("cli-projects-pull-request-needs-title"));
            }
            // What was opened: its number, and where it is.
            let outcome: Option<(u64, String)> = if let Some(client) = ctx.node_client().await {
                let act = Act {
                    route: format!("/workstreams/{id}/pr"),
                    body: json!({"title": title.trim(), "body": body, "draft": draft}),
                    lands_as: &["pr_open"],
                };
                published_by_node(ctx, &client, out, id, act, yes)
                    .await?
                    .map(|answer| opened_as_the_node_said(&answer))
                    .transpose()?
            } else {
                let (engine, _) = ctx.engine().await?;
                let engine = Arc::new(engine);
                let inner = Arc::clone(engine.inner());
                let req = bisa_engine::codehost::PrRequest {
                    title: title.trim().to_string(),
                    body: body.clone(),
                    draft,
                    ..Default::default()
                };
                let outcome = publish(&engine, out, id, yes, async move {
                    bisa_engine::codehost::open_pr(&inner, id, req).await
                })
                .await;
                shutdown(engine).await;
                outcome?.map(|pr| (pr.number, pr.url))
            };
            match outcome {
                Some((number, url)) => {
                    out.say(&bisa_core::text!(
                        "cli-projects-opened-pull-request",
                        a0 = number.to_string(),
                        a1 = url.to_string()
                    ));
                    out.json_value(json!({"workstream": id.to_string(), "opened": true,
                                          "pr": {"number": number, "url": url}}));
                }
                None => out.json_value(json!({"workstream": id.to_string(), "opened": false})),
            }
            Ok(())
        }
        WorkstreamCmd::Close { workstream, tree } => {
            let id = parse_workstream(&workstream)?;
            let answer = if let Some(client) = ctx.node_client().await {
                client
                    .delete(&format!(
                        "/workstreams/{id}{}",
                        if tree { "?tree=true" } else { "" }
                    ))
                    .await?
            } else {
                let (engine, _) = ctx.engine().await?;
                let done = eng::close_workstream(engine.inner(), id, tree).await;
                engine.shutdown().await;
                let closed = done?;
                json!({"workstream": closed.workstream, "stopped_sessions": closed.stopped_sessions})
            };
            let stopped = answer["stopped_sessions"].as_u64().unwrap_or(0);
            out.say(&bisa_core::text!(
                "cli-projects-workstream-closed-stopped-checkout",
                id = id.to_string(),
                a0 = (match stopped {
                    0 => bisa_i18n::say(&bisa_core::text!("cli-projects-no-session")),
                    1 => bisa_i18n::say(&bisa_core::text!("cli-projects-1-session")),
                    n => format!("{n} sessions"),
                })
                .to_string(),
                a1 = (if tree {
                    bisa_i18n::say(&bisa_core::text!("cli-projects-removed"))
                } else {
                    bisa_i18n::say(&bisa_core::text!("cli-projects-left-disk-2"))
                })
                .to_string()
            ));
            out.json_value(
                json!({"workstream": answer["workstream"], "removed_tree": tree,
                                  "stopped_sessions": stopped}),
            );
            Ok(())
        }
    }
}

/// A copy has no repository; the primary and a worktree both do.
fn require_git(w: &Workstream) -> Result<()> {
    Ok(w.require_repository()?)
}

async fn open_one(
    engine: &Engine,
    goal: Option<GoalId>,
    project: &Project,
    label: Option<&str>,
    agent: Option<String>,
    source: bisa_core::WorkstreamSource,
    base: Option<String>,
) -> Result<Workstream> {
    let label = label.unwrap_or(project.slug.as_str()).trim().to_string();
    let place = eng::open_workstream_for(
        engine.inner(),
        project,
        bisa_engine::WorkstreamRequest {
            goal,
            work_item: None,
            label,
            agent,
            source,
            base: base.filter(|b| !b.trim().is_empty()),
        },
    )
    .await?;
    if place.workstream.is_primary() {
        bail!(bisa_core::text!(
            "cli-projects-project-has-no-commits-yet-make",
            a0 = (project.slug).to_string(),
            a1 = (place.cwd.display()).to_string()
        ));
    }
    let w = place.workstream;
    Ok(w)
}

#[derive(serde::Serialize)]
struct WorkstreamView {
    branch: Option<String>,
    base: Option<String>,
    staged: u32,
    unstaged: u32,
    untracked: u32,
    ahead_of_base: u32,
    clean: bool,
}

async fn git_view(w: &Workstream, checkout: &std::path::Path) -> Option<WorkstreamView> {
    if matches!(w.kind, WorkstreamKind::Copy) {
        return None;
    }
    let base = base_of(w).map(str::to_string);
    let path = checkout.to_path_buf();
    if !path.is_dir() {
        return None;
    }
    let base_for_count = base.clone();
    tokio::task::spawn_blocking(move || {
        use bisa_vcs::git;
        let status = git::status(&path).ok()?;
        let ahead = base_for_count
            .and_then(|b| git::ahead_behind(&path, &b).ok())
            .map(|(a, _)| a)
            .unwrap_or(0);
        Some(WorkstreamView {
            branch: status.branch,
            base,
            staged: status.staged,
            unstaged: status.unstaged,
            untracked: status.untracked,
            ahead_of_base: ahead,
            clean: status.is_clean,
        })
    })
    .await
    .ok()
    .flatten()
}

async fn shutdown(engine: Arc<Engine>) {
    if let Ok(engine) = Arc::try_unwrap(engine) {
        engine.shutdown().await;
    }
}

/// Run a publishing call and deal with the `Publish` gate it opens.
///
/// `Ok(Some(value))` — it went out. `Ok(None)` — a gate is open and nothing
/// left the machine, which is a normal outcome here rather than an error: the
/// gate id is printed so a person can decide it. Errors are the engine's own
/// (a manual-publishing project, a declined gate, git's refusal).
async fn publish<T, F>(
    engine: &Arc<Engine>,
    out: &Out,
    workstream: WorkstreamId,
    yes: bool,
    call: F,
) -> Result<Option<T>>
where
    F: std::future::Future<Output = Result<T, bisa_engine::EngineError>> + Send + 'static,
    T: Send + 'static,
{
    let subject = format!("workstream:{workstream}");
    let mut events = engine.events();
    let mut task = tokio::spawn(call);
    let interactive = std::io::stdin().is_terminal();
    loop {
        tokio::select! {
            biased;
            joined = &mut task => {
                return match joined.map_err(|e| anyhow!(bisa_core::text!("cli-projects-publish-task-did-not-finish", e = e.to_string())))? {
                    Ok(v) => Ok(Some(v)),
                    Err(e) => Err(e.into()),
                };
            }
            event = events.recv() => {
                let Ok(event) = event else { continue };
                let EnginePayload::GateOpened { gate_id, gate, question } = &event.payload else {
                    continue;
                };
                if *gate != bisa_core::Gate::Publish {
                    continue;
                }
                if engine.inbox().into_iter().all(|g| g.id != *gate_id || g.subject != subject) {
                    continue;
                }
                if yes {
                    let note = bisa_i18n::say(&bisa_core::text!("cli-projects-approved-on-command-line"));
                    engine.decide(gate_id, true, Some(&note), None, None)?;
                    continue;
                }
                if interactive {
                    let approved = ask(question).await;
                    engine.decide(gate_id, approved, None, None, None)?;
                    continue;
                }
                // No terminal and no `--yes`: the honest outcome is to say so
                // and push nothing. The engine dies with this process, so an
                // undecided gate cannot escape later.
                out.error(&bisa_i18n::say(&bisa_core::text!("cli-projects-publish-gate-open-there-no-terminal", gate_id = gate_id.to_string())));
                return Ok(None);
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
    }
}

/// A publishing act as the node is asked it.
struct Act {
    route: String,
    body: serde_json::Value,
    /// The states of the workstream that say the act landed.
    lands_as: &'static [&'static str],
}

/// How long the end of an act that was approved is waited for: the node's
/// own bound for a push to a slow remote.
const PUBLISH_PATIENCE: Duration = Duration::from_secs(120);

/// A publishing act through the running node (ide/08 §Outward actions).
///
/// `Ok(Some(answer))` — it went out: the route's own answer, or the
/// workstream as it stands once an approved act landed. `Ok(None)` — its
/// gate is open and nobody here decides it: nothing left the machine. Where
/// the project asks first, the gate is decided here — `--yes`, or at the
/// terminal — through its home's own door, and the act's end is **heard**
/// on the node's events: the workstream moved to where the act lands, or
/// the act failed in words (`workstream_publish_failed`). The stream is
/// opened before anything is asked, so the end is never missed.
async fn published_by_node(
    ctx: &Ctx,
    client: &crate::client::NodeClient,
    out: &Out,
    id: WorkstreamId,
    act: Act,
    yes: bool,
) -> Result<Option<serde_json::Value>> {
    let mut events = client.events().await?;
    let answer = client.post(&act.route, act.body).await?;
    if answer["status"] != "awaiting_publish_gate" {
        return Ok(Some(answer));
    }
    let gate = answer["gate"]
        .as_str()
        .context(bisa_core::text!(
            "cli-projects-unexpected-workstream-payload-from-node"
        ))?
        .to_string();
    let ws = ctx.workspace()?;
    let w = ws.get_workstream(id)?;
    let home = match (w.goal, w.work_item) {
        (Some(goal), _) => bisa_core::Home::Goal { goal },
        (None, Some(item)) => ws.home_of_work_item(item)?,
        // A gate that opened has a home: the node refuses the act otherwise.
        (None, None) => bail!(bisa_core::text!(
            "cli-projects-unexpected-workstream-payload-from-node"
        )),
    };
    drop(ws);
    let approve = if yes {
        true
    } else if std::io::stdin().is_terminal() {
        let question = question_of(client, &gate).await;
        ask(&question).await
    } else {
        out.error(&bisa_i18n::say(&bisa_core::text!(
            "cli-projects-publish-gate-open-there-no-terminal",
            gate_id = gate.to_string()
        )));
        return Ok(None);
    };
    let note =
        yes.then(|| bisa_i18n::say(&bisa_core::text!("cli-projects-approved-on-command-line")));
    client
        .post(
            &crate::target::decide_route(&home),
            json!({"approve": approve, "gate": gate, "rationale": note}),
        )
        .await?;
    if !approve {
        bail!(bisa_core::text!("cli-projects-publish-declined"));
    }
    let wid = id.to_string();
    let heard = tokio::time::timeout(PUBLISH_PATIENCE, async {
        while let Some(frame) = events.recv().await {
            let said = &frame["payload"]["payload"];
            if frame["stream"] != "engine" || said["workstream"] != wid.as_str() {
                continue;
            }
            match said["type"].as_str() {
                Some("workstream_publish_failed") => {
                    return Err(anyhow!(bisa_core::text!(
                        "cli-projects-approved-and-did-not-go-out",
                        what = said["what"].as_str().unwrap_or_default().to_string(),
                        reason = said["reason"].as_str().unwrap_or_default().to_string()
                    )));
                }
                Some("workstream_changed")
                    if said["state"]["state"]
                        .as_str()
                        .is_some_and(|state| act.lands_as.contains(&state)) =>
                {
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(anyhow!(bisa_core::text!(
            "cli-projects-node-went-before-the-end"
        )))
    })
    .await
    .map_err(|_| anyhow!(bisa_core::text!("cli-projects-publish-end-not-heard")))?;
    heard?;
    Ok(Some(client.get(&format!("/workstreams/{id}")).await?))
}

/// The question a gate asks, read from the Inbox; the gate's id when the
/// Inbox no longer holds it.
async fn question_of(client: &crate::client::NodeClient, gate: &str) -> String {
    let asked = client.get("/inbox?filter=needs_you").await.ok();
    asked
        .iter()
        .flat_map(|inbox| inbox["rows"].as_array().into_iter().flatten())
        .flat_map(|row| row["needs_action"].as_array().into_iter().flatten())
        .find(|need| need["gate_id"] == gate)
        .and_then(|need| need["question"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| gate.to_string())
}

/// A merge as the node answered it: the merge route's own answer when no
/// gate stood in the way, the workstream's record once an approved one
/// landed.
fn merged_as_the_node_said(answer: &serde_json::Value) -> bisa_engine::codehost::MergeOutcome {
    let landed = answer["workstream"]["state"]["state"] == "merged";
    bisa_engine::codehost::MergeOutcome {
        merged: answer["merged"].as_bool().unwrap_or(landed),
        sha: answer["sha"].as_str().map(str::to_string),
        message: answer["message"].as_str().unwrap_or_default().to_string(),
        remote_branch_deleted: answer["remote_branch_deleted"].as_bool(),
    }
}

/// The pull request as the node answered it: the route's `pr` when no gate
/// stood in the way, the record's own once an approved one was opened.
fn opened_as_the_node_said(answer: &serde_json::Value) -> Result<(u64, String)> {
    let pr = match answer.get("pr") {
        Some(pr) if !pr.is_null() => pr,
        _ => &answer["workstream"]["state"],
    };
    let number = pr["number"].as_u64().context(bisa_core::text!(
        "cli-projects-unexpected-pull-request-payload-from-node"
    ))?;
    Ok((number, pr["url"].as_str().unwrap_or_default().to_string()))
}

async fn ask(question: &str) -> bool {
    let q = format!("{question} [y/N] ");
    tokio::task::spawn_blocking(move || crate::output::confirm_on_stderr(&q))
        .await
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Assignment
// ---------------------------------------------------------------------------

/// `bisa assign <goal> <assignee>...` — the agents that take this
/// goal's work and the humans that may decide its gates.
pub async fn assign(
    ctx: &Ctx,
    out: &Out,
    goal: &str,
    assignees: &[String],
    replace: bool,
) -> Result<()> {
    let id = crate::parse_goal_id(goal)?;
    let wanted = parse_assignees(assignees)?;
    if wanted.is_empty() {
        bail!(bisa_core::text!(
            "cli-projects-name-least-one-assignee-agent-id"
        ));
    }
    let current = ctx.workspace()?.get_goal(id)?.assignees;
    let updated = carried_by(ctx, id, merge_assignees(current, wanted, replace)).await?;
    report_assignees(out, &updated);
    Ok(())
}

/// Set who carries a goal — through the node when one runs, since one
/// engine holds a workspace and a goal's record is rewritten whole; through
/// an engine of this command's own otherwise. The same rule either way:
/// somebody the workspace does not have carries nothing.
async fn carried_by(
    ctx: &Ctx,
    goal: bisa_core::GoalId,
    assignees: Vec<Assignee>,
) -> Result<bisa_core::Goal> {
    if let Some(node) = ctx.node_client().await {
        let carried: Vec<String> = assignees.iter().map(|a| a.to_string()).collect();
        let answer = node
            .put(
                &format!("/goals/{goal}/assignees"),
                json!({ "assignees": carried }),
            )
            .await?;
        return serde_json::from_value(answer["goal"].clone()).context(bisa_core::text!(
            "cli-projects-unexpected-goal-payload-from-node"
        ));
    }
    let (engine, _) = ctx.engine().await?;
    let carried = bisa_engine::ops::set_goal_assignees(engine.inner(), goal, assignees);
    engine.shutdown().await;
    Ok(carried?)
}

/// `bisa unassign <goal> [<assignee>...]` — named entries come off; no
/// names clears the whole list.
pub async fn unassign(ctx: &Ctx, out: &Out, goal: &str, assignees: &[String]) -> Result<()> {
    let id = crate::parse_goal_id(goal)?;
    let remaining = if assignees.is_empty() {
        vec![]
    } else {
        let drop = parse_assignees(assignees)?;
        ctx.workspace()?
            .get_goal(id)?
            .assignees
            .into_iter()
            .filter(|a| !drop.contains(a))
            .collect()
    };
    let updated = carried_by(ctx, id, remaining).await?;
    report_assignees(out, &updated);
    Ok(())
}

fn report_assignees(out: &Out, goal: &bisa_core::Goal) {
    let who: Vec<String> = goal.assignees.iter().map(|a| a.to_string()).collect();
    out.say(&bisa_core::text!(
        "cli-projects-goal-carried",
        a0 = (goal.id).to_string(),
        a1 = (if who.is_empty() {
            "nobody".to_string()
        } else {
            who.join(", ")
        })
        .to_string()
    ));
    out.json_value(json!({"goal": goal.id.to_string(), "assignees": who}));
}
