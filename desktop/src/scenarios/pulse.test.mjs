/**
 * The Pulse as a person reads it, stepped through the models the screen
 * steps (`views/_pulse/pulseModel.mjs`): the head arrives, the window comes
 * near the end and the next page is asked for by `before` and `before_seq`,
 * a second shared across a page edge loses nothing and shows nothing twice,
 * a row said on the stream nudges its tab and *All*, a row taken back leaves
 * the screen, a happening recorded twice is drawn once, a feed left open
 * stays bounded, and a restart reads it back as
 * deep as it was left. The node is a list of facts and a keyset over it —
 * no socket, no window.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs desktop/src/scenarios/pulse.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { pulseLine } from "../activityModel.mjs";
import {
  CONCEPTS,
  HELD_MAX,
  MAX_PAGES,
  PAGE,
  PAGE_AHEAD,
  conceptWords,
  cursorOf,
  depthOf,
  heldFeed,
  itemOf,
  joinHead,
  linkOf,
  mergePage,
  nudgeOf,
  parseConcept,
  parseDepth,
  wantsDepth,
  sameCursor,
  wantsMore,
  withDividers,
} from "../views/_pulse/pulseModel.mjs";

/** The node's feed: facts newest first by `(at, seq)`, served a page at a time strictly before a cursor. */
function node() {
  const facts = [];
  let seq = 0;
  return {
    say: (at, concept, type, source = { kind: "goal", id: "G1" }) => {
      seq += 1;
      facts.push({ seq, at, concept, kind: type, source, title: "ship the thing", author: null, event: { type, run: "R1", outcome: "done" } });
      return facts.at(-1);
    },
    page: (concept, before, limit) => {
      const all = facts.filter((f) => concept === "all" || f.concept === concept).sort((a, b) => b.at - a.at || b.seq - a.seq);
      const from = before ? all.filter((f) => f.at < before.at || (f.at === before.at && f.seq < before.seq)) : all;
      const rows = from.slice(0, limit);
      const more = from.length > rows.length;
      return { rows, next: more ? { at: rows.at(-1).at, seq: rows.at(-1).seq } : null };
    },
    count: () => facts.length,
    /** The frame the node sends for a row once it can be read. */
    frame: (fact) => ({ seq: fact.seq, at: fact.at, concept: fact.concept, kind: fact.kind, source: fact.source }),
    /** A fact taken back — a message retracted: its row leaves the feed. */
    retract: (fact) => void facts.splice(facts.indexOf(fact), 1),
    /** The journal's fact of a happening and the engine's echo of it: two rows, the second naming the first. */
    twice: (at, concept, type) => {
      seq += 1;
      const journal = { seq, at, concept, kind: "run", source: { kind: "goal", id: "G1" }, title: "ship the thing", author: "owner", event: { type: "run", run: "R1", event: { fact: "finished", outcome: "done" } } };
      seq += 1;
      const echo = { seq, at, concept, kind: type, source: journal.source, title: journal.title, author: null, echo_of: journal.seq, event: { type, run: "R1", outcome: "done" } };
      facts.push(journal, echo);
      return [journal, echo];
    },
  };
}

