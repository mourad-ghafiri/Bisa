/**
 * The harnesses standing under a heading of the project rail (ide/07 §Why
 * switching is free): which projects of a group — or of a goal's or a
 * workflow's section — have a harness open right now, so a heading says who
 * is working under it without being unfolded.
 *
 * "Open" is the footer's rule (`footerSessionsModel`): a roster session the
 * rail draws (`workstreamSessionsModel.isDrawn`) in a live state, or a shell
 * no session claims whose harness is known — launched, or typed into it —
 * and whose PTY is live. A harness that finished or failed has left; a plain
 * shell is not a harness; a sub-agent is its parent's.
 *
 * Facts and words, no React: the heading draws one mark per distinct harness
 * (`headingMarks`, bounded) and a tip of one line per harness
 * (`headingHarnessWords`, bounded) — *Shop › app — Claude Code · running
 * Edit*. The marks are identity, never attention: the accent is the
 * session's own mark's, on its row.
 */

import { isLive, label as stateLabel } from "../../ui/sessionState.mjs";
import { workstreamSessionRows } from "./workstreamSessionsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The most marks a heading wears — one per distinct harness; a fourth is in the words alone. */
export const MAX_HEADING_MARKS = 3;
/** The most lines the heading's tip holds before *+N more*. */
export const MAX_HEADING_LINES = 4;

/**
 * The harnesses open in one workstream, in the rail's order: the builder's
 * top-level rows that have a harness and are alive.
 * @param {readonly import("../../types").SessionRow[]} sessions
 * @param {readonly import("../../shell/terminalsModel.mjs").TerminalSessionState[]} terminals
 * @param {string} workstream
 * @returns {import("./railHarnessesModel.d.mts").StandingHarness[]}
 */
export function standingHarnesses(sessions, terminals, workstream) {
  const out = [];
  for (const row of workstreamSessionRows(sessions, terminals, workstream)) {
    if (row.parent !== null || !row.harness) continue;
    if (row.kind === "agent") {
      if (!isLive(row.state)) continue;
      out.push({ harness: row.harness, state: row.state ?? null, sessionId: row.id, terminalKey: row.terminalKey ?? null });
    } else if (row.liveness?.status === "live") {
      // A shell whose harness does not report: alive is all it can say.
      out.push({ harness: row.harness, state: null, sessionId: null, terminalKey: row.id });
    }
  }
  return out;
}

/**
 * The marks a heading wears: the distinct harnesses under it, first seen
 * first, at most `MAX_HEADING_MARKS`.
 * @param {readonly {harness: string}[]} list
 * @returns {string[]}
 */
export function headingMarks(list) {
  const out = [];
  for (const h of list) {
    if (out.includes(h.harness)) continue;
    out.push(h.harness);
    if (out.length >= MAX_HEADING_MARKS) break;
  }
  return out;
}

/**
 * The tip's words: how many are open, one line per harness — its place, its
 * name, what it is doing — at most `MAX_HEADING_LINES`, and how many more.
 * @param {readonly import("./railHarnessesModel.d.mts").HeadingHarness[]} list
 * @param {Readonly<Record<string, string>>} labels the harness labels, by id
 * @returns {import("./railHarnessesModel.d.mts").HeadingHarnessWords}
 */
export function headingHarnessWords(list, labels) {
  const lines = list.slice(0, MAX_HEADING_LINES).map((h) => ({
    harness: h.harness,
    text: t("workbench-rail-harnesses-model-line", {
      project: h.project,
      workstream: h.workstream,
      harness: labels?.[h.harness] ?? h.harness,
      state: h.state ? stateLabel(h.state) : t("workbench-rail-harnesses-model-in-shell"),
    }),
  }));
  return { title: t("workbench-rail-harnesses-model-harnesses-open", { n: list.length }), lines, more: Math.max(0, list.length - lines.length) };
}
