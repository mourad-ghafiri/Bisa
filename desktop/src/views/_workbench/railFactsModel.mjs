/**
 * The facts a workstream row wears beside its name in the project rail:
 * how far its branch stands from its base — `+3` ahead, `−1` behind — and
 * nothing else. The tree's state (staged, unstaged, untracked) is never a
 * word here: on a working checkout it is nearly always on, so it says
 * nothing and adds noise to every row; the Git tab's mark on the right
 * panel's rail (`railBadgesModel.gitBadge`) says it once, where it is acted
 * on. The Workstreams card reads the same counts the same way
 * (`workstreamCardModel`), so the rail and the card agree on the branch.
 */

import { t } from "../../i18n/l10n.mjs";

/** The rows' separator between facts. */
const JOIN = " · ";

/**
 * @param {number | null | undefined} n
 * @returns {n is number}
 */
function some(n) {
  return typeof n === "number" && n > 0;
}

/**
 * @param {number} n
 * @returns {string}
 */
function commits(n) {
  return t("workbench-rail-facts-commit-commits", { n });
}

/**
 * The row's facts, in order: ahead of the base, then behind it. A missing
 * status, the primary (no base) or a branch level with its base wears none.
 *
 * @param {{ base?: string | null, ahead_of_base?: number | null, behind_base?: number | null } | null | undefined} status
 * @returns {readonly { text: string, title: string }[]}
 */
export function workstreamFacts(status) {
  if (!status) return [];
  const base = status.base ?? t("workbench-rail-facts-base");
  const out = [];
  if (some(status.ahead_of_base)) out.push({ text: `+${status.ahead_of_base}`, title: t("workbench-rail-facts-beyond", { ahead_of_base: commits(status.ahead_of_base), base }) });
  if (some(status.behind_base)) out.push({ text: `−${status.behind_base}`, title: t("workbench-rail-facts-not-here", { behind_base: commits(status.behind_base), base }) });
  return out;
}

/**
 * The one string the row paints, and the one its tooltip carries.
 *
 * @param {readonly { text: string, title: string }[]} facts
 * @returns {{ text: string, title: string }}
 */
export function factsWords(facts) {
  return { text: facts.map((f) => f.text).join(JOIN), title: facts.map((f) => f.title).join(JOIN) };
}
