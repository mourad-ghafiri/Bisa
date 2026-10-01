/**
 * Where a person was: an arrival is given what its path remembers, an exact
 * entry is taken as it is, a section's door returns to where they were, and
 * nothing a link hands over once — an invitation code least of all — reaches
 * the memory. Run with `node --test desktop/src/shell/placeMemoryModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";
import { ROUTE_NAMES, href } from "../routeModel.mjs";
import { sourceFiles } from "../testWalk.mjs";
import { PANEL_PARAM } from "../views/_workflow/designerPanelModel.mjs";
import {
  CLASSIFIED,
  MAX_PATHS,
  MAX_QUERY,
  ONE_SHOT,
  PANE_KEYS,
  PLACES_KEY,
  PLACES_VERSION,
  REMEMBERED,
  adoptPlaces,
  arrive,
  emptyPlaces,
  forgetPath,
  indexHash,
  keptQuery,
  knowsPath,
  launchHash,
  parsePlaces,
  pathOf,
  rememberedQuery,
  routeOf,
  sectionHash,
  splitQuery,
} from "./placeMemoryModel.mjs";

/** Walk a person through hashes, each an arrival unless it says `exact`; answers the memory and where each landed. */
function walk(steps, from = emptyPlaces()) {
  let places = from;
  const landed = [];
  for (const step of steps) {
    const [hash, how] = Array.isArray(step) ? step : [step, {}];
    const out = arrive(places, hash, how);
    places = out.places;
    landed.push(out.land);
  }
  return { places, landed };
}

test("every route has its row in the table, and no key is in two classes", () => {
  assert.deepEqual(Object.keys(REMEMBERED).sort(), [...ROUTE_NAMES].sort());
  const state = new Set(Object.values(REMEMBERED).flat());
  for (const key of PANE_KEYS) assert.ok(!state.has(key), `${key} is the pane's, not a screen's`);
  for (const key of ONE_SHOT) assert.ok(!state.has(key) && !PANE_KEYS.includes(key), `${key} is handed over once and kept by no route`);
  assert.deepEqual([...CLASSIFIED].sort(), [...new Set([...state, ...PANE_KEYS, ...ONE_SHOT])].sort());
  assert.equal(PLACES_KEY, "bisa.view.places");
});

test("a hash names a route or names none, and a path is the router's own spelling", () => {
  assert.deepEqual(routeOf("#/goals/01G?tab=workflow"), { name: "goal", id: "01G" });
  assert.equal(routeOf(""), null);
  assert.equal(routeOf("#/"), null);
  assert.equal(routeOf("#/triggers"), null);
  assert.equal(pathOf("#/goals/01G?tab=workflow"), "/goals/01G");
  assert.equal(pathOf("#/goals/"), "/goals", "a trailing slash is the same place");
  assert.equal(pathOf("#/nowhere/"), "/nowhere");
  assert.equal(pathOf(""), "/");
});

test("a query is split for the route it is on: the screen's state, the pane's, and what the link carries", () => {
  assert.deepEqual(splitQuery("goal", "tab=workflow&step=build&aux=inspector&insp=projects&run=01R"), {
    state: [["tab", "workflow"]],
    pane: [["aux", "inspector"], ["insp", "projects"]],
    carried: [["step", "build"], ["run", "01R"]],
  });
  assert.deepEqual(splitQuery("workbench", "doc=file%3Asrc%2Fa.rs&conversation=01C&panel=agents").carried, [["conversation", "01C"], ["panel", "agents"]], "a workbench's conversation is handed over once");
  assert.deepEqual(splitQuery("workflow", "conversation=01C").state, [["conversation", "01C"]], "a designer's is its state");
  assert.equal(keptQuery("goal", "?tab=workflow&step=build&q="), "tab=workflow", "an empty value is no value");
  assert.equal(keptQuery("channel", "item=01M&aux=thread&auxId=01M"), "aux=thread&auxId=01M", "a key the route does not list is dropped");
});

test("a bare arrival is given the query its path remembers", () => {
  const { places, landed } = walk([
    "#/goals/01G",
    ["#/goals/01G?tab=workflow", { exact: true }],
    "#/inbox",
    "#/goals/01G",
  ]);
  assert.deepEqual(landed, ["#/goals/01G", "#/goals/01G?tab=workflow", "#/inbox", "#/goals/01G?tab=workflow"]);
  assert.equal(rememberedQuery(places, "/goals/01G"), "tab=workflow");
  assert.equal(places.last, "#/goals/01G?tab=workflow");
});

