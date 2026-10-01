/**
 * Which settings a project may hold for itself, and where in the Project IDE
 * each is edited (ide/13): Settings is the workspace's and this machine's
 * screen and never resolves for a project, so every key whose `ScopeSet`
 * admits `Project` has one home here — by what it is about. Pure, so
 * `node --test` reads it; the test holds these lists equal to the registry.
 */

import { t } from "../../i18n/l10n.mjs";

/** About › Settings — *Git*: how this repository's branch moves and merges. */
export const PROJECT_GIT_KEYS = Object.freeze(["git.default_branch", "git.merge_strategy", "git.pull", "git.delete_branch_after_merge"]);

/** About › Settings — *Workstreams*: how this project's checkouts are made and cleaned up (the scripts are their own card). */
export const PROJECT_WORKSTREAM_KEYS = Object.freeze(["workstreams.cleanup", "workstreams.after_merge"]);

/** The Browser & devices card: where a tab opens, what an agent may do in one, and who may drive a device (ide/18, ide/19). */
export const PROJECT_BROWSER_KEYS = Object.freeze(["browser.home", "browser.agents", "browser.agents.reach", "browser.agents.headless", "browser.agents.scripts", "mobile_development.agents", "draw.agents"]);

/** About — *Editor & terminal*: how files in this project are edited and which harness a terminal opens with. */
export const PROJECT_EDITOR_KEYS = Object.freeze(["editor.tab_size", "editor.insert_spaces", "editor.word_wrap", "editor.format_on_save", "terminal.default_harness"]);

/**
 * About — *Agents & decisions*: the mode a conversation about one of this
 * project's checkouts starts in (ide/20), how hard a model works here when
 * nobody said otherwise (06 §Effort), and whether — and where, and how sure
 * — the Decision-Making Agent decides for this project (15).
 */
export const PROJECT_AGENT_KEYS = Object.freeze(["agents.conversation.mode", "agents.effort", "decisions.enabled", "decisions.points_off", "decisions.confidence.act"]);

/**
 * Project-scope keys with a hand-written editor of their own: the agent pane's
 * addressee chip writes `agents.default`, and the Workstream scripts card
 * writes the four script texts and the timeout of the three the engine runs.
 */
export const PROJECT_KEYS_ELSEWHERE = Object.freeze([
  "agents.default",
  "workstreams.script.pre_create",
  "workstreams.script.post_create",
  "workstreams.script.clean",
  "workstreams.script.run",
  "workstreams.script.timeout_secs",
]);

/**
 * Where a project card's value comes from, in words.
 * @param {"default" | "machine" | "workspace" | "project"} origin
 */
export function originWords(origin) {
  switch (origin) {
    case "project":
      return t("work-project-settings-set-project");
    case "workspace":
      return t("work-project-settings-workspace-s-value");
    case "machine":
      return t("work-project-settings-machine-s-value");
    default:
      return t("work-project-settings-default");
  }
}

/**
 * The card's row facts: the resolved value, where it comes from, and whether
 * *Inherit* has anything to clear.
 * @param {{ key: string; value: unknown; origin: "default" | "machine" | "workspace" | "project" }} resolved
 */
export function projectRow(resolved) {
  return {
    key: resolved.key,
    value: resolved.value,
    origin: originWords(resolved.origin),
    own: resolved.origin === "project",
  };
}
