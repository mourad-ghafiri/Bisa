/**
 * The goal's mode — the core's `GoalMode` in words, so the capture dialog,
 * the chips and the Settings panel all say the same thing.
 *
 * - **auto**: the Workflow Agent designs the workflow, and the platform
 *   adopts it, starts the run, repairs a failed run and restarts by itself;
 *   the run is unattended, so a step that may write runs commands too
 *   (`goals.auto.ceiling`) and a permission above a step's ceiling is the
 *   classifier's to read (`goals.auto.permissions`). Only what a person
 *   alone can do waits for one: a step the workflow hands to a person, a
 *   guard rule that asks or refuses, a call the classifier finds harmful, a
 *   gated publish, a question the agent still asks.
 * - **guided**: the agent proposes; the person adopts, asks for changes or
 *   declines, and approves every amendment.
 * - **manual**: the person designs on the goal's Workflow tab, with the
 *   agent on request.
 *
 * Plain `.mjs` with a `.d.mts` beside it; the test reads the Rust enum.
 */

import { t } from "../../i18n/l10n.mjs";

/** Every mode, in the order a dialog offers them — the core's `GoalMode::ALL`. */
export const GOAL_MODES = Object.freeze(["auto", "guided", "manual"]);

export const MODE_LABEL = Object.freeze({ auto: t("goal-goal-mode-auto"), guided: t("goal-goal-mode-guided"), manual: t("goal-goal-mode-manual") });

/** One sentence each: what happens after the capture. */
export const MODE_MEANING = Object.freeze({
  auto: t("goal-goal-mode-workflow-agent-designs-workflow-platform-adopts"),
  guided: t("goal-goal-mode-workflow-agent-designs-proposes-adopt-ask"),
  manual: t("goal-goal-mode-design-workflow-goal-s-workflow-tab"),
});

/** The kit glyph each mode wears, by name in `ICON`. */
export const MODE_ICON = Object.freeze({ auto: "run", guided: "coreAgent", manual: "person" });

/** The settings key the capture dialog and the CLI read their default from. */
export const DEFAULT_MODE_KEY = "goals.default_mode";
/** The settings key saying what an auto goal does above a step's ceiling: the classifier reads it, or you are asked. */
export const AUTO_PERMISSIONS_KEY = "goals.auto.permissions";
/** Its two answers, in the control's order, and the core's default. */
export const AUTO_PERMISSIONS = Object.freeze(["classify", "ask"]);
export const AUTO_PERMISSIONS_LABEL = Object.freeze({ classify: t("goal-goal-mode-classifier-reads"), ask: t("goal-goal-mode-asked") });
export const AUTO_PERMISSIONS_MEANING = Object.freeze({
  classify: t("goal-goal-mode-tool-above-step-s-ceiling-no"),
  ask: t("goal-goal-mode-tool-above-step-s-ceiling-no-2"),
});
export const DEFAULT_AUTO_PERMISSIONS = "classify";
/** The settings key saying the ceiling an auto goal's steps run under: a `write` step runs commands too, or each step's own stands. */
export const AUTO_CEILING_KEY = "goals.auto.ceiling";
/** Its two answers, in the control's order, and the core's default. */
export const AUTO_CEILING = Object.freeze(["exec", "step"]);
export const AUTO_CEILING_LABEL = Object.freeze({ exec: t("goal-goal-mode-runs-commands"), step: t("goal-goal-mode-step-s-own") });
export const AUTO_CEILING_MEANING = Object.freeze({
  exec: t("goal-goal-mode-step-may-change-files-may-also-run"),
  step: t("goal-goal-mode-every-step-keeps-ceiling-its-design"),
});
export const DEFAULT_AUTO_CEILING = "exec";
/** The core's default, for a workspace that says nothing. */
export const DEFAULT_MODE = "auto";

/** The Workflow Agent designs the workflow at capture — the core's `GoalMode::designs`. */
export function designs(mode) {
  return mode === "auto" || mode === "guided";
}

/** The mode a goal row carries, or the core's default for a shape that names none. */
export function modeOf(goal) {
  const m = goal && typeof goal === "object" ? goal.mode : undefined;
  return GOAL_MODES.includes(m) ? m : DEFAULT_MODE;
}

/** The segments a `SegmentedControl` draws, in order. */
export function modeSegments() {
  return GOAL_MODES.map((id) => ({ id, label: MODE_LABEL[id], icon: MODE_ICON[id] }));
}

/** The dialog's description under the statement, by mode. */
export function captureHint(mode) {
  switch (mode) {
    case "guided":
      return t("goal-goal-mode-say-plainly-workflow-agent-reads-asks");
    case "manual":
      return t("goal-goal-mode-say-plainly-then-design-how-runs");
    default:
      return t("goal-goal-mode-say-plainly-workflow-agent-designs-how");
  }
}

/**
 * Where the dialog lands after the capture: the goal's page, and for a
 * manual goal its Workflow tab with the designer open.
 */
export function afterCapture(mode) {
  return mode === "manual" ? { tab: "workflow", edit: "1" } : null;
}

/** What the toast says once the goal landed. */
export function captureToast(mode) {
  switch (mode) {
    case "guided":
      return t("goal-goal-mode-captured-workflow-agent-designing-how-runs");
    case "manual":
      return t("goal-goal-mode-captured-design-how-runs-workflow-tab");
    default:
      return t("goal-goal-mode-captured-workflow-agent-designing-how-runs-2");
  }
}
