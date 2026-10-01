/**
 * A slot as a published fact: the element a pane draws for a view to fill,
 * announced when the pane mounts it and withdrawn when the pane goes, so a
 * portal follows a pane that closed and opened again instead of holding
 * the element it found the first time. The desktop's Details pane fills
 * this way (`AuxPane.tsx`); a lookup by id captured once was how the pane
 * came back empty after a close — the new element was never seen.
 *
 * Plain `.mjs` so `node --test` reads the rule without a DOM: the element
 * is any value, the store never inspects it.
 */

/**
 * One slot: `publish` the element (or `null` when it is gone), `current`
 * reads it, `subscribe` hears every change — the same element twice is
 * not a change.
 * @template T
 * @returns {{publish: (el: T | null) => void, current: () => T | null, subscribe: (l: () => void) => () => void}}
 */
export function createSlot() {
  /** @type {T | null} */
  let el = null;
  const listeners = new Set();
  return {
    publish(next) {
      const value = next ?? null;
      if (value === el) return;
      el = value;
      for (const l of listeners) l();
    },
    current: () => el,
    subscribe(l) {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
  };
}
