/**
 * A tiny least-recently-used map.
 *
 * A `Map` keeps insertion order, so "most recently used" is just "re-inserted
 * last": `get`/`set` move a key to the end, and a `set` past the cap drops the
 * key at the front. Used where a cache would otherwise grow without bound for
 * the life of a weeks-long session (the per-root path index), keeping only the
 * few roots in active use.
 *
 * @template V
 */
export class Lru {
  /** @param {number} capacity the most entries to keep; must be >= 1 */
  constructor(capacity) {
    if (!Number.isInteger(capacity) || capacity < 1) throw new Error(`Lru capacity must be a positive integer, got ${capacity}`);
    this.capacity = capacity;
    /** @type {Map<string, V>} */
    this.map = new Map();
  }

  /** The value for a key, moved to most-recently-used, or undefined. @param {string} key */
  get(key) {
    const v = this.map.get(key);
    if (v === undefined) return undefined;
    this.map.delete(key);
    this.map.set(key, v);
    return v;
  }

  /** Whether a key is present, without touching its recency. @param {string} key */
  has(key) {
    return this.map.has(key);
  }

  /** Change the cap, evicting the oldest until at or under it. @param {number} capacity */
  setCapacity(capacity) {
    if (!Number.isInteger(capacity) || capacity < 1) return;
    this.capacity = capacity;
    while (this.map.size > this.capacity) {
      const oldest = this.map.keys().next().value;
      if (oldest === undefined) break;
      this.map.delete(oldest);
    }
  }

  /** Store as most-recently-used, evicting the oldest past the cap. @param {string} key @param {V} value */
  set(key, value) {
    this.map.delete(key);
    this.map.set(key, value);
    while (this.map.size > this.capacity) {
      const oldest = this.map.keys().next().value;
      if (oldest === undefined) break;
      this.map.delete(oldest);
    }
  }

  /** Drop a key. @param {string} key @returns {boolean} whether it was present */
  delete(key) {
    return this.map.delete(key);
  }

  /** How many entries are held. */
  get size() {
    return this.map.size;
  }

  /** The keys, oldest first — for tests and inspection. @returns {string[]} */
  keys() {
    return [...this.map.keys()];
  }
}
