//! `bisa skill`: the shared library of procedures agents follow.
//!
//! An agent's system prompt is its *role*; a skill is a *procedure* — the
//! checklist it works through for one specific kind of thing. They live in one
//! library rather than inside each agent, so twenty agents share one
//! code-review checklist instead of twenty prompts each growing a drifting
//! copy of it. Agents reference a skill by id; `bisa agent skill add`
//! makes that reference.
//!
//! A skill cannot be deleted while an agent carries it. `bisa skill
//! usage <id>` says who does; detaching is a decision you make per agent,
//! not something a delete does behind your back.
//!
//! Reads go to the workspace directly; a write goes through the running node
//! when there is one, and through this process's own store when there is
//! none.
//!
//! The body comes from a file. A procedure is a markdown document people edit
//! in an editor and keep in version control, so `--file` is the path this
//! command is built around; `--markdown` exists for a one-liner and a script.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::tags::{keeps, TagFilterArgs, TagSetArgs};
use anyhow::{bail, Context as _, Result};
use bisa_core::tags::TagEntity;
use bisa_core::Origin;
use bisa_core::{Skill, SkillId};
use bisa_store::{NewSkill, UsageKind};
use clap::Subcommand;
use serde_json::json;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum SkillCmd {
    /// List the library
    List {
        #[command(flatten)]
        filter: TagFilterArgs,
    },
    /// Show one skill, including its markdown
    Show { id: String },
    /// Add a skill to the library
    Add {
        /// Stable id agents reference: lowercase letters, digits, `-` and `_`
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        /// One line telling a model when to reach for this skill. It is the
        /// only part read before the skill is opened, so it decides whether
        /// the skill is ever used at all.
        #[arg(long)]
        description: String,
        /// Markdown file holding the procedure
        #[arg(long, conflicts_with = "markdown")]
        file: Option<PathBuf>,
        /// The procedure as text, instead of --file
        #[arg(long)]
        markdown: Option<String>,
        #[command(flatten)]
        tags: TagSetArgs,
    },
    /// Change a skill. The id is fixed: agents reference it, and a rename
    /// would detach every one of them.
    Edit {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Replace the procedure from a markdown file
        #[arg(long, conflicts_with = "markdown")]
        file: Option<PathBuf>,
        /// Replace the procedure with this text
        #[arg(long)]
        markdown: Option<String>,
        #[command(flatten)]
        tags: TagSetArgs,
    },
    /// Which agents carry this skill
    Usage { id: String },
    /// Delete a skill. Refused while any agent still carries it — detach it
    /// there first (`bisa skill usage <id>` lists them).
    #[command(alias = "remove")]
    Rm { id: String },
}

fn body(file: Option<PathBuf>, markdown: Option<String>) -> Result<Option<String>> {
    match (file, markdown) {
        (Some(path), None) => Ok(Some(
            std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?,
        )),
        (None, Some(text)) => Ok(Some(text)),
        (None, None) => Ok(None),
        // clap already refuses both; this arm keeps the function total.
        (Some(_), Some(_)) => bail!(bisa_core::text!("cli-skills-pass-file-markdown-not-both")),
    }
}

fn skill_id(s: &str) -> Result<SkillId> {
    SkillId::new(s).map_err(|e| anyhow::anyhow!("{e}"))
}

fn render(out: &Out, s: &Skill) {
    let origin = match &s.origin {
        Origin::Catalog { slug } => bisa_i18n::say(&bisa_core::text!(
            "cli-skills-from-catalog",
            slug = slug.to_string()
        )),
        Origin::Local => String::new(),
    };
    out.say(&bisa_core::text!(
        "cli-skills-tags-bytes",
        a0 = (s.id).to_string(),
        a1 = (s.name).to_string(),
        origin = origin.to_string(),
        a2 = (s.description).to_string(),
        a3 = (crate::tags::label(&s.tags)).to_string(),
        a4 = (s.markdown.len()).to_string()
    ));
}

