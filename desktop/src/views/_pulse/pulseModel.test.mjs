/**
 * The Pulse's rules. Run with `node --test desktop/src/views/_pulse/pulseModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { CONCEPTS, FACT_CONCEPT, HELD_MAX, HOSTED_FACTS, MAX_PAGES, NOT_ACTIVITY, PAGE, PAGE_AHEAD, conceptOfFact, conceptWords, cursorOf, depthOf, heldFeed, itemOf, joinHead, linkOf, mergePage, parseConcept, parseDepth, wantsDepth, wantsMore, wantsNudge, withDividers } from "./pulseModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("the concepts are All and the core's seven, in the core's order, each with a word, a real glyph and an empty state", () => {
  const rust = readFileSync(join(here, "../../../../crates/bisa-core/src/activity.rs"), "utf8");
  const table = rust.match(/pub fn as_str\(self\) -> &'static str \{\s*match self \{([\s\S]*?)\}\s*\}/)[1];
  const names = [...table.matchAll(/ActivityConcept::[A-Z][a-z]+ => "([a-z]+)"/g)].map((m) => m[1]);
  assert.deepEqual([...CONCEPTS], ["all", ...names]);
  const icons = readFileSync(join(here, "../../ui/icons.ts"), "utf8");
  for (const c of CONCEPTS) {
    const w = conceptWords(c);
    assert.ok(w.label && w.empty && w.hint, c);
    assert.match(icons, new RegExp(`^  ${w.icon}: `, "m"), `${c} wears a glyph that exists`);
  }
  assert.deepEqual([...CONCEPTS], ["all", "workspace", "goals", "workflows", "projects", "channels", "agents", "node"], "seven concepts: an event's facts are filed under its workflow or its goal");
  assert.equal(parseConcept("workflows"), "workflows");
  assert.equal(parseConcept("triggers"), "all", "a link to the retired Triggers tab reads as All");
  assert.equal(parseConcept("everything"), "all", "an unknown word in the URL is All");
  assert.equal(parseConcept(null), "all");
});

test("a frame nudges the tab it belongs to and All; a page is asked for near the end and only once", () => {
  assert.ok(wantsNudge("all", "projects"));
  assert.ok(wantsNudge("projects", "projects"));
  assert.ok(!wantsNudge("projects", "goals"));
  assert.ok(wantsNudge("projects", null), "a frame the client cannot place nudges every tab");
  const next = { at: 100, seq: 9 };
  assert.ok(wantsMore({ last: 100 - PAGE_AHEAD, total: 100, next, inFlight: false }));
  assert.ok(!wantsMore({ last: 10, total: 100, next, inFlight: false }), "far from the end, nothing is asked");
  assert.ok(!wantsMore({ last: 99, total: 100, next: null, inFlight: false }), "the last page has no next");
  assert.ok(!wantsMore({ last: 99, total: 100, next, inFlight: true }), "one page in flight at a time");
  assert.ok(!wantsMore({ last: 0, total: 0, next, inFlight: false }));
  assert.ok(PAGE > PAGE_AHEAD);
  assert.deepEqual(cursorOf({ next }), next);
  assert.equal(cursorOf({}), null);
});

test("a page joins what is shown by seq — the head replaces its overlap, an older page appends — newest first", () => {
  const row = (seq, at) => ({ seq, at });
  const shown = [row(5, 50), row(4, 40), row(3, 30)];
  const head = mergePage(shown, [row(7, 70), row(6, 60), row(5, 50)], "head");
  assert.deepEqual(head.map((r) => r.seq), [7, 6, 5, 4, 3], "the overlap is one row, not two");
  const older = mergePage(head, [row(2, 20), row(1, 20)], "older");
  assert.deepEqual(older.map((r) => r.seq), [7, 6, 5, 4, 3, 2, 1], "two rows in one second keep their order by seq");
  assert.deepEqual(mergePage([], [row(1, 1)], "older").map((r) => r.seq), [1]);
});

test("dividers fall between local days and a heading opens each run of rows on one thing", () => {
  const day = 86_400;
  const items = [
    { key: "a", at: 3 * day + 100, title: "ship it" },
    { key: "b", at: 3 * day + 50, title: "ship it" },
    { key: "c", at: 3 * day + 10, title: "watercooler" },
    { key: "d", at: 1 * day + 10, title: "watercooler" },
    { key: "e", at: 1 * day + 5 },
  ];
  const slots = withDividers(items);
  assert.deepEqual(
    slots.map((s) => (s.slot === "day" ? "—" : `${s.item.key}${s.heading ? `:${s.heading}` : ""}`)),
    ["—", "a:ship it", "b", "c:watercooler", "—", "d:watercooler", "e"],
  );
  assert.deepEqual(withDividers([]), []);
});

test("a row's door follows what it is about, and a thing with no screen has none", () => {
  const kinds = { C1: "channel", D1: "dm" };
  const kindOf = (id) => kinds[id] ?? null;
  assert.deepEqual(linkOf({ kind: "goal", id: "G1" }, kindOf), { name: "goal", id: "G1" });
  assert.deepEqual(linkOf({ kind: "channel", id: "C1" }, kindOf), { name: "channel", id: "C1" });
  assert.deepEqual(linkOf({ kind: "channel", id: "D1" }, kindOf), { name: "dm", id: "D1" });
  assert.equal(linkOf({ kind: "channel", id: "gone" }, kindOf), null, "a channel the workspace no longer lists opens nothing");
  assert.deepEqual(linkOf({ kind: "conversation", id: "C9" }, kindOf), { name: "conversation", id: "C9" }, "a conversation opens on its own screen");
  assert.deepEqual(linkOf({ kind: "workstream", id: "W1" }, kindOf), { name: "workbench", scope: "workstream", id: "W1" });
  assert.deepEqual(linkOf({ kind: "project", id: "P1" }, kindOf), { name: "workbench", scope: "workstream", id: "P1" }, "a project opens as its primary, whose id is the project's");
  assert.deepEqual(linkOf({ kind: "workflow", id: "F1" }, kindOf), { name: "workflow", id: "F1" });
  assert.deepEqual(linkOf({ kind: "agent", id: "dev" }, kindOf), { name: "agents" });
  assert.equal(linkOf({ kind: "node", id: "" }, kindOf), null);
  assert.equal(linkOf({ kind: "workspace", id: "" }, kindOf), null);
  assert.equal(linkOf(null, kindOf), null);
});

const feedRow = (seq, at = 1000) => ({ seq, at });
const span = (from, to, at = 1000) => Array.from({ length: from - to + 1 }, (_, i) => feedRow(from - i, at));

test("a fresh head joins what is shown when it reaches it, and the tail stays where the last older page left it", () => {
  const shown = span(50, 41);
  const joined = joinHead({ shown, page: span(53, 44), next: { at: 1000, seq: 44 }, tail: { at: 1000, seq: 41 }, size: 10 });
  assert.deepEqual(joined.items.map((r) => r.seq), span(53, 41).map((r) => r.seq), "every seq once, newest first");
  assert.deepEqual(joined.next, { at: 1000, seq: 41 }, "the head's cursor is not the tail's");
  assert.equal(joined.restarted, false);
  const ended = joinHead({ shown, page: span(53, 44), next: { at: 1000, seq: 44 }, tail: null, size: 10 });
  assert.equal(ended.next, null, "a feed read to its end is not read again from its head");
});

test("nothing shown: the page is the feed and its cursor the tail; a short page is the whole feed", () => {
  const first = joinHead({ shown: [], page: span(9, 1), next: null, tail: null, size: 10 });
  assert.deepEqual([first.items.length, first.next, first.restarted], [9, null, false]);
  const full = joinHead({ shown: [], page: span(20, 11), next: { at: 1000, seq: 11 }, tail: null, size: 10 });
  assert.deepEqual(full.next, { at: 1000, seq: 11 });
  const short = joinHead({ shown: span(3, 1), page: span(5, 4), next: null, tail: null, size: 10 });
  assert.deepEqual(short.items.map((r) => r.seq), [5, 4, 3, 2, 1], "a short page cannot have skipped anything");
  assert.equal(joinHead({ shown: span(3, 1), page: [], next: null, tail: null, size: 10 }).items.length, 3, "an empty head changes nothing");
});

test("more landed than a page holds: the feed starts over from the head, never a hole nothing fills", () => {
  const shown = span(50, 41);
  const joined = joinHead({ shown, page: span(200, 191), next: { at: 1000, seq: 191 }, tail: null, size: 10 });
  assert.equal(joined.restarted, true);
  assert.deepEqual(joined.items.map((r) => r.seq), span(200, 191).map((r) => r.seq), "what was shown is let go: rows 190…51 were never read");
  assert.deepEqual(joined.next, { at: 1000, seq: 191 }, "and older rows are paged in again from here");
  // Walked back down, the feed is whole again.
  const older = mergePage(joined.items, span(190, 181), "older");
  assert.deepEqual(older.map((r) => r.seq), span(200, 181).map((r) => r.seq));
});

test("a second shared across a page edge: the older page joins without a row twice or a row lost", () => {
  // Five rows in second 1000; the first page ended in the middle of it.
  const head = [feedRow(9, 1001), feedRow(8, 1000), feedRow(7, 1000)];
  const next = [feedRow(6, 1000), feedRow(5, 1000), feedRow(4, 1000), feedRow(3, 999)];
  const merged = mergePage(head, next, "older");
  assert.deepEqual(merged.map((r) => r.seq), [9, 8, 7, 6, 5, 4, 3]);
  // The same page twice — a retry after a timeout that had in fact landed.
  assert.deepEqual(mergePage(merged, next, "older").map((r) => r.seq), [9, 8, 7, 6, 5, 4, 3]);
  assert.ok(wantsMore({ last: merged.length - 1, total: merged.length, next: { at: 999, seq: 3 }, inFlight: false }));
  assert.ok(!wantsMore({ last: merged.length - 1, total: merged.length, next: null, inFlight: false }), "the end is the end");
  assert.ok(!wantsMore({ last: 0, total: 0, next: { at: 1, seq: 1 }, inFlight: false }), "an empty feed asks for nothing more");
});

test("an item is the line's words beside the row's facts — one builder for the Pulse and an Inbox row's notices", () => {
  const row = { seq: 7, at: 120, concept: "goals", source: { kind: "goal", id: "K" }, kind: "run_finished", event: { type: "run_finished" } };
  const line = { key: "k7", text: "the run failed", tone: "fail", title: "build the thing", extra: "never carried" };
  assert.deepEqual(itemOf(row, line), { key: "k7", seq: 7, at: 120, text: "the run failed", tone: "fail", concept: "goals", source: { kind: "goal", id: "K" }, author: undefined, title: "build the thing", icon: undefined, detail: undefined });
  for (const view of ["../Pulse.tsx", "../Inbox.tsx"]) {
    const text = readFileSync(join(dirname(fileURLToPath(import.meta.url)), view), "utf8");
    assert.ok(text.includes("itemOf(") && !text.includes("detail: line.detail"), `${view} builds no item of its own`);
  }
});


test("how far a feed is read is kept in rows, held to ten pages, and nothing for a feed no deeper than its head", () => {
  assert.equal(MAX_PAGES, 10);
  assert.equal(depthOf(0), 0);
  assert.equal(depthOf(PAGE), 0, "the head page is read on every visit: no memory of it");
  assert.equal(depthOf(PAGE + 1), PAGE + 1);
  assert.equal(depthOf(3 * PAGE), 3 * PAGE);
  assert.equal(depthOf(40 * PAGE), MAX_PAGES * PAGE, "further than ten pages, a person scrolls again");
  assert.equal(depthOf(25, 10), 25);
  assert.equal(depthOf(2.5 * PAGE + 0.5), 0, "a count that is no count keeps nothing");
});

test("a depth reads back the same, held to ten pages, and what is no depth is nothing", () => {
  assert.equal(parseDepth(JSON.parse(JSON.stringify(depthOf(3 * PAGE)))), 3 * PAGE);
  assert.equal(parseDepth(0), 0);
  assert.equal(parseDepth(1_000_000), MAX_PAGES * PAGE, "a memory written by hand reads no deeper");
  for (const bad of [null, undefined, "240", -1, 2.5, Number.NaN, Infinity, [], {}]) assert.equal(parseDepth(bad), undefined, JSON.stringify(bad));
});

test("a feed read back asks for its next page until it is as deep as it was left, or at its end", () => {
  const next = { at: 100, seq: 9 };
  assert.ok(wantsDepth({ loaded: PAGE, wanted: 3 * PAGE, next, inFlight: false }));
  assert.ok(wantsDepth({ loaded: 2 * PAGE, wanted: 3 * PAGE, next, inFlight: false }));
  assert.ok(!wantsDepth({ loaded: 3 * PAGE, wanted: 3 * PAGE, next, inFlight: false }), "as deep as it was");
  assert.ok(!wantsDepth({ loaded: PAGE, wanted: 3 * PAGE, next: null, inFlight: false }), "the feed ended sooner than it did");
  assert.ok(!wantsDepth({ loaded: PAGE, wanted: 3 * PAGE, next, inFlight: true }), "one page in flight at a time");
  assert.ok(!wantsDepth({ loaded: 0, wanted: 3 * PAGE, next, inFlight: false }), "the head page is the screen's own read");
  assert.ok(!wantsDepth({ loaded: PAGE, wanted: 0, next, inFlight: false }), "nothing kept asks for nothing");
  // Ten pages at most, whatever the memory says.
  let asked = 0;
  for (let loaded = PAGE; wantsDepth({ loaded, wanted: parseDepth(99 * PAGE), next, inFlight: false }); loaded += PAGE) asked += 1;
  assert.equal(asked, MAX_PAGES - 1, "the head page and nine more");
});

/** A variant's wire tag: its name in snake case, as `rename_all` spells it. */
const tagOf = (variant) => variant.replace(/(?<!^)(?=[A-Z])/g, "_").toLowerCase();

