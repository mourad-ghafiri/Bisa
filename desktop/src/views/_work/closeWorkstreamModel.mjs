/**
 * What closing a workstream terminates, as facts (ide/07): the harnesses a
 * person opened in its terminals, the plain shells there, and the agent
 * sessions the engine runs in it — counted once, said once, in the
 * vocabulary each has. A harness is **terminated** (closing its tab is what
 * ends it), a shell is **closed**, an engine session is **aborted**. The
 * engine stops the sessions when the record closes; the desktop closes the
 * tabs (`closeWorkstream.ts`). The consent line in every close dialog and
 * the toast after read from here, so the two never disagree.
 *
 * A harness in a terminal is also a roster row (`kind: interactive`) — the
 * tab *is* the row (`workstreamSessionsModel`) — so it is counted as the
 * tab and never again as a session.
 */

import { isLive as sessionIsLive } from "../../ui/sessionState.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/**
 * @typedef {{harnesses: number, shells: number, agents: number}} TerminationCounts
 *   live terminal tabs rooted at the workstream running a harness; live tabs
 *   running a plain shell; live engine sessions on the workstream that are
 *   not a terminal's.
 */

/**
 * What a close would terminate, counted from the two stores.
 * @param {readonly import("../../types").SessionRow[] | null | undefined} sessions the roster
 * @param {readonly import("../../shell/terminalsModel.mjs").TerminalSessionState[] | null | undefined} terminals this app's tabs
 * @param {string} workstream
 * @returns {TerminationCounts}
 */
export function terminationCounts(sessions, terminals, workstream) {
  const tabs = (terminals ?? []).filter((t) => t.scope === "workstream" && t.id === workstream && t.liveness?.status === "live");
  const harnesses = tabs.filter((t) => t.harness !== null && t.harness !== undefined).length;
  const agents = (sessions ?? []).filter((s) => s.workstream === workstream && s.kind !== "terminal" && sessionIsLive(s.state)).length;
  return { harnesses, shells: tabs.length - harnesses, agents };
}

/**
 * The phrase for what a close terminates — *1 harness terminated, 2 shells
 * closed and 1 agent session aborted* — or `null` when nothing stands there.
 * @param {TerminationCounts} counts
 */
export function terminationWords(counts) {
  const parts = [];
  // How many of each, and the word's number with it, are the message's to say.
  if (counts.harnesses > 0) parts.push(tr("work-close-workstream-terminated", { n: counts.harnesses }));
  if (counts.shells > 0) parts.push(tr("work-close-workstream-closed", { n: counts.shells }));
  if (counts.agents > 0) parts.push(tr("work-close-workstream-aborted", { n: counts.agents }));
  if (parts.length === 0) return null;
  if (parts.length === 1) return parts[0];
  return tr("work-close-workstream-words", { parts: parts.slice(0, -1).join(", "), parts2: parts[parts.length - 1] });
}

/**
 * The sentence a close dialog says before the person confirms — the one
 * consent for the tabs, which close without the terminal guard asking again
 * — or `null` when there is nothing to say.
 * @param {TerminationCounts} counts
 */
export function terminationConsent(counts) {
  const words = terminationWords(counts);
  return words ? tr("work-close-workstream-closing-ends-what-stands", { words }) : null;
}
