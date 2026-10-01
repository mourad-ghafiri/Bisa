/**
 * The conversation surface, as facts (13 — Conversations): what every owner
 * — a goal's page, a workflow's designer, the drawers beside a note or a
 * drawing, the IDE's Agent pane and its Agent mode — lists, says, picks and
 * starts through one hook and one painter (`useConversationSurface.ts`,
 * `ConversationSurface.tsx`). A conversation is reached where it is about:
 * there is no screen that lists every one, so each owner draws the same
 * surface of its own — the **Conversations** door with its count, the rows,
 * a search over the words said in them, live or archived, *New conversation*
 * — and the one it is on.
 *
 * Two facts, kept apart on purpose. **The pick is the owner's**: an id the
 * address names or the owner remembers (`pickedId`), whose record is read by
 * id and held to the owner (`belongsTo`). **The list is a view**: what the
 * search and the *Archived* switch admit (`listQuery`). Nothing that narrows
 * the list can change what the surface is on — a search, the switch, a page,
 * a reload in flight. No owner guesses a conversation (`surfaceView`): with
 * nothing picked it shows its own surface where it has one, else the list
 * while conversations exist, else the empty state. Plain `.mjs`, so
 * `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { href } from "../../routeModel.mjs";
import { parseRemembered, remember } from "../_workbench/conversationPaneModel.mjs";

/** How many rows a surface's list asks for at once. */
export const PAGE = 100;

/**
 * The `localStorage` key every owner's pick lives under: owner key →
 * conversation id, the newest `MAX_REMEMBERED` owners kept. One memory for
 * the IDE's roots (`workstream:<wid>`), the designer's workflows, the notes
 * and the drawings alike — `ownerKey` names the owner the same way for each.
 */
export const PICK_KEY = "bisa.conversations.pick";

/**
 * @typedef {{owner: {kind: string, id?: string | null, project?: string | null}, project?: string | null, place?: string | null}} SurfaceSource
 *   `owner` — what *New conversation* is about, and whose pick is remembered;
 *   `project` — list every conversation standing in this project instead of
 *   the owner's own (the IDE's pane: the project's, every checkout's);
 *   `place` — where the list's search and switch are kept (`ownerPlace` when absent).
 */

/** What a source's conversations are about, in a sentence about starting one. */
function aboutWords(source) {
  if (source?.project) return t("studio-conversation-surface-project");
  switch (source?.owner?.kind) {
    case "goal":
      return t("studio-conversation-surface-goal");
    case "workflow":
      return t("studio-conversation-surface-workflow");
    case "workstream":
      return t("studio-conversation-surface-checkout");
    case "project":
      return t("studio-conversation-surface-project");
    case "drawing":
      return t("studio-conversation-surface-drawing");
    case "note":
      return t("studio-conversation-surface-note");
    case "node":
      return t("studio-conversations-node");
    default:
      return t("studio-conversations-workspace");
  }
}

/**
 * The surface's words: the one door label, what a press does now, the empty
 * sentence, the reading line. Starting one asks nothing, so there is no
 * prompt to word.
 * @param {SurfaceSource | null | undefined} source
 * @param {{count: number, open?: boolean, picked?: boolean, hasFallback?: boolean}} at
 *   `count` — how many the list holds; `open` — the list is the view;
 *   `picked` — a conversation is picked, so leaving the list shows it;
 *   `hasFallback` — the owner has a surface of its own to go back to
 */
export function surfaceWords(source, { count, open = false, picked = false, hasFallback = false }) {
  const about = aboutWords(source);
  let hint;
  if (open && picked) hint = t("studio-conversation-surface-back-to-conversation");
  else if (open && hasFallback) hint = t("studio-conversation-surface-hide-conversations");
  else if (count === 0 && !open) hint = t("studio-conversation-surface-no-conversation-about-yet-start-one", { about });
  else if (source?.project) hint = t("studio-conversation-surface-project-conversations-every-checkout");
  else hint = t("studio-conversation-surface-conversations-about-pick-one-up", { about });
  return {
    door: t("studio-conversation-rows-conversations"),
    hint,
    empty: t("studio-conversation-surface-no-conversation-about-yet", { about }),
    reading: t("studio-no-conversation-reading"),
  };
}

/** *3 conversations* — the Agent mode header's count, the catalog's plural. @param {number} n */
export function countWords(n) {
  return t("studio-conversation-surface-count", { n });
}

/**
 * The row the surface shows from its list: the one the pick names while it is
 * listed, else none — the record is then read by id (`useConversation`).
 * @template {{id: string}} R
 * @param {readonly R[] | null | undefined} rows
 * @param {string | null | undefined} wanted
 * @returns {R | null}
 */
export function selectedOf(rows, wanted) {
  if (!wanted) return null;
  return (rows ?? []).find((r) => r.id === wanted) ?? null;
}