/** The engine's own placing of every fact, read from `activity::concept_of`: tag → concept, `none` or `host`. */
function enginePlacing() {
  const rust = readFileSync(join(here, "../../../../crates/bisa-engine/src/activity.rs"), "utf8");
  const body = rust.slice(rust.indexOf("pub fn concept_of"), rust.indexOf("/// The concept a host's listening files under."));
  const placed = new Map();
  // An arm is the variants it names, then `=>`, then what it answers — up to the next arm's first variant.
  const tokens = [...body.matchAll(/EnginePayload::([A-Za-z]+)|=>/g)];
  let variants = [];
  tokens.forEach((token, i) => {
    if (token[1]) {
      variants.push(tagOf(token[1]));
      return;
    }
    // A `=>` inside an answer — a `match` over the listener — names no arm.
    if (variants.length === 0) return;
    const next = tokens.slice(i + 1).find((t) => t[1]);
    const answer = body.slice(token.index, next ? next.index : body.length);
    const concept = /return None/.test(answer) ? "none" : /host_concept/.test(answer) ? "host" : (answer.match(/C::([A-Z][a-z]+)/)?.[1].toLowerCase() ?? null);
    assert.ok(concept, `an arm nobody can read: ${variants.join(", ")}`);
    for (const v of variants) placed.set(v, concept);
    variants = [];
  });
  return placed;
}

