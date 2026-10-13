//! Agent skill materialization.
//!
//! A skill is a named markdown procedure from the workspace's skill library,
//! attached to an Agent definition by id. At session launch it is delivered
//! the way the harness understands best:
//! - **claude-code**: written to `<cwd>/.claude/skills/<id>/SKILL.md`
//!   (Claude Code's native skills dir, copied on launch). No prompt appendix needed.
//! - **every other harness**: returned as a markdown appendix the caller
//!   appends to the session instructions.
//!
//! A pre-existing skill file with *different* content is never overwritten —
//! the user's copy wins and the skill falls back to the prompt appendix.

use crate::types::SkillPayload;
use std::path::Path;

/// How the skills reach the session.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SkillInjection {
    /// Markdown to append to the session instructions ("" / None when every
    /// skill was materialized as files).
    pub prompt_appendix: Option<String>,
}

fn appendix_block(skill: &SkillPayload) -> String {
    format!(
        "\n\n## Skill: {}\n\n_{}_\n\n{}",
        skill.name, skill.description, skill.markdown
    )
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

/// Materialize `skills` for a session running in `session_cwd` under
/// `harness_id`. Filesystem failures degrade to the prompt appendix — skills
/// must never break a launch.
/// Deliver every skill in the prompt and write nothing.
///
/// The rule for an adopted root: the platform never puts its own
/// scratch into a folder somebody else owns, and a `.claude/skills/` tree is
/// exactly that. The appendix costs a harness its native skill loading and
/// buys the owner a repository that is still theirs.
pub fn as_appendix(skills: &[SkillPayload]) -> SkillInjection {
    if skills.is_empty() {
        return SkillInjection::default();
    }
    let appendix: String = skills.iter().map(appendix_block).collect();
    SkillInjection {
        prompt_appendix: Some(appendix),
    }
}

pub fn materialize(
    skills: &[SkillPayload],
    session_cwd: &Path,
    harness_id: &str,
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
            let body = format!(
                "---\nname: {id}\ndescription: {}\n---\n\n{}\n",
                yaml_string(&skill.description),
                skill.markdown
            );
            match std::fs::read_to_string(&path) {
                Ok(existing) if existing == body => {
                    delivered_as_file = true; // already materialized, identical
                }
                Ok(_) => {
                    // A different file exists — the user's copy wins.
                    tracing::warn!(
                        "skill {id:?}: {} exists with different content — \
                         leaving it, delivering via prompt",
                        path.display() // LCOV_EXCL_LINE: a tracing event's field, evaluated under a subscriber only
                    );
                }
                Err(_) => {
                    if std::fs::create_dir_all(&skill_dir).is_ok()
                        && std::fs::write(&path, &body).is_ok()
                    {
                        delivered_as_file = true;
                    } else {
                        tracing::warn!("skill {id:?}: cannot write {}", path.display());
                    }
                }
            }
        }
        if !delivered_as_file {
            appendix.push_str(&appendix_block(skill));
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

    #[test]
    fn claude_code_gets_files_and_no_appendix() {
        let dir = tempfile::tempdir().unwrap();
        let skills = vec![skill(
            "workstream-workflow",
            "Use this when you touch code.",
        )];
        let inj = materialize(&skills, dir.path(), "claude-code");
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

        // Idempotent: a second launch in the same cwd rewrites nothing.
        let again = materialize(&skills, dir.path(), "claude-code");
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
        let inj = as_appendix(&skills);
        let appendix = inj.prompt_appendix.expect("an appendix");
        assert_eq!(appendix.matches("## Skill: ").count(), 2, "{appendix}");
        assert!(
            !dir.path().join(".claude").exists(),
            "an adopted root must not gain a .claude/ directory"
        );
        assert_eq!(as_appendix(&[]), SkillInjection::default());
    }

    #[test]
    fn a_description_with_a_colon_stays_valid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let skills = vec![skill(
            "release-checklist",
            "Use this when cutting a release: tag, changelog, rollback.",
        )];
        materialize(&skills, dir.path(), "claude-code");
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
            materialize(&[], dir.path(), "claude-code"),
            SkillInjection::default()
        );
    }
}
