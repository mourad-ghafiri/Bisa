/**
 * Why a session exists, who it is, where it stands and where it opens — one
 * reading of a roster row for every surface that names one: the footer, the
 * Agents screen, the rail, the Inbox card, the pet, a notification, the
 * resource overlay. The engine says the facts (`SessionRow.origin`, `cwd`;
 * 06 — Agents and teams §Sessions and presence); this model puts them into
 * words with what the window knows — goals by their labels, workstreams by
 * their places, agents by their names, channels by theirs — and never by an
 * id where a name is to hand. A row from an older node that carries no
 * `origin` is read by its kind and the ids it does carry, so the desktop
 * stays a client of any 0.x node.
 *
 * Plain `.mjs` with a `.d.mts` beside it; the test reads the Rust enums.
 */

import { emptyPlaceIndex, placeIndex, placeWords } from "./footerSessionsModel.mjs";
import { pointLabel } from "../views/_settings/decisionsModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/** The kinds of session the roster holds — the store's `SessionKind`, in its order. */
export const KINDS = Object.freeze(["worker", "guided", "conversation", "terminal", "ask"]);

/** The origins a row may carry — the core's `SessionOrigin` tags, in its order. */
export const ORIGINS = Object.freeze(["step", "design", "turn", "terminal", "ask"]);

/** A kind's word: *worker*, *design*, *turn*, *terminal*, *one-shot ask*; a kind this build does not know by its wire word. */
export function kindWord(kind) {
  switch (kind) {
    case "worker":
      return tr("shell-session-kind-worker");
    case "guided":
      return tr("shell-session-kind-guided");
    case "conversation":
      return tr("shell-session-kind-conversation");
    case "terminal":
      return tr("shell-session-kind-terminal");
    case "ask":
      return tr("shell-session-kind-ask");
    default:
      return String(kind ?? "");
  }
}

/**
 * The place index, with what naming a session needs beside places: agents
 * by id, channels and direct messages by id, the harness labels, and the
 * directory's word for a person.
 * @param {{workstreams?: readonly object[], projects?: readonly object[], goals?: readonly object[], agents?: readonly {id: string, name: string}[], channels?: readonly {channel: {id: string, name?: string | null}}[], dms?: readonly {channel: {id: string}}[], nameOf?: (pubkey: string) => string}} ws
 * @param {Record<string, string>} [harnessLabels]
 */
export function originIndex(ws, harnessLabels = {}) {
  return {
    ...placeIndex(ws),
    agents: new Map((ws?.agents ?? []).map((a) => [a.id, a.name])),
    channels: new Map((ws?.channels ?? []).map((c) => [c.channel.id, c.channel.name || c.channel.id])),
    dms: new Set((ws?.dms ?? []).map((c) => c.channel.id)),
    harnessLabels: { ...harnessLabels },
    nameOf: typeof ws?.nameOf === "function" ? ws.nameOf : (pubkey) => `${String(pubkey ?? "").slice(0, 8)}…`,
  };
}

/** The empty index: every name falls back to its id, every place to its kind and tail. */
export function emptyOriginIndex() {
  return originIndex({});
}

/**
 * The origin a row carries, or the one its kind and ids imply — a row from a
 * node that did not say: a worker is a step's, a design wake designs, a turn
 * is a turn of its conversation, a terminal a terminal.
 * @param {object | null | undefined} row
 */
export function originOfRow(row) {
  const o = row?.origin;
  if (o && typeof o === "object" && typeof o.origin === "string") return o;
  switch (row?.kind) {
    case "guided":
      return { origin: "design", phase: "design" };
    case "conversation":
      return { origin: "turn", scope: row.conversation ?? row.goal ?? "" };
    case "terminal":
      return { origin: "terminal" };
    case "ask":
      return { origin: "ask", purpose: null };
    default:
      return { origin: "step", resumed: false };
  }
}

/** The word of an ask's purpose, for a title. */
function askTitle(purpose) {
  switch (purpose?.kind) {
    case "classifier":
      return tr("shell-session-ask-title-classifier");
    case "decision":
      return tr("shell-session-ask-title-decision");
    case "commit_message":
      return tr("shell-session-ask-title-commit");
    case "pull_request_message":
      return tr("shell-session-ask-title-pr");
    default:
      return tr("shell-session-kind-ask");
  }
}

/**
 * Who the session is: the agent's name, else the harness's label, and for a
 * one-shot ask what it is for — the one *who* rule.
 * @param {object} row @param {ReturnType<typeof originIndex>} [index]
 */
export function titleOf(row, index = emptyOriginIndex()) {
  const o = originOfRow(row);
  if (o.origin === "ask") return askTitle(o.purpose);
  if (row?.agent) return index.agents.get(row.agent) ?? row.agent;
  if (row?.harness) return index.harnessLabels[row.harness] ?? row.harness;
  return tr("shell-session-origin-unknown");
}

/** The last `n` segments of a path, for a folder said in a few words. */
function lastSegments(path, n) {
  const parts = String(path ?? "")
    .split(/[\\/]+/)
    .filter(Boolean);
  return parts.slice(-n).join("/");
}

/**
 * Where the session stands: its workstream's place, else its project, else
 * the folder its harness runs in, else nowhere.
 * @param {object} row @param {ReturnType<typeof originIndex>} [index]
 */
export function placeOf(row, index = emptyOriginIndex()) {
  if (row?.workstream) return placeWords("workstream", row.workstream, index);
  if (row?.project) return placeWords("project", row.project, index);
  if (row?.cwd) return tr("shell-session-cwd", { dir: lastSegments(row.cwd, 2) });
  return tr("shell-footer-sessions-checkout");
}

