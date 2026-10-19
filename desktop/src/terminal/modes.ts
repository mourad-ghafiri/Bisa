/**
 * What each terminal's program asked of its emulator, by tab key — whether it
 * takes a **bracketed paste** (DECSET 2004), which is how a request of many
 * lines reaches a harness's input as text (ide/06 §Start words). Each mount
 * registers a reader over its own xterm; unmount unregisters. Read lazily.
 */

const readers = new Map<string, () => { bracketed: boolean }>();

export function registerTerminalModes(key: string, read: () => { bracketed: boolean }): () => void {
  readers.set(key, read);
  return () => {
    if (readers.get(key) === read) readers.delete(key);
  };
}

/** Whether that tab's program asked for bracketed paste, or null when the tab has no live emulator. */
export function terminalBracketed(key: string): boolean | null {
  const r = readers.get(key);
  return r ? r().bracketed : null;
}
