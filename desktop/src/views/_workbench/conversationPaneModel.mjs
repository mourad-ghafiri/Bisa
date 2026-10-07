/**
 * The IDE's Agent pane over conversations (ide/09): the sessions that are a
 * conversation's turns and what its bar says of them, which conversation a
 * hand-off from the editor lands in, and the memory of which roots show the
 * list. No DOM.
 *
 * The pane's surface — the list, the pick, the views, *New conversation* —
 * is every owner's (`_studio/conversationSurfaceModel.mjs`): the IDE brings
 * its own thread wiring and its own source (the project's conversations,
 * every checkout's) to the one surface. What stays the IDE's alone is the
 * hand-off: a page annotation, a selection, a terminal's tail goes to the
 * conversation the checkout is on when its turns run here — the remembered
 * pick, else the newest whose turns run here (`currentConversation`, the one
 * guess the desktop makes, and it writes the pick) — and starts one about the
 * workstream otherwise: an annotation is only ever sent to a conversation,
 * never to a session.
 */

import { titleOf } from "../../shell/sessionOriginModel.mjs";
import { projectConversations, runsHere } from "../_studio/conversationsModel.mjs";
import { counts, isLive, isStoppable, loudest, sortRows } from "../../ui/sessionState.mjs";
import { headlineOf } from "./workstreamPulseModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The `localStorage` key the roots whose pane shows the list live under (`agentPaneViewStore.ts`). */
export const VIEW_KEY = "bisa.ide.agents.view";
/** The one value a root remembers under `VIEW_KEY`: its pane is on the list. */
export const LIST_VIEW = "list";
/** How many roots' choices are kept. */
export const MAX_REMEMBERED = 32;

/**
 * A memory as stored: key → value, both strings. Anything that is not a
 * record of strings is no memory at all — the picks' and the folds' shape.
 * @param {unknown} raw
 * @returns {Record<string, string>}
 */
export function parseRemembered(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
  const out = {};
  for (const [k, v] of Object.entries(raw)) if (typeof v === "string" && v) out[k] = v;
  return out;
}

/**
 * Remember a value for a key, dropping the oldest past the cap. `null`
 * forgets the key's.
 * @param {Record<string, string>} memory
 * @param {string} root
 * @param {string | null} conversation
 */
export function remember(memory, root, conversation) {
  const next = { ...memory };
  delete next[root];
  if (conversation) next[root] = conversation;
  const keys = Object.keys(next);
  while (keys.length > MAX_REMEMBERED) delete next[keys.shift()];
  return next;
}

/**
 * The conversation a hand-off from a checkout lands in: the remembered one
 * while it is still listed live in the project — a sibling checkout's
 * included, since the person picked it — else the newest live one whose
 * turns run here (the checkout's own or the project's), else the newest of
 * the project, else none. A turn runs where its conversation is about (13
 * §Origins), so a hand-off lands in a sibling's conversation only by a
 * person's choice. The surface itself never guesses: `ensureConversation`
 * reads this and writes the pick.
 * @template {{id: string, origin: {kind: string, id?: string}, project?: string | null, archived: boolean, last_message_at?: number | null, created_at: number, agents: readonly string[]}} R
 * @param {readonly R[]} rows every conversation the panel holds
 * @param {string} workstream
 * @param {string} project
 * @param {string | null | undefined} remembered
 * @returns {R | null}
 */
export function currentConversation(rows, workstream, project, remembered) {
  const here = projectConversations(rows, project);
  return here.find((r) => r.id === remembered) ?? here.find((r) => runsHere(r, workstream, project)) ?? here[0] ?? null;
}

/**
 * The roster rows that are a conversation's turns — every kind of session
 * that names it — with their sub-agents one level in, by attention then
 * recency. What the panel's session line, *Stop* and the transcript door read.
 * @param {readonly import("../../types").SessionRow[]} sessions
 * @param {string | null | undefined} conversation
 */
export function sessionsOf(sessions, conversation) {
  if (!conversation) return [];
  return sortRows((sessions ?? []).filter((s) => s.conversation === conversation)).map((s) => ({
    id: s.id,
    label: titleOf(s),
    kind: s.kind,
    state: s.state,
    since: s.since,
    harness: s.harness ?? null,
    model: s.model ?? null,
    effort: s.effort ?? null,
    cost: s.cost ?? null,
    goal: s.goal ?? null,
    workItem: s.work_item ?? null,
    workstream: s.workstream ?? null,
    gateId: s.state && typeof s.state === "object" && s.state.state === "waiting" ? (s.state.on?.gate_id ?? null) : null,
    abortable: isStoppable(s.state),
    transcript: Boolean(s.transcript_path),
    terminalKey: null,
    children: (s.children ?? []).map((c) => ({ id: c.id, name: c.name, description: c.description, state: c.state, since: c.since })),
  }));
}

/**
 * What the panel's bar says of a conversation's turns, and the one *Stop*
 * names. The words are the roster's: `counts` includes sub-agents and
 * `headlineOf` is the pulse line's wording.
 * @param {readonly ReturnType<typeof sessionsOf>[number][]} rows
 */
export function turnSummary(rows) {
  const list = rows ?? [];
  const tally = counts(list);
  const parts = [];
  if (tally.working > 0) parts.push(t("workbench-conversation-pane-working", { working: tally.working }));
  if (tally.waiting > 0) parts.push(t("workbench-conversation-pane-waiting", { waiting: tally.waiting }));
  if (parts.length === 0 && tally.live > 0) parts.push(t("workbench-conversation-pane-idle", { live: tally.live }));
  const subject = list.find((r) => isLive(r.state)) ?? null;
  return {
    live: tally.live,
    waiting: tally.waiting,
    working: tally.working,
    state: loudest(list),
    words: list.length === 0 ? "" : parts.join(" · ") || t("workbench-conversation-pane-idle-2"),
    activity: subject ? `${subject.label} — ${headlineOf(subject.state)}` : null,
    stoppable: list.find((r) => r.abortable) ?? null,
  };
}

/**
 * What a hand-off from the editor needs: the conversation it goes to, or
 * the origin a new one is started with. A hand-off carries this checkout's
 * files, so it lands only where the turn runs here — the checkout's own
 * conversation or the project's — never a sibling checkout's, whatever the
 * checkout is on; and it never names a session.
 * @param {{id: string, origin?: {kind?: string, id?: string} | null} | null | undefined} current the conversation the checkout is on
 * @param {string} workstream
 * @param {string} project
 * @returns {{conversation: string} | {start: {kind: "workstream", id: string, project: string}}}
 */
export function handOffTarget(current, workstream, project) {
  if (current && runsHere(current, workstream, project)) return { conversation: current.id };
  return { start: { kind: "workstream", id: workstream, project } };
}

/**
 * Whether a remembered conversation is still the checkout's to open: live,
 * and standing in this project — else the newest, or a new one, is chosen.
 * @param {{project?: string | null, archived?: unknown} | null | undefined} conversation
 * @param {string} pid
 */
export function rememberedStillHere(conversation, pid) {
  if (!conversation || conversation.archived) return false;
  return conversation.project === pid;
}

export function handOffWords(agentName, started) {
  return started ? t("workbench-conversation-pane-sent-new-conversation", { agentName }) : t("workbench-conversation-pane-sent", { agentName });
}
