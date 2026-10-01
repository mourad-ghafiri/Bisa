/**
 * Which browser tabs an agent is working in right now (ide/18), as facts:
 * a request the bridge performs on a tab begins and ends; a tab with a
 * request in flight is busy; two requests on one tab end one at a time; a
 * tab at zero is forgotten. The state is a plain object of counts by tab
 * key, never mutated. Plain `.mjs`, so `node --test` reads it.
 */

export const NO_ACTIVITY = Object.freeze({});

/** A request began on a tab. @param {Readonly<Record<string, number>>} state @param {string} key */
export function began(state, key) {
  return { ...state, [key]: (state[key] ?? 0) + 1 };
}

/** A request ended on a tab; an end nobody began is nothing. @param {Readonly<Record<string, number>>} state @param {string} key */
export function ended(state, key) {
  const n = state[key] ?? 0;
  if (n === 0) return state;
  if (n === 1) {
    const { [key]: _gone, ...rest } = state;
    return rest;
  }
  return { ...state, [key]: n - 1 };
}

/** A tab closed: whatever was counted on it is forgotten. */
export function forgotten(state, key) {
  if (!(key in state)) return state;
  const { [key]: _gone, ...rest } = state;
  return rest;
}

/** The tabs an agent is working in. @param {Readonly<Record<string, number>>} state */
export function busyKeys(state) {
  return Object.keys(state);
}