test("a link with only one-shot keys keeps them and is given the rest", () => {
  const { places, landed } = walk([
    ["#/workflows/01W?conversation=01C", { exact: true }],
    "#/runs/01R",
    "#/workflows/01W?panel=runs",
    // The designer takes `panel` off the address: an exact entry, written by the router.
    ["#/workflows/01W?conversation=01C", { exact: true }],
  ]);
  assert.equal(landed[2], "#/workflows/01W?conversation=01C&panel=runs");
  assert.equal(rememberedQuery(places, "/workflows/01W"), "conversation=01C", "what was handed over once was never kept");
});

test("an explicit query is exact and the pane is filled from memory", () => {
  const { landed } = walk([
    ["#/goals/01G?tab=progress&aux=inspector&insp=projects", { exact: true }],
    "#/pulse",
    "#/goals/01G?tab=workflow",
    "#/pulse",
    // A link that names its own pane gets that pane, and the screen's state from memory.
    "#/goals/01G?aux=session&auxId=01S",
  ]);
  assert.equal(landed[2], "#/goals/01G?tab=workflow&aux=inspector&insp=projects");
  assert.equal(landed[4], "#/goals/01G?tab=workflow&aux=session&auxId=01S");
});

test("a pane closed by hand stays closed, and a filter cleared by hand stays cleared", () => {
  const { places, landed } = walk([
    ["#/goals/01G?tab=workflow&aux=inspector", { exact: true }],
    ["#/goals/01G?tab=workflow", { exact: true }],
    "#/inbox",
    "#/goals/01G",
    ["#/goals?holder=you&q=ship", { exact: true }],
    ["#/goals", { exact: true }],
    "#/inbox",
    "#/goals",
  ]);
  assert.equal(landed[3], "#/goals/01G?tab=workflow");
  assert.equal(landed[7], "#/goals");
  assert.equal(rememberedQuery(places, "/goals"), "", "been on, and left with nothing");
  assert.equal(knowsPath(places, "/goals"), true);
  assert.equal(knowsPath(places, "/teams"), false);
});

test("an entry already resolved is never rewritten: Back onto a bare one is bare", () => {
  const { landed } = walk([
    ["#/goals", { exact: true }],
    ["#/goals?holder=you", { exact: true }],
    "#/inbox",
    ["#/goals", { exact: true }],
  ]);
  assert.equal(landed[3], "#/goals");
});

test("a bare navigation on the screen one is on keeps what it shows", () => {
  const { landed } = walk([["#/goals?holder=you", { exact: true }], "#/goals"]);
  assert.equal(landed[1], "#/goals?holder=you", "the New goal chord on a filtered list");
});

test("a one-shot key is never remembered and an invitation code never reaches the memory", () => {
  const { places, landed } = walk(["#/settings?tab=people&join=SECRETCODE", "#/goals/01G?tab=workflow&step=build&edit=1&run=01R"]);
  assert.equal(landed[0], "#/settings?tab=people&join=SECRETCODE", "the link is stood on whole");
  const stored = JSON.stringify(places);
  for (const word of ["SECRETCODE", "join", "step", "edit", "01R"]) assert.ok(!stored.includes(word), `${word} is in no memory`);
  assert.equal(places.last, "#/goals/01G?tab=workflow");
  assert.equal(places.sections.settings, "#/settings?tab=people");
});

test("a hash that names no screen lands as it is and records nothing", () => {
  const before = walk(["#/goals?holder=you"]).places;
  const out = arrive(before, "#/triggers?x=1");
  assert.equal(out.land, "#/triggers?x=1");
  assert.equal(out.places, before);
  assert.equal(out.known, false);
  assert.equal(arrive(before, "").places, before);
});

test("an arrival says whether its path had been on before", () => {
  const first = arrive(emptyPlaces(), "#/goals/01G");
  assert.equal(first.known, false);
  assert.equal(arrive(first.places, "#/goals/01G").known, true);
  assert.equal(arrive(first.places, "#/goals/01H").known, false);
});

test("the same landing is the same memory", () => {
  const once = arrive(emptyPlaces(), "#/goals?holder=you", { exact: true });
  assert.equal(arrive(once.places, "#/goals?holder=you", { exact: true }).places, once.places);
});

test("a query too long to keep is stood on and not remembered", () => {
  const long = `#/goals?q=${"x".repeat(MAX_QUERY)}`;
  const out = arrive(emptyPlaces(), long, { exact: true });
  assert.equal(out.land, long);
  assert.equal(out.places.last, null);
});

