//! `bisa catalog`: the agents, skills, teams and channels a workspace can
//! install, and installs nothing until it is asked.
//!
//! A workspace opens with one agent — `general-agent` — and everything else
//! is opt-in. The catalog is compiled into the binary, so `list` and `show`
//! answer without touching the network and without creating anything; only
//! `install` writes.
//!
//! The three verbs exist in that order for a reason. **`list`** says what
//! there is and what is already here. **`show`** says what installing one
//! entry would pull in, because an install is transitive — a team brings its
//! agents and each agent brings its skills — and something that quietly
//! creates six objects is something you cannot undo confidently. **`install`**
//! then reports every object it actually created, per kind, so the list of
//! what to undo is the list it just printed.
//!
//! Filtering reuses [`TagFilterArgs`], but not its index lookup: a catalog
//! entry has tags and no index row until it is installed, so the match runs
//! against the entry's own tags.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::tags::TagFilterArgs;
use anyhow::{anyhow, Result};
use bisa_store::{CatalogEntry, CatalogKind, Installed, Workspace};
use clap::Subcommand;
use serde_json::json;
use std::str::FromStr;

#[derive(Subcommand)]
pub enum CatalogCmd {
    /// List what can be installed, marking what already is
    List {
        /// Only one kind: agent | skill | team | channel | connector | workflow | addon
        #[arg(long)]
        kind: Option<String>,
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Show one entry and everything installing it would create
    Show { kind: String, slug: String },
    /// Install an entry and everything it needs. Installing twice is not an
    /// error: what is already here is left exactly as it is.
    Install { kind: String, slug: String },
}

/// The seven kinds, named in the error rather than left to be guessed.
/// `CatalogKind::from_str` refuses the same strings, but its message says only
/// that the kind is unknown.
fn catalog_kind(s: &str) -> Result<CatalogKind> {
    CatalogKind::from_str(s).map_err(|_| {
        anyhow!(bisa_core::text!(
            "cli-catalog-unknown-kind-agent-skill-team-channel",
            s = format!("{s:?}")
        ))
    })
}

/// What an entry's `requires` names: skills for an agent, agents for a team or
/// a channel. A skill is the leaf and needs nothing.
///
/// The store owns the same rule inside an install; here it only decides what
/// to call the ids when they are printed.
fn requires_kind(kind: CatalogKind) -> Option<CatalogKind> {
    match kind {
        CatalogKind::Agent => Some(CatalogKind::Skill),
        CatalogKind::Team | CatalogKind::Channel | CatalogKind::Workflow => {
            Some(CatalogKind::Agent)
        }
        CatalogKind::Skill | CatalogKind::Connector | CatalogKind::Addon => None,
    }
}

/// Everything an install of `kind`/`slug` would touch, dependencies first —
/// the same walk the store's `install` does, repeated here so `show` can say
/// what a choice costs before it is made.
///
/// Entries already installed are in the list and marked as such: an install
/// leaves them alone, and knowing which ones are already yours is most of what
/// makes the total readable.
fn plan(ws: &Workspace, kind: CatalogKind, slug: &str, into: &mut Vec<CatalogEntry>) -> Result<()> {
    if into.iter().any(|e| e.kind == kind && e.slug == slug) {
        return Ok(());
    }
    let entry = ws.catalog_entry(kind, slug)?;
    if let Some(required) = requires_kind(kind) {
        for id in entry.requires.clone() {
            plan(ws, required, &id, into)?;
        }
    }
    into.push(entry);
    Ok(())
}

/// Listing position for a kind: the catalog's own order, so `show` groups the
/// way `list` does.
fn rank(kind: CatalogKind) -> usize {
    CatalogKind::ALL
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0)
}

fn state(entry: &CatalogEntry) -> &'static str {
    if entry.installed {
        "installed"
    } else {
        "available"
    }
}

