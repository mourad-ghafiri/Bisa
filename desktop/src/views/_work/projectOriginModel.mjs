/**
 * What a project's origin says, as facts every surface
 * reads the same way: which step made it — under either the `goal` or the
 * `step` variant — whether an agent's step made it at all, and which projects
 * a goal, a step or a library workflow made. The About chip, the activity line,
 * the committer dialog, the notifications and the Goal and Workflows screens
 * all ask here rather than matching `origin.origin === "step"` by hand.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The step that made a project, or null: `{run, step, workflow}` flat on a
 * `step` origin, nested under `step` on a `goal` origin made by the goal's
 * own design.
 * @param {object | null | undefined} origin a `ProjectOrigin`
 */
export function stepOf(origin) {
  if (!origin) return null;
  if (origin.origin === "step") return { run: origin.run, step: origin.step, workflow: origin.workflow };
  if (origin.origin === "goal" && origin.step) return { run: origin.step.run, step: origin.step.step, workflow: origin.step.workflow };
  return null;
}

/** An agent's step made it — whichever tab it sits in. */
export function madeByAgentStep(origin) {
  return stepOf(origin) !== null;
}

/** Born of this goal — by a person, an agent, or a step of the goal's own design. */
export function madeByGoal(origin, goal) {
  return !!origin && origin.origin === "goal" && origin.goal === goal;
}

/**
 * The words for where it was born, for a sentence: `" by a workflow step"`,
 * `" by a step of its goal's design"`, `" from its goal"`, or nothing.
 */
export function bornWords(origin) {
  if (!origin) return "";
  if (origin.origin === "step") return ` ${t("work-goal-inspector-workflow-step")}`;
  if (origin.origin === "goal") return origin.step ? ` ${t("work-project-origin-step-goal-s-design")}` : ` ${t("work-git-identity-from-goal")}`;
  return "";
}

/**
 * The sentence for a project a library workflow's step made: on a goal's
 * run, or — with no goal at all — in a run in the workspace.
 * @param {{ step: string, goal?: string | null }} origin a `step` origin
 * @param {(goal: string) => string} goalTitle the goal's label
 */
export function madeByStepWords(origin, goalTitle) {
  return origin.goal
    ? t("work-project-origin-chip-created-step-run", { step: origin.step, goal: goalTitle(origin.goal) })
    : t("work-project-origin-chip-created-step-workspace-run", { step: origin.step });
}

/**
 * The projects one step of one run made — a goal's run or a run of the
 * workspace, the design's step or a library workflow's: the step names its
 * run in either variant, so the same step id in another run is another
 * project.
 */
export function projectsMadeByStep(projects, run, stepId) {
  return (projects ?? []).filter((p) => {
    const by = stepOf(p.project?.origin ?? p.origin);
    return !!by && by.run === run && by.step === stepId;
  });
}

/** The projects a library workflow's steps made — what its card counts; a design's are its goal's. */
export function projectsMadeByWorkflow(projects, workflow) {
  return (projects ?? []).filter((p) => {
    const o = p.project?.origin ?? p.origin;
    return !!o && o.origin === "step" && o.workflow === workflow;
  });
}
