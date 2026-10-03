/**
 * What a terminal's lines are called once they are attached to a message
 * (ide/09, *Send to agent*): the tab's own words — what runs in it, and
 * where — never a bare `shell` literal or the tab's internal key. Two tabs
 * that read alike are told apart by their order along the strip, so the
 * second shell's lines never take the first one's chip (a chip is the same
 * chip when its name is the same).
 */

import { terminalTabLabel } from "./terminalsModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * @param {{ key: string }} session the tab whose lines are attached
 * @param {ReadonlyArray<{ key: string }>} sessions every open tab, in strip order
 */
export function terminalChipName(session, sessions) {
  const label = terminalTabLabel(/** @type {any} */ (session));
  const alike = sessions.filter((s) => terminalTabLabel(/** @type {any} */ (s)) === label);
  if (alike.length < 2) return label;
  return t("shell-terminal-chip-nth", { label, n: alike.findIndex((s) => s.key === session.key) + 1 });
}
