//! What every agent is told about the UI/UX Design skill (06 — Agents and
//! teams § Skills), in one place: the id of the skill a workspace holds its
//! agents to, the sentence a session reads when it does, and the hint the
//! `bisa` MCP server's instructions carry whatever the workspace says. The
//! policy itself (`design.agents`) is the engine's to read; the words are
//! here because the engine and the MCP server both depend on this crate and
//! on nothing of each other.

/// The catalog skill a workspace holds its agents to for what a person sees
/// or does on a screen (`design.agents`).
pub const DESIGN_SKILL: &str = "ui-ux-design";

/// The sentence a session reads when the workspace follows the skill and
/// this agent does not carry it: read it with the tool before the work.
pub const DESIGN_NOTE: &str = "UI/UX: this workspace follows the UI/UX Design skill whenever work \
changes what a person sees or does on a screen — read it with skill_read (the skill is \
ui-ux-design) before that work, and its reference sections as the skill says; then look at the \
result in the embedded browser the way the skill does, never from the code alone.";

/// The same, for a harness with no platform tools to read the skill with:
/// the whole skill is appended to the prompt under its own heading.
pub const DESIGN_NOTE_APPENDED: &str = "UI/UX: this workspace follows the UI/UX Design skill \
whenever work changes what a person sees or does on a screen — it is appended to this prompt \
under its own heading; follow it before that work, and look at the result in the embedded \
browser the way it says.";

/// The hint the MCP server's instructions carry whatever the workspace
/// says: the tool exists; the prompt says whether the skill is followed.
pub const DESIGN_HINT: &str = "Skills: skill_read reads an installed skill by id — its body and \
the names of its reference sections, or one section — the on-demand half of a skill your prompt \
carries by name; a workspace that follows the UI/UX Design skill for what a person sees or does \
on a screen says so in your prompt (Settings › Capabilities › Design).";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_names_the_tool_and_the_skill_and_the_appended_one_says_where_it_is() {
        assert!(DESIGN_NOTE.contains("skill_read"));
        assert!(DESIGN_NOTE.contains(DESIGN_SKILL));
        assert!(
            DESIGN_NOTE.contains("embedded browser"),
            "the skill's evidence is the page, not the code"
        );
        assert!(
            !DESIGN_NOTE_APPENDED.contains("skill_read"),
            "a harness with no tools is not told to use one"
        );
        assert!(DESIGN_NOTE_APPENDED.contains("appended to this prompt"));
        assert!(
            DESIGN_HINT.contains("skill_read")
                && DESIGN_HINT.contains("Settings › Capabilities › Design")
        );
        for note in [DESIGN_NOTE, DESIGN_NOTE_APPENDED, DESIGN_HINT] {
            assert!(!note.contains('\n'), "one paragraph each");
        }
    }
}
