/**
 * A condition as the editor makes and edits it — the rules behind
 * `ConditionEditor.tsx`. The ten conditions are a closed set
 * (`stepKinds.mjs`'s `CONDITIONS`): six leaves and four that hold other
 * conditions. What is here: the condition a new rule starts as, the one a
 * change of kind makes of what was there, which kinds can be drawn with
 * what exists, and how a value is typed and shown.
 *
 * **A reference is a real id or absent, never `""`**: a kind that names a
 * step or an input is offered only when one exists, and every function here
 * is total without ever writing a blank reference.
 */

import { COMBINATORS, MAX_CONDITION_DEPTH } from "../stepKinds.mjs";

/** The conditions that name a step, offered only when one runs before this step. */
const NEEDS_STEP = Object.freeze(["output_equals", "output_matches", "answered", "outcome"]);
/** The conditions that name an input, offered only when the workflow declares one. */
const NEEDS_INPUT = Object.freeze(["input_equals"]);
/** The kinds of step an `outcome` judges. */
const JUDGED = Object.freeze(["check", "approval"]);

/** Whether a condition holds a list of others: `all`, `any`, `one`. */
export function isGroup(condition) {
  return condition?.condition === "all" || condition?.condition === "any" || condition?.condition === "one";
}

/**
 * A fresh condition, from what exists: the outcome of the first check or
 * approval upstream, else the first upstream step's; else the first input;
 * else a group to fill. The `decide` form mints its rules through it too.
 */
export function freshCondition(upstream, inputs) {
  const steps = upstream ?? [];
  const judged = steps.find((s) => JUDGED.includes(s.kind))?.id ?? steps[0]?.id;
  if (judged !== undefined) return { condition: "outcome", step: judged, passed: true };
  const input = (inputs ?? [])[0]?.name;
  if (input !== undefined) return { condition: "input_equals", input, value: "" };
  return { condition: "all", of: [] };
}

/** Whether a condition kind can be drawn with what exists. */
export function offeredWith(kind, upstream, inputs) {
  if (NEEDS_STEP.includes(kind)) return (upstream ?? []).length > 0;
  if (NEEDS_INPUT.includes(kind)) return (inputs ?? []).length > 0;
  return true;
}

/**
 * The kinds the select offers at `depth`: a group one level from the bound
 * may not hold another group, and a kind that names a step or an input is
 * offered only when there is one to name — but the kind a condition already
 * is stays listed, so what was stored is shown as it is.
 * @param {readonly {condition: string, label: string}[]} kinds `CONDITIONS`
 * @param {string} current the kind the condition is
 * @param {number} depth how deep the editor sits; a leaf is 1
 */
export function offeredKinds(kinds, current, depth, upstream, inputs) {
  const nests = mayNest(depth);
  return (kinds ?? []).filter((c) => (nests || !COMBINATORS.includes(c.condition)) && (c.condition === current || offeredWith(c.condition, upstream, inputs)));
}

/** Whether an editor at `depth` may hold a group: the core allows `MAX_CONDITION_DEPTH`. */
export function mayNest(depth) {
  return depth < MAX_CONDITION_DEPTH;
}

/**
 * The condition a change of kind makes. A leaf starts from what exists; a
 * group starts holding what was there, so switching to `all` wraps the
 * condition rather than losing it, and another group keeps its children.
 */
export function conditionOf(kind, upstream, inputs, current) {
  const steps = upstream ?? [];
  const first = steps[0]?.id;
  const input = (inputs ?? [])[0]?.name;
  const fresh = () => freshCondition(upstream, inputs);
  switch (kind) {
    case "input_equals":
      return input === undefined ? fresh() : { condition: "input_equals", input, value: "" };
    case "output_equals":
      return first === undefined ? fresh() : { condition: "output_equals", step: first, path: "", value: "" };
    case "output_matches":
      return first === undefined ? fresh() : { condition: "output_matches", step: first, path: "", contains: "" };
    case "answered": {
      const step = steps.find((s) => s.kind === "human")?.id ?? first;
      return step === undefined ? fresh() : { condition: "answered", step, option: "" };
    }
    case "outcome": {
      const step = steps.find((s) => JUDGED.includes(s.kind))?.id ?? first;
      return step === undefined ? fresh() : { condition: "outcome", step, passed: true };
    }
    case "between":
      return { condition: "between", from_hour: 9, to_hour: 18 };
    case "all":
    case "any":
    case "one":
      return { condition: kind, of: isGroup(current) ? current.of : current?.condition === "not" ? [current.of] : current ? [current] : [] };
    case "not":
      return { condition: "not", of: current?.condition === "not" ? current.of : (current ?? fresh()) };
    default:
      return current ?? fresh();
  }
}

/** A value typed as JSON when it parses — a number, `true`, a list — else the text itself. */
export function parseValue(text) {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/**
 * A value as its field shows it, so that **what is shown is what would be
 * written back**: text is shown bare, unless bare it would read as another
 * value — the text `5`, `true` or `[1]` is shown quoted, or the next
 * keystroke would turn it into the number, the yes or the list.
 */
export function valueText(value) {
  if (typeof value !== "string") return JSON.stringify(value) ?? "";
  try {
    JSON.parse(value);
  } catch {
    return value;
  }
  return JSON.stringify(value);
}

/** An hour of the day as it is typed: whole, within 0–23; what is no number is 0. */
export function hourOf(text) {
  return Math.max(0, Math.min(23, Math.trunc(Number(text) || 0)));
}
