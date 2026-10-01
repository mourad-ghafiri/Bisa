/**
 * Conversations (13 — Conversations): a saved exchange with agents, with an
 * origin — the facts a list draws and the rules a screen follows. No DOM.
 *
 * A row is the node's `ConversationView`: its origin (`{kind, id?, project?}`),
 * a title if one was given, the first line of its first post otherwise, when
 * it last moved, how much was said, who took part, how much since its last
 * summary, whether it is archived. The words a row wears, the order a list
 * keeps, which rows a checkout's panel admits and where a row opens are
 * decided here, once, for every owner's surface
 * (`conversationSurfaceModel.mjs`), the IDE's Agent pane and quick open.
 * Searching is the node's (`?q=`): no list narrows rows on its own.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * Every origin kind, in the order a picker offers them — the Rust enum's
 * order (`bisa-core/src/conversation.rs`, `ConversationOrigin::KINDS`);
 * the test reads the source so the two cannot drift.
 */
export const ORIGIN_KINDS = Object.freeze(["node", "workspace", "goal", "workflow", "project", "workstream", "drawing", "note"]);

/** What a row with no title and no words yet is called. */
export const UNTITLED = t("studio-conversations-bar-new-conversation");

/**
 * The name a row wears: its title, else the first line of its first post,
 * else the untitled word. A title is a person's and wins; the first line is
 * a stand-in until they give one.
 * @param {{title?: string | null, first_line?: string | null} | null | undefined} row
 */
export function titleOf(row) {
  const title = row?.title?.trim();
  if (title) return title;
  const line = row?.first_line?.trim();
  if (line) return line.length > 80 ? `${line.slice(0, 79)}…` : line;
  return UNTITLED;
}

/**
 * Whether a kind names one record (a goal, a workflow, a project, a
 * workstream) or stands alone (the node, the workspace).
 * @param {string} kind
 */
export function originTakesId(kind) {
  return kind !== "node" && kind !== "workspace";
}

/**
 * The words for what a conversation is about, with the names a caller knows:
 * *this node* · *the workspace* · *goal Dark mode* · *workflow Release* ·
 * *project web-app* · *workstream main of web-app*. A record the names do
 * not hold is said by the tail of its id, never by nothing.
 * @param {{kind: string, id?: string, project?: string} | null | undefined} origin
 * @param {{goal?: (id: string) => string | null | undefined, workflow?: (id: string) => string | null | undefined, project?: (id: string) => string | null | undefined, workstream?: (id: string) => string | null | undefined, drawing?: (id: string) => string | null | undefined, note?: (id: string) => string | null | undefined}} [names]
 */
export function originWords(origin, names = {}) {
  if (!origin) return "";
  const tail = (id) => (id ? id.slice(-6) : "");
  const named = (look, id) => (look ? look(id) : null) || tail(id);
  switch (origin.kind) {
    case "node":
      return t("studio-conversations-node");
    case "workspace":
      return t("studio-conversations-workspace");
    case "goal":
      return t("studio-conversations-goal-named", { name: named(names.goal, origin.id) });
    case "workflow":
      return t("studio-conversations-workflow-named", { name: named(names.workflow, origin.id) });
    case "project":
      return t("studio-conversations-project-named", { name: named(names.project, origin.id) });
    case "workstream": {
      const project = origin.project ? named(names.project, origin.project) : "";
      const ws = named(names.workstream, origin.id);
      return project ? t("studio-conversations-workstream", { ws, project }) : t("studio-conversations-workstream-named", { name: ws });
    }
    case "drawing":
      return t("studio-conversations-drawing-named", { name: named(names.drawing, origin.id) });
    case "note":
      return t("studio-conversations-note-named", { name: named(names.note, origin.id) });
    default:
      // A kind this desktop does not know — a newer node's — is said by its wire word, never by nothing.
      return String(origin.kind ?? "");
  }
}

/**
 * The glyph of each origin kind — a `ui/icons` name, as the rail uses them.
 * Every kind of the wire's `ConversationOrigin` has a row: the workspace's
 * conversation wears the plain conversation glyph on purpose, because it is
 * about nothing narrower than the workspace itself.
 */
export const ORIGIN_ICON = Object.freeze({
  node: "settings",
  workspace: "dm",
  goal: "goal",
  workflow: "workflow",
  project: "project",
  workstream: "workstream",
  drawing: "draw",
  note: "note",
});

/** The glyph key for an origin kind; one this desktop does not know wears the conversation's own. */
export function originIcon(kind) {
  return ORIGIN_ICON[kind] ?? "dm";
}

/**
 * The moment a row ranks by: when it last moved, else when it was made.
 * @param {{last_message_at?: number | null, created_at: number}} row
 */
