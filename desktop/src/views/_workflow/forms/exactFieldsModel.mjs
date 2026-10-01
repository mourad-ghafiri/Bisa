/**
 * Exact matches on an event's fields — `path = value`, every one must hold —
 * and an emit's payload, as the rows a person edits (03-workflows §Start
 * events · §Throw and end). The wire's shape is a map, so a path is a key:
 * two rows cannot share one, and a rename that landed on another row's
 * path would take that row's value with it. So a path is renamed **where
 * it stands** — its row keeps its place and its value — and never onto
 * another; a new row takes a path no row has; a value is written on its
 * row alone. No function writes to the map it is handed.
 *
 * Pure, so `node --test` reads it; `ExactFieldsEditor.tsx` draws the rows
 * and commits a path when the field is left.
 */

import { t } from "../../../i18n/l10n.mjs";

/** The longest path a field may have. */
export const MAX_PATH = 128;

/** The path a new row starts from: `field`, `field-2`, … */
const PATH_ROOT = "field"; // for the machine

/**
 * The rows, in the order they were written.
 * @param {Record<string, string> | null | undefined} fields
 * @returns {{path: string, value: string}[]}
 */
export function fieldRows(fields) {
  return Object.entries(fields ?? {}).map(([path, value]) => ({ path, value }));
}

/** A path no row has: `field`, then `field-2`, `field-3`… — the first that is free. */
export function freshPath(fields) {
  const taken = new Set(Object.keys(fields ?? {}));
  if (!taken.has(PATH_ROOT)) return PATH_ROOT;
  for (let n = 2; ; n++) {
    const path = `${PATH_ROOT}-${n}`;
    if (!taken.has(path)) return path;
  }
}

/** The map with a fresh, empty row at its end. */
export function addField(fields) {
  return { ...(fields ?? {}), [freshPath(fields)]: "" };
}

/** The map without one row; the same map when no row has the path. */
export function removeField(fields, path) {
  if (!fields || !Object.prototype.hasOwnProperty.call(fields, path)) return fields;
  const { [path]: _gone, ...rest } = fields;
  return rest;
}

/** The map with one row's value written, its place kept; the same map when no row has the path. */
export function setFieldValue(fields, path, value) {
  if (!fields || !Object.prototype.hasOwnProperty.call(fields, path)) return fields;
  return Object.fromEntries(Object.entries(fields).map(([k, v]) => [k, k === path ? value : v]));
}

/**
 * One row's path renamed where it stands. Refused, with the reason, when
 * the path is blank or too long, when no row has the old one, and when
 * another row already has the new one — that row would be lost.
 * @param {Record<string, string> | null | undefined} fields
 * @param {string} from
 * @param {string} to
 * @returns {{ok: true, fields: Record<string, string>} | {ok: false, reason: string}}
 */
export function renameField(fields, from, to) {
  const next = String(to ?? "").trim();
  if (!fields || !Object.prototype.hasOwnProperty.call(fields, from)) return { ok: false, reason: t("workflow-exact-fields-no-such-field", { path: from }) };
  if (next === from) return { ok: true, fields };
  if (!next) return { ok: false, reason: t("workflow-exact-fields-needs-path") };
  if (next.length > MAX_PATH) return { ok: false, reason: t("workflow-exact-fields-path-too-long", { max: MAX_PATH }) };
  if (Object.prototype.hasOwnProperty.call(fields, next)) return { ok: false, reason: t("workflow-exact-fields-path-taken", { path: next }) };
  return { ok: true, fields: Object.fromEntries(Object.entries(fields).map(([k, v]) => [k === from ? next : k, v])) };
}
