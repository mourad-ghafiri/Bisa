//! `--tag` on every noun, and `bisa tags`: the one filing system.
//!
//! Agents, teams, channels, skills, MCP servers, projects, goals, workflows
//! and connectors all carry [`Tags`], so the flags that set and filter them
//! are declared once here and flattened into each subcommand. That is the
//! point of the shared newtype: one normalization rule, one error message, one
//! `--match` default.
//!
//! **Setting replaces.** Passing any `--tag` swaps the whole list rather than
//! appending — the same rule `--model` already follows on `agent edit`. A
//! repeatable flag cannot be patched piecewise without guessing whether the
//! caller meant "add this" or "these are all of them", and guessing wrong
//! silently drops a tag.
//!
//! **Filtering goes through the index**, never a scan of the definitions we
//! just printed: `bisa tags` counts the same rows `--tag` selects, so the
//! facet and the filtered list can never disagree.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::Result;
use bisa_core::tags::{TagEntity, TagMatch, VOCABULARY};
use bisa_core::Tags;
use bisa_store::Workspace;
use clap::Args;
use serde_json::json;
use std::collections::HashSet;
use std::str::FromStr;

/// The tags a create or edit command sets.
#[derive(Args, Clone, Debug, Default)]
pub struct TagSetArgs {
    /// Category tag (repeatable). Passing any --tag replaces the whole list,
    /// so name every tag you want kept. See `bisa tags`.
    #[arg(long = "tag")]
    pub tags: Vec<String>,
}

impl TagSetArgs {
    pub fn given(&self) -> bool {
        !self.tags.is_empty()
    }

    /// Normalize and validate. A tag that will not normalize fails here,
    /// naming itself, rather than being dropped on the way to the store.
    pub fn parse(&self) -> Result<Tags> {
        parse_tags(&self.tags)
    }

    /// The tags to write, keeping `current` when no `--tag` was passed.
    pub fn apply(&self, current: Tags) -> Result<Tags> {
        if self.given() {
            self.parse()
        } else {
            Ok(current)
        }
    }
}

/// How several `--tag` values on a list command combine.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchArg {
    /// Carries at least one of the tags.
    #[default]
    Any,
    /// Carries every one of the tags.
    All,
}

impl From<MatchArg> for TagMatch {
    fn from(m: MatchArg) -> Self {
        match m {
            MatchArg::Any => TagMatch::Any,
            MatchArg::All => TagMatch::All,
        }
    }
}

/// The tags a list command filters by.
#[derive(Args, Clone, Debug, Default)]
pub struct TagFilterArgs {
    /// Show only what carries this tag (repeatable). See `bisa tags`.
    #[arg(long = "tag")]
    pub tags: Vec<String>,
    /// How several --tag values combine: any (default) or all.
    #[arg(long = "match", value_name = "MODE", value_enum, default_value_t = MatchArg::Any)]
    pub match_mode: MatchArg,
}

impl TagFilterArgs {
    /// The ids this filter admits, or `None` when no `--tag` was passed and
    /// the list is unfiltered. Validating the tags first means a typo is an
    /// error rather than an empty list that looks like an honest answer.
    pub fn admitted(&self, ws: &Workspace, entity: TagEntity) -> Result<Option<HashSet<String>>> {
        if self.tags.is_empty() {
            return Ok(None);
        }
        let tags = parse_tags(&self.tags)?;
        let ids = ws.ids_with_tags(entity, tags.as_slice(), self.match_mode.into())?;
        Ok(Some(ids.into_iter().collect()))
    }

    /// The same filter as a predicate over an object's own tags, for the one
    /// list the index cannot answer: the catalog is compiled into the binary
    /// and its entries have no index rows until they are installed. The tags
    /// are still validated here, so a typo is an error rather than an empty
    /// list that looks like an honest answer.
    pub fn matcher(&self) -> Result<TagMatcher> {
        Ok(TagMatcher {
            tags: parse_tags(&self.tags)?,
            mode: self.match_mode.into(),
        })
    }
}

/// A parsed [`TagFilterArgs`], ready to be asked about one object at a time.
pub struct TagMatcher {
    tags: Tags,
    mode: TagMatch,
}

impl TagMatcher {
    /// True when `tags` survives the filter. No `--tag` admits everything, the
    /// same way [`TagFilterArgs::admitted`] returns `None`.
    pub fn keeps(&self, tags: &Tags) -> bool {
        match self.mode {
            TagMatch::Any if self.tags.is_empty() => true,
            TagMatch::Any => self.tags.iter().any(|t| tags.contains(t)),
            TagMatch::All => self.tags.iter().all(|t| tags.contains(t)),
        }
    }
}

/// True when `id` survives the filter `admitted` returned.
pub fn keeps(admitted: &Option<HashSet<String>>, id: &str) -> bool {
    match admitted {
        Some(ids) => ids.contains(id),
        None => true,
    }
}

pub fn parse_tags(raw: &[String]) -> Result<Tags> {
    Tags::new(raw).map_err(|e| anyhow::anyhow!("{e}"))
}

/// One line of tags for a `show` or `list` rendering.
pub fn label(tags: &Tags) -> String {
    if tags.is_empty() {
        "—".to_string()
    } else {
        tags.as_slice().join(", ")
    }
}

#[derive(Args)]
pub struct TagsArgs {
    /// Count only one kind: agent | team | channel | skill | mcp | project |
    /// goal | workflow | connector
    #[arg(long)]
    pub entity: Option<String>,
}

/// The facet list: which tags exist, on what, and how many things carry them.
pub fn tags(ctx: &Ctx, out: &Out, args: TagsArgs) -> Result<()> {
    let ws = ctx.workspace()?;
    let entity = args
        .entity
        .as_deref()
        .map(|e| TagEntity::from_str(e).map_err(|e| anyhow::anyhow!("{e}")))
        .transpose()?;
    let counts = ws.tag_counts(entity)?;
    if counts.is_empty() {
        out.say(&bisa_core::text!("cli-tags-nothing-tagged-yet"));
    }
    for c in &counts {
        out.human(&format!("{:<10} {:<24} {}", c.entity, c.tag, c.count));
    }
    // The catalog's agents, teams, channels and skills draw only from this
    // list, so showing it is how someone learns which tags will actually match
    // something before they invent a near-synonym of their own.
    out.say(&bisa_core::text!(
        "cli-tags-catalog-uses-free-form-tags-legal",
        a0 = (VOCABULARY.join(", ")).to_string()
    ));
    out.json_value(json!({"tags": counts, "vocabulary": VOCABULARY}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use bisa_core::tags::TagEntity;
    use clap::CommandFactory;

    /// `--entity` names every kind that carries tags, in the core's order,
    /// and no kind that does not.
    #[test]
    fn the_entity_help_names_exactly_the_tagged_kinds() {
        let cli = crate::Cli::command();
        let tags = cli.find_subcommand("tags").expect("`tags` is a verb");
        let help = tags
            .get_arguments()
            .find(|arg| arg.get_id() == "entity")
            .and_then(|arg| arg.get_help())
            .map(|help| help.to_string())
            .expect("`--entity` has help");
        let listed: Vec<&str> = help
            .split_once(": ")
            .map(|(_, kinds)| kinds.split(" | ").map(str::trim).collect())
            .unwrap_or_default();
        let kinds: Vec<&str> = TagEntity::ALL.iter().map(|e| e.as_str()).collect();
        assert_eq!(kinds.len(), 9);
        assert_eq!(listed, kinds, "{help}");
    }
}
