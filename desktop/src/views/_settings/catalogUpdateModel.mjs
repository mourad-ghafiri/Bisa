/**
 * Update from the catalog, as the Library row offers it (06 § Skills; the
 * guide's catalog section). A row's one act follows two facts the node
 * sends: `installed` (the id is taken) and `installed_differs` (this entry's
 * own copy no longer says what the catalog says — present only for a skill,
 * an agent or a team installed from this very entry). Pure, so `node --test`
 * reads it; the view words what it returns.
 */

/**
 * The kinds a person may bring back to the catalog's definition; every other
 * kind is installed once and is then theirs.
 */
export const UPDATABLE_KINDS = Object.freeze(["skill", "agent", "team"]);

/**
 * What the row offers: `install` while the id is free, `update` when this
 * entry's copy differs from the catalog, `installed` otherwise — an id held
 * by something else included, since an update would refuse it by name.
 * @param {{ kind: string, installed: boolean, installed_differs?: boolean | null }} entry
 * @returns {"install" | "update" | "installed"}
 */
export function rowAction(entry) {
  if (!entry.installed) return "install";
  return entry.installed_differs === true && UPDATABLE_KINDS.includes(entry.kind) ? "update" : "installed";
}

/**
 * What an update's answer means for the person: `restated` when the copy was
 * brought back to the catalog's definition, `created` when nothing was
 * replaced but something the entry needed was installed on the way, and
 * `unchanged` when it already matched and nothing was missing.
 * @param {{ restated: boolean }} updated
 * @param {Record<string, unknown>} installed the install report, a list per kind
 * @returns {"restated" | "created" | "unchanged"}
 */
export function updateOutcome(updated, installed) {
  if (updated.restated) return "restated";
  const created = Object.values(installed).some((v) => Array.isArray(v) && v.length > 0);
  return created ? "created" : "unchanged";
}
