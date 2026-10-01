/**
 * A memory of how things stood, by place and by name — what a screen keeps
 * of itself so that leaving it, and closing the app, lose nothing
 * (`crates/desktop.md` §Per-viewer state). A **place** is what the memory is
 * about — a route's path, a document, a thread — and a **name** one thing
 * kept of it: a filter's text, the rows opened, where it was scrolled.
 *
 * One class, so the rules have one home:
 *
 * - **Bounded.** The newest places are kept and the oldest forgotten past
 *   the cap; a value whose text is over its bytes lives for the window and
 *   is never written; what is written stops at the total, newest first.
 * - **Versioned.** A stored shape of another version is refused — the
 *   memory starts empty — never reinterpreted.
 * - **One workspace's.** The memory is stamped with its owner; another
 *   owner's is forgotten whole (`adopt`).
 * - **Furniture.** A storage that cannot be read starts empty and one that
 *   cannot be written keeps the memory for the window: nothing here throws,
 *   because every read and write goes through `storedPrefModel`.
 * - **Quiet.** A change tells only the listeners of its own place and name,
 *   and a scroll (`keepQuietly`) tells nobody: it is read once, at mount.
 * - **Written on a beat.** A change waits `beat` milliseconds for the next
 *   one and `atMost` in all; `flush` writes at once.
 * - **Sealed once forgotten.** *Forget where I was* forgets, then opens the
 *   window again — and between the two a screen still on its way out hands
 *   its last scroll over, and the window's own flush would write it. A
 *   sealed memory keeps nothing more and writes nothing more, so what was
 *   forgotten stays forgotten; the next window's memory is a new one.
 *
 * No React and no window here: the clock and the storage are hands the
 * caller gives, so `node --test` reads it with fakes.
 */

import { readPref, writePref } from "./storedPrefModel.mjs";

/** How long a change waits for the next one before it is written, in milliseconds. */
export const WRITE_BEAT_MS = 400;
/** The longest a change waits, however many follow it. */
export const WRITE_AT_MOST_MS = 2000;

/** The key a listener is held under: a place and a name never hold a newline. */
function listenerKey(place, name) {
  return `${place}\n${name}`;
}

/**
 * A value as it is stored, or `undefined` for one that cannot be — a
 * function, a cycle, `undefined` itself.
 * @param {unknown} value
 * @returns {string | undefined}
 */
function textOf(value) {
  try {
    return JSON.stringify(value);
  } catch {
    return undefined;
  }
}

/**
 * A stored memory read back: the owner and the places, oldest first — or
 * `undefined`, which is the fallback, for anything that is not this version's
 * shape.
 * @param {string} raw
 * @param {number} version
 */
function parseStored(raw, version) {
  const stored = JSON.parse(raw);
  if (!stored || typeof stored !== "object" || stored.v !== version || !Array.isArray(stored.places)) return undefined;
  const places = [];
  for (const entry of stored.places) {
    if (!Array.isArray(entry) || entry.length !== 2) return undefined;
    const [place, names] = entry;
    if (typeof place !== "string" || !names || typeof names !== "object" || Array.isArray(names)) return undefined;
    places.push([place, names]);
  }
  return { owner: typeof stored.owner === "string" && stored.owner ? stored.owner : null, places };
}

export class KeptMemory {
  /**
   * @param {object} options
   * @param {string} options.key the storage key
   * @param {number} options.version the shape's version; another is refused
   * @param {{places: number, valueBytes: number, totalBytes: number}} options.caps
   * @param {{storage: () => (Storage | null), now: () => number, later: (fn: () => void, ms: number) => unknown, cancel: (handle: unknown) => void}} options.hands
   * @param {number} [options.beat]
   * @param {number} [options.atMost]
   */
  constructor({ key, version, caps, hands, beat = WRITE_BEAT_MS, atMost = WRITE_AT_MOST_MS }) {
    this.key = key;
    this.version = version;
    this.caps = caps;
    this.hands = hands;
    this.beat = beat;
    this.atMost = atMost;
    /** @type {Map<string, Map<string, {value: unknown, text: string | undefined}>>} place → name → what is kept, newest place last */
    this.places = new Map();
    /** @type {string | null} */
    this.owner = null;
    /** @type {Map<string, Set<() => void>>} */
    this.listeners = new Map();
    /** The timer a pending write waits on, and when the first unwritten change was made. */
    this.timer = null;
    this.dirtySince = null;
    /** When the storage last took a write, by the hands' clock; 0 before any. */
    this.writtenAt = 0;
    /** What the storage holds, as text — a write of the same text is no write. */
    this.written = null;
    /** Whether the memory takes anything more: not once it is sealed. */
    this.sealed = false;
    this.load();
  }