test("which facts are in the feed, and under which concept, is the engine's rule word for word", () => {
  const placed = enginePlacing();
  assert.ok(placed.size > 70 && placed.get("run_finished") === "goals" && placed.get("agent_streamed") === "none", `the engine's match is read: ${placed.size} facts`);
  const none = [...placed].filter(([, c]) => c === "none").map(([t]) => t);
  assert.deepEqual([...NOT_ACTIVITY].sort(), none.sort(), "the facts that are no activity");
  const hosted = [...placed].filter(([, c]) => c === "host").map(([t]) => t);
  assert.deepEqual([...HOSTED_FACTS].sort(), hosted.sort(), "the facts a listener's host places");
  const filed = Object.fromEntries([...placed].filter(([, c]) => c !== "none" && c !== "host"));
  assert.deepEqual({ ...FACT_CONCEPT }, filed, "every other fact, under the concept the engine files it");
  for (const c of Object.values(FACT_CONCEPT)) assert.ok(CONCEPTS.includes(c), `${c} is a tab`);
});

test("a fact nudges the tab it is filed under and All; one that is no activity nudges nobody; one nobody can place nudges every tab", () => {
  assert.equal(conceptOfFact({ type: "run_finished" }), "goals");
  assert.equal(conceptOfFact({ type: "workflow_changed" }), "workflows");
  assert.equal(conceptOfFact({ type: "settings_changed" }), "node");
  for (const type of NOT_ACTIVITY) assert.equal(conceptOfFact({ type }), false, type);
  // What a listener heard and did is its host's story.
  assert.equal(conceptOfFact({ type: "listener_fired", listener: "workspace:01WF/ticket" }), "workflows");
  assert.equal(conceptOfFact({ type: "listener_failed", listener: "goal:01G/ticket" }), "goals");
  assert.equal(conceptOfFact({ type: "listening_changed", host: "goal:01G" }), "goals");
  assert.equal(conceptOfFact({ type: "listening_changed", host: "workspace:01WF" }), "workflows");
  assert.equal(conceptOfFact({ type: "signal_received", listener: null }), "workflows", "a named signal no listener was named for");
  for (const odd of [null, undefined, {}, { type: 7 }, { type: "invented_later" }]) assert.equal(conceptOfFact(odd), null, `${JSON.stringify(odd)}`);

  const nudged = (type, extra = {}) => CONCEPTS.filter((tab) => wantsNudge(tab, conceptOfFact({ type, ...extra })));
  assert.deepEqual(nudged("run_finished"), ["all", "goals"]);
  assert.deepEqual(nudged("workstream_committed"), ["all", "projects"]);
  assert.deepEqual(nudged("agent_streamed"), [], "an agent's words as they come are no rows: nobody reads the head for them");
  assert.deepEqual(nudged("session"), []);
  assert.deepEqual(nudged("file_changed"), []);
  assert.deepEqual(nudged("invented_later"), [...CONCEPTS], "a read too many costs less than a stale screen");
  const screen = readFileSync(join(here, "../Pulse.tsx"), "utf8");
  assert.ok(screen.includes("wantsNudge(concept, conceptOfFact(e.payload))"), "the screen asks the model about every frame");
  assert.ok(!screen.includes("wantsNudge(concept, null)"), "and nudges for none it was not asked about");
});

