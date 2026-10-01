/**
 * The *New drawing* dialog's rules (19 — Drawings), as facts, no React: what
 * a new drawing is called before a hand names it, where it is filed when
 * several places are offered, and when *Create* may be pressed.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

/**
 * The title a template gives a new drawing: its label, or the untitled word
 * for *Empty*.
 * @param {{id: string, label: string} | null | undefined} template
 * @param {string} untitledWord
 */
export function defaultTitle(template, untitledWord) {
  if (!template || template.id === "empty") return untitledWord;
  return template.label;
}

/**
 * Whether the title field still holds a template's default — so switching
 * template may re-fill it — or a person's own words, which are kept.
 * @param {string} current what the field holds
 * @param {string} previousDefault the default of the template just left
 */
export function titleFollowsTemplate(current, previousDefault) {
  return current.trim() === "" || current.trim() === previousDefault.trim();
}

/**
 * The place preselected when several are offered: the one the person stands
 * on (*here*), else the first.
 * @template {{here?: boolean}} T
 * @param {readonly T[]} targets
 * @returns {T | null}
 */
export function firstTarget(targets) {
  if (!targets || targets.length === 0) return null;
  return targets.find((t) => t.here) ?? targets[0];
}

/** A key for a place, so a `<select>` can name it. */
export function targetKey(scope) {
  return `${scope.scope}:${scope.id ?? ""}`;
}

/** *Create* takes a title with words in it. */
export function canCreate(title) {
  return typeof title === "string" && title.trim().length > 0;
}