  /** Read what the last window left. Called once, by the constructor. */
  load() {
    const stored = readPref(this.hands.storage(), this.key, (raw) => parseStored(raw, this.version), null);
    if (!stored) return;
    this.owner = stored.owner;
    for (const [place, names] of stored.places.slice(-this.caps.places)) {
      const kept = new Map();
      for (const [name, value] of Object.entries(names)) kept.set(name, { value, text: textOf(value) });
      if (kept.size > 0) this.places.set(place, kept);
    }
  }

  /**
   * What is kept under a place and a name — the very value that was kept,
   * by identity, until it changes — or `undefined`. A read moves nothing.
   * @param {string} place
   * @param {string} name
   */
  read(place, name) {
    return this.places.get(place)?.get(name)?.value;
  }

  /** Whether anything is kept of a place. @param {string} place */
  knows(place) {
    return this.places.has(place);
  }

  /** The places kept, oldest first — for tests and for what is forgotten by rule. @returns {string[]} */
  keptPlaces() {
    return [...this.places.keys()];
  }

  /**
   * Keep `value` under a place and a name, the place the newest; `undefined`
   * or `null` forgets the name. The same value — the same text — keeps
   * nothing and tells nobody.
   * @param {string} place
   * @param {string} name
   * @param {unknown} value
   * @returns {boolean} whether anything changed
   */
  keep(place, name, value) {
    const changed = this.put(place, name, value);
    if (changed) this.tell(place, name);
    return changed;
  }

  /**
   * `keep`, telling nobody: for what is read once at mount — a scroll, a
   * canvas's place — so writing it re-renders no one.
   * @param {string} place
   * @param {string} name
   * @param {unknown} value
   * @returns {boolean} whether anything changed
   */
  keepQuietly(place, name, value) {
    return this.put(place, name, value);
  }

  /** The write both keeps share. */
  put(place, name, value) {
    if (this.sealed) return false;
    const names = this.places.get(place);
    if (value === undefined || value === null) {
      if (!names?.has(name)) return false;
      names.delete(name);
      if (names.size === 0) this.places.delete(place);
      this.schedule();
      return true;
    }
    const text = textOf(value);
    const was = names?.get(name);
    if (was && was.text !== undefined && was.text === text) return false;
    const next = names ?? new Map();
    next.set(name, { value, text });
    // Newest last: a place written to is the one a person is on.
    this.places.delete(place);
    this.places.set(place, next);
    while (this.places.size > Math.max(1, this.caps.places)) {
      const oldest = this.places.keys().next().value;
      if (oldest === undefined) break;
      this.drop(oldest);
    }
    this.schedule();
    return true;
  }

  /** A place gone from the memory, its listeners told. */
  drop(place) {
    const names = this.places.get(place);
    if (!names) return false;
    this.places.delete(place);
    for (const name of names.keys()) this.tell(place, name);
    return true;
  }

  /** Forget a place whole. @param {string} place @returns {boolean} whether it was kept */
  forget(place) {
    const dropped = this.drop(place);
    if (dropped) this.schedule();
    return dropped;
  }

  /**
   * Forget every place that begins with `prefix` — a route's path and what
   * is under it.
   * @param {string} prefix
   * @returns {number} how many were forgotten
   */
  forgetUnder(prefix) {
    let n = 0;
    for (const place of [...this.places.keys()]) {
      if (place.startsWith(prefix) && this.drop(place)) n += 1;
    }
    if (n > 0) this.schedule();
    return n;
  }

  /**
   * A thing renamed: what was kept under `from` is kept under `to`, the
   * newest. Nothing kept moves nothing.
   * @param {string} from
   * @param {string} to
   */
  move(from, to) {
    const names = this.places.get(from);
    if (!names || from === to) return false;
    this.places.delete(from);
    this.places.delete(to);
    this.places.set(to, names);
    for (const name of names.keys()) {
      this.tell(from, name);
      this.tell(to, name);
    }
    this.schedule();
    return true;
  }