/// The node's answer to `POST /catalog/install` (`InstalledDto`: a workflow
/// as `{slug, id}`) read back into the store's own record, so the CLI prints
/// and reports one shape whichever door the install went through.
fn installed_from_node(v: &serde_json::Value) -> Result<Installed> {
    let names = |field: &str| -> Vec<String> {
        v[field]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let workflows = v["workflows"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|w| {
                    let slug = w["slug"].as_str()?.to_string();
                    let id = w["id"].as_str()?.parse().ok()?;
                    Some((slug, id))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Installed {
        agents: names("agents"),
        skills: names("skills"),
        teams: names("teams"),
        channels: names("channels"),
        workflows,
        connectors: names("connectors"),
        addons: names("addons"),
    })
}

pub async fn catalog(ctx: &Ctx, out: &Out, cmd: CatalogCmd) -> Result<()> {
    let ws = ctx.workspace()?;
    match cmd {
        CatalogCmd::List { kind, filter } => {
            let kind = kind.as_deref().map(catalog_kind).transpose()?;
            let matcher = filter.matcher()?;
            let entries: Vec<CatalogEntry> = ws
                .catalog_entries(kind)?
                .into_iter()
                .filter(|e| matcher.keeps(&e.tags))
                .collect();
            if entries.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-catalog-no-catalog-entries-match-bisa-catalog"
                ));
            }
            for e in &entries {
                out.human(&format!(
                    "{:<8} {:<30} {:<24} {:<10} {}",
                    e.kind,
                    e.slug,
                    e.name,
                    state(e),
                    crate::tags::label(&e.tags)
                ));
            }
            if !entries.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-catalog-installed-bisa-catalog-show-kind-slug",
                    a0 = (entries.iter().filter(|e| e.installed).count()).to_string(),
                    a1 = (entries.len()).to_string()
                ));
            }
            out.json_value(json!({"entries": entries}));
        }
        CatalogCmd::Show { kind, slug } => {
            let kind = catalog_kind(&kind)?;
            let entry = ws.catalog_entry(kind, &slug)?;
            out.say(&bisa_core::text!(
                "cli-catalog-tags",
                a0 = (entry.kind).to_string(),
                a1 = (entry.slug).to_string(),
                a2 = (entry.name).to_string(),
                a3 = (state(&entry)).to_string(),
                a4 = (entry.description).to_string(),
                a5 = (crate::tags::label(&entry.tags)).to_string()
            ));
            let mut whole = Vec::new();
            plan(&ws, kind, &slug, &mut whole)?;
            // The entry itself is the last of its own plan; what precedes it is
            // what comes along with it.
            whole.pop();
            // The plan's own order is dependency order, which is what the
            // install needs and not what a reader does: grouped by kind, a
            // team's five agents are a number you can weigh at a glance.
            whole.sort_by_key(|e| (rank(e.kind), e.slug.clone()));
            if whole.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-catalog-installing-creates-nothing-else"
                ));
            } else {
                out.say(&bisa_core::text!("cli-catalog-installing-also-creates"));
                for e in &whole {
                    out.human(&format!(
                        "    {:<8} {:<30} {}",
                        e.kind,
                        e.slug,
                        if e.installed {
                            bisa_i18n::say(&bisa_core::text!("cli-catalog-already-here"))
                        } else {
                            bisa_i18n::say(&bisa_core::text!("cli-catalog-new"))
                        }
                    ));
                }
            }
            out.json_value(json!({"entry": entry, "creates": whole}));
        }
        CatalogCmd::Install { kind, slug } => {
            let kind = catalog_kind(&kind)?;
            // Through the running node when there is one — its registry and
            // bus hear the install, so an open desktop draws what landed —
            // and straight into the store otherwise. A collision is the
            // store's refusal either way; it names the id that is taken and
            // what holds it, so it travels to the user unwrapped.
            let installed = match ctx.node_client().await {
                Some(node) => {
                    let v = node
                        .post(
                            "/catalog/install",
                            json!({"kind": kind.to_string(), "slug": slug}),
                        )
                        .await?;
                    installed_from_node(&v["installed"])?
                }
                None => ws.install(kind, &slug)?,
            };
            if installed.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-catalog-everything-needs-were-already-here-nothing",
                    kind = kind.to_string(),
                    slug = slug.to_string()
                ));
            } else {
                out.say(&bisa_core::text!(
                    "cli-catalog-installed",
                    kind = kind.to_string(),
                    slug = slug.to_string()
                ));
                let workflows: Vec<String> = installed
                    .workflows
                    .iter()
                    .map(|(slug, id)| format!("{slug} ({id})"))
                    .collect();
                for (label, ids) in [
                    ("skills   ", &installed.skills),
                    ("agents   ", &installed.agents),
                    ("teams    ", &installed.teams),
                    ("channels ", &installed.channels),
                    ("workflows", &workflows),
                ] {
                    if !ids.is_empty() {
                        out.human(&format!("  {label}  {}", ids.join(", ")));
                    }
                }
            }
            out.json_value(json!({"installed": installed}));
        }
    }
    Ok(())
}
