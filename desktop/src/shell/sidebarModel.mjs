/**
 * The sidebar's facts (guide/the-desktop.md §The sidebar): the Inbox badge —
 * everything not yet dealt with, one number, accented by what is owed —
 * the two modes the sidebar has, expanded and collapsed, and the doors the
 * collapsed rail draws with their marks and tooltips. One model for the
 * row, the rail and the words, so they never disagree. Pure, so `node
 * --test` reads it; `Sidebar.tsx` and `SidebarRail.tsx` draw.
 *
 * The badge reads each Inbox row through the Inbox's own rule (`rowState`):
 * a row that needs the person — an ask, a join to admit, a harness waiting
 * in its terminal — or one they have not read counts once; a read row never.
 * What is *owed* is the asks alone: they are the accent, and the count is
 * neutral when it is only unread.
 */

import { rowState } from "../views/_studio/inboxModel.mjs";
import { isMember, unreadOf } from "./hostedModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** The sidebar's two modes, and where the one chosen is remembered. */
export const SIDEBAR_MODES = Object.freeze(["expanded", "collapsed"]);
export const SIDEBAR_MODE_KEY = "bisa.sidebar.mode";
/** The collapsed rail's width, in pixels — the occupant rail's. */
export const RAIL_WIDTH = 44;
/** A rail door's side, in pixels: the square an icon sits in (`h-8 w-8`). */
export const RAIL_DOOR = 32;
/**
 * How far a door's marks hang outside it, in pixels (`-right-1`, `-top-1`):
 * the count in the icon's corner, the *agent is writing* dot, the active
 * door's bar.
 */
export const RAIL_OVERHANG = 4;

/**
 * The room between a door and the rail's edge, each side. **What hangs off a
 * door must fit in it, and must hang inside the rail's scrollport**: the
 * doors' `<nav>` scrolls when the rail is long, and a box that scrolls on one
 * axis clips on both — so the nav is as wide as the rail, and its gutters are
 * where a badge's overhang is drawn. A nav shrunk to its doors clipped every
 * count at the door's edge.
 */
export function railGutter() {
  return (RAIL_WIDTH - RAIL_DOOR) / 2;
}

/**
 * The mode a stored value means: `collapsed` alone collapses; anything
 * else — nothing stored, an old value, garbage — is expanded.
 * @param {unknown} stored
 * @returns {"expanded" | "collapsed"}
 */
export function readMode(stored) {
  return stored === "collapsed" ? "collapsed" : "expanded";
}

/** The other mode. */
export function toggledMode(mode) {
  return mode === "collapsed" ? "expanded" : "collapsed";
}

/** The toggle's label, by the mode it would leave. */
export function toggleWords(mode) {
  return mode === "collapsed" ? t("shell-sidebar-expand-sidebar") : t("shell-sidebar-collapse-sidebar");
}

/**
 * The count of things that need the person, in words — the sidebar's badge,
 * the menu bar icon's line and its tooltip all say it this way.
 * @param {number} needs
 */
export function needsWords(needs) {
  if (!(needs > 0)) return t("shell-sidebar-nothing-needs");
  return needs === 1 ? t("shell-sidebar-1-needs") : t("shell-sidebar-need", { needs });
}

/**
 * The Inbox badge: how many rows need the person and how many they have
 * not read, the one number the badge shows, its tone and its words.
 * @param {readonly import("../types").InboxRow[] | null | undefined} rows
 * @returns {{count: number, needs: number, unread: number, tone: "accent" | "neutral", title: string | null}}
 */
export function inboxBadge(rows) {
  let needs = 0;
  let unread = 0;
  for (const row of Array.isArray(rows) ? rows : []) {
    if (!row) continue;
    const state = rowState(row);
    if (state === "waiting") needs += 1;
    else if (state === "unread") unread += 1;
  }
  const parts = [];
  if (needs > 0) parts.push(needsWords(needs));
  if (unread > 0) parts.push(t("shell-sidebar-unread", { unread }));
  return { count: needs + unread, needs, unread, tone: needs > 0 ? "accent" : "neutral", title: parts.length ? parts.join(" · ") : null };
}

/**
 * The unread and the life of a list of channels, for a rail door: the
 * unread summed (the live map first, the listing's count else) and whether
 * an agent is writing in any.
 * @param {readonly {channel: {id: string}, unread_count?: number}[]} entries
 * @param {Readonly<Record<string, number>>} unreadMap
 * @param {Readonly<Record<string, readonly string[]>>} working
 */
function tally(entries, unreadMap, working) {
  let unread = 0;
  let busy = false;
  for (const e of entries ?? []) {
    const id = e?.channel?.id;
    if (!id) continue;
    unread += unreadMap?.[id] ?? e.unread_count ?? 0;
    if ((working?.[id] ?? []).length > 0) busy = true;
  }
  return { unread, busy };
}

/**
 * The rail's doors, top to bottom: the destinations `nav` lists (the
 * sidebar's `PRIMARY_NAV`) — the Inbox with its badge — then Channels and
 * Messages with their unread summed, since the rail has no room for a list.
 * A hosted workspace's channels are reached by expanding; Channels says so
 * in its tooltip while any is unread. The glyphs are the component's to
 * pick by key: this is a fact, not a drawing.
 * @param {{inbox?: readonly object[], channels?: readonly object[], dms?: readonly object[], hosted?: readonly object[], unread?: Readonly<Record<string, number>>, working?: Readonly<Record<string, readonly string[]>>}} ws
 * @param {readonly {key: string, label: string, route: object}[]} nav
 * @returns {{key: string, label: string, route: object, badge: {count: number, tone: "accent" | "neutral", title: string | null} | null, live: string | null, note: string | null}[]}
 */
export function railDoors(ws, nav) {
  const inbox = inboxBadge(ws?.inbox);
  const doors = (nav ?? []).map((entry) => ({
    key: entry.key,
    label: entry.label,
    route: entry.route,
    badge: entry.key === "inbox" && inbox.count > 0 ? { count: inbox.count, tone: inbox.tone, title: inbox.title } : null,
    live: null,
    note: null,
  }));
  const channels = tally(ws?.channels, ws?.unread, ws?.working);
  const dms = tally(ws?.dms, ws?.unread, ws?.working);
  const hostedUnread = (ws?.hosted ?? []).filter((s) => isMember(s?.host)).reduce((n, s) => n + unreadOf(s), 0);
  const mark = (unread) => (unread > 0 ? { count: unread, tone: "neutral", title: t("shell-sidebar-unread", { unread }) } : null);
  doors.push({
    key: "channels",
    label: t("shell-sidebar-channels"),
    route: { name: "channels" },
    badge: mark(channels.unread),
    live: channels.busy ? t("shell-sidebar-agent-writing") : null,
    note: hostedUnread > 0 ? t("shell-sidebar-hosted-unread", { n: hostedUnread }) : null,
  });
  doors.push({
    key: "messages",
    label: t("shell-sidebar-messages"),
    route: { name: "messages" },
    badge: mark(dms.unread),
    live: dms.busy ? t("shell-sidebar-agent-writing") : null,
    note: null,
  });
  return doors;
}

/**
 * What a rail door's tooltip — and its accessible name — reads: the name,
 * then its badge's words, its life and its note, each after a dot.
 * @param {{label: string, badge: {title: string | null} | null, live: string | null, note: string | null}} door
 */
export function railTooltip(door) {
  const parts = [door.label];
  if (door.badge?.title) parts.push(door.badge.title);
  if (door.live) parts.push(door.live);
  if (door.note) parts.push(door.note);
  return parts.join(" · ");
}
