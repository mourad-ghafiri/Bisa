/**
 * A `judge` step as its form edits it (15 §The judge step): the
 * Decision-Making Agent reads `state` and picks one of `options` by what
 * each means, `otherwise` when it is not sure enough or gives no answer
 * that holds to the contract — the step never fails for that. Naming the
 * step *is* switching the Decision-Making Agent on for it, so the form
 * carries no switch: only the options, `otherwise`, and how sure a pick
 * must be — a number from 0 to 1, or none, and the workspace's own bar
 * (`decisions.confidence.act`) applies.
 *
 * A branch is renamed through `workflowGraph.relabelBranch`; a new one is
 * named by `workflowGraph.freshBranch`. Every function returns a new step.
 * Pure, so `node --test` reads it; `JudgeStepForm.tsx` draws the form.
 */

import { t } from "../../../i18n/l10n.mjs";
import { freshBranch } from "../workflowGraph.mjs";

/** A judgement chooses between at least this many options — the core's bound (`empty_rules`). */
export const MIN_OPTIONS = 2;

/** How sure a pick must be, as the field shows it: the number, or nothing for none. */
export function confidenceText(step) {
  return step?.min_confidence === undefined || step?.min_confidence === null ? "" : String(step.min_confidence);
}

/**
 * What was typed in the field, read: nothing is none — the workspace's own
 * bar applies; a number from 0 to 1 is the bar; anything else is refused,
 * with the sentence that says why. A comma is a decimal mark.
 * @param {string} text
 * @returns {{ok: true, value: number | null} | {ok: false, reason: string}}
 */
export function readConfidence(text) {
  const typed = String(text ?? "").trim().replace(",", ".");
  if (typed === "") return { ok: true, value: null };
  const n = Number(typed);
  if (!Number.isFinite(n) || n < 0 || n > 1) return { ok: false, reason: t("workflow-judge-step-confidence-out-of-range") };
  return { ok: true, value: n };
}

/**
 * The step with its bar set from what was typed. A blank field takes the
 * bar off the step — absent on the wire, never `null`; a refused one writes
 * nothing, so a confidence the node would refuse never reaches the step.
 * @returns {{ok: true, step: object} | {ok: false, reason: string}}
 */
export function setConfidence(step, text) {
  const read = readConfidence(text);
  if (!read.ok) return read;
  if (read.value === null) {
    if (!("min_confidence" in step)) return { ok: true, step };
    const { min_confidence: _bar, ...rest } = step;
    return { ok: true, step: rest };
  }
  return { ok: true, step: step.min_confidence === read.value ? step : { ...step, min_confidence: read.value } };
}

/** The step with a fresh option at the end: a branch name it has for nothing else, and its meaning to write. */
export function addOption(step) {
  return { ...step, options: [...(step.options ?? []), { branch: freshBranch(step), meaning: "" }] };
}

/** The step with one option's meaning written; the same step when the row is not there. */
export function setMeaning(step, index, meaning) {
  const options = step.options ?? [];
  if (index < 0 || index >= options.length) return step;
  return { ...step, options: options.map((o, i) => (i === index ? { ...o, meaning } : o)) };
}

/**
 * The step without one option. A flow still labelled with its branch is the
 * validator's to report, never silently rewritten.
 */
export function removeOption(step, index) {
  const options = step.options ?? [];
  if (index < 0 || index >= options.length) return step;
  return { ...step, options: options.filter((_, i) => i !== index) };
}

/**
 * What the form says of the step beside its fields: how many options a
 * judgement still needs — `null` once it has two — and what the run takes
 * when the Decision-Making Agent is not sure enough.
 * @returns {{options: string | null, otherwise: string}}
 */
export function judgeWords(step) {
  const missing = MIN_OPTIONS - (step?.options ?? []).length;
  return {
    options: missing > 0 ? t("workflow-judge-step-needs-options", { n: missing }) : null,
    otherwise: t("workflow-judge-step-takes-otherwise", { otherwise: step?.otherwise ?? "" }),
  };
}
