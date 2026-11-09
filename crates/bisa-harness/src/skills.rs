//! Agent skill materialization.
//!
//! A skill is a named markdown procedure from the workspace's skill library,
//! attached to an Agent definition by id. Its markdown is a body and, after
//! it, reference sections a session reads on demand (`bisa_core::skill::parts`).
//! At session launch it is delivered the way the harness understands best:
//! - **claude-code**: written to `<cwd>/.claude/skills/<id>/SKILL.md` (Claude
//!   Code's native skills dir) — the body, a line naming the reference files,
//!   and one `reference/<slug>.md` per section beside it. No prompt appendix.
//! - **every other harness**: returned as a markdown appendix the caller
//!   appends to the session instructions — the whole text on a harness that
//!   has no platform tools to read a section with later ([`Delivery::Full`]),
//!   the body and an index naming the sections for one that has
//!   ([`Delivery::BodyAndIndex`], read with `skill_read`).
//!
//! A skill file the platform wrote carries its marker in the front matter's
//! `metadata` (the Agent Skills specification's own map): the skill's id and
//! a fingerprint of the file as written. A stale copy of ours — the skill
//! changed since — is rewritten; a file without the marker, or one somebody
//! edited after we wrote it, is the person's: it is never overwritten, and
//! the skill falls back to the prompt appendix. Nothing here is ever deleted:
//! a goal's scratch is shared by every work item of the goal, and a file
//! another session reads is not ours to remove.

use crate::types::SkillPayload;
use bisa_core::skill::{content_hash, parts, SkillParts};
use bisa_core::HarnessCaps;
use std::path::Path;

/// The tool a session with the platform's servers reads a reference section with.
pub const READ_TOOL: &str = "skill_read";

/// How a skill's reference sections reach a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// The whole text, body and every section: a harness without the
    /// platform's tools has no way to read a section later.
    Full,
    /// The body, then one line naming the sections to read with `skill_read`.
    BodyAndIndex,
}

impl Delivery {
    /// What a harness can do decides: the platform's MCP servers carry the
    /// reader, so a harness that mounts them gets the index.
    pub fn for_caps(caps: HarnessCaps) -> Self {
        if caps.contains(HarnessCaps::MCP_SERVERS) {
            Delivery::BodyAndIndex
        } else {
            Delivery::Full
        }
    }
}

/// How the skills reach the session.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SkillInjection {
    /// Markdown to append to the session instructions ("" / None when every
    /// skill was materialized as files).
    pub prompt_appendix: Option<String>,
}

fn appendix_block(skill: &SkillPayload, delivery: Delivery) -> String {
    match delivery {
        Delivery::Full => format!(
            "\n\n## Skill: {}\n\n_{}_\n\n{}",
            skill.name,
            skill.description,
            skill.markdown.trim_end()
        ),
        Delivery::BodyAndIndex => {
            let split = parts(&skill.markdown);
            let mut out = format!(
                "\n\n## Skill: {}\n\n_{}_\n\n{}",
                skill.name, skill.description, split.body
            );
            if let Some(index) = split.index_line(READ_TOOL) {
                out.push_str("\n\n");
                out.push_str(&index);
            }
            out
        }
    }
}

/// Skill ids become directory names and YAML keys — keep them tame even
/// though the library validates them, because a hand-edited truth file can
/// still put anything here.
fn safe_id(id: &str) -> String {
    let s: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if s.trim_matches('-').is_empty() {
        "skill".to_string()
    } else {
        s
    }
}

/// Render a one-line YAML double-quoted scalar.
///
/// The description is free text a human wrote, and an unquoted `description:`
/// containing a colon is a YAML parse error — which would silently cost the
/// session the whole skill. Quote it, escape what YAML reserves inside quotes,
/// and flatten newlines so the scalar stays one line.
fn yaml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The front matter keys of the platform's marker — prefixed, as the Agent
/// Skills specification asks of a client's own `metadata` keys.
const MARKER_SKILL: &str = "bisa-skill";
const MARKER_HASH: &str = "bisa-hash";

/// The folder beside `SKILL.md` that holds the reference sections.
const REFERENCE_DIR: &str = "reference";

