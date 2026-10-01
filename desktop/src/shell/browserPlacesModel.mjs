/**
 * Where a browser tab is at home, by name (ide/18): the one index every
 * list of tabs — the footer's Browser count, a screen's Browser button, the
 * pane's strip — names a tab's home from, and the words each row wears.
 * A tab's home is a scope and an id (`browsersModel.BROWSER_SCOPES`); a
 * person never sees an id, so the index turns it into the place's own
 * name — the goal's title, *Bisa › feat/a*, `#general`, the workflow's name
 * — and its **origin**: Project IDE, Goal, Workflow, Channel, Message,
 * Conversation, or the Workspace for a tab at home nowhere in particular.
 * Plain `.mjs`, so `node --test` reads it.
 */

import { placeIndex, placeWords } from "./footerSessionsModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** The origins, in the order a list groups them. */
export const ORIGINS = Object.freeze([
  { kind: "ide", label: t("shell-browser-places-project-ide") },
  { kind: "goal", label: t("shell-browser-places-goal") },
  { kind: "workflow", label: t("shell-browser-places-workflow") },
  { kind: "channel", label: t("shell-browser-places-channel") },
  { kind: "dm", label: t("shell-browser-places-message") },
  { kind: "conversation", label: t("shell-browser-places-conversation") },
  { kind: "workspace", label: t("shell-sidebar-workspace") },
]);

const SEP = " · ";
const tail = (id) => String(id ?? "").slice(-6);
const firstLine = (text) => String(text ?? "").split("\n")[0].trim();

/**
 * One index over the workspace's rows: what names a home.
 * @param {{
 *   goals?: readonly {id: string, title?: string | null, statement?: string}[],
 *   workstreams?: readonly object[],
 *   projects?: readonly object[],
 *   channels?: readonly {channel: {id: string, name: string}}[],
 *   dms?: readonly {channel: {id: string, name: string}}[],
 *   inbox?: readonly {key: string, kind: string, title: string}[],
 * }} ws the workspace as the shell holds it
 * @param {readonly {workflow: {id: string, name: string}}[]} [workflows] the workflow rows, read once
 */
export function browserPlaces(ws, workflows = []) {
  const goals = (ws?.goals ?? []).map((g) => ({ id: g.id, label: goalLabel(g) }));
  const places = placeIndex({ workstreams: ws?.workstreams ?? [], projects: ws?.projects ?? [], goals });
  const named = (rows) => new Map((rows ?? []).map((r) => [r.channel.id, r.channel.name]));
  const conversations = new Map((ws?.inbox ?? []).filter((r) => r.kind === "conversation").map((r) => [r.key, r.title]));
  const flows = new Map((workflows ?? []).map((r) => [r.workflow.id, r.workflow.name]));
  return { places, channels: named(ws?.channels), dms: named(ws?.dms), conversations, workflows: flows };
}

/** A goal's word: its title, else its statement's first line, else its tail — the board's own rule. */
function goalLabel(g) {
  const title = String(g?.title ?? "").trim();
  if (title) return title;
  const stated = firstLine(g?.statement);
  return stated || t("shell-browser-places-goal-tail", { tail: tail(g?.id) });
}

/**
 * The origin a home belongs to.
 * @param {{scope: string, id: string} | null | undefined} home
 * @returns {{kind: string, label: string}}
 */
export function originOf(home) {
  const kind = !home ? "workspace" : home.scope === "workstream" || home.scope === "work_item" ? "ide" : home.scope;
  return ORIGINS.find((o) => o.kind === kind) ?? ORIGINS[ORIGINS.length - 1];
}

/**
 * The name of the place a home names, or `null` when the workspace's rows
 * do not know it — a workstream's project and branch, a goal's title, a
 * workflow's name, a channel's, a direct message's, a conversation's title.
 * @param {{scope: string, id: string} | null | undefined} home
 * @param {ReturnType<typeof browserPlaces>} places
 */
export function placeName(home, places) {
  if (!home) return null;
  switch (home.scope) {
    case "workstream":
      return places.places.workstreams.has(home.id) ? placeWords("workstream", home.id, places.places) : null;
    case "goal":
      return places.places.goals.get(home.id)?.label ?? null;
    case "work_item":
      return null;
    case "workflow":
      return places.workflows.get(home.id) ?? null;
    case "channel":
      return places.channels.get(home.id) ?? null;
    case "dm":
      return places.dms.get(home.id) ?? null;
    case "conversation":
      return places.conversations.get(home.id) ?? null;
    default:
      return null;
  }
}

/**
 * The one sentence every list wears for where a tab is at home: the origin,
 * then the place's name — *Goal · Ship the storefront*, *Project IDE · Bisa ›
 * feat/a*, *Channel · #general*, *Workspace* — and the kind with the id's
 * tail when nothing names the place yet.
 * @param {{home: {scope: string, id: string} | null} | null | undefined} session
 * @param {ReturnType<typeof browserPlaces>} places
 */
export function whereWords(session, places) {
  const home = session?.home ?? null;
  const origin = originOf(home);
  if (!home) return origin.label;
  const name = placeName(home, places);
  if (name) return `${origin.label}${SEP}${home.scope === "channel" ? `#${name}` : name}`;
  return `${origin.label}${SEP}${t("shell-browser-places-scope-tail", { scope: home.scope, tail: tail(home.id) })}`;
}

/**
 * The tabs by origin, in `ORIGINS` order, empty origins left out — each
 * group its label and its tabs in the order they were opened.
 * @param {readonly {home: {scope: string, id: string} | null}[]} sessions
 * @returns {{kind: string, label: string, tabs: object[]}[]}
 */
export function groupTabs(sessions) {
  return ORIGINS.map((o) => ({ kind: o.kind, label: o.label, tabs: (sessions ?? []).filter((s) => originOf(s.home).kind === o.kind) })).filter((g) => g.tabs.length > 0);
}

/**
 * The footer's button in a sentence: how many tabs, how many of them agents
 * opened, how many out of sight, how many an agent works in, and the tab a
 * person is looking at — so a number that surprises says whose it is.
 * @param {{count: number, headless: number, busy: number, agents?: number, shown: string | null}} facts `agents`: the tabs an agent's request opened
 */
export function footerBrowserWords({ count, headless, busy, agents = 0, shown }) {
  if (count === 0) return t("shell-browser-places-browser-tab-open-new-tab-here");
  const parts = [count === 1 ? t("shell-browser-places-1-browser-tab") : t("shell-browser-places-browser-tabs", { count })];
  const asides = [];
  if (agents > 0) asides.push(t("shell-browser-places-opened-by-agents", { agents }));
  if (headless > 0) asides.push(t("shell-browser-places-out-sight", { headless }));
  if (busy > 0) asides.push(t("shell-browser-places-agent-browsing", { busy }));
  const words = asides.length ? t("shell-browser-places-with-asides", { words: parts[0], asides: asides.join(", ") }) : parts[0];
  return shown ? t("shell-browser-places-showing", { words, shown }) : words;
}
