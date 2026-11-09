//! `Skill`: a named markdown procedure an agent follows. Held once in the
//! library and referenced by id; twenty agents share one checklist instead of
//! growing twenty drifting copies.
//!
//! A skill's markdown is a **body** and, after it, any number of **reference
//! sections** — each opened by a heading `## Reference: <Title>` — which a
//! session reads on demand ([`parts`]): the body rides every prompt of an
//! agent that carries the skill, a section only when the agent asks for it
//! (`skill_read`, or the file beside `SKILL.md` on a harness that loads skill
//! folders). One markdown, one record, one editor: the convention is the
//! heading, not a second field.

use crate::id::SkillId;
use crate::origin::Origin;
use crate::tags::Tags;
use serde::{Deserialize, Serialize};

/// Longest skill markdown, in bytes — the body a session always reads and
/// the reference sections it reads on demand together. A skill reaches every
/// session that carries it, whole on a harness without the platform's tools,
/// so an unbounded one is a tax on every launch; this is the ceiling the
/// record keeps at every write.
pub const MAX_SKILL_BYTES: usize = 64 * 1024;

/// Longest **body** of a skill the catalog ships — the part every prompt of a
/// carrier reads. The bundle test holds it (a shipped skill is read by many);
/// the record does not, since a person's own skill is theirs to size.
pub const MAX_SKILL_BODY_BYTES: usize = 32 * 1024;

// The body cap is a part of the whole; a skill of one section would otherwise be refused by its own body.
const _: () = assert!(MAX_SKILL_BODY_BYTES < MAX_SKILL_BYTES);

/// The heading that opens a reference section. Everything before the first
/// one is the skill's body; each heading runs to the next or to the end.
pub const REFERENCE_HEADING: &str = "## Reference:";

/// A skill's markdown split into what a session always reads and what it
/// reads on demand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillParts<'a> {
    /// The procedure: everything before the first reference heading, its
    /// trailing blank lines dropped.
    pub body: &'a str,
    /// The reference sections, in the order written.
    pub sections: Vec<SkillSection<'a>>,
}

/// One reference section: its title as written, the slug a tool names it by,
/// and its markdown without the heading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillSection<'a> {
    pub slug: String,
    pub title: &'a str,
    pub markdown: &'a str,
}

impl SkillParts<'_> {
    /// The section a name points at — its slug, or its title, case and
    /// surrounding blanks aside — or none.
    pub fn section(&self, name: &str) -> Option<&SkillSection<'_>> {
        let want = name.trim();
        if want.is_empty() {
            return None;
        }
        self.sections.iter().find(|s| {
            s.slug.eq_ignore_ascii_case(want) || s.title.trim().eq_ignore_ascii_case(want)
        })
    }

    /// The one line a body is followed by where the sections are read with a
    /// tool: `Reference sections, read with skill_read: a · b · c`. Nothing
    /// for a skill with no sections — kept here so the appendix and the tool
    /// never drift apart.
    pub fn index_line(&self, tool: &str) -> Option<String> {
        if self.sections.is_empty() {
            return None;
        }
        let slugs: Vec<&str> = self.sections.iter().map(|s| s.slug.as_str()).collect();
        Some(format!(
            "Reference sections, read with {tool}: {}",
            slugs.join(" · ")
        ))
    }
}

