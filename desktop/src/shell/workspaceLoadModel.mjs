/**
 * What one workspace load leaves the shell with when some of its reads fail.
 *
 * The shell reads nine routes at boot and after every state-changing frame.
 * They used to go through one `Promise.all`: one 500 — a goal list that
 * could not read a run, an inbox that could not read a goal — rejected the
 * whole load, emptied every list and put the *node unreachable* banner up,
 * and every run frame did it again. A failed run looked like a dead
 * platform.
 *
 * So each read settles on its own. A route that answered is applied; one
 * that did not keeps the last value the shell had and is named in
 * `degraded`, for the chrome to say. *Offline* is one fact only: the node
 * itself could not be reached.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * The reads, settled.
 * @template T
 * @param {readonly string[]} names one per read, in the order the results come
 * @param {readonly PromiseSettledResult<T>[]} results
 * @param {(reason: unknown) => boolean} isOffline whether a rejection is "the node could not be reached"
 * @returns {{ok: Record<string, T>, degraded: string[], offline: boolean}}
 */
export function settleLoads(names, results, isOffline) {
  const ok = {};
  const degraded = [];
  let offline = false;
  names.forEach((name, i) => {
    const r = results[i];
    if (r && r.status === "fulfilled") {
      ok[name] = r.value;
      return;
    }
    const reason = r ? r.reason : new Error("no answer");
    if (isOffline(reason)) offline = true;
    degraded.push(failedRead(name, reason));
  });
  return { ok, degraded, offline };
}

/**
 * The name a read that did not answer goes under: the read, and the reason
 * in its own words.
 * @param {string} name @param {unknown} reason
 */
export function failedRead(name, reason) {
  return `${name}: ${messageOf(reason)}`;
}

/**
 * The reads that did not answer, as the chrome names them: the last load's,
 * then the hosted sections' — two lists, each replaced whole by the read
 * that renews it. The hosted sections are read again on their own
 * (`hosted_changed`), so what one of those reads failed is **its** word
 * alone: the next one that answers whole takes it back. Added to the load's
 * list instead, a host that failed once stayed named until the next full
 * load, and every differently-worded failure was one more line.
 * @param {readonly string[]} load @param {readonly string[]} hosted
 * @returns {string[]}
 */
export function degradedReads(load, hosted) {
  return [...new Set([...(load ?? []), ...(hosted ?? [])])];
}

/**
 * Whether one of the workspace's lists could not be read: the node is away,
 * or the last load's read of that list failed (`failedRead`'s `name:`). A
 * list in that state is not empty, only unknown — a screen says so and
 * offers the read again, never a door to create the first one.
 * @param {readonly string[] | null | undefined} degraded the reads that did not answer
 * @param {string | null | undefined} offline the node's absence, when it is away
 * @param {string} name a read's name (`LOAD_NAMES`): `goals`, `channels`, `dms` …
 */
export function listUnread(degraded, offline, name) {
  if (offline) return true;
  const prefix = `${name}:`;
  return (degraded ?? []).some((d) => typeof d === "string" && d.startsWith(prefix));
}

/** The one sentence a failed read leaves: an error's message, else its text. */
function messageOf(reason) {
  if (reason instanceof Error) return reason.message;
  return String(reason);
}

/**
 * The chrome's words for the reads that failed — none when every read
 * answered; the node being unreachable is said elsewhere and not repeated.
 * @param {readonly string[]} degraded
 * @param {boolean} offline
 * @returns {{label: string, title: string} | null}
 */
export function degradedWords(degraded, offline) {
  if (offline || degraded.length === 0) return null;
  const n = degraded.length;
  return {
    label: n === 1 ? t("shell-workspace-load-1-list-could-read") : t("shell-workspace-load-lists-could-read", { n }),
    title: t("shell-workspace-load-last-values-shown", { degraded: degraded.join("\n") }),
  };
}

/**
 * Reload once on the bus's closed→open edge — and only that edge: the
 * first `open` a page sees is its boot, which loaded already, and a
 * `connecting` in between is not a gap. What a node forgot in a restart
 * (the sessions it drove, the gates it held) and what it did at boot (the
 * steps it resumed or failed, the questions it withdrew) reach the page by
 * no frame, so the lists are read again whole.
 * A `lagged` pulse — the node dropped events this client was too slow for —
 * is a gap too, and reloads at once.
 * @param {(cb: (state: "connecting" | "open" | "closed" | "lagged") => void) => () => void} watch the bus's connection watcher
 * @param {() => void} reload
 * @returns {() => void} stop watching
 */
