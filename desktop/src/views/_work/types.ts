/**
 * Local corrections to shared wire types, and the helpers that grew up beside
 * them.
 *
 * `BoardRow` used to live here — a corrected copy of `GoalRow`, because the
 * shared type claimed `GET /goals` sent no statement and no tags when it
 * sends both. `GoalRow` says what the node actually sends now, so the copy
 * is gone and its callers use the shared type.
 *
 * What is left is the governance pair below, which is still wrong upstream,
 * and two helpers every list of goals wants.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * `Assignee` on the wire is a tagged object; the routes that *accept* one take
 * the compact string form instead (`agent:<id>`, `human:<hex>`, `team:<id>`),
 * because a CLI argument and a governance `Listed` entry both need it flat.
 *
 * The conversion itself used to be written out here *and* in
 * `AssigneePicker.tsx`, which is the arrangement where one copy learns about a
 * fourth kind and the other silently does not. It lives in `assigneeWire.mjs`
 * now — plain JavaScript, so `node --test` can check the round trip — and this
 * is the name the boards have always imported.
 */
export { assigneeToWire as assigneeWire } from "./assigneeWire.mjs";

/** What to call a goal: its title, else its own words, else its id. */
export function boardLabel(row: {
  title?: string | null;
  statement?: string;
  id: string;
}): string {
  if (row.title?.trim()) return row.title;
  const stated = row.statement?.trim();
  if (stated) return stated;
  return t("work-types-goal", { row: row.id.slice(-6) });
}
