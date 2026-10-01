/**
 * An assignee as a step's form holds it: the picker speaks words —
 * `agent:<id>`, `team:<id>`, `human:<hex>` — the wire carries `{ agent }`,
 * `{ team }`, `{ human }`, and a step may name an input instead, `{ input }`,
 * read when the run starts. The grammar of the words is the platform's one
 * (`_work/assigneeWire.mjs`); what is here is what the step forms add: a
 * field that takes one or an input, and a list whose input references ride
 * along behind the fixed ones — a picker that only knows fixed assignees
 * must never drop them.
 *
 * Every step form reads these — `agent`, `human`, `notify`, `spawn`, a
 * message filter, a boundary's post — so a word is turned into an assignee
 * in one place.
 */

import { assigneeToWire, wireToAssignee } from "../../_work/assigneeWire.mjs";

const isInput = (ref) => !!ref && typeof ref === "object" && "input" in ref;

/** The picker's word for a fixed assignee; `null` for an input reference or nothing. */
export function wordOf(ref) {
  if (!ref || typeof ref !== "object" || isInput(ref)) return null;
  return assigneeToWire(ref);
}

/** The wire's assignee for a picker's word; `null` for a word that names nobody. */
export function refOf(word) {
  return wireToAssignee(word);
}

/** What a field that takes one holds: the input reference, the picker's word for a fixed one, `null` for none. */
export function valueOf(ref) {
  if (!ref) return null;
  return isInput(ref) ? { input: ref.input } : wordOf(ref);
}

/** What a field that takes one writes: nothing, the input it names, or the assignee its word names. */
export function picked(value) {
  if (value === null || value === undefined || value === "") return null;
  return typeof value === "object" ? { input: value.input } : refOf(value);
}

/** The same for a field that takes an agent and nobody else — who speaks a `notify`: a word that names no agent is nothing. */
export function pickedAgent(value) {
  const ref = picked(value);
  return ref && ("input" in ref || "agent" in ref) ? ref : null;
}

/** The fixed assignees of a list, as the picker's words. */
export function fixedWords(refs) {
  return (refs ?? []).map(wordOf).filter((word) => word !== null);
}

/** The inputs a list reads an assignee from, by name. */
export function inputNames(refs) {
  return (refs ?? []).filter(isInput).map((ref) => ref.input);
}

/** The list with its fixed assignees replaced by the picker's `words`, its input references kept behind them. */
export function withFixed(refs, words) {
  return [
    ...(words ?? []).map(refOf).filter((ref) => ref !== null),
    ...(refs ?? []).filter(isInput),
  ];
}

/** The list reading `name` too, or no longer when it did; everything else as it was. */
export function toggleInput(refs, name) {
  const list = refs ?? [];
  return list.some((ref) => isInput(ref) && ref.input === name) ? list.filter((ref) => !(isInput(ref) && ref.input === name)) : [...list, { input: name }];
}
