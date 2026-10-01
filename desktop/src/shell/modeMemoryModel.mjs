/**
 * A screen's mode, remembered per thing — the rules the Project IDE keeps
 * for its centre (`ideModeModel.mjs`), written once so a second screen with
 * modes of its own would keep them the same way. A mode is
 * furniture: which one a key — a root, a workflow, a goal — is in is kept
 * newest last and capped; which one it *opens* in, when nothing was
 * remembered, is a setting the caller resolves. The vocabularies are the
 * callers': every rule here takes the modes offered and the one to fall to.
 */

/** How many keys a memory holds before the oldest is forgotten. */
export const DEFAULT_CAP = 32;

/**
 * A stored or typed mode made safe to render: one of `offered`, else `fallback`.
 * @template {string} M
 * @param {unknown} value
 * @param {readonly M[]} offered
 * @param {M} fallback
 * @returns {M}
 */
export function modeIn(value, offered, fallback) {
  return typeof value === "string" && offered.includes(/** @type {M} */ (value)) ? /** @type {M} */ (value) : fallback;
}

/**
 * The mode a key is in: what was remembered for it when that is still
 * offered, else the settings' default — itself clamped, since it crosses
 * the wire — else the vocabulary's own.
 * @template {string} M
 * @param {unknown} remembered
 * @param {unknown} setting
 * @param {readonly M[]} offered
 * @param {M} fallback
 * @returns {M}
 */
export function modeFor(remembered, setting, offered, fallback) {
  if (typeof remembered === "string" && offered.includes(/** @type {M} */ (remembered))) return /** @type {M} */ (remembered);
  return modeIn(setting, offered, fallback);
}

/**
 * The mode a cycle goes to: the next in the switch's order, round to the
 * first, over the modes offered.
 * @template {string} M
 * @param {unknown} mode
 * @param {readonly M[]} offered
 * @param {M} fallback
 * @returns {M}
 */
export function nextMode(mode, offered, fallback) {
  const at = offered.indexOf(modeIn(mode, offered, fallback));
  return offered[(at + 1) % offered.length];
}

/**
 * Remember a key's mode, newest last, the oldest forgotten past `cap` —
 * the right panel's own rule for its occupant memory. The same memory when
 * nothing changed, so a store commits nothing.
 * @template {string} M
 * @param {Readonly<Record<string, M>>} byKey
 * @param {string} key
 * @param {M} mode
 * @param {number} [cap]
 * @returns {Readonly<Record<string, M>>}
 */
export function rememberMode(byKey, key, mode, cap = DEFAULT_CAP) {
  if (byKey[key] === mode) return byKey;
  const entries = Object.entries(byKey).filter(([k]) => k !== key);
  entries.push([key, mode]);
  return Object.fromEntries(entries.slice(-Math.max(1, cap)));
}

/**
 * A stored memory read back: only `kind:id` keys and offered modes survive,
 * so a vocabulary that changed in a later version selects nothing that no
 * longer exists.
 * @template {string} M
 * @param {unknown} raw
 * @param {readonly M[]} offered
 * @returns {Record<string, M>}
 */
export function parseRememberedModes(raw, offered) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
  /** @type {Record<string, M>} */
  const out = {};
  for (const [key, mode] of Object.entries(raw)) {
    if (typeof key === "string" && key.includes(":") && typeof mode === "string" && offered.includes(/** @type {M} */ (mode))) out[key] = /** @type {M} */ (mode);
  }
  return out;
}
