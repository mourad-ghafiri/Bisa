/**
 * A conversation's mode — the core's `ConversationMode` in words, so the
 * composer's control, the plan banner and the CLI's words all say the same
 * thing. Only a conversation about a checkout (origin `project` or
 * `workstream` — "a conversation about a checkout") carries one; every other
 * kind of conversation shows no control at all.
 *
 * - **manual**: reads and file edits run; a file edit lands on disk pending
 *   and stays until Keep/Undo; a command a guard rule does not decide is
 *   asked in the conversation.
 * - **auto**: the same tool ceiling as manual for reads and file edits, but
 *   commands run under the guard and the classifier as before, and a pending
 *   change is kept automatically when the next turn begins — still
 *   restorable until then.
 * - **plan**: reads run; a file edit is refused with a sentence; a command is
 *   asked. The reply is the plan; *Build this plan* sets the mode back to
 *   what it was before plan and posts a hand-off message.
 *
 * Plain `.mjs` with a `.d.mts` beside it, following `_goal/goalMode.mjs`.
 */

import { t } from "../../i18n/l10n.mjs";

/** Every mode, in the order the control offers them — the core's `ConversationMode::ALL`. */
export const CONVERSATION_MODES = Object.freeze(["manual", "auto", "plan"]);

export const MODE_LABEL = Object.freeze({ manual: t("studio-conversation-mode-manual"), auto: t("studio-conversation-mode-auto"), plan: t("studio-conversation-mode-plan") });

/** One sentence each: what an edit or a command does in this mode. */
export const MODE_MEANING = Object.freeze({
  manual: t("studio-conversation-mode-file-edits-land-disk-pending-until"),
  auto: t("studio-conversation-mode-file-edits-land-same-way-but"),
  plan: t("studio-conversation-mode-nothing-written-file-edit-refused-sentence"),
});

/** The kit glyph each mode wears, by name in `ICON`. */
export const MODE_ICON = Object.freeze({ manual: "person", auto: "run", plan: "toolRead" });

/** The core's default, for a checkout conversation that names none. */
export const DEFAULT_MODE = "manual";

/** The origin kinds a mode control shows for — "a conversation about a checkout". */
export const CHECKOUT_ORIGIN_KINDS = Object.freeze(["project", "workstream"]);

/** Whether a conversation of this origin kind carries a mode at all. */
export function isCheckoutOrigin(kind) {
  return CHECKOUT_ORIGIN_KINDS.includes(kind);
}

/** The mode a conversation view carries, or the default for one that names none. */
export function modeOf(conversation) {
  const m = conversation && typeof conversation === "object" ? conversation.mode : undefined;
  return CONVERSATION_MODES.includes(m) ? m : DEFAULT_MODE;
}

/**
 * The next mode in the cycle — what the composer's chord does. An unknown
 * value reads as the first mode, as `modeOf` reads it, so the step from it
 * is the step from there; and the cycle walks only the modes the control
 * offers: plan is skipped where the menu disables it (`modeChoices`),
 * since a chord must not reach what a click cannot.
 * @param {string} mode
 * @param {boolean} [toolGuard] whether the addressed agent's harness carries a tool guard
 */
export function nextMode(mode, toolGuard = true) {
  const offered = CONVERSATION_MODES.filter((m) => m !== "plan" || toolGuard);
  const i = Math.max(offered.indexOf(mode), 0);
  // Plan held on a harness that lost its guard steps on to the first mode.
  return offered.includes(mode) || !CONVERSATION_MODES.includes(mode) ? offered[(i + 1) % offered.length] : offered[0];
}

/** Why plan is unavailable, when it is. */
export const PLAN_NEEDS_GUARD_HINT = t("studio-conversation-mode-plan-needs-harness-tool-commands-guard");

/**
 * The choices a `ChoiceMenu` draws, in order: every mode with its
 * one-sentence meaning under its name, plan disabled with a hint when the
 * addressed agent's harness has no tool guard (`HarnessRow.tool_guard`) — a
 * wake into plan would be refused there, and saying so up front beats a mode
 * that silently does nothing.
 * @param {boolean} toolGuard whether the addressed agent's harness carries a tool guard
 */
export function modeChoices(toolGuard) {
  return CONVERSATION_MODES.map((id) => ({
    id,
    label: MODE_LABEL[id],
    description: MODE_MEANING[id],
    icon: MODE_ICON[id],
    disabled: id === "plan" && !toolGuard,
    hint: id === "plan" && !toolGuard ? PLAN_NEEDS_GUARD_HINT : undefined,
  }));
}

/** What the plan banner offers once a turn has landed while the mode is `plan`. */
export function planBannerWords() {
  return { build: t("studio-conversation-mode-build-plan"), refine: t("studio-conversation-mode-refine") };
}

/** The message a plan build hands off with. */
export const BUILD_MESSAGE = t("studio-conversation-mode-build-plan-2");