  /** Forget everything, and write that at once. The owner stays. */
  clear() {
    const places = [...this.places.keys()];
    for (const place of places) this.drop(place);
    this.dirtySince = this.dirtySince ?? this.hands.now();
    this.flush();
  }

  /**
   * The workspace this memory is for. Another owner's memory is forgotten
   * whole; a memory with no owner yet takes this one.
   * @param {string | null | undefined} owner
   * @returns {boolean} whether the memory was another's and is now empty
   */
  adopt(owner) {
    if (!owner || owner === this.owner) return false;
    const foreign = this.owner !== null;
    this.owner = owner;
    if (foreign) {
      this.clear();
      return true;
    }
    this.schedule();
    return false;
  }

  /**
   * Hear the changes of one place and name.
   * @param {string} place
   * @param {string} name
   * @param {() => void} listener
   * @returns {() => void} stop hearing
   */
  subscribe(place, name, listener) {
    const key = listenerKey(place, name);
    let set = this.listeners.get(key);
    if (!set) {
      set = new Set();
      this.listeners.set(key, set);
    }
    set.add(listener);
    return () => {
      set.delete(listener);
      if (set.size === 0) this.listeners.delete(key);
    };
  }

  tell(place, name) {
    const set = this.listeners.get(listenerKey(place, name));
    if (!set) return;
    for (const listener of [...set]) listener();
  }

  /**
   * Take nothing more and write nothing more, for the rest of this window:
   * what waits is written first, so the storage holds the memory as it
   * stands now — and nothing handed over after can reach it.
   */
  seal() {
    if (this.sealed) return;
    this.flush();
    this.sealed = true;
  }

  /** A change is waiting to be written: the beat starts over, the longest wait does not. */
  schedule() {
    if (this.sealed) return;
    const now = this.hands.now();
    if (this.dirtySince === null) this.dirtySince = now;
    if (this.timer !== null) this.hands.cancel(this.timer);
    const left = Math.max(0, this.dirtySince + this.atMost - now);
    this.timer = this.hands.later(() => this.flush(), Math.min(this.beat, left));
  }

  /**
   * What would be written: the places newest first until the total is
   * reached, then put back oldest first; a value over its bytes left out.
   * `null` for a memory with nothing to write.
   * @returns {string | null}
   */
  serialized() {
    const kept = [];
    let total = 0;
    for (const [place, names] of [...this.places].reverse()) {
      const out = {};
      let size = place.length;
      let any = false;
      for (const [name, { value, text }] of names) {
        if (text === undefined || text.length > this.caps.valueBytes) continue;
        out[name] = value;
        size += name.length + text.length;
        any = true;
      }
      if (!any) continue;
      if (total + size > this.caps.totalBytes) break;
      total += size;
      kept.push([place, out]);
    }
    if (kept.length === 0 && this.owner === null) return null;
    return JSON.stringify({ v: this.version, owner: this.owner, places: kept.reverse() });
  }

  /**
   * Write what is waiting, now. The same text as the storage holds is no
   * write; a storage that refuses keeps the memory for the window.
   * @returns {boolean} whether the storage took a write
   */
  flush() {
    if (this.timer !== null) this.hands.cancel(this.timer);
    this.timer = null;
    if (this.sealed || this.dirtySince === null) return false;
    this.dirtySince = null;
    const text = this.serialized();
    if (text === this.written) return false;
    if (!writePref(this.hands.storage(), this.key, text)) return false;
    this.written = text;
    this.writtenAt = this.hands.now();
    return true;
  }

  /** Whether a change is waiting to be written. */
  waiting() {
    return this.dirtySince !== null;
  }
}

/**
 * How long a way out waits for the storage to settle: what is left of
 * `settle` since the last write, nothing when the last write is older — or
 * when there never was one.
 * @param {number} writtenAt when the last write was taken, 0 for none
 * @param {number} now
 * @param {number} settle
 * @returns {number} milliseconds
 */
export function settleLeft(writtenAt, now, settle) {
  if (!writtenAt) return 0;
  const since = now - writtenAt;
  if (since < 0 || since >= settle) return 0;
  return settle - since;
}