test("the newest paths keep their query, the oldest past the cap forgotten", () => {
  let places = emptyPlaces();
  for (let i = 0; i < MAX_PATHS + 5; i++) places = arrive(places, `#/goals/g${i}?tab=workflow`, { exact: true }).places;
  assert.equal(places.queries.length, MAX_PATHS);
  assert.equal(knowsPath(places, "/goals/g0"), false);
  assert.equal(knowsPath(places, `/goals/g${MAX_PATHS + 4}`), true);
});

test("a section's door returns to where the person was, to its index from inside it, and nowhere from its index", () => {
  const { places } = walk([
    ["#/goals?holder=you", { exact: true }],
    "#/goals/01G",
    ["#/goals/01G?tab=workflow", { exact: true }],
    "#/inbox",
  ]);
  assert.equal(sectionHash(places, "goals", "#/inbox"), "#/goals/01G?tab=workflow", "from another section: where they were");
  assert.equal(sectionHash(places, "goals", "#/goals/01G?tab=workflow"), "#/goals?holder=you", "from inside the goal: the list, as it was left");
  assert.equal(sectionHash(places, "goals", "#/goals?holder=you"), "#/goals?holder=you", "from the list: nowhere");
  assert.equal(sectionHash(places, "workflows", "#/inbox"), "#/workflows", "a section never been in opens on its index");
  assert.equal(sectionHash(places, "workflows", "#/runs/01R"), "#/workflows", "a run belongs to Workflows");
  assert.equal(sectionHash(places, "goals", ""), "#/goals/01G?tab=workflow", "from no place at all");
  assert.equal(indexHash(places, "goals"), "#/goals?holder=you");
  assert.equal(indexHash(places, "teams"), href({ name: "teams" }));
});

test("projects has no list: from inside the IDE its door leads nowhere", () => {
  const { places } = walk(["#/projects/workstream/01S?doc=file%3Aa.rs", "#/inbox"]);
  assert.equal(sectionHash(places, "projects", "#/inbox"), "#/projects/workstream/01S?doc=file%3Aa.rs");
  assert.equal(sectionHash(places, "projects", "#/projects/workstream/01S?doc=file%3Aa.rs"), "#/projects/workstream/01S?doc=file%3Aa.rs");
  assert.equal(sectionHash(emptyPlaces(), "projects", "#/inbox"), "#/projects");
});

test("a place that is gone is forgotten and its section falls to the index", () => {
  const { places } = walk([
    ["#/goals?holder=you", { exact: true }],
    ["#/goals/01G?tab=workflow", { exact: true }],
    ["#/hosts/h1/channels/general?aux=thread&auxId=01M", { exact: true }],
    ["#/hosts/h1/messages/01D", { exact: true }],
  ]);
  const left = forgetPath(places, "/hosts/h1");
  assert.equal(knowsPath(left, "/hosts/h1/channels/general"), false);
  assert.equal(knowsPath(left, "/hosts/h1/messages/01D"), false);
  assert.equal(left.last, null, "the last place stood there");
  assert.equal(left.sections.channels, undefined);
  assert.equal(left.sections.messages, undefined);
  assert.equal(left.sections.goals, "#/goals/01G?tab=workflow");

  const gone = forgetPath(left, "/goals/01G");
  assert.equal(sectionHash(gone, "goals", "#/inbox"), "#/goals?holder=you", "the door leads to the list");
  assert.equal(knowsPath(gone, "/goals"), true, "the list is not under the goal");
  assert.equal(forgetPath(gone, "/goals/01G"), gone, "nothing stood there: the same memory");
  // A path that only begins the same is another place.
  assert.equal(knowsPath(forgetPath(walk([["#/goals/01GG?tab=workflow", { exact: true }]]).places, "/goals/01G"), "/goals/01GG"), true);
});

test("nothing remembered opens on the home; a last place opens there", () => {
  assert.equal(launchHash(emptyPlaces()), null);
  const { places } = walk(["#/goals/01G", ["#/goals/01G?tab=workflow&aux=inspector", { exact: true }]]);
  assert.equal(launchHash(places), "#/goals/01G?tab=workflow&aux=inspector");
  assert.equal(launchHash({ ...places, last: "#/triggers" }), null, "a place that names no screen is no place");
});

test("another workspace's memory is forgotten whole", () => {
  const { places } = walk(["#/goals/01G"]);
  const mine = adoptPlaces(places, "alice");
  assert.equal(mine.owner, "alice");
  assert.equal(mine.last, "#/goals/01G", "a memory with no owner takes the first");
  assert.equal(adoptPlaces(mine, "alice"), mine);
  assert.equal(adoptPlaces(mine, ""), mine, "no owner known yet changes nothing");
  assert.deepEqual(adoptPlaces(mine, "bob"), emptyPlaces("bob"));
});

