/**
 * The order of the app's one way out (ide/03, ide/13), without the window:
 * ask first when the switch says so, then save what is dirty, then keep
 * what is remembered, then go — and stop at the first no. The question
 * comes before the save, so a cancelled quit has written nothing; a save
 * that fails keeps the window and says so; what is remembered — where the
 * person was, how each screen stood — is kept last, so it is of the moment
 * they left, and a keep that fails never holds the window: a memory is
 * furniture, a way out is not;
 * a second request while one is being answered is dropped, since the red
 * button, ⌘Q and the menu bar's Quit can land together. `useCloseGuard`
 * hands in the window's own hands; a test hands in fakes. The shell holds
 * every close request before this runs (`main.rs`), so nothing here decides
 * whether the window is held — only how it goes.
 */

/**
 * Save every one, in order, trying the rest after a failure — one document
 * that cannot be written must not leave its neighbours unsaved. A save that
 * throws is a save that failed; `onError` hears why.
 * @param {readonly T[]} items
 * @param {(item: T) => Promise<boolean>} saveOne
 * @param {(item: T, error: unknown) => void} [onError]
 * @returns {Promise<boolean>} whether every one saved
 * @template T
 */
export async function saveEvery(items, saveOne, onError) {
  let clean = true;
  for (const item of items) {
    try {
      clean = (await saveOne(item)) === true && clean;
    } catch (e) {
      onError?.(item, e);
      clean = false;
    }
  }
  return clean;
}

/** How a flow ended — what a test reads, and what the hook logs. */
export const CLOSE_OUTCOMES = Object.freeze(["busy", "cancelled", "unsaved", "closed"]);

/**
 * Whether the shell is told *no* after a flow ended this way. An OS that
 * asked — a `terminate:` held on macOS, a logout waiting on it — is owed an
 * answer: a cancelled quit and a document that would not save are a no;
 * `closed` has answered through `quit_app`; and `busy` is a second request
 * whose first is still being asked, which answers for both.
 * @param {string} outcome
 */
export function declines(outcome) {
  return outcome === "cancelled" || outcome === "unsaved";
}

/**
 * One flow over the window's hands. `confirms` — the quit switch; `ask` —
 * the question, answered yes or no; `dirty` — whether anything is unsaved;
 * `save` — save it all, true when all of it saved; `unsaved` — say that the
 * window stays; `keep` — write what is remembered, and wait for it to
 * settle; `keepFailed` — hear why a keep failed. The returned function runs
 * the flow and ends with `finish`; it answers how it ended.
 * @param {{confirms: () => boolean, ask: () => Promise<boolean>, dirty: () => boolean, save: () => Promise<boolean>, unsaved: () => void, keep: () => Promise<void>, keepFailed?: (error: unknown) => void}} hands
 */
export function closeFlow(hands) {
  let running = false;
  const run = async (finish) => {
    if (running) return "busy";
    running = true;
    try {
      if (hands.confirms() && !(await hands.ask())) return "cancelled";
      if (hands.dirty() && !(await hands.save())) {
        hands.unsaved();
        return "unsaved";
      }
      try {
        await hands.keep();
      } catch (e) {
        hands.keepFailed?.(e);
      }
      await finish();
      return "closed";
    } finally {
      running = false;
    }
  };
  return Object.freeze({ run, running: () => running });
}