export function reloadOnReconnect(watch, reload) {
  let wasClosed = false;
  return watch((state) => {
    if (state === "lagged") {
      reload();
      return;
    }
    if (state === "open" && wasClosed) reload();
    if (state !== "connecting") wasClosed = state === "closed";
  });
}

/**
 * The name a hosted read that failed goes under in `degraded` — the host by
 * its name, the list that could not be read, the reason — so a host whose
 * channels could not be read is never mistaken for a host with none.
 * @param {string} hostLabel @param {"channels" | "dms"} what @param {unknown} reason
 */
export function hostedFailure(hostLabel, what, reason) {
  return t("shell-workspace-load-hosted-read-failed", { host: hostLabel, what: what === "dms" ? "dms" : "channels", reason: messageOf(reason) });
}

/**
 * The shell's line while the node cannot be reached: the reason the
 * shell gave when it could not start one, else the plain wait.
 */
export function offlineWords(reason) {
  return reason ? t("shell-workspace-load-retrying", { reason }) : t("shell-workspace-load-waiting-node");
}

/**
 * The shell's line while the node is away, `null` while it is there. Two
 * things say it is away: a load that could not reach it, and the bus — a
 * stream that ended says so the moment it ends, where a load says so only
 * when one is made, and none is while no frame arrives. So a node that goes
 * away under an open window is said at once, and the line stands until the
 * stream is back and the load it starts has answered.
 * @param {string | null} loadSaid what the last load left: its offline line, or `null`
 * @param {"connecting" | "open" | "closed" | "lagged"} conn the bus's word
 * @param {string | null} reason why the shell has no node, when it said
 * @returns {string | null}
 */
export function offlineLine(loadSaid, conn, reason) {
  if (conn === "closed") return offlineWords(reason);
  return loadSaid ?? null;
}

/** The toast when the lists were read again after the node came back. */
export function reconnectWords() {
  return t("shell-workspace-load-node-came-back-everything-read-again");
}

/**
 * The engine facts that move what the workspace index holds — the goals and
 * their statuses, the projects and workstreams, the workflows, the members,
 * what is owed — and so are worth reading it again (coalesced by the caller).
 * An agent's tokens, a file changing, a terminal's state are not among them.
 */
export const RELOADS_WORKSPACE = Object.freeze([
  // People on other nodes and invitations move the members, the rosters
  // and the Inbox (14-collaboration).
  "people_changed",
  "invite_changed",
  "message_held",
  "message_released",
  // Projects and workstreams are part of the one index.
  "project_created",
  "project_changed",
  "attachment_changed",
  "workstream_opened",
  "workstream_changed",
  "workstream_edited",
  "gate_opened",
  "question_asked",
  "gate_decided",
  // A run moving is the goal's status moving; a workflow proposed or saved
  // changes what the Goals and Workflows screens list.
  // A goal or a project made, archived or deleted anywhere — the CLI, an
  // agent, a step, another window — moves the lists this index holds.
  "goal_created",
  "goal_archived",
  "goal_deleted",
  "project_archived",
  "project_deleted",
  "run_started",
  "run_queued",
  "run_finished",
  "run_cancelled",
  "step_changed",
  "goal_closed",
  "workflow_proposed",
  "workflow_changed",
  "workflow_deleted",
  "workflow_archived",
  "result_accepted",
  // A start event that could not start its run, and the Workflow Agent's own
  // phases — a repair that stalls or a proposal — change what is owed too.
  "listener_failed",
  "guided",
  // A workflow turned On or Off, a goal armed or cleared: what a row says it
  // listens for, and a listening goal's status, move with it.
  "listening_changed",
  "execution_ended",
  "scheduled",
]);

/** Whether an engine fact of this type is worth reading the workspace index again. */
export function reloadsWorkspace(type) {
  return typeof type === "string" && RELOADS_WORKSPACE.includes(type);
}