/**
 * The id the surface is on: the address's while it names one, else the
 * owner's remembered pick, else none. Never a row of the list — a surface
 * that guessed would change what it is on as its list moved.
 * @param {string | null | undefined} wanted the address's pick
 * @param {string | null | undefined} remembered the owner's last pick
 * @returns {string | null}
 */
export function pickedId(wanted, remembered) {
  return wanted ?? remembered ?? null;
}

/**
 * Whether a conversation is one of this surface's: about the owner itself,
 * or — for a surface that lists a project's — standing in that project. A
 * stale link's conversation, or another owner's, is not, and is dropped; an
 * archived one still is — its thread says so and offers *Take it back out*.
 * @param {{origin?: {kind?: string, id?: string | null} | null, project?: string | null} | null | undefined} row
 * @param {SurfaceSource} source
 */
export function belongsTo(row, source) {
  if (!row?.origin) return false;
  if (source.project) return row.project === source.project;
  const owner = source.owner;
  return row.origin.kind === owner.kind && (row.origin.id ?? null) === (owner.id ?? null);
}

/**
 * Which view the surface shows — never two at once:
 * - **thread** — the pick's conversation; **list** while the list is asked for;
 * - **reading** — an id is named and its record not read yet (nothing flashes
 *   between *New conversation* and the new thread), or the first page is still
 *   loading with nothing picked;
 * - **error** — the list or the record failed for a reason that is no
 *   *not found*: a node that is down never reads as "no conversations";
 * - **list** — asked for, while there are rows; or nothing picked and
 *   conversations exist — a drawer opened fresh over three of them shows them;
 * - **fallback** — nothing picked and the owner has a surface of its own (the
 *   goal's thread);
 * - **empty** — none at all: the one sentence and *New conversation*.
 * @param {{selected: {id: string} | null | undefined, settling: boolean, error: unknown, listOpen: boolean, loading: boolean, count: number, hasFallback: boolean}} at
 * @returns {"thread" | "list" | "reading" | "error" | "fallback" | "empty"}
 */
export function surfaceView({ selected, settling, error, listOpen, loading, count, hasFallback }) {
  if (selected) return listOpen ? "list" : "thread";
  if (settling) return "reading";
  if (error) return "error";
  if (listOpen && count > 0) return "list";
  if (hasFallback) return "fallback";
  if (loading) return "reading";
  return count > 0 ? "list" : "empty";
}

/**
 * The key an owner's pick is remembered under: its origin's kind and id —
 * `workflow:<id>`, `workstream:<wid>` (the IDE's root key, letter for letter)
 * — or the kind alone for an origin with no id.
 * @param {{kind: string, id?: string | null}} origin
 */
export function ownerKey(origin) {
  return origin.id ? `${origin.kind}:${origin.id}` : origin.kind;
}

/**
 * The place an owner's list keeps its search and its switch under
 * (`viewMemoryStore.ts`): the owner's own page where it has one — a goal's,
 * a workflow's — so what is kept goes with the page when its thing is gone;
 * a name for every other owner, which no address reaches. The IDE names its
 * root's place itself (`idePlace`), so the root's memory takes it along.
 * @param {{kind: string, id?: string | null}} origin
 */
export function ownerPlace(origin) {
  if (origin.id && (origin.kind === "goal" || origin.kind === "workflow")) return href({ name: origin.kind, id: origin.id }).slice(1);
  return `conversations:${ownerKey(origin)}`;
}

/**
 * The remembered picks as stored: owner key → conversation id. Anything that
 * is not a record of strings is no memory at all.
 * @param {unknown} raw
 * @returns {Record<string, string>}
 */
export function parsePicks(raw) {
  return parseRemembered(raw);
}

/**
 * Remember `conversation` as the owner's pick, the newest, dropping the
 * oldest owner past the cap; `null` forgets the owner's. A pick already
 * remembered as the newest — or a forget of nothing — hands back the same
 * memory, so a store writes nothing.
 * @param {Readonly<Record<string, string>>} memory
 * @param {string} owner
 * @param {string | null} conversation
 * @returns {Readonly<Record<string, string>>}
 */
export function rememberPick(memory, owner, conversation) {
  const keys = Object.keys(memory);
  const unchanged = conversation ? memory[owner] === conversation && keys[keys.length - 1] === owner : !(owner in memory);
  return unchanged ? memory : remember(memory, owner, conversation);
}

/**
 * The query the list reads: the owner's own conversations — or every one
 * standing in the project, for a source that names one — the words typed
 * (trimmed; none when empty), live or archived, one page.
 * @param {SurfaceSource} source
 * @param {{q?: string | null, archived?: boolean}} [at]
 */
export function listQuery(source, at = {}) {
  const q = (at.q ?? "").trim();
  const scope = source.project ? { project: source.project } : { origin: source.owner.kind, ...(source.owner.id ? { id: source.owner.id } : {}) };
  return {
    ...scope,
    ...(q ? { q } : {}),
    archived: at.archived === true,
    limit: PAGE,
  };
}