test("what was written reads back the same, for a place of every route", () => {
  const hashes = [
    "#/pulse?concept=goals",
    "#/inbox?filter=all&source=goals&item=01G",
    "#/goals?holder=you&workflow=01W&q=ship&archived=1",
    "#/goals/01G?tab=conversation&conversation=01C&aux=inspector&insp=files",
    "#/workflows?view=templates&q=review&status=on",
    "#/workflows/01W?conversations=1",
    "#/runs/01R",
    "#/projects/workstream/01S?doc=file%3Asrc%2Fmain.rs",
    "#/hosts/abcd/channels/general",
    "#/hosts/abcd/messages/01D",
    "#/channels",
    "#/channels/general?aux=thread&auxId=01M",
    "#/messages",
    "#/messages/01D",
    "#/conversations/01C",
    "#/agents?tab=running",
    "#/agents/general-agent",
    "#/teams?team=01T",
    "#/settings?tab=catalog-agent&kind=agent",
  ];
  const { places, landed } = walk(hashes.map((h) => [h, { exact: true }]));
  assert.deepEqual(landed, hashes);
  assert.deepEqual([...new Set(hashes.map((h) => routeOf(h).name)), "projects"].sort(), [...ROUTE_NAMES].sort(), "the fixture covers the table");
  const back = parsePlaces(JSON.parse(JSON.stringify(places)));
  assert.deepEqual(back, places);
});

test("another version is refused, and within this one only what still names a screen survives", () => {
  assert.equal(parsePlaces(null), undefined);
  assert.equal(parsePlaces("words"), undefined);
  assert.equal(parsePlaces([]), undefined);
  assert.equal(parsePlaces({ v: PLACES_VERSION + 1, last: "#/goals", sections: {}, queries: [] }), undefined);
  const read = parsePlaces({
    v: PLACES_VERSION,
    owner: 7,
    last: "#/triggers?x=1",
    sections: { goals: "#/goals/01G?tab=workflow&join=CODE", workflows: "#/goals", pulse: 3 },
    queries: [["/goals/01G", "tab=workflow&step=build"], ["/triggers", "x=1"], ["/goals"], "words", ["/teams", 4]],
  });
  assert.deepEqual(read, {
    v: PLACES_VERSION,
    owner: null,
    last: null,
    sections: { goals: "#/goals/01G?tab=workflow" },
    queries: [["/goals/01G", "tab=workflow"]],
  });
});

// ---------------------------------------------------------------------------
// The source guard: every query key the desktop reads or writes is classified
// ---------------------------------------------------------------------------

const here = dirname(fileURLToPath(import.meta.url));
const src = join(here, "..");

/** The keys a source names in `useSearchValue("k")` and in the literal patches of `setSearch({ k: … })`. */
function keysNamed(text) {
  const keys = new Set();
  for (const m of text.matchAll(/useSearchValue\("([A-Za-z_]+)"\)/g)) keys.add(m[1]);
  for (const m of text.matchAll(/\bsetSearch\(\{([^}]*)\}/g)) {
    for (const k of m[1].matchAll(/(?:^|,)\s*([A-Za-z_]+)\s*:/g)) keys.add(k[1]);
  }
  return keys;
}

test("every query key the desktop reads or writes is in a table", () => {
  const found = new Map();
  for (const file of sourceFiles(src, (p) => /\.(ts|tsx)$/.test(p))) {
    for (const key of keysNamed(readFileSync(file, "utf8"))) found.set(key, relative(src, file));
  }
  assert.ok(found.size > 15, `found ${found.size} keys; the scanner is broken`);
  const unclassified = [...found].filter(([key]) => !CLASSIFIED.includes(key)).map(([key, file]) => `${key} (${file})`);
  assert.deepEqual(unclassified, [], "a query key is state a route remembers, the pane's, or handed over once — say which in placeMemoryModel.mjs");
  assert.ok(CLASSIFIED.includes(PANEL_PARAM), "the designer's panel parameter is read through its constant");
});

test("the scanner reads both spellings", () => {
  assert.deepEqual([...keysNamed('const [a] = useSearchValue("tab"); setSearch({ aux: "x", auxId: id ?? null }); setSearch({ [K]: null }, { replace: true });')].sort(), ["aux", "auxId", "tab"]);
});