test("a Pulse left open holds ten pages at most below what the window draws, and pages the rest in again from where it was cut", () => {
  const rows = (n, from = 100_000) => Array.from({ length: n }, (_, i) => ({ seq: from - i, at: from - i }));
  assert.equal(HELD_MAX, PAGE * MAX_PAGES);
  const small = { items: rows(HELD_MAX), next: null };
  assert.strictEqual(heldFeed(small), small, "within the bound the feed is the same object — its end still its end");
  const grown = heldFeed({ items: rows(HELD_MAX + 37), next: null });
  assert.equal(grown.items.length, HELD_MAX);
  assert.deepEqual(grown.next, { at: grown.items.at(-1).at, seq: grown.items.at(-1).seq }, "the cursor is the last row kept: the page after it is what was let go");
  assert.equal(grown.items[0].seq, 100_000, "the head is what is kept");
  // The reader is far down: nothing is cut under their eyes.
  const deep = { items: rows(2000), next: { at: 1, seq: 1 } };
  assert.strictEqual(heldFeed(deep, 1990), deep, "the window draws row 1990: a page below it is kept");
  const lower = heldFeed(deep, 1200);
  assert.equal(lower.items.length, 1200 + PAGE);
  assert.deepEqual(lower.next, { at: lower.items.at(-1).at, seq: lower.items.at(-1).seq });
  for (const odd of [Number.NaN, -5, 1.5, undefined]) assert.equal(heldFeed({ items: rows(HELD_MAX + 1), next: null }, odd).items.length, HELD_MAX, `${odd} is no row`);
  // A day of nudges: the head joins page after page and the feed never grows past the bound.
  let feed = { items: rows(PAGE, 80), next: { at: 1, seq: 1 } };
  // Each head page reaches the one before it by a row, as a nudge's read does.
  for (let head = 80 + 79; head <= 80 * 40; head += 79) {
    const joined = joinHead({ shown: feed.items, page: rows(PAGE, head), next: { at: head - 79, seq: head - 79 }, tail: feed.next });
    assert.equal(joined.restarted, false);
    feed = heldFeed({ items: joined.items, next: joined.next });
    assert.ok(feed.items.length <= HELD_MAX);
  }
  assert.equal(feed.items.length, HELD_MAX);
  assert.equal(new Set(feed.items.map((r) => r.seq)).size, HELD_MAX, "every fact once");
  // Scrolling down asks for the page after the cut: what was let go comes back, and nothing twice.
  const older = rows(PAGE, feed.next.seq - 1);
  const again = mergePage(feed.items, older, "older");
  assert.equal(again.length, HELD_MAX + PAGE);
  assert.equal(again[HELD_MAX].seq, feed.next.seq - 1, "no hole at the cut");
  const screen = readFileSync(join(here, "../Pulse.tsx"), "utf8");
  assert.ok(screen.includes("hold(heldFeed({ items: joined.items, next: joined.next }, range.current[1]))"), "the screen holds what the model says");
});

test("the screen asks the workspace's one rule which kind a channel id is", () => {
  const screen = readFileSync(join(here, "../Pulse.tsx"), "utf8");
  assert.ok(screen.includes("scopeKindOf(id, ws.channels, ws.dms)") && !screen.includes("ws.dms.some("));
});