/// A skill as the node answered it.
fn skill_said(answer: &serde_json::Value) -> Result<Skill> {
    Ok(serde_json::from_value(answer["skill"].clone())?)
}

pub async fn skill(ctx: &Ctx, out: &Out, cmd: SkillCmd) -> Result<()> {
    let ws = ctx.workspace()?;
    match cmd {
        SkillCmd::List { filter } => {
            let admitted = filter.admitted(&ws, TagEntity::Skill)?;
            let skills: Vec<Skill> = ws
                .list_skills()?
                .into_iter()
                .filter(|s| keeps(&admitted, s.id.as_str()))
                .collect();
            if skills.is_empty() {
                out.say(&bisa_core::text!(
                    "cli-skills-no-skills-match-bisa-catalog-list"
                ));
            }
            for s in &skills {
                out.human(&format!(
                    "{:<34} {:<24} {}",
                    s.id,
                    s.name,
                    crate::tags::label(&s.tags)
                ));
            }
            out.json_value(json!({"skills": skills}));
        }
        SkillCmd::Show { id } => {
            let s = ws.get_skill(&skill_id(&id)?)?;
            render(out, &s);
            out.human(&format!("\n{}", s.markdown));
            out.json_value(json!({"skill": s}));
        }
        SkillCmd::Add {
            id,
            name,
            description,
            file,
            markdown,
            tags,
        } => {
            let markdown = body(file, markdown)?.context(bisa_core::text!(
                "cli-skills-skill-needs-procedure-file-path-md"
            ))?;
            let new = NewSkill {
                id: skill_id(&id)?,
                name,
                description,
                tags: tags.parse()?,
                markdown,
            };
            let s = match ctx.node_client().await {
                Some(node) => {
                    let body = json!({
                        "id": new.id,
                        "name": new.name,
                        "description": new.description,
                        "tags": new.tags,
                        "markdown": new.markdown,
                    });
                    skill_said(&node.post("/skills", body).await?)?
                }
                None => ws.create_skill(new)?,
            };
            render(out, &s);
            out.json_value(json!({"skill": s}));
        }
        SkillCmd::Edit {
            id,
            name,
            description,
            file,
            markdown,
            tags,
        } => {
            let mut def = ws.get_skill(&skill_id(&id)?)?;
            if let Some(n) = name {
                def.name = n;
            }
            if let Some(d) = description {
                def.description = d;
            }
            if let Some(m) = body(file, markdown)? {
                def.markdown = m;
            }
            def.tags = tags.apply(def.tags.clone())?;
            let s = match ctx.node_client().await {
                Some(node) => {
                    let body = json!({
                        "name": def.name,
                        "description": def.description,
                        "tags": def.tags,
                        "markdown": def.markdown,
                    });
                    skill_said(&node.patch(&format!("/skills/{}", def.id), body).await?)?
                }
                None => ws.update_skill(def)?,
            };
            render(out, &s);
            out.json_value(json!({"skill": s}));
        }
        SkillCmd::Usage { id } => {
            let usage = ws.usage_of(UsageKind::Skill, &id)?;
            crate::usage::render(out, UsageKind::Skill, &id, &usage);
        }
        SkillCmd::Rm { id } => {
            // Nothing is detached on the way out any more: one delete used to
            // rewrite every agent that carried the skill, and a rewritten
            // definition looks exactly like one that never had it. The store's
            // refusal names the agents, so it travels unwrapped.
            let skill = skill_id(&id)?;
            match ctx.node_client().await {
                Some(node) => {
                    node.delete(&format!("/skills/{skill}")).await?;
                }
                None => ws.remove_skill(&skill)?,
            }
            out.say(&bisa_core::text!(
                "cli-skills-skill-removed",
                id = id.to_string()
            ));
            out.json_value(json!({"removed": id}));
        }
    }
    Ok(())
}