/// A title as a slug: lower-case letters and digits, every other run one
/// dash, no dash at either end; `section` when nothing is left.
pub fn slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            for lower in c.to_lowercase() {
                out.push(lower);
            }
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Split a skill's markdown into its body and its reference sections. A
/// heading counts only at the start of a line outside a ``` fence, so a
/// skill that quotes the convention in a code block is not split by it; a
/// second section of the same title takes the slug with `-2`, then `-3`.
pub fn parts(markdown: &str) -> SkillParts<'_> {
    // The byte where each heading line starts, with the title after the marker.
    let mut heads: Vec<(usize, usize, &str)> = Vec::new(); // (line start, line end, title)
    let mut fenced = false;
    let mut at = 0usize;
    for line in markdown.split_inclusive('\n') {
        let start = at;
        at += line.len();
        let text = line.trim_end_matches(['\n', '\r']);
        if text.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some(rest) = text.strip_prefix(REFERENCE_HEADING) {
            heads.push((start, at, rest.trim()));
        }
    }
    let body_end = heads.first().map_or(markdown.len(), |h| h.0);
    let body = markdown[..body_end].trim_end();
    let mut sections = Vec::with_capacity(heads.len());
    let mut taken: Vec<String> = Vec::new();
    for (i, (_, line_end, title)) in heads.iter().enumerate() {
        let section_end = heads.get(i + 1).map_or(markdown.len(), |h| h.0);
        let title: &str = if title.is_empty() { "Reference" } else { title };
        let base = slug(title);
        let mut candidate = base.clone();
        let mut n = 2;
        while taken.iter().any(|t| t == &candidate) {
            candidate = format!("{base}-{n}");
            n += 1;
        }
        taken.push(candidate.clone());
        sections.push(SkillSection {
            slug: candidate,
            title,
            markdown: markdown[*line_end..section_end].trim(),
        });
    }
    SkillParts { body, sections }
}

/// A fingerprint of the markdown, sixteen hex digits: what a skill file the
/// platform wrote carries in its front matter, so a stale copy of the
/// platform's own is told from a person's. FNV-1a over the bytes — stable
/// across builds and dependency-free; it tells two texts apart, and claims
/// nothing more (a collision costs one needless rewrite of a file of ours).
pub fn content_hash(markdown: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in markdown.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    /// Immutable: agents reference it, and it becomes a directory name inside
    /// a session's skills dir.
    pub id: SkillId,
    pub name: String,
    /// One line telling a model *when* to reach for this skill. It is the only
    /// part read before the skill is opened — so it is required.
    pub description: String,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    pub markdown: String,
    pub origin: Origin,
    pub created_at: u64,
}

