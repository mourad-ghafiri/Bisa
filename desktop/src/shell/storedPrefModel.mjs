/**
 * How a store keeps a person's convenience in the browser's storage: one
 * read, one write, one forget — and none of them ever throws. Storage is
 * furniture: a store that cannot be read starts at its default, one that
 * cannot be written holds the choice for the session, and a value that does
 * not parse is the default too. Every `*Store.ts` that remembers a choice
 * reads through here, so the rule has one home. Plain `.mjs`, so
 * `node --test` reads it.
 */

/**
 * The web storage, or `null` where there is none or asking for it throws —
 * a webview with storage denied, a test under `node`.
 * @returns {Storage | null}
 */
export function webStorage() {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

/**
 * The stored value under `key`, read through `parse`, else `fallback`: when
 * there is no storage, no value, or `parse` throws or answers `undefined`.
 * @template T
 * @param {Storage | null | undefined} storage
 * @param {string} key
 * @param {(raw: string) => T} parse
 * @param {T} fallback
 * @returns {T}
 */
export function readPref(storage, key, parse, fallback) {
  let raw = null;
  try {
    raw = storage ? storage.getItem(key) : null;
  } catch {
    return fallback;
  }
  if (raw === null || raw === undefined) return fallback;
  try {
    const v = parse(raw);
    return v === undefined ? fallback : v;
  } catch {
    return fallback;
  }
}

/**
 * Keep `value` under `key` — as text, or as JSON when `value` is not a
 * string — and forget the key when `value` is `null` or `undefined`.
 * @param {Storage | null | undefined} storage
 * @param {string} key
 * @param {unknown} value
 * @returns {boolean} whether the storage took it
 */
export function writePref(storage, key, value) {
  if (!storage) return false;
  try {
    if (value === null || value === undefined) storage.removeItem(key);
    else storage.setItem(key, typeof value === "string" ? value : JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

/** Forget `key`; nothing throws. @param {Storage | null | undefined} storage @param {string} key */
export function forgetPref(storage, key) {
  return writePref(storage, key, null);
}

/**
 * Forget every key that begins with `prefix` — what is kept a key a thing
 * (a message being written, per conversation) — but the keys that begin
 * with one of `spared`: another store's, under the same prefix. The keys
 * are listed before any is forgotten, since a storage renumbers as it
 * forgets. Nothing throws: a storage that refuses leaves what it holds.
 * @param {Storage | null | undefined} storage
 * @param {string} prefix
 * @param {readonly string[]} [spared]
 * @returns {number} how many were forgotten
 */
export function forgetPrefsUnder(storage, prefix, spared = []) {
  if (!storage || !prefix) return 0;
  const keys = [];
  try {
    for (let i = 0; i < storage.length; i += 1) {
      const key = storage.key(i);
      if (typeof key !== "string" || !key.startsWith(prefix)) continue;
      if (spared.some((kept) => kept !== "" && key.startsWith(kept))) continue;
      keys.push(key);
    }
  } catch {
    return 0;
  }
  return keys.filter((key) => forgetPref(storage, key)).length;
}

/** A parser for a JSON value, for `readPref`: the parsed value, whatever it is. @param {string} raw */
export function jsonPref(raw) {
  return JSON.parse(raw);
}

/** A parser for a `"1"` / `"0"` switch, for `readPref`: `undefined` — the fallback — for any other word. @param {string} raw */
export function switchPref(raw) {
  return raw === "1" ? true : raw === "0" ? false : undefined;
}

/** A switch as it is stored. @param {boolean} on */
export function switchWord(on) {
  return on ? "1" : "0";
}