/// `SKILL.md` as the platform writes it, with `hash` in the marker's slot —
/// empty while the fingerprint is being taken, the fingerprint itself after.
fn render(skill: &SkillPayload, id: &str, split: &SkillParts<'_>, hash: &str) -> String {
    let mut out = format!(
        "---\nname: {id}\ndescription: {}\nmetadata:\n  {MARKER_SKILL}: {}\n  {MARKER_HASH}: {hash}\n---\n\n{}\n",
        yaml_string(&skill.description),
        yaml_string(&skill.id),
        split.body
    );
    if !split.sections.is_empty() {
        let files: Vec<String> = split
            .sections
            .iter()
            .map(|s| format!("{REFERENCE_DIR}/{}.md", s.slug))
            .collect();
        out.push_str(&format!(
            "\nReference sections, in `{REFERENCE_DIR}/` beside this file: {}\n",
            files.join(" · ")
        ));
    }
    out
}

/// The file with its fingerprint: the text is rendered once with the slot
/// empty, fingerprinted, and rendered again carrying it — so a reader can
/// blank the slot and check the file is still what the platform wrote.
fn skill_file(skill: &SkillPayload, id: &str, split: &SkillParts<'_>) -> String {
    let unhashed = render(skill, id, split, "");
    let hash = content_hash(&unhashed);
    render(skill, id, split, &hash)
}