/** What a session is about — its goal by label, else its run of the workspace — or nothing it can name. */
function aboutKnown(row, index) {
  if (row?.goal) return placeWords("goal", row.goal, index);
  if (row?.run) return placeWords("run", row.run, index);
  return null;
}

/** The checkout a session stands in — its workstream's place, else its project — or none. */
function checkoutKnown(row, index) {
  if (row?.workstream) return placeWords("workstream", row.workstream, index);
  if (row?.project) return placeWords("project", row.project, index);
  return null;
}

function turnWords(row, o, index) {
  const scope = typeof o.scope === "string" && o.scope ? o.scope : row?.conversation ?? null;
  const checkout = checkoutKnown(row, index);
  let words;
  if (row?.goal) words = tr("shell-session-origin-turn-goal", { goal: placeWords("goal", row.goal, index) });
  else if (scope && index.channels.has(scope)) words = tr("shell-session-origin-turn-channel", { channel: index.channels.get(scope) });
  else if (scope && index.dms.has(scope)) words = tr("shell-session-origin-turn-dm");
  else if (checkout) words = tr("shell-session-origin-turn-about", { where: checkout });
  else words = tr("shell-session-origin-turn-conversation");
  if (o.on_behalf_of) words = `${words} — ${tr("shell-session-origin-turn-woken-by", { who: index.nameOf(o.on_behalf_of) })}`;
  return words;
}

function askWords(row, purpose, index) {
  const about = aboutKnown(row, index);
  const checkout = checkoutKnown(row, index);
  switch (purpose?.kind) {
    case "classifier":
      return about ? tr("shell-session-origin-ask-classifier", { where: about }) : tr("shell-session-origin-ask-classifier-alone");
    case "decision": {
      const point = purpose.point ? pointLabel(purpose.point) : tr("shell-session-ask-no-point");
      return about ? tr("shell-session-origin-ask-decision", { point, where: about }) : tr("shell-session-origin-ask-decision-alone", { point });
    }
    case "commit_message":
      return checkout ? tr("shell-session-origin-ask-commit", { where: checkout }) : tr("shell-session-origin-ask-commit-alone");
    case "pull_request_message":
      return checkout ? tr("shell-session-origin-ask-pr", { where: checkout }) : tr("shell-session-origin-ask-pr-alone");
    default:
      return about ? tr("shell-session-origin-ask", { where: about }) : tr("shell-session-origin-ask-alone");
  }
}

/**
 * What the session is for, in one sentence — *step Build of a run on Ship
 * the cart*, *the Workflow Agent designing Ship the cart*, *a turn in
 * #general*, *the classifier reading a call for Ship the cart*.
 * @param {object} row @param {ReturnType<typeof originIndex>} [index]
 */
export function originWords(row, index = emptyOriginIndex()) {
  const o = originOfRow(row);
  switch (o.origin) {
    case "step": {
      const about = aboutKnown(row, index);
      const step = o.name || o.step || null;
      if (!about) return step ? tr("shell-session-origin-step-alone", { step }) : tr("shell-session-origin-unknown");
      if (!step) return tr("shell-session-origin-step-unnamed", { where: about });
      if (o.resumed) return tr("shell-session-origin-step-resumed", { step, where: about });
      return tr("shell-session-origin-step", { step, where: about });
    }
    case "design": {
      const about = aboutKnown(row, index);
      if (o.phase === "repair") return about ? tr("shell-session-origin-repair", { where: about }) : tr("shell-session-origin-repair-alone");
      return about ? tr("shell-session-origin-design", { where: about }) : tr("shell-session-origin-design-alone");
    }
    case "turn":
      return turnWords(row, o, index);
    case "terminal": {
      const checkout = checkoutKnown(row, index);
      return checkout ? tr("shell-session-origin-terminal", { where: checkout }) : tr("shell-session-origin-terminal-alone");
    }
    case "ask":
      return askWords(row, o.purpose, index);
    default:
      // An origin this build does not know — a newer node's — is said by its wire word, never by nothing.
      return String(o.origin ?? "");
  }
}

/**
 * Where the row opens — a route the router takes — or `null` for a session
 * that stands nowhere the window can show: a step's or a design wake's goal
 * (its run's page says the step's name and instructions), a run of the
 * workspace, a turn's thread, channel or conversation, a worker's or a
 * terminal's workstream.
 * @param {object} row @param {ReturnType<typeof originIndex>} [index]
 */
export function doorOf(row, index = emptyOriginIndex()) {
  const o = originOfRow(row);
  const workstream = row?.workstream ? { name: "workbench", scope: "workstream", id: row.workstream } : null;
  const goal = row?.goal ? { name: "goal", id: row.goal } : null;
  const run = row?.run ? { name: "run", id: row.run } : null;
  switch (o.origin) {
    case "turn": {
      if (goal) return goal;
      const scope = typeof o.scope === "string" && o.scope ? o.scope : row?.conversation ?? null;
      if (scope && index.channels.has(scope)) return { name: "channel", id: scope };
      if (scope && index.dms.has(scope)) return { name: "dm", id: scope };
      if (row?.conversation) return { name: "conversation", id: row.conversation };
      return workstream;
    }
    case "terminal":
      return workstream;
    default:
      return goal ?? run ?? workstream;
  }
}

/**
 * Everything above, for one row: who, what for, where, where it opens, and
 * its kind's word. What the row already says — its state, its start, its
 * wait — stays the row's (`sessionState.mjs`).
 * @param {object} row @param {ReturnType<typeof originIndex>} [index]
 */
export function originOf(row, index = emptyOriginIndex()) {
  return {
    title: titleOf(row, index),
    origin: originWords(row, index),
    place: placeOf(row, index),
    door: doorOf(row, index),
    kindWord: kindWord(row?.kind),
  };
}

export { emptyPlaceIndex };
