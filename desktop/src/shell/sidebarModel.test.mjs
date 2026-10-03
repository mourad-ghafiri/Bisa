/**
 * The sidebar's facts: the Inbox badge, the modes, the rail's doors. Run
 * with `node --test desktop/src/shell/sidebarModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { RAIL_DOOR, RAIL_OVERHANG, RAIL_WIDTH, SIDEBAR_MODES, SIDEBAR_MODE_KEY, inboxBadge, needsWords, railDoors, railGutter, railTooltip, readMode, toggleWords, toggledMode } from "./sidebarModel.mjs";

import { badgeText } from "../ui/badgeModel.mjs";

const NAV = [
  { key: "inbox", label: "Inbox", route: { name: "inbox" } },
  { key: "agents", label: "Agents", route: { name: "agents" } },
  { key: "teams", label: "Teams", route: { name: "teams" } },
  { key: "projects", label: "Projects", route: { name: "projects" } },
  { key: "workflows", label: "Workflows", route: { name: "workflows" } },
  { key: "goals", label: "Goals", route: { name: "goals" } },
  { key: "pulse", label: "Pulse", route: { name: "pulse" } },
];
const row = (over = {}) => ({ key: "k", kind: "goal", title: "t", latest_at: 1, unread_count: 0, read: true, handled: false, mentioned: false, needs_action: [], notices: [], unread_notices: 0, ...over });
const ask = { kind: "ask", id: "a1" };

test("the Inbox badge is two counts — what needs you and what is unread — each row once, never a read one, and never one summed number", () => {
  const none = { needs: 0, unread: 0, needsTitle: null, unreadTitle: null, title: null };
  assert.deepEqual(inboxBadge([]), none);
  assert.deepEqual(inboxBadge(null), none);
  const b = inboxBadge([row({ needs_action: [ask, ask], read: false }), row({ read: false }), row({ read: false, unread_notices: 1, notices: [{}] }), row({ read: true })]);
  assert.deepEqual([b.needs, b.unread], [1, 2], "a row with two asks is one row; a notice-only unread row counts; a read row does not");
  assert.ok(!("count" in b) && !("tone" in b), "no sum and no single tone: the accent is the owed count's alone");
  assert.deepEqual([b.needsTitle, b.unreadTitle], ["1 needs you", "2 unread"], "each count has its own words");
  assert.equal(b.title, "1 needs you · 2 unread");
  const quiet = inboxBadge([row({ read: false }), row({ read: false })]);
  assert.deepEqual([quiet.needs, quiet.unread, quiet.needsTitle, quiet.title], [0, 2, null, "2 unread"], "only unread: no owed count at all");
  assert.equal(inboxBadge([row({ join: { id: "j" } })]).title, "1 needs you", "a join to admit needs you, read or not");
  assert.equal(inboxBadge([row({ waiting: { session: "s" }, read: true })]).needs, 1, "a harness waiting in its terminal needs you");
  assert.equal(inboxBadge([row({ needs_action: [ask] }), row({ needs_action: [ask] }), row({ read: false })]).title, "2 need you · 1 unread");
  assert.equal(needsWords(0), "Nothing needs you", "the menu bar's line at none");
  assert.equal(needsWords(1), "1 needs you");
  assert.equal(needsWords(4), "4 need you");
});

test("a full Inbox: each count is exact, the title names both parts, and what the rail draws is capped while the tooltip is not", () => {
  const rows = [...Array.from({ length: 120 }, () => row({ read: false })), ...Array.from({ length: 12 }, () => row({ needs_action: [ask] }))];
  const b = inboxBadge(rows);
  assert.deepEqual([b.needs, b.unread], [12, 120]);
  assert.equal(b.title, "12 need you · 120 unread", "the exact numbers are the tooltip's");
  assert.equal(badgeText(b.unread, "md"), "99+");
  assert.equal(badgeText(b.needs, "sm"), "9+", "the collapsed rail's corner mark has room for a digit and a plus");
  const door = railDoors({ inbox: rows }, NAV).find((d) => d.key === "inbox");
  assert.deepEqual(door.badge, { count: 12, tone: "accent", title: "12 need you · 120 unread" }, "the corner counts what is owed, in the accent");
  assert.equal(door.unreadDot, true, "and a neutral dot says something is unread besides");
  assert.equal(door.arrival, 12, "the arrival is the owed count's");
});

test("the rail's Inbox corner: the owed count in the accent, else the unread count neutral, else nothing — never one sum", () => {
  const owedOnly = railDoors({ inbox: [row({ needs_action: [ask] })] }, NAV)[0];
  assert.deepEqual([owedOnly.badge, owedOnly.unreadDot], [{ count: 1, tone: "accent", title: "1 needs you" }, false]);
  const unreadOnly = railDoors({ inbox: [row({ read: false }), row({ read: false })] }, NAV)[0];
  assert.deepEqual([unreadOnly.badge, unreadOnly.unreadDot, unreadOnly.arrival], [{ count: 2, tone: "neutral", title: "2 unread" }, false, 0], "nothing owed: the unread, neutral — and nothing to announce");
  const both = railDoors({ inbox: [row({ needs_action: [ask] }), row({ read: false }), row({ read: false })] }, NAV)[0];
  assert.deepEqual([both.badge.count, both.badge.tone, both.unreadDot], [1, "accent", true]);
});

test("rows the node could not type never count and never throw", () => {
  assert.equal(inboxBadge([null, undefined, row({ read: false })]).unread, 1);
  assert.deepEqual([inboxBadge("not a list").needs, inboxBadge("not a list").unread], [0, 0]);
  assert.ok(inboxBadge([{}]).needs + inboxBadge([{}]).unread <= 1, "a bare row is counted once or not at all");
  assert.equal(railDoors({ inbox: [] }, NAV).find((d) => d.key === "inbox").badge, null, "nothing owed draws no badge");
  assert.equal(railDoors(null, NAV).find((d) => d.key === "inbox").badge, null, "before the workspace has loaded there is no badge");
});

test("the sidebar has two modes, remembered under one key; anything but collapsed reads as expanded", () => {
  assert.deepEqual([...SIDEBAR_MODES], ["expanded", "collapsed"]);
  assert.equal(SIDEBAR_MODE_KEY, "bisa.sidebar.mode");
  assert.equal(readMode("collapsed"), "collapsed");
  assert.equal(readMode("expanded"), "expanded");
  assert.equal(readMode(null), "expanded");
  assert.equal(readMode("0"), "expanded", "the old open flag is not a mode");
  assert.equal(toggledMode("expanded"), "collapsed");
  assert.equal(toggledMode("collapsed"), "expanded");
  assert.equal(toggleWords("expanded"), "Collapse sidebar");
  assert.equal(toggleWords("collapsed"), "Expand sidebar");
  assert.equal(RAIL_WIDTH, 44);
});

test("the rail's doors are the seven destinations, the Inbox with its badge, then Channels and Messages with their unread summed and their life", () => {
  const ws = {
    inbox: [row({ needs_action: [ask] }), row({ read: false })],
    channels: [{ channel: { id: "c1" }, unread_count: 4 }, { channel: { id: "c2" }, unread_count: 1 }],
    dms: [{ channel: { id: "d1" }, unread_count: 0 }],
    hosted: [{ host: { state: { state: "member" } }, channels: [{ channel: { id: "h1" }, unread_count: 2 }], dms: [] }, { host: { state: { state: "invited" } }, channels: [{ channel: { id: "h2" }, unread_count: 9 }], dms: [] }],
    unread: { c1: 3 },
    working: { d1: ["agent"] },
  };
  const doors = railDoors(ws, NAV);
  assert.deepEqual(
    doors.map((d) => d.key),
    ["inbox", "agents", "teams", "projects", "workflows", "goals", "pulse", "channels", "messages"],
  );
  assert.deepEqual(doors[0].badge, { count: 1, tone: "accent", title: "1 needs you · 1 unread" }, "the owed count, never the sum");
  assert.equal(doors[0].unreadDot, true);
  assert.equal(doors.find((d) => d.key === "goals").badge, null, "Goals carries no count: the Inbox is the one place the total appears");
  const channels = doors.find((d) => d.key === "channels");
  assert.deepEqual(channels.badge, { count: 4, tone: "neutral", title: "4 unread" }, "the live map first, the listing's count else");
  assert.equal(channels.live, null);
  assert.equal(channels.note, "2 unread messages in a hosted workspace — expand the sidebar", "a hosted workspace's unread is named, a non-member's not counted");
  assert.deepEqual(channels.route, { name: "channels" });
  const messages = doors.find((d) => d.key === "messages");
  assert.equal(messages.badge, null);
  assert.equal(messages.live, "an agent is writing");
  assert.equal(railTooltip(doors[0]), "Inbox · 1 needs you · 1 unread");
  assert.equal(railTooltip(channels), "Channels · 4 unread · 2 unread messages in a hosted workspace — expand the sidebar");
  assert.equal(railTooltip(messages), "Messages · an agent is writing");
  assert.equal(railTooltip(doors.find((d) => d.key === "goals")), "Goals");
  const empty = railDoors({}, NAV);
  assert.equal(empty.length, 9);
  assert.ok(empty.every((d) => d.badge === null && !d.unreadDot && d.live === null && d.note === null));
});

/** Tailwind's spacing scale: one unit is 4px. */
const PX_PER_UNIT = 4;

