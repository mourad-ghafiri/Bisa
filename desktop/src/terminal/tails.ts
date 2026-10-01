/**
 * The last lines of each terminal, by tab key — what *Send to agent* attaches
 * (ide/09). Each mount registers a reader over its own buffer; unmount
 * unregisters. Read lazily, so nothing is copied until somebody asks.
 */

const readers = new Map<string, () => string[]>();

export function registerTerminalTail(key: string, read: () => string[]): () => void {
  readers.set(key, read);
  return () => {
    if (readers.get(key) === read) readers.delete(key);
  };
}

/** The buffer's lines, or null when that tab has no live emulator. */
export function terminalTail(key: string): string[] | null {
  const r = readers.get(key);
  return r ? r() : null;
}
