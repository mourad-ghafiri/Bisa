/**
 * The models kept for the documents' tabs (ide/03 §Editing features): one
 * Monaco model per tab, made at the first mount of its editor, kept across
 * every unmount — a pane draws one tab at a time, so a tab switch, a mode
 * switch and the review lens each unmount the editor — and disposed when
 * the tab closes, in the one place tabs change (`workbenchStore`'s `set`,
 * beside the buffer and the place). The undo history lives on the model, so
 * it survives what the mount does not. Pure bookkeeping over anything with a
 * `dispose()`; `ui/monacoModels.ts` puts Monaco's models through it, and
 * `node --test` reads this.
 */

export class KeptModels {
  constructor() {
    /** @type {Map<string, {dispose: () => void}>} */
    this.held = new Map();
  }

  /**
   * The model held under `key`, else `make()` — held from now on.
   * @template {{dispose: () => void}} T
   * @param {string} key
   * @param {() => T} make
   * @returns {T}
   */
  acquire(key, make) {
    const had = this.held.get(key);
    if (had) return /** @type {T} */ (had);
    const made = make();
    this.held.set(key, made);
    return made;
  }

  /** Whether a model is held under `key`. @param {string} key */
  holds(key) {
    return this.held.has(key);
  }

  /**
   * Dispose and let go of each held key; a key nobody held is nothing.
   * Answers how many went.
   * @param {readonly string[]} keys
   */
  release(keys) {
    let gone = 0;
    for (const key of keys) {
      const held = this.held.get(key);
      if (!held) continue;
      this.held.delete(key);
      held.dispose();
      gone++;
    }
    return gone;
  }

  /** How many tabs hold a model — never how many editors are mounted. */
  get count() {
    return this.held.size;
  }
}
