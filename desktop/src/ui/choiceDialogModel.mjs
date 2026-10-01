/**
 * A `ChoiceDialog`'s arithmetic: which choice the focus opens on, and where
 * the arrows take it. No React, so `node --test` reads it.
 *
 * A choice that is disabled — it says why instead of acting — is never a
 * stop for the arrows and never where the focus opens.
 */

/** Whether a choice can be taken. @param {{disabled?: string | null}} choice */
export function isOpen(choice) {
  return !choice?.disabled;
}

/**
 * Where the focus opens: the first choice that can be taken and destroys
 * nothing, else the first that can be taken at all, else none — Cancel keeps
 * the focus then.
 * @param {readonly {id: string, tone?: string, disabled?: string | null}[]} choices
 * @returns {string | null}
 */
export function openingChoice(choices) {
  const open = (choices ?? []).filter(isOpen);
  return (open.find((c) => c.tone !== "danger") ?? open[0])?.id ?? null;
}

/**
 * The choice an arrow moves to from `current`, over the ones that can be
 * taken, stopping at the ends; from nowhere, Down is the first and Up the last.
 * @param {readonly {id: string, disabled?: string | null}[]} choices
 * @param {string | null} current
 * @param {1 | -1} step
 * @returns {string | null}
 */
export function stepChoice(choices, current, step) {
  const ids = (choices ?? []).filter(isOpen).map((c) => c.id);
  if (ids.length === 0) return null;
  const at = ids.indexOf(current);
  if (at === -1) return step === 1 ? ids[0] : ids[ids.length - 1];
  return ids[Math.min(ids.length - 1, Math.max(0, at + step))];
}
