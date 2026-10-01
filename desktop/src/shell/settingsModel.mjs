/**
 * Reading a resolved setting — the fact behind every "the default comes from
 * the setting" control.
 *
 * `GET /settings/resolved[?project=]` answers a list of `{key, value, origin}`;
 * this turns it into one lookup with a fallback, and says whether a value is
 * one of the words a `Choice` key allows — so a stale or foreign value falls
 * back instead of being rendered as a label nobody wrote.
 */

/**
 * The resolved value of `key`, or `fallback` when the list has none.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 */
export function settingOf(resolved, key, fallback) {
  const row = (resolved ?? []).find((r) => r.key === key);
  return row === undefined || row.value === undefined || row.value === null ? fallback : row.value;
}

/**
 * The resolved value of a `Choice` key, kept to the words it allows.
 * @template {string} T
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @param {string} key
 * @param {readonly T[]} allowed
 * @param {T} fallback
 * @returns {T}
 */
export function choiceOf(resolved, key, allowed, fallback) {
  const v = settingOf(resolved, key, fallback);
  return typeof v === "string" && allowed.includes(/** @type {T} */ (v)) ? /** @type {T} */ (v) : fallback;
}

/**
 * The resolved value of a numeric key, kept inside `[min, max]` when bounds
 * are given; anything that is not a finite number is the fallback. A string
 * that parses is accepted, because the raw settings panel writes what was
 * typed.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @param {string} key
 * @param {number} fallback
 * @param {{min?: number, max?: number}} [bounds]
 * @returns {number}
 */
export function numberOf(resolved, key, fallback, bounds = {}) {
  const raw = settingOf(resolved, key, fallback);
  const n = typeof raw === "number" ? raw : typeof raw === "string" && raw.trim() !== "" ? Number(raw) : Number.NaN;
  if (!Number.isFinite(n)) return fallback;
  const lo = bounds.min ?? -Infinity;
  const hi = bounds.max ?? Infinity;
  return Math.min(hi, Math.max(lo, n));
}

/**
 * The resolved value of a text key, trimmed; anything that is not a string
 * is the fallback. An empty string is a value — a key whose empty means
 * *off* reads it as such.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @param {string} key
 * @param {string} fallback
 * @returns {string}
 */
export function stringOf(resolved, key, fallback) {
  const v = settingOf(resolved, key, fallback);
  return typeof v === "string" ? v.trim() : fallback;
}

/** The resolved value of a boolean key; anything that is not a boolean is the fallback. */
export function boolOf(resolved, key, fallback) {
  const v = settingOf(resolved, key, fallback);
  return typeof v === "boolean" ? v : fallback;
}