/** The screen's half: what it holds of one concept, and the two reads it makes. */
function screen(n, concept) {
  let feed = { items: [], next: null };
  const asked = [];
  return {
    feed: () => feed,
    asked,
    head: (shownLast = 0, restart = false) => {
      asked.push({ concept, before: null });
      const page = n.page(concept, null, PAGE);
      const before = restart ? { items: [], next: null } : feed;
      const joined = joinHead({ shown: before.items, page: page.rows, next: cursorOf(page), tail: before.next });
      feed = heldFeed({ items: joined.items, next: joined.next }, shownLast);
      return joined;
    },
    more: () => {
      asked.push({ concept, before: feed.next });
      const page = n.page(concept, feed.next, PAGE);
      feed = { items: mergePage(feed.items, page.rows, "older"), next: cursorOf(page) };
    },
    /** A frame of the feed, as the screen takes it: what the model says it asks, done. */
    hear: (frame) => {
      const asks = nudgeOf(concept, frame, feed.items);
      if (asks !== "none") {
        asked.push({ concept, before: null, restart: asks === "restart" });
        const page = n.page(concept, null, PAGE);
        const before = asks === "restart" ? { items: [], next: null } : feed;
        const joined = joinHead({ shown: before.items, page: page.rows, next: cursorOf(page), tail: before.next });
        feed = heldFeed({ items: joined.items, next: joined.next }, 0);
      }
      return asks;
    },
    /** An older page asked for and landing later: appended only where it was asked from. */
    ask: () => {
      const from = feed.next;
      const page = n.page(concept, from, PAGE);
      return () => {
        if (!sameCursor(from, feed.next)) return false;
        feed = { items: mergePage(feed.items, page.rows, "older"), next: cursorOf(page) };
        return true;
      };
    },
  };
}

test("reading down the feed: a page at a time by the last row's moment and place, every fact once, to the feed's end", () => {
  const n = node();
  // Two hundred facts, several a second — the keyset's tiebreak is what keeps them apart.
  for (let i = 0; i < 200; i++) n.say(1_000 + Math.floor(i / 7), "goals", "step_changed");
  const s = screen(n, "all");
  s.head();
  assert.equal(s.feed().items.length, PAGE);
  assert.deepEqual(s.feed().next, { at: s.feed().items.at(-1).at, seq: s.feed().items.at(-1).seq }, "the cursor is the last row's");
  // The window is far from the end: nothing is asked.
  assert.ok(!wantsMore({ last: 10, total: withDividers(s.feed().items).length, next: s.feed().next, inFlight: false }));
  // It comes within a few rows of it.
  let guard = 0;
  while (s.feed().next !== null && guard++ < 10) {
    const total = withDividers(s.feed().items).length;
    assert.ok(wantsMore({ last: total - PAGE_AHEAD, total, next: s.feed().next, inFlight: false }));
    assert.ok(!wantsMore({ last: total - PAGE_AHEAD, total, next: s.feed().next, inFlight: true }), "one page in flight at a time");
    s.more();
  }
  assert.equal(s.feed().items.length, 200);
  assert.equal(new Set(s.feed().items.map((r) => r.seq)).size, 200, "every fact once");
  assert.deepEqual(s.feed().items.map((r) => r.seq), [...s.feed().items.map((r) => r.seq)].sort((a, b) => b - a), "newest first throughout");
  assert.equal(s.feed().next, null, "the last page has no next");
  assert.deepEqual(s.asked.map((a) => (a.before ? "older" : "head")), ["head", "older", "older"]);
  // Each older page was asked strictly before the page above it: a second shared across the edge is cut by `seq`.
  const edges = s.asked.filter((a) => a.before).map((a) => a.before);
  assert.ok(edges.every((e) => Number.isInteger(e.at) && Number.isInteger(e.seq)));
  assert.ok(!wantsMore({ last: 199, total: 200, next: s.feed().next, inFlight: false }), "a feed read to its end asks for nothing");
});

