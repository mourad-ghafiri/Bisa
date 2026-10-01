/**
 * *Run…* on a library workflow, decided before the dialog draws
 * (03-workflows §Runs of the workspace): the ways in it offers — *By hand*
 * when the workflow has a start by hand, then each event start as a test,
 * *as if* its event had happened — the inputs each asks, a test's sample
 * payload, the body the node takes, and how a refusal reads. A run in the
 * workspace captures no goal: nothing here asks for one.
 *
 * A workflow whose steps read the goal it serves never offers *Run…*
 * (`workflowVerbs`); when the row the card drew was older than the
 * workflow, the node refuses the start by name — `needs_goal` — and
 * `runRefusal` says it in the desktop's words and asks for the row again.
 * Plain `.mjs`, so `node --test` reads it; the dialog is paint.
 */

import { t } from "../../i18n/l10n.mjs";
import { problemsFromErrorBody } from "./designerSession.mjs";
import { eventPhrase, eventStarts, manualEntry, samplePayload } from "./forms/startForm.mjs";
import { toRequest, validateInputs } from "./workflowForm.mjs";

/** The entry *By hand* in the select: no step id reads so. */
export const BY_HAND = "*";

/**
 * The ways in the dialog offers, in the order it draws them: by hand when
 * the workflow begins so, then every event start as a test run.
 * @param {{steps?: object[]}} workflow
 * @returns {{id: string, test: boolean, label: string, hint: string | null}[]}
 */
export function runEntries(workflow) {
  const out = [];
  if (manualEntry(workflow) !== null) out.push({ id: BY_HAND, test: false, label: t("workflow-run-workflow-dialog-by-hand"), hint: null });
  for (const s of eventStarts(workflow)) {
    out.push({ id: s.id, test: true, label: t("workflow-run-workflow-dialog-test-as-if", { start: s.name || s.id }), hint: eventPhrase(s.on) });
  }
  return out;
}

/** The entry the dialog opens on: by hand when it can begin so, else its first event start; `null` when nothing begins it. */
export function firstEntry(workflow) {
  return runEntries(workflow)[0]?.id ?? null;
}

/** The event start a test entry names; `null` by hand, and for a step that is no event start of the workflow. */
export function entryStart(workflow, entry) {
  if (entry === BY_HAND) return null;
  return eventStarts(workflow).find((s) => s.id === entry) ?? null;
}

/** The input definitions an entry asks: every one by hand; for a test, the ones its start's mapping does not fill from the event. */
function askedDefs(workflow, entry) {
  const inputs = workflow?.inputs ?? [];
  const start = entryStart(workflow, entry);
  if (!start) return inputs;
  const mapping = start.inputs ?? {};
  return inputs.filter((d) => !(d.name in mapping));
}

/**
 * What an entry asks of the person — the names, in the inputs' own order —
 * and how many inputs the event fills instead.
 * @returns {{asked: string[], mapped: number}}
 */
export function askedInputs(workflow, entry) {
  const asked = askedDefs(workflow, entry);
  return { asked: asked.map((d) => d.name), mapped: (workflow?.inputs ?? []).length - asked.length };
}

/** A test entry's sample payload as the person edits it; nothing by hand. @param {number} [now] unix seconds */
export function sampleText(workflow, entry, now) {
  const start = entryStart(workflow, entry);
  if (!start) return "";
  return JSON.stringify(samplePayload(start.on, now) ?? {}, null, 2);
}

/**
 * The payload box read: the JSON it holds — an empty box is an empty
 * occurrence — or the sentence that says why it is none.
 * @param {string} text
 * @returns {{ok: true, event: unknown} | {ok: false, error: string}}
 */
export function payloadOf(text) {
  try {
    return { ok: true, event: JSON.parse(String(text ?? "").trim() || "{}") };
  } catch (e) {
    return { ok: false, error: t("workflow-run-workflow-dialog-payload-invalid", { error: e instanceof Error ? e.message : String(e) }) };
  }
}

/**
 * What the dialog sends, or why it sends nothing. By hand: the workflow's
 * inputs. A test: the inputs its event does not fill, the start it begins
 * at and the event — never an input the mapping fills, whatever the form
 * still holds. Refused: a sentence per input that is wrong, the payload's
 * when it is no JSON; an entry that is no way in refuses with neither.
 * @param {{inputs?: object[], steps?: object[]}} workflow
 * @param {string} entry
 * @param {Record<string, unknown>} values
 * @param {string} payload the payload box, read only for a test
 */
export function runRequest(workflow, entry, values, payload) {
  const start = entryStart(workflow, entry);
  const offered = runEntries(workflow).some((e) => e.id === entry);
  if (!offered) return { kind: "refused", errors: {}, payloadError: null };
  const asked = askedDefs(workflow, entry);
  const errors = validateInputs(asked, values);
  const read = start ? payloadOf(payload) : { ok: true, event: null };
  if (Object.keys(errors).length > 0 || !read.ok) return { kind: "refused", errors, payloadError: read.ok ? null : read.error };
  const inputs = toRequest(asked, values);
  return start ? { kind: "test", body: { inputs, start: start.id, event: read.event } } : { kind: "run", inputs };
}

/**
 * A start the node refused, in words. A definition that reads its goal is
 * refused by name (`needs_goal`, one problem a step): the sentence names
 * the steps and says why. Any refusal that carries problems means the row
 * the card drew is older than the workflow — it is read again, and the card
 * says *runs on a goal*, or counts its problems. A refusal that names no
 * problem is said as it came.
 * @param {unknown} body the refusal's parsed body, when the node sent one
 * @param {string} message the refusal's sentence
 * @returns {{words: string, needsGoal: boolean, reread: boolean}}
 */
export function runRefusal(body, message) {
  const problems = problemsFromErrorBody(body);
  const steps = [...new Set(problems.filter((p) => p?.kind === "needs_goal" && p.step).map((p) => String(p.step)))];
  if (steps.length === 0) return { words: message, needsGoal: false, reread: problems.length > 0 };
  const last = steps[steps.length - 1];
  const names = steps.length === 1 ? last : t("workflow-run-dialog-refused-steps", { first: steps.slice(0, -1).join(", "), last });
  return { words: t("workflow-run-dialog-refused-needs-goal", { n: steps.length, steps: names }), needsGoal: true, reread: true };
}
