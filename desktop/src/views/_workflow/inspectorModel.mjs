/**
 * What the right panel draws over a workflow, as facts (03-workflows §The
 * designer): one view per selection — the **workflow** itself when nothing
 * is picked, a **step**, a **flow**, or **several** — each with its title,
 * the sections it stacks in order, why it is read-only when it is, the
 * problems that belong to it, and, in a run, the step's record and whose
 * move it is. Both panels — the designer's Properties pane and the goal's
 * Workflow tab column — ask this one model, so they agree on every one of
 * those facts and the component only draws.
 *
 * **Read-only has one reason.** The hosts know why nothing may be edited —
 * a closed goal, a run going, goals running the library copy, a finished
 * run being read, a proposal not adopted, a design saved but not yet
 * opened for editing, a library workflow a goal points at — and hand the
 * facts in; the first that holds, in that order, is the reason the panel
 * says, with the door that answers it when there is one. The amendment's
 * finer grain — a step that has started while the rest may change — stays
 * per step (`stepStarted`).
 *
 * Facts only, no React. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { progressRows } from "../_goal/progressModel.mjs";
import { stepActions, yours } from "./runView.mjs";
import { flowFacts } from "./selectionModel.mjs";
import { kindLabel } from "./stepKinds.mjs";

/** The sections of each view, in the order they stack; the optional ones are decided per view. */
const STEP_SECTIONS = Object.freeze(["run", "identity", "kind", "flow", "problems", "danger"]);
const WORKFLOW_SECTIONS = Object.freeze(["identity", "runs", "inputs", "problems"]);

/** A door the read-only note offers: to the goal's editor, or to the library's designer. */
const EDIT = Object.freeze({ act: "edit" });
const DESIGNER = Object.freeze({ act: "designer" });

/**
 * Why the panel is read-only — the first reason that holds — with its door,
 * or `null` while it may be edited.
 * @param {boolean} readOnly
 * @param {{closed?: boolean, runLive?: boolean, runningIn?: number, runFinished?: boolean, proposed?: boolean, ownDesign?: boolean, library?: boolean} | null | undefined} why
 */
export function readOnlyReason(readOnly, why) {
  if (!readOnly) return null;
  const w = why ?? {};
  if (w.closed) return { reason: t("workflow-inspector-read-only-closed"), door: null };
  if (w.runLive) return { reason: t("workflow-inspector-read-only-run-live"), door: null };
  if ((w.runningIn ?? 0) > 0) return { reason: t("workflow-inspector-read-only-running-in", { n: w.runningIn }), door: null };
  if (w.runFinished) return { reason: t("workflow-inspector-read-only-run-finished"), door: null };
  if (w.proposed) return { reason: t("workflow-inspector-read-only-proposed"), door: { ...EDIT, label: t("workflow-goal-workflow-tab-edit-steps") } };
  if (w.ownDesign) return { reason: t("workflow-inspector-read-only-own-design"), door: { ...EDIT, label: t("workflow-goal-workflow-tab-edit-steps") } };
  if (w.library) return { reason: t("workflow-inspector-read-only-library"), door: { ...DESIGNER, label: t("workflow-inspector-door-open-designer") } };
  return { reason: t("workflow-inspector-read-only-plain"), door: null };
}

/** Whether a step's fields are disabled: the whole panel read-only, or the step one an amendment may not touch. */
function frozenStep(edit, id) {
  return !!edit?.readOnly || (edit?.editable != null && !edit.editable.has(id));
}

/**
 * The view.
 * @param {{
 *   selection: unknown,
 *   workflow: {name?: string, steps?: readonly object[]},
 *   problems?: readonly {step?: string | null}[],
 *   unreadable?: string | null,
 *   edit?: {readOnly?: boolean, editable?: Set<string> | null} | null,
 *   why?: object | null,
 *   run?: {steps?: Record<string, object>} | null,
 * }} facts
 */
export function inspectorView({ selection, workflow, problems = [], unreadable = null, edit = null, why = null, run = null }) {
  const readOnly = readOnlyReason(!!edit?.readOnly, why);
  const steps = workflow?.steps ?? [];
  const canRun = problems.length === 0 && !unreadable;

  if (selection?.kind === "step") {
    const step = steps.find((s) => s.id === selection.id) ?? null;
    if (!step) return workflowView(workflow, problems, unreadable, readOnly, canRun);
    const frozen = frozenStep(edit, step.id);
    const mine = problems.filter((p) => p.step === step.id);
    const record = run?.steps?.[step.id] ?? null;
    const facts = run && record ? { record, yours: yours(step, record), actions: stepActions(step, record), row: progressRows(run, null).find((r) => r.id === step.id) ?? null } : null;
    const sections = STEP_SECTIONS.filter((s) => (s === "run" ? facts !== null : s === "problems" ? mine.length > 0 : s === "danger" ? !frozen : true));
    return {
      kind: "step",
      title: step.name || step.id,
      subtitle: kindLabel(step.kind),
      step,
      frozen,
      // The amendment's grain: this step ran while the rest may still change.
      stepStarted: frozen && !edit?.readOnly,
      readOnly,
      sections,
      problems: { list: mine, count: mine.length, canRun },
      run: facts,
      remove: !frozen,
    };
  }

  if (selection?.kind === "flow") {
    const flow = flowFacts(workflow, selection);
    const frozen = !!edit?.readOnly || (edit?.editable != null && !edit.editable.has(selection.from));
    const remove = !frozen && flow.exists;
    return {
      kind: "flow",
      title: t("workflow-inspector-title-flow"),
      subtitle: t("workflow-inspector-flow-ends", { from: flow.from?.name || selection.from, to: flow.to?.name || selection.to }),
      flow,
      frozen,
      readOnly,
      sections: remove ? ["flow", "danger"] : ["flow"],
      problems: { list: [], count: 0, canRun },
      remove,
    };
  }

  if (selection?.kind === "many") {
    const n = selection.steps.length + selection.flows.length;
    return {
      kind: "many",
      title: t("workflow-inspector-title-many", { n }),
      subtitle: t("workflow-inspector-many-hint"),
      many: { steps: selection.steps.map((id) => steps.find((s) => s.id === id)).filter(Boolean), flows: selection.flows, n },
      frozen: !!edit?.readOnly,
      readOnly,
      sections: ["many"],
      problems: { list: [], count: 0, canRun },
      remove: false,
    };
  }

  return workflowView(workflow, problems, unreadable, readOnly, canRun);
}

function workflowView(workflow, problems, unreadable, readOnly, canRun) {
  return {
    kind: "workflow",
    title: t("workflow-inspector-title-workflow"),
    subtitle: workflow?.name || t("workflow-goal-workflow-tab-untitled-workflow"),
    frozen: readOnly !== null,
    readOnly,
    sections: [...WORKFLOW_SECTIONS],
    problems: { list: problems, count: problems.length, canRun, unreadable },
    remove: false,
  };
}