test("what hangs off a rail door fits the gutter, and the gutter is inside the scrollport — so a count is never cut off", () => {
  assert.equal(railGutter(), 6);
  assert.ok(RAIL_OVERHANG <= railGutter(), "a mark hangs no further than the room beside its door");

  const source = readFileSync(new URL("./SidebarRail.tsx", import.meta.url), "utf8");
  // The doors' <nav> scrolls, and a box that scrolls on one axis clips on both: it must span the rail.
  const nav = source.match(/<nav\b[^>]*className="([^"]*)"/)?.[1] ?? "";
  const classes = nav.split(/\s+/);
  assert.ok(classes.includes("overflow-y-auto"), "the rail scrolls when it is long");
  assert.ok(classes.includes("w-full"), "the scrollport is the rail's width, not the doors' column — else every overhang is clipped at the door's edge");
  assert.ok(classes.includes("items-center"), "and the doors stay centred in it");

  // The literal classes are the model's numbers: Tailwind needs them static, this keeps them honest.
  const side = RAIL_DOOR / PX_PER_UNIT;
  assert.ok(source.includes(`h-${side} w-${side}`), `a door is ${RAIL_DOOR}px square`);
  const hangs = [...source.matchAll(/-(?:right|left|top|bottom)-(\d+(?:\.\d+)?)/g)].map((m) => Number(m[1]) * PX_PER_UNIT);
  assert.ok(hangs.length > 0, "the corner marks and the active bar hang off the door");
  assert.ok(Math.max(...hangs) <= RAIL_OVERHANG, `nothing hangs further than ${RAIL_OVERHANG}px: ${hangs.join(", ")}`);
  assert.ok(source.includes('size="sm"'), "the count in an icon's corner is the small badge — a digit and a plus at most");
});