test("something happens while you look: its tab and All read their head again, and another concept's tab reads nothing", () => {
  const n = node();
  for (let i = 0; i < 30; i++) n.say(1_000 + i, i % 2 ? "goals" : "projects", i % 2 ? "step_changed" : "workstream_committed");
  const tabs = Object.fromEntries(["all", "goals", "projects", "workflows", "node"].map((c) => [c, screen(n, c)]));
  for (const s of Object.values(tabs)) s.head();
  assert.deepEqual([tabs.all.feed().items.length, tabs.goals.feed().items.length, tabs.projects.feed().items.length, tabs.node.feed().items.length], [30, 15, 15, 0]);
  assert.equal(conceptWords("node").empty, "This node has not changed yet", "a tab with nothing says so");

  // A run finishes: the node says the row once it can be read.
  const frame = n.frame(n.say(2_000, "goals", "run_finished"));
  const heard = Object.fromEntries(Object.entries(tabs).map(([c, s]) => [c, s.hear(frame)]));
  assert.deepEqual(heard, { all: "head", goals: "head", projects: "none", workflows: "none", node: "none" });
  assert.equal(tabs.goals.feed().items[0].kind, "run_finished");
  assert.equal(tabs.all.feed().items.length, 31);
  assert.equal(tabs.projects.feed().items.length, 15, "another concept's tab read nothing");

  // A run of the workspace has no goal: its step is filed under Workflows,
  // and the frame says so — a step's payload alone reads as a goal's.
  const step = n.frame(n.say(2_001, "workflows", "step_changed", { kind: "workflow", id: "W1" }));
  assert.equal(tabs.workflows.hear(step), "head");
  assert.equal(tabs.workflows.feed().items[0].kind, "step_changed");
  assert.equal(tabs.goals.hear(step), "none");

  // What an agent wrote into a journal — a note — has no engine frame at all, and is said the same way.
  const note = n.frame(n.say(2_002, "goals", "note"));
  assert.equal(tabs.goals.hear(note), "head");
  assert.equal(tabs.goals.feed().items[0].kind, "note");
});

test("a message taken back leaves the screen at the next read, and a feed rebuilt starts over", () => {
  const n = node();
  for (let i = 0; i < 10; i++) n.say(1_000 + i, "channels", "message", { kind: "channel", id: "C1" });
  const said = n.say(1_020, "channels", "message", { kind: "channel", id: "C1" });
  const s = screen(n, "channels");
  s.head();
  assert.equal(s.feed().items.length, 11);
  // It is retracted; something else happens and nudges the tab.
  n.retract(said);
  assert.equal(s.hear(n.frame(n.say(1_030, "channels", "message", { kind: "channel", id: "C1" }))), "head");
  assert.ok(!s.feed().items.some((r) => r.seq === said.seq), "the head page no longer carries it: it is gone from the screen");
  assert.equal(s.feed().items.length, 11);
  // The index is rebuilt: every row may wear another seq, and what is held cannot be joined by it.
  assert.equal(s.hear({ concept: null }), "restart");
  assert.equal(s.asked.at(-1).restart, true);
  assert.equal(s.feed().items.length, 11);
});

test("a fact another node wrote lands where it happened: a feed loaded past it starts over and finds it", () => {
  const n = node();
  for (let i = 0; i < 3 * PAGE; i++) n.say(10_000 + i, "goals", "step_changed");
  const s = screen(n, "all");
  s.head();
  s.more();
  assert.equal(s.feed().items.length, 2 * PAGE);
  // A peer's note, written an hour of rows ago and synced now: below the head page, inside what is loaded.
  const late = n.say(10_000 + 2 * PAGE - 30, "goals", "note");
  assert.equal(s.hear(n.frame(late)), "restart", "reading the head would never show it");
  assert.equal(s.feed().items.length, PAGE);
  s.more();
  assert.ok(s.feed().items.some((r) => r.seq === late.seq), "paged down again, it is where it happened");
  assert.equal(new Set(s.feed().items.map((r) => r.seq)).size, s.feed().items.length);
});

test("a run finishing is recorded twice and drawn once", () => {
  const n = node();
  n.say(1_000, "goals", "step_changed");
  const [journal, echo] = n.twice(1_001, "goals", "run_finished");
  const s = screen(n, "goals");
  s.head();
  const items = s.feed().items.map((row) => itemOf(row, pulseLine(row)));
  assert.equal(items.length, 3, "both rows are held: the joins are by every seq");
  assert.deepEqual(items.map((i) => i.echo), [true, false, false]);
  const rows = withDividers(items).filter((slot) => slot.slot === "row");
  assert.deepEqual(rows.map((slot) => slot.item.seq), [journal.seq, 1], "the journal's row says the run finished");
  assert.ok(!rows.some((slot) => slot.item.seq === echo.seq));
});

