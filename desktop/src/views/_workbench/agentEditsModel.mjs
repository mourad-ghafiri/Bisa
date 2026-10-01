/**
 * An edit handed to an agent, followed to its end (ide/03 §Annotate,
 * ide/09): when the annotation tray or the selection toolbar asks an agent
 * to edit a file, the document remembers who was asked and when, and the
 * roster's next move of that agent's turn in that conversation — from a
 * live state to a settled one — is the moment the page reads the file
 * again and says so. The record is the session's, never persisted, and
 * forgotten once it settled.
 *
 * The state words are `sessionState.mjs`'s; a test holds the two lists to
 * that vocabulary.
 */

import { stateOf } from "../../ui/sessionState.mjs";
import { t } from "../../i18n/l10n.mjs";

/** A session that is at work — starting, thinking, running a tool, or waiting on the person. */
export const LIVE = Object.freeze(["starting", "thinking", "running", "waiting"]);
/** A session that has come to rest — between turns, finished, failed, stopped, or parked. */
export const SETTLED = Object.freeze(["idle", "done", "failed", "aborted", "parked"]);

/**
 * Where a document's record is kept: the root and the path.
 * @param {string} scope
 * @param {string} id
 * @param {string} path
 */
export function recordKey(scope, id, path) {
  return `${scope}:${id}|${path}`;
}

/**
 * Whether a roster move settles a record: the same conversation, the same
 * agent, a live state before and a settled one after, entered no earlier
 * than the ask. A session first seen already settled (`prev` null) is one
 * that was at work before the roster knew it — it settles too when it is
 * newer than the ask.
 * @param {{scope: string, id: string, agentId: string, at: number}} record
 * @param {{state: string} | string | null} prev the state before, `null` for a session first seen
 * @param {{state: {state: string} | string, since: number, agent?: string | null, conversation?: string | null}} row the roster row now
 */
export function settles(record, prev, row) {
  if (record.scope !== "conversation" || row.conversation !== record.id || row.agent !== record.agentId) return false;
  const after = stateOf(row.state);
  if (!SETTLED.includes(after)) return false;
  if (prev !== null && !LIVE.includes(stateOf(prev))) return false;
  return row.since >= record.at;
}

/**
 * What the page says when the agent came to rest, by how it did.
 * @param {string} agentName
 * @param {string} path
 * @param {string} state the settled state word
 */
export function settledWords(agentName, path, state) {
  const name = String(path ?? "").split("/").pop() || String(path ?? "");
  switch (state) {
    case "failed":
      return t("workbench-agent-edits-failed-while-editing-agent-panel-has", { agentName, name });
    case "aborted":
      return t("workbench-agent-edits-stopped-before-finished", { agentName, name });
    default:
      return t("workbench-agent-edits-finished-keep-undo-change-conversation-file", { agentName, name });
  }
}

/**
 * The toast's tone for a settled state: an error for a failure, a note for
 * a stop, a success otherwise.
 * @param {string} state
 */
export function settledTone(state) {
  if (state === "failed") return "error";
  if (state === "aborted") return "info";
  return "ok";
}