export function movedAt(row) {
  return row.last_message_at ?? row.created_at;
}

/**
 * The order a list keeps: the most recently moved first, archived ones
 * last, ties by id descending — the node's order, so a client-side sort of
 * a merged page reads the same as the node's.
 * @template {{id: string, archived: boolean, last_message_at?: number | null, created_at: number}} R
 * @param {readonly R[]} rows
 * @returns {R[]}
 */
export function sortConversations(rows) {
  return [...(rows ?? [])].sort((a, b) => {
    if (a.archived !== b.archived) return a.archived ? 1 : -1;
    const dt = movedAt(b) - movedAt(a);
    if (dt !== 0) return dt;
    return b.id < a.id ? -1 : b.id > a.id ? 1 : 0;
  });
}

/**
 * The rows the IDE's Agent panel lists: every conversation standing in the
 * project — the project's own and every checkout's, this one's and its
 * siblings' alike — the live ones, or the archived ones when the panel's
 * switch says so, newest first. The node's `project` field is the fact
 * (`GET /conversations?project=`); nothing is re-derived from the origin.
 * @template {{project?: string | null, archived: boolean, id: string, last_message_at?: number | null, created_at: number, agents: readonly string[]}} R
 * @param {readonly R[]} rows
 * @param {string} project
 * @param {boolean} [archived] the archived ones instead of the live
 */
export function projectConversations(rows, project, archived = false) {
  return sortConversations((rows ?? []).filter((r) => r.archived === archived && r.project === project));
}

/**
 * Whether a conversation is one this checkout's turns run in: about the
 * checkout itself, or about its project (a project's turn runs in the
 * primary). A sibling checkout's is the project's too, but its turns run
 * there — so it is listed, never landed on unasked.
 * @param {{origin?: {kind?: string, id?: string} | null} | null | undefined} row
 * @param {string} workstream
 * @param {string} project
 */
export function runsHere(row, workstream, project) {
  const o = row?.origin;
  return (o?.kind === "workstream" && o.id === workstream) || (o?.kind === "project" && o.id === project);
}

/**
 * Where a row opens — its origin's address, since a conversation is reached
 * where it is about: one about a checkout or a project in the IDE with the
 * Agent panel on it (the primary's id is the project's); one about a goal on
 * the goal's Conversation tab; one about a workflow in the designer's
 * Agent pane; one about the node or the workspace, which nothing owns, on
 * the one page of its own (`#/conversations/:id`, the door every link goes
 * through — `ConversationDoor` resolves the rest to the addresses above).
 * @param {{id: string, origin: {kind: string, id?: string, project?: string}}} row
 * @returns {{route: {name: string, id?: string, scope?: string}, search: Record<string, string> | null}}
 */
export function routeOf(row) {
  const o = row.origin;
  if (o.kind === "workstream" && o.id) {
    return { route: { name: "workbench", scope: "workstream", id: o.id }, search: { conversation: row.id, panel: "agents" } };
  }
  if (o.kind === "project" && o.id) {
    return { route: { name: "workbench", scope: "workstream", id: o.id }, search: { conversation: row.id, panel: "agents" } };
  }
  if (o.kind === "goal" && o.id) {
    return { route: { name: "goal", id: o.id }, search: { tab: "conversation", conversation: row.id } };
  }
  if (o.kind === "workflow" && o.id) {
    // The designer's Agent pane, on this conversation.
    return { route: { name: "workflow", id: o.id }, search: { panel: "agent", conversation: row.id } };
  }
  return { route: { name: "conversation", id: row.id }, search: null };
}

/**
 * The agents a row names, as words: *General Agent, Reviewer*; empty when
 * nobody has spoken yet.
 * @param {readonly string[] | null | undefined} ids
 * @param {(id: string) => string | null | undefined} nameOf
 */
export function agentsWords(ids, nameOf) {
  return (ids ?? []).map((id) => nameOf(id) || id).join(", ");
}

/**
 * A row's list line: its name, what it is about, who is in it, and the count.
 * @param {{title?: string | null, first_line?: string | null, origin: {kind: string, id?: string, project?: string}, agents: readonly string[], message_count: number, archived: boolean}} row
 * @param {{names?: Parameters<typeof originWords>[1], agentName?: (id: string) => string | null | undefined}} [words]
 */
export function rowWords(row, words = {}) {
  return {
    title: titleOf(row),
    about: originWords(row.origin, words.names),
    agents: agentsWords(row.agents, words.agentName ?? ((id) => id)),
    count: `${row.message_count} ${row.message_count === 1 ? "message" : "messages"}`,
    archived: row.archived,
  };
}