test("an older page that lands after the feed started over is let go, never joined below a tail that moved", () => {
  const n = node();
  for (let i = 0; i < 2 * PAGE; i++) n.say(1_000 + i, "goals", "step_changed");
  const s = screen(n, "all");
  s.head();
  const landing = s.ask();
  // While it is on its way, more lands than a page holds and the head read starts the feed over.
  for (let i = 0; i < PAGE + 30; i++) n.say(5_000 + i, "goals", "step_changed");
  assert.equal(s.head().restarted, true);
  assert.equal(landing(), false, "a page of a feed that is gone");
  assert.equal(s.feed().items.length, PAGE);
  // Asked again from where the feed stands, the page joins with no hole.
  assert.equal(s.ask()(), true);
  const seqs = s.feed().items.map((r) => r.seq);
  assert.deepEqual(seqs, [...seqs].sort((a, b) => b - a));
  assert.equal(seqs[0] - seqs.at(-1) + 1, seqs.length, "every row between the head and the tail");
});

test("a row says what happened in the words the fact gets everywhere, and opens what it is about", () => {
  const n = node();
  const fact = n.say(1_000, "goals", "run_finished");
  const item = itemOf(fact, pulseLine(fact));
  assert.equal(item.seq, fact.seq);
  assert.ok(item.text.length > 0 && item.key, "the line is the activity's own");
  assert.deepEqual(linkOf(item.source, () => null, item.gone), { name: "goal", id: "G1" });
  // The goal is deleted since: the node can no longer title the row, and it opens nothing.
  const gone = itemOf({ ...fact, title: null }, pulseLine({ ...fact, title: null }));
  assert.equal(linkOf(gone.source, () => null, gone.gone), null, "a row about a deleted goal has no door");
  assert.deepEqual(linkOf({ kind: "agent", id: "dev" }, () => null), { name: "agent", id: "dev" });
  assert.equal(linkOf({ kind: "node", id: "n" }, () => null), null, "this node has no screen: no door");
  assert.deepEqual(linkOf({ kind: "channel", id: "C1" }, (id) => (id === "C1" ? "dm" : null)), { name: "dm", id: "C1" });
  const slots = withDividers([item, { ...item, key: "k2", seq: 0, at: 1_000 - 86_400 * 2 }]);
  assert.deepEqual(slots.map((s) => s.slot), ["day", "row", "day", "row"], "a divider between two days");
  assert.equal(slots[1].heading, "ship the thing");
});

test("a Pulse left open for a day stays within its bound, and what it let go comes back when you scroll to it", () => {
  const n = node();
  for (let i = 0; i < PAGE; i++) n.say(1_000 + i, "goals", "step_changed");
  const s = screen(n, "all");
  s.head();
  // The workspace moves all day; every burst is a nudge, every nudge a head page that reaches what is shown.
  for (let burst = 0; burst < 40; burst++) {
    for (let i = 0; i < 50; i++) n.say(10_000 + burst * 100 + i, "goals", "step_changed");
    const joined = s.head();
    assert.equal(joined.restarted, false);
    assert.ok(s.feed().items.length <= HELD_MAX, `burst ${burst}: ${s.feed().items.length} rows held`);
  }
  assert.equal(n.count(), PAGE + 40 * 50);
  assert.equal(s.feed().items.length, HELD_MAX);
  assert.equal(HELD_MAX, PAGE * MAX_PAGES);
  // The tail was let go with its cursor moved up: the next page is the one below the cut.
  const cut = s.feed().items.at(-1);
  assert.deepEqual(s.feed().next, { at: cut.at, seq: cut.seq });
  s.more();
  assert.equal(s.feed().items.length, HELD_MAX + PAGE);
  assert.equal(new Set(s.feed().items.map((r) => r.seq)).size, HELD_MAX + PAGE, "nothing twice at the cut");
  assert.equal(s.feed().items[HELD_MAX].seq, cut.seq - 1, "and nothing missing");
  // A reader far down is never cut under: the window's last row is what decides.
  const deep = s.head(HELD_MAX + PAGE - 5);
  assert.equal(deep.restarted, false);
  assert.equal(s.feed().items.length, HELD_MAX + PAGE);
});

