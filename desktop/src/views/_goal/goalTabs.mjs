/**
 * The goal page's tabs: Progress first — the run read downward —
 * then the conversation, then the canvas. `?tab=` carries the choice; an
 * unknown or absent value is Progress, so a stale link never lands nowhere.
 */

import { t } from "../../i18n/l10n.mjs";
export const GOAL_TABS = Object.freeze(["progress", "conversation", "workflow"]);
export const DEFAULT_TAB = "progress";
export const GOAL_TAB_LABEL = Object.freeze({ progress: t("goal-goal-tabs-progress"), conversation: t("goal-goal-tabs-conversation"), workflow: t("goal-goal-tabs-workflow") });

export function tabOf(param) {
  return typeof param === "string" && GOAL_TABS.includes(param) ? param : DEFAULT_TAB;
}
