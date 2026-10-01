/**
 * The words around a session's transcript (ide/09): what the pane and the
 * tab call it, and the one line under the title saying whether the text is
 * still growing. The transcript itself is the harness's own file as the
 * node serves it — lines, tailed by byte — so nothing here parses it.
 * Pure: the roster row in, words out.
 */

import { isLive } from "../../ui/sessionState.mjs";
import { headlineOf } from "../_workbench/workstreamPulseModel.mjs";
import { modelWords } from "../../ui/modelWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The title: the agent (else the harness), then the harness and model
 * behind it, and the effort it runs at when the row carries one —
 * *dev · claude-code · claude-opus-5-5[1m] · high*.
 * @param {{agent?: string | null, harness?: string | null, model?: string | null, effort?: string | null} | null | undefined} row
 */
export function transcriptTitle(row) {
  if (!row) return t("work-session-transcript-transcript");
  const who = row.agent ?? row.harness ?? "session";
  const behind = [row.agent ? row.harness : null, modelWords(row.model, row.effort)?.short ?? null].filter(Boolean);
  return [who, ...behind].join(" · ");
}

/**
 * The line under the title: the state, and whether the tail still follows.
 * A row the roster no longer holds is gone; what it wrote stays readable.
 * @param {{state: string | object} | null | undefined} row
 * @returns {{words: string, live: boolean}}
 */
export function transcriptWords(row) {
  if (!row) return { words: t("work-session-transcript-session-gone-from-roster-what-wrote"), live: false };
  const live = isLive(row.state);
  // The pulse line's words: a running tool with what it was given.
  return { words: live ? t("work-session-transcript-following-writes", { state: headlineOf(row.state) }) : t("work-session-transcript-transcript-complete", { state: headlineOf(row.state) }), live };
}

/**
 * The workbench tab's word for it: the agent, else the harness.
 * @param {{agent?: string | null, harness?: string | null} | null | undefined} row
 */
export function transcriptTabTitle(row) {
  return row?.agent ?? row?.harness ?? "";
}