/// The platform's marker in an existing file, read without a YAML parser:
/// inside the opening fence, the `bisa-skill:` and `bisa-hash:` lines. `None`
/// for a file that carries none — a person's.
fn platform_marker(existing: &str) -> Option<(String, String)> {
    let mut lines = existing.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut id = None;
    let mut hash = None;
    for line in lines {
        if line == "---" {
            break;
        }
        let text = line.trim();
        if let Some(v) = text
            .strip_prefix(MARKER_SKILL)
            .and_then(|r| r.strip_prefix(':'))
        {
            id = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = text
            .strip_prefix(MARKER_HASH)
            .and_then(|r| r.strip_prefix(':'))
        {
            hash = Some(v.trim().to_string());
        }
    }
    Some((id?, hash?))
}

/// Whether an existing file is one the platform wrote and nobody touched:
/// its marker's fingerprint is the fingerprint of the file with that slot
/// blanked. An edited file fails the check, and the person's edit stands.
fn untouched_platform_file(existing: &str, hash: &str) -> bool {
    let slot = format!("  {MARKER_HASH}: {hash}");
    let blanked = existing.replacen(&slot, &format!("  {MARKER_HASH}: "), 1);
    blanked != existing && content_hash(&blanked) == hash
}

/// Write a file whole: into a sibling first, then into place, so a session
/// reading the folder meanwhile sees the old file or the new one, never half.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Write the reference files a skill's sections make — those missing or
/// stale; a file already carrying the section's text is left as it is.
fn write_references(skill_dir: &Path, split: &SkillParts<'_>) -> std::io::Result<()> {
    if split.sections.is_empty() {
        return Ok(());
    }
    let dir = skill_dir.join(REFERENCE_DIR);
    std::fs::create_dir_all(&dir)?;
    for section in &split.sections {
        let path = dir.join(format!("{}.md", section.slug));
        let text = format!("# {}\n\n{}\n", section.title, section.markdown);
        if std::fs::read_to_string(&path).is_ok_and(|existing| existing == text) {
            continue;
        }
        write_whole(&path, &text)?;
    }
    Ok(())
}

/// What an existing `SKILL.md` under a skill's name means for this launch.
enum Standing {
    /// Ours and current: nothing to write.
    Current,
    /// Nothing there, or ours and stale: write the folder.
    Write,
    /// A person's file, or ours edited since: left alone, the prompt carries the skill.
    Theirs,
}

fn standing(path: &Path, rendered: &str) -> Standing {
    match std::fs::read_to_string(path) {
        Ok(existing) if existing == rendered => Standing::Current,
        Ok(existing) => match platform_marker(&existing) {
            Some((_, hash)) if untouched_platform_file(&existing, &hash) => Standing::Write,
            _ => Standing::Theirs,
        },
        Err(_) => Standing::Write,
    }
}

/// Deliver every skill in the prompt and write nothing.
///
/// The rule for an adopted root: the platform never puts its own
/// scratch into a folder somebody else owns, and a `.claude/skills/` tree is
/// exactly that. The appendix costs a harness its native skill loading and
/// buys the owner a repository that is still theirs.
pub fn as_appendix(skills: &[SkillPayload], delivery: Delivery) -> SkillInjection {
    if skills.is_empty() {
        return SkillInjection::default();
    }
    let appendix: String = skills.iter().map(|s| appendix_block(s, delivery)).collect();
    SkillInjection {
        prompt_appendix: Some(appendix),
    }
}

/// Materialize `skills` for a session running in `session_cwd` under
/// `harness_id`. Filesystem failures degrade to the prompt appendix — skills
/// must never break a launch.
pub fn materialize(
    skills: &[SkillPayload],
    session_cwd: &Path,
    harness_id: &str,
    delivery: Delivery,
) -> SkillInjection {
    if skills.is_empty() {
        return SkillInjection::default();
    }
    let mut appendix = String::new();

    let native_dir = if harness_id == "claude-code" {
        Some(session_cwd.join(".claude").join("skills"))
    } else {
        None
    };

    for skill in skills {
        let id = safe_id(&skill.id);
        let mut delivered_as_file = false;
        if let Some(dir) = &native_dir {
            let skill_dir = dir.join(&id);
            let path = skill_dir.join("SKILL.md");
            let split = parts(&skill.markdown);
            let rendered = skill_file(skill, &id, &split);
            match standing(&path, &rendered) {
                Standing::Current => {
                    // Already materialized, identical — a reference file gone missing is put back.
                    delivered_as_file = write_references(&skill_dir, &split).is_ok();
                }
                Standing::Write => {
                    let written = std::fs::create_dir_all(&skill_dir)
                        .and_then(|()| write_references(&skill_dir, &split))
                        .and_then(|()| write_whole(&path, &rendered));
                    match written {
                        Ok(()) => delivered_as_file = true,
                        Err(e) => {
                            tracing::warn!("skill {id:?}: cannot write {}: {e}", path.display())
                        }
                    }
                }
                Standing::Theirs => {
                    // A different file that is not ours, or ours edited since — the person's copy wins.
                    tracing::warn!(
                        "skill {id:?}: {} exists with content that is not the platform's — \
                         leaving it, delivering via prompt",
                        path.display() // LCOV_EXCL_LINE: a tracing event's field, evaluated under a subscriber only
                    );
                }
            }
        }
        if !delivered_as_file {
            appendix.push_str(&appendix_block(skill, delivery));
        }
    }

    SkillInjection {
        prompt_appendix: (!appendix.is_empty()).then_some(appendix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(id: &str, description: &str) -> SkillPayload {
        SkillPayload {
            id: id.to_string(),
            name: "Workstream workflow".to_string(),
            description: description.to_string(),
            markdown: "# Workstream workflow\n\n1. Open the workstream.\n".to_string(),
        }
    }

    /// A skill with a body and two reference sections.
    fn with_sections() -> SkillPayload {
        SkillPayload {
            id: "ui-ux-design".to_string(),
            name: "UI/UX Design".to_string(),
            description: "Use when work changes a screen.".to_string(),
            markdown: "# UI/UX Design\n\nLook first.\n\n## Reference: Craft floor\n\nContrast 4.5:1.\n\n## Reference: Modes in depth\n\nOperate · Persuade.\n".to_string(),
        }
    }

    #[test]
    fn claude_code_gets_files_and_no_appendix() {
        let dir = tempfile::tempdir().unwrap();
        let skills = vec![skill(
            "workstream-workflow",
            "Use this when you touch code.",
        )];
        let inj = materialize(&skills, dir.path(), "claude-code", Delivery::BodyAndIndex);
        assert_eq!(inj.prompt_appendix, None);
        let written = std::fs::read_to_string(
            dir.path()
                .join(".claude/skills/workstream-workflow/SKILL.md"),
        )
        .unwrap();
        assert!(written.contains("name: workstream-workflow"), "{written}");
        assert!(
            written.contains(r#"description: "Use this when you touch code.""#),
            "{written}"
        );
        assert!(
            written.contains("metadata:\n  bisa-skill: \"workstream-workflow\"\n  bisa-hash: "),
            "the platform's marker rides the front matter's metadata: {written}"
        );

        // Idempotent: a second launch in the same cwd rewrites nothing.
        let again = materialize(&skills, dir.path(), "claude-code", Delivery::BodyAndIndex);
        assert_eq!(again.prompt_appendix, None);
    }

    /// The library's description is what ships, not a line scraped out of the
    /// markdown — and a colon in it must not become a YAML parse error.
    #[test]
    fn an_appendix_delivery_writes_nothing_and_carries_every_skill() {
        let dir = tempfile::tempdir().unwrap();
        let skills = vec![
            skill("workstream-workflow", "Use this when you touch code."),
            skill("release-checklist", "Use this when cutting a release."),
        ];
        let inj = as_appendix(&skills, Delivery::Full);
        let appendix = inj.prompt_appendix.expect("an appendix");
        assert_eq!(appendix.matches("## Skill: ").count(), 2, "{appendix}");
        assert!(
            !dir.path().join(".claude").exists(),
            "an adopted root must not gain a .claude/ directory"
        );
        assert_eq!(as_appendix(&[], Delivery::Full), SkillInjection::default());
    }

    #[test]
    fn a_description_with_a_colon_stays_valid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let skills = vec![skill(
            "release-checklist",
            "Use this when cutting a release: tag, changelog, rollback.",
        )];
        materialize(&skills, dir.path(), "claude-code", Delivery::BodyAndIndex);
        let written =
            std::fs::read_to_string(dir.path().join(".claude/skills/release-checklist/SKILL.md"))
                .unwrap();
        // The whole point of quoting: the colon stays inside the scalar
        // instead of splitting the line into a nested mapping.
        assert!(
            written.lines().any(|l| l
                == r#"description: "Use this when cutting a release: tag, changelog, rollback.""#),
            "{written}"
        );
    }

    #[test]
    fn other_harnesses_get_an_appendix_carrying_the_description() {
        let dir = tempfile::tempdir().unwrap();
        let inj = materialize(
            &[skill("test-strategy", "Use this before you write tests.")],
            dir.path(),
            "codex",
            Delivery::BodyAndIndex,
        );
        let appendix = inj.prompt_appendix.expect("appendix");
        assert!(
            appendix.contains("## Skill: Workstream workflow"),
            "{appendix}"
        );
        assert!(
            appendix.contains("Use this before you write tests."),
            "{appendix}"
        );
        assert!(!dir.path().join(".claude").exists());
    }

    /// A skill id becomes a directory name: anything that is no word
    /// character is a dash, and an id of nothing but dashes is *skill*.
    #[test]
    fn a_skill_id_is_tamed_into_a_directory_name() {
        assert_eq!(safe_id("release checklist/v2"), "release-checklist-v2");
        assert_eq!(safe_id("release_checklist-2"), "release_checklist-2");
        assert_eq!(safe_id("///"), "skill");
        assert_eq!(safe_id(""), "skill");
    }

    /// The description is one quoted YAML scalar whatever it carries: a
    /// quote and a backslash escaped, a line break flattened to a space.
    #[test]
    fn a_description_is_one_quoted_scalar_whatever_it_carries() {
        assert_eq!(
            yaml_string("say \"hi\" C:\\\\x\nnext\ttab\r"),
            "\"say \\\"hi\\\" C:\\\\\\\\x next tab \""
        );
        let dir = tempfile::tempdir().unwrap();
        materialize(
            &[skill("quoted", "say \"hi\"\nand more")],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        let written =
            std::fs::read_to_string(dir.path().join(".claude/skills/quoted/SKILL.md")).unwrap();
        assert!(
            written
                .lines()
                .any(|l| l == r#"description: "say \"hi\" and more""#),
            "{written}"
        );
    }

    /// A file of the person's own under the skill's name is left alone and
    /// the skill rides the prompt; a path that cannot be written — a file
    /// where the skills directory should be — does the same.
    #[test]
    fn the_persons_own_file_wins_and_an_unwritable_path_rides_the_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let theirs = dir.path().join(".claude/skills/release-checklist");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("SKILL.md"), "# mine\n").unwrap();
        let inj = materialize(
            &[skill(
                "release-checklist",
                "Use this when cutting a release.",
            )],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        let appendix = inj.prompt_appendix.expect("delivered via the prompt");
        assert!(
            appendix.contains("## Skill: Workstream workflow"),
            "{appendix}"
        );
        assert_eq!(
            std::fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
            "# mine\n",
            "the person's copy stands"
        );

        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(dir.path().join(".claude/skills"), "not a directory").unwrap();
        let inj = materialize(
            &[skill(
                "release-checklist",
                "Use this when cutting a release.",
            )],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        assert!(
            inj.prompt_appendix.is_some(),
            "nothing could be written, so the prompt carries it"
        );
    }

    #[test]
    fn no_skills_means_no_injection() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            materialize(&[], dir.path(), "claude-code", Delivery::Full),
            SkillInjection::default()
        );
    }

    #[test]
    fn the_delivery_follows_the_harnesss_tools() {
        assert_eq!(
            Delivery::for_caps(HarnessCaps::MCP_SERVERS),
            Delivery::BodyAndIndex
        );
        assert_eq!(
            Delivery::for_caps(HarnessCaps::MCP_SERVERS | HarnessCaps::FOLLOW_UP),
            Delivery::BodyAndIndex
        );
        assert_eq!(Delivery::for_caps(HarnessCaps::FOLLOW_UP), Delivery::Full);
        assert_eq!(Delivery::for_caps(HarnessCaps::empty()), Delivery::Full);
    }

    #[test]
    fn a_harness_without_platform_tools_gets_every_reference_section_in_the_appendix() {
        let appendix = as_appendix(&[with_sections()], Delivery::Full)
            .prompt_appendix
            .expect("an appendix");
        assert!(appendix.contains("## Skill: UI/UX Design"), "{appendix}");
        assert!(appendix.contains("## Reference: Craft floor"), "{appendix}");
        assert!(appendix.contains("Operate · Persuade."), "{appendix}");
        assert!(
            !appendix.contains("Reference sections, read with"),
            "nothing to read later: the text is all here"
        );
    }

    #[test]
    fn a_harness_with_platform_tools_gets_the_body_and_an_index_naming_skill_read() {
        let appendix = as_appendix(&[with_sections()], Delivery::BodyAndIndex)
            .prompt_appendix
            .expect("an appendix");
        assert!(appendix.contains("## Skill: UI/UX Design\n\n_Use when work changes a screen._\n\n# UI/UX Design\n\nLook first."), "{appendix}");
        assert!(
            appendix.ends_with(
                "Reference sections, read with skill_read: craft-floor · modes-in-depth"
            ),
            "{appendix}"
        );
        assert!(
            !appendix.contains("Contrast 4.5:1."),
            "a section is read on demand, never appended"
        );
        // The same for a harness other than Claude Code through materialize.
        let dir = tempfile::tempdir().unwrap();
        let appendix = materialize(
            &[with_sections()],
            dir.path(),
            "codex",
            Delivery::BodyAndIndex,
        )
        .prompt_appendix
        .expect("an appendix");
        assert!(appendix.contains("read with skill_read"), "{appendix}");
    }

    #[test]
    fn claude_code_gets_a_skill_file_with_the_platforms_marker_and_a_reference_file_per_section() {
        let dir = tempfile::tempdir().unwrap();
        let inj = materialize(
            &[with_sections()],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        assert_eq!(inj.prompt_appendix, None);
        let folder = dir.path().join(".claude/skills/ui-ux-design");
        let written = std::fs::read_to_string(folder.join("SKILL.md")).unwrap();
        assert!(written.starts_with("---\nname: ui-ux-design\ndescription: \"Use when work changes a screen.\"\nmetadata:\n  bisa-skill: \"ui-ux-design\"\n  bisa-hash: "), "{written}");
        assert!(
            written.contains("\n---\n\n# UI/UX Design\n\nLook first.\n"),
            "{written}"
        );
        assert!(
            written.ends_with("Reference sections, in `reference/` beside this file: reference/craft-floor.md · reference/modes-in-depth.md\n"),
            "{written}"
        );
        assert!(
            !written.contains("Contrast 4.5:1."),
            "a section lives in its own file"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("reference/craft-floor.md")).unwrap(),
            "# Craft floor\n\nContrast 4.5:1.\n"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("reference/modes-in-depth.md")).unwrap(),
            "# Modes in depth\n\nOperate · Persuade.\n"
        );
        let (id, hash) = platform_marker(&written).expect("the marker reads back");
        assert_eq!(id, "ui-ux-design");
        assert!(
            untouched_platform_file(&written, &hash),
            "the file is what the platform wrote"
        );
        // A reference file that went missing is put back at the next launch, the folder current otherwise.
        std::fs::remove_file(folder.join("reference/craft-floor.md")).unwrap();
        let again = materialize(
            &[with_sections()],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        assert_eq!(again.prompt_appendix, None);
        assert!(folder.join("reference/craft-floor.md").exists());
    }

    #[test]
    fn a_stale_platform_copy_is_rewritten_and_an_edited_or_foreign_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(".claude/skills/ui-ux-design");
        let mut older = with_sections();
        older.markdown = "# UI/UX Design\n\nAn older procedure.\n".to_string();
        materialize(&[older], dir.path(), "claude-code", Delivery::BodyAndIndex);
        let stale = std::fs::read_to_string(folder.join("SKILL.md")).unwrap();
        assert!(stale.contains("An older procedure."));
        // The skill changed: the platform's own stale copy is rewritten, sections and all.
        let inj = materialize(
            &[with_sections()],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        assert_eq!(
            inj.prompt_appendix, None,
            "rewritten, not pushed into the prompt"
        );
        let fresh = std::fs::read_to_string(folder.join("SKILL.md")).unwrap();
        assert!(
            fresh.contains("Look first.") && !fresh.contains("An older procedure."),
            "{fresh}"
        );
        assert!(folder.join("reference/craft-floor.md").exists());
        // A person edited our file: the marker is there but the fingerprint no longer fits — theirs now.
        let edited = fresh.replace("Look first.", "Look first, then ask me.");
        std::fs::write(folder.join("SKILL.md"), &edited).unwrap();
        let mut newer = with_sections();
        newer.markdown.push_str("\nA newer line.\n");
        let inj = materialize(&[newer], dir.path(), "claude-code", Delivery::BodyAndIndex);
        assert!(
            inj.prompt_appendix.is_some(),
            "the edit stands and the skill rides the prompt"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            edited
        );
        // A file with no marker at all is a person's, as ever.
        std::fs::write(
            folder.join("SKILL.md"),
            "---\nname: ui-ux-design\n---\n# mine\n",
        )
        .unwrap();
        let inj = materialize(
            &[with_sections()],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        assert!(inj.prompt_appendix.is_some());
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            "---\nname: ui-ux-design\n---\n# mine\n"
        );
    }

    #[test]
    fn the_platform_marker_is_read_without_a_yaml_parser() {
        assert_eq!(
            platform_marker(
                "---\nname: x\nmetadata:\n  bisa-skill: \"x\"\n  bisa-hash: abcd\n---\n\nbody\n"
            ),
            Some(("x".to_string(), "abcd".to_string()))
        );
        assert_eq!(platform_marker("# no front matter\n"), None);
        assert_eq!(
            platform_marker("---\nname: x\n---\n"),
            None,
            "a fence with no marker is nobody's claim"
        );
        assert_eq!(
            platform_marker(
                "---\nname: x\nmetadata:\n  bisa-skill: \"x\"\n---\n\nbisa-hash: later\n"
            ),
            None,
            "a key after the fence is text, not the marker"
        );
        assert!(
            !untouched_platform_file("---\nmetadata:\n  bisa-hash: zzzz\n---\n", "zzzz"),
            "a fingerprint that does not fit"
        );
        assert!(
            !untouched_platform_file("# mine\n", "zzzz"),
            "no slot to blank"
        );
    }

    #[test]
    fn a_skill_with_no_reference_section_has_no_index_and_no_reference_folder() {
        let dir = tempfile::tempdir().unwrap();
        materialize(
            &[skill("plain", "Use this.")],
            dir.path(),
            "claude-code",
            Delivery::BodyAndIndex,
        );
        let folder = dir.path().join(".claude/skills/plain");
        let written = std::fs::read_to_string(folder.join("SKILL.md")).unwrap();
        assert!(!written.contains("Reference sections"), "{written}");
        assert!(!folder.join("reference").exists());
        let appendix = as_appendix(&[skill("plain", "Use this.")], Delivery::BodyAndIndex)
            .prompt_appendix
            .unwrap();
        assert!(!appendix.contains("Reference sections"), "{appendix}");
        assert_eq!(
            appendix,
            as_appendix(&[skill("plain", "Use this.")], Delivery::Full)
                .prompt_appendix
                .unwrap(),
            "no sections: both deliveries are the same text"
        );
    }
}