test("more landed than a page holds while nobody looked: the feed starts over from its head rather than keep a hole", () => {
  const n = node();
  for (let i = 0; i < 100; i++) n.say(1_000 + i, "goals", "step_changed");
  const s = screen(n, "all");
  s.head();
  s.more();
  assert.equal(s.feed().items.length, 100);
  for (let i = 0; i < PAGE + 30; i++) n.say(5_000 + i, "goals", "step_changed");
  const joined = s.head();
  assert.equal(joined.restarted, true);
  assert.equal(s.feed().items.length, PAGE);
  assert.ok(s.feed().next, "older rows are paged in again, the missed ones first");
  s.more();
  assert.equal(new Set(s.feed().items.map((r) => r.seq)).size, s.feed().items.length);
});

test("closing the app and opening it: the tab is the address's, and the feed is read back as deep as it was left — ten pages at most", () => {
  assert.equal(parseConcept("workflows"), "workflows");
  assert.equal(parseConcept("triggers"), "all", "a link to a tab that is gone reads as All");
  const n = node();
  for (let i = 0; i < 1_000; i++) n.say(1_000 + i, "goals", "step_changed");
  // Left at three pages deep.
  const kept = depthOf(3 * PAGE);
  assert.equal(kept, 240);
  assert.equal(depthOf(PAGE), 0, "a feed no deeper than its head keeps nothing: every visit reads the head anyway");
  assert.equal(depthOf(50 * PAGE), PAGE * MAX_PAGES);
  // The memory is read back by the model: what is no depth is nothing.
  assert.equal(parseDepth(kept), 240);
  for (const odd of ["240", -3, 1.5, null, {}]) assert.equal(parseDepth(odd), undefined, `${JSON.stringify(odd)}`);
  const s = screen(n, "all");
  s.head();
  let guard = 0;
  while (wantsDepth({ loaded: s.feed().items.length, wanted: parseDepth(kept), next: s.feed().next, inFlight: false }) && guard++ < 20) s.more();
  assert.equal(s.feed().items.length, 240);
  assert.deepEqual(s.asked.map((a) => (a.before ? "older" : "head")), ["head", "older", "older"]);
});

test("the screen keeps to the model: the cursor's two words on the wire, the bound at the head, a read when the node comes back", () => {
  const api = readFileSync(new URL("../api.ts", import.meta.url), "utf8");
  assert.ok(api.includes('q.set("before", String(before.at))') && api.includes('q.set("before_seq", String(before.seq))'), "the keyset is both words");
  const view = readFileSync(new URL("../views/Pulse.tsx", import.meta.url), "utf8");
  assert.ok(view.includes("joinHead({ shown: before.items, page: rows, next: cursorOf(page), tail: before.next })"));
  assert.ok(view.includes("heldFeed({ items: joined.items, next: joined.next }, range.current[1])"));
  assert.ok(view.includes("useReloadOnReconnect(() => nudgeHead())"), "what a node did at boot is in the feed and was said by no frame");
  assert.ok(view.includes("nudgeOf(concept, frame, held.current.items)"), "a frame of the feed is placed by the model");
  assert.ok(view.includes("if (!sameCursor(asked, held.current.next))"), "an older page is appended only where it was asked from");
  assert.ok(view.includes('failed?.read === "older" ? loadMore() : loadHead()'), "Retry repeats the read that failed");
  assert.ok(view.includes("linkOf(slot.item.source, channelKind, slot.item.gone)"), "a row about a thing that is gone has no door");
  const bus = readFileSync(new URL("../bus.ts", import.meta.url), "utf8");
  assert.ok(bus.includes('useBus({ stream: "pulse" }'), "the feed's frames are their own stream");
});