impl Skill {
    pub fn validate(&self) -> Result<(), SkillError> {
        if self.name.trim().is_empty() {
            return Err(SkillError::EmptyName);
        }
        if self.description.trim().is_empty() {
            return Err(SkillError::EmptyDescription);
        }
        if self.markdown.len() > MAX_SKILL_BYTES {
            return Err(SkillError::TooLarge(self.markdown.len()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SkillError {
    #[error("a skill needs a name")]
    EmptyName,
    #[error(
        "a skill needs a one-line description: it is the only part a model reads before opening it"
    )]
    EmptyDescription,
    #[error("skill body is {0} bytes; the cap is {MAX_SKILL_BYTES}")]
    TooLarge(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill() -> Skill {
        Skill {
            id: SkillId::new("code-review-checklist").unwrap(),
            name: "Code review checklist".into(),
            description: "Reviewing a diff: what to check, in order.".into(),
            tags: Tags::default(),
            markdown: "# Checklist\n".into(),
            origin: Origin::Local,
            created_at: 0,
        }
    }

    #[test]
    fn a_description_is_required_and_the_body_is_capped() {
        assert!(skill().validate().is_ok());
        let mut s = skill();
        s.description = " ".into();
        assert_eq!(s.validate(), Err(SkillError::EmptyDescription));
        let mut s = skill();
        s.markdown = "x".repeat(MAX_SKILL_BYTES + 1);
        assert!(matches!(s.validate(), Err(SkillError::TooLarge(_))));
        let mut s = skill();
        s.markdown = "x".repeat(MAX_SKILL_BODY_BYTES + 1);
        assert!(
            s.validate().is_ok(),
            "the body cap is the catalog's rule for what it ships; a record takes the whole cap"
        );
    }

    const WITH_SECTIONS: &str = "# Design\n\nUse this.\n\n## Procedure\n\n1. Look.\n\n## Reference: Craft floor\n\nContrast 4.5:1.\n\n## Reference: Modes in depth\n\nOperate · Persuade.\n";

    #[test]
    fn a_skill_without_reference_headings_is_all_body() {
        let p = parts("# Checklist\n\n## Procedure\n\n1. Do it.\n\n");
        assert_eq!(p.body, "# Checklist\n\n## Procedure\n\n1. Do it.");
        assert!(p.sections.is_empty());
        assert_eq!(p.index_line("skill_read"), None, "no sections, no index");
        assert_eq!(
            p.section("procedure"),
            None,
            "a body heading is not a section"
        );
    }

    #[test]
    fn reference_sections_split_at_their_headings_and_take_kebab_slugs() {
        let p = parts(WITH_SECTIONS);
        assert_eq!(p.body, "# Design\n\nUse this.\n\n## Procedure\n\n1. Look.");
        assert_eq!(
            p.sections
                .iter()
                .map(|s| (s.slug.as_str(), s.title, s.markdown))
                .collect::<Vec<_>>(),
            vec![
                ("craft-floor", "Craft floor", "Contrast 4.5:1."),
                ("modes-in-depth", "Modes in depth", "Operate · Persuade."),
            ]
        );
        assert_eq!(
            p.index_line("skill_read").as_deref(),
            Some("Reference sections, read with skill_read: craft-floor · modes-in-depth")
        );
        assert_eq!(slug("Type, colour & space!"), "type-colour-space");
        assert_eq!(
            slug("  ---  "),
            "section",
            "nothing left is a plain word, never empty"
        );
        assert_eq!(
            slug("Éléments visuels"),
            "éléments-visuels",
            "letters stay letters"
        );
    }

    #[test]
    fn a_heading_inside_a_code_fence_is_text_not_a_section() {
        let md = "# Skill\n\nWrite sections as:\n\n```markdown\n## Reference: Example\n```\n\nThen go on.\n\n## Reference: Real one\n\nBody.\n";
        let p = parts(md);
        assert_eq!(p.sections.len(), 1, "{:?}", p.sections);
        assert_eq!(p.sections[0].slug, "real-one");
        assert!(
            p.body.contains("## Reference: Example"),
            "the quoted heading stays in the body"
        );
    }

    #[test]
    fn a_section_is_found_by_slug_or_title_whatever_the_case() {
        let p = parts(WITH_SECTIONS);
        assert_eq!(
            p.section("craft-floor").map(|s| s.title),
            Some("Craft floor")
        );
        assert_eq!(
            p.section("CRAFT-FLOOR").map(|s| s.title),
            Some("Craft floor")
        );
        assert_eq!(
            p.section(" modes in depth ").map(|s| s.slug.as_str()),
            Some("modes-in-depth")
        );
        assert_eq!(
            p.section("Modes In Depth").map(|s| s.slug.as_str()),
            Some("modes-in-depth")
        );
        assert_eq!(p.section("motion"), None);
        assert_eq!(p.section("  "), None);
    }

    #[test]
    fn two_sections_of_one_title_keep_distinct_slugs() {
        let p = parts(
            "Body\n\n## Reference: Notes\n\nA\n\n## Reference: Notes\n\nB\n\n## Reference:\n\nC\n",
        );
        assert_eq!(
            p.sections
                .iter()
                .map(|s| s.slug.as_str())
                .collect::<Vec<_>>(),
            vec!["notes", "notes-2", "reference"],
            "the second takes -2; a heading with no title is called Reference"
        );
        assert_eq!(p.sections[2].title, "Reference");
        assert_eq!(p.section("notes-2").map(|s| s.markdown), Some("B"));
    }

    #[test]
    fn the_cap_is_the_whole_markdown_and_the_hash_names_its_content() {
        let big = format!(
            "Body\n\n## Reference: Big\n\n{}",
            "x".repeat(MAX_SKILL_BYTES)
        );
        let mut s = skill();
        s.markdown = big;
        assert!(
            matches!(s.validate(), Err(SkillError::TooLarge(_))),
            "sections count toward the cap"
        );
        assert_eq!(content_hash("a").len(), 16);
        assert_eq!(content_hash("a"), content_hash("a"));
        assert_ne!(content_hash("a"), content_hash("b"));
        assert!(content_hash("a").chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn roundtrip() {
        let s = skill();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Skill>(&json).unwrap(), s);
    }

    // added by the coverage pass: skill.rs

    #[test]
    fn a_skill_needs_a_name() {
        let mut s = skill();
        s.name = " ".into();
        assert_eq!(s.validate(), Err(SkillError::EmptyName));
    }
}
