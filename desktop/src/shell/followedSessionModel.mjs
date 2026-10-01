/**
 * Which harness session a workstream is *about* — the one the pet follows
 * while that workstream is in view (ide/09).
 *
 * Two rules, in order. The session the person last **chose** in the
 * workstream wins while it is still on the roster and still in that
 * workstream — clicking a row in the Agent panel, an agent row on the rail,
 * or bringing a harness tab to the centre all choose. With no choice, or a
 * choice that left, the **loudest live** session stands in — attention rank,
 * then newest, the same order the Agent panel sorts by, so the pet and the
 * panel's status line agree — and failing that the newest ended one still on
 * the roster, because a harness that just failed is exactly what the pet
 * should be saying.
 */

import { isEnded, isLive, sortRows } from "../ui/sessionState.mjs";
import { t as tr } from "../i18n/l10n.mjs";

const TERMINAL_DOC = "terminal:";

/**
 * The chosen session, when it can still be followed here.
 * @param {string | null | undefined} chosen
 * @param {readonly import("../types").SessionRow[]} sessions
 * @param {string} workstream
 */
export function chosenSession(chosen, sessions, workstream) {
  if (!chosen) return null;
  const row = (sessions ?? []).find((s) => s.id === chosen);
  return row && row.workstream === workstream ? row : null;
}

/**
 * The session a workstream is about when nobody chose one.
 * @param {readonly import("../types").SessionRow[]} sessions
 * @param {string} workstream
 */
export function fallbackSession(sessions, workstream) {
  const here = (sessions ?? []).filter((s) => s.workstream === workstream);
  const live = sortRows(here.filter((s) => isLive(s.state)));
  if (live.length > 0) return live[0];
  const ended = here.filter((s) => isEnded(s.state)).sort((a, b) => (b.since ?? 0) - (a.since ?? 0));
  return ended[0] ?? null;
}

/**
 * The followed session: the choice, else the fallback; null off a
 * workstream or in one with nothing to follow.
 * @param {string | null | undefined} chosen
 * @param {readonly import("../types").SessionRow[]} sessions
 * @param {string | null | undefined} workstream
 */
export function followedSession(chosen, sessions, workstream) {
  if (!workstream) return null;
  return chosenSession(chosen, sessions, workstream) ?? fallbackSession(sessions, workstream);
}

/**
 * The roster session behind the centre's document, when that document is a
 * terminal tab whose harness the node registered — how bringing a tab to
 * the centre chooses.
 * @param {readonly { key: string, sessionId: string | null }[]} terminals
 * @param {string | null | undefined} doc
 */
export function terminalSessionOf(terminals, doc) {
  if (typeof doc !== "string" || !doc.startsWith(TERMINAL_DOC)) return null;
  const key = doc.slice(TERMINAL_DOC.length);
  return (terminals ?? []).find((t) => t.key === key)?.sessionId ?? null;
}

/**
 * Who is being followed, for a title: `general-agent on claude-code`, or
 * just the harness when the session has no agent behind it.
 * @param {{ agent?: string | null, harness: string }} row
 */
export function followWords(row) {
  if (row.agent && row.agent !== row.harness) return tr("shell-followed-session-words", { agent: row.agent, harness: row.harness });
  return row.harness;
}
