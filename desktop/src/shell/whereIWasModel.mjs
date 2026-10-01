/**
 * *Forget where I was* (Settings › Desktop), without the window: what is
 * forgotten, what is kept, and the order it happens in.
 *
 * **Forgotten** — every memory of *where*: the places and what each screen
 * kept of itself (`memories`), the workstream the Project IDE was last in
 * (`root`), the conversation each pane was last on (`conversations`) — and
 * the messages being written (`drafts`) only when the person says so.
 * **Kept** — the furniture that is no place (sizes, docks, the sidebar's
 * order, the theme, the window's size and position) and every document.
 *
 * The order: what is unsaved is saved first, and a save that fails forgets
 * nothing — the window is about to open again, and a document must not pay
 * for a memory. Then each thing is forgotten, the rest still tried after one
 * that fails: a memory that stays is a lesser harm than a half-forgotten
 * app. Then the memories are **sealed** — they take nothing more and write
 * nothing more — and the window leaves for the home and opens again, so
 * nothing a screen holds in its hands outlives what was forgotten: not what
 * a store still holds, and not the last word a screen hands over on its way
 * out, which the window's own flush would otherwise write back. A second
 * request while one is running is dropped. `whereIWas.ts` hands in the
 * window's own hands; a test hands in fakes.
 */

/** What is always forgotten, in order. */
export const FORGOTTEN = Object.freeze(["memories", "root", "conversations"]);

/** What is forgotten only when the person says so. */
export const FORGOTTEN_ON_REQUEST = Object.freeze(["drafts"]);

/** The name a seal that failed is heard under. */
export const SEAL = "seal";

/** How a flow ended — what a test reads, and what the card says. */
export const WHERE_I_WAS_OUTCOMES = Object.freeze(["busy", "unsaved", "forgotten"]);

/**
 * What one request forgets, in order.
 * @param {{ drafts?: boolean }} [asked]
 * @returns {string[]}
 */
export function forgottenBy(asked) {
  return asked?.drafts === true ? [...FORGOTTEN, ...FORGOTTEN_ON_REQUEST] : [...FORGOTTEN];
}

/**
 * One flow over the window's hands. `dirty` — whether anything is unsaved;
 * `save` — save it all, true when all of it saved; `forget` — one hand a
 * thing, by its name; `failed` — hear which thing stayed, and why; `seal` —
 * the memories take nothing more until the window opens again; `leave` — go
 * home and open the window again. A seal that throws is heard as a thing
 * that stayed, and the window still leaves. The returned function runs the
 * flow and answers how it ended.
 * @param {{dirty: () => boolean, save: () => Promise<boolean>, forget: Record<string, () => void>, failed?: (what: string, error: unknown) => void, seal?: () => void, leave: () => void | Promise<void>}} hands
 */
export function forgetFlow(hands) {
  let running = false;
  const run = async (asked) => {
    if (running) return "busy";
    running = true;
    try {
      if (hands.dirty() && !(await hands.save())) return "unsaved";
      for (const what of forgottenBy(asked)) {
        try {
          hands.forget[what]();
        } catch (e) {
          hands.failed?.(what, e);
        }
      }
      try {
        hands.seal?.();
      } catch (e) {
        hands.failed?.(SEAL, e);
      }
      await hands.leave();
      return "forgotten";
    } finally {
      running = false;
    }
  };
  return Object.freeze({ run, running: () => running });
}
