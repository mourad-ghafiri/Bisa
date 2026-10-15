import assert from "node:assert/strict";
import test from "node:test";
import { emptyPlaceIndex, footerSessions, placeWords } from "./footerSessionsModel.mjs";
import { channelDefOf, stateWords } from "./hostedModel.mjs";
import { addTab, closeLeaf, dividers, leaves, singleLeaf, splitLeaf } from "./paneTreeModel.mjs";
import { dimensionRows, metricWords, overlayRows } from "./resourceModel.mjs";
import { originWords, titleOf } from "./sessionOriginModel.mjs";
import { createSingleFlight } from "./singleFlight.mjs";
import { KeptMemory } from "./keptMemoryModel.mjs";
import { webStorage } from "./storedPrefModel.mjs";
import { updateState, updateWords } from "./updateModel.mjs";
import { countWords } from "./statusBarModel.mjs";
import { initialState, reduce } from "../ui/fileTreeModel.mjs";
import { nextCopyName } from "../ui/fileTreeMutations.mjs";
import { dropSlots, stepSlot } from "../ui/tree/treeListModel.mjs";
import { checkedWords, policyWords } from "../views/_settings/mobileDevelopmentSettingsModel.mjs";
import { roleChangeWords } from "../views/_settings/peopleModel.mjs";
import { matcherWords } from "../views/_settings/securityRules.mjs";

test("the footer says a scope it does not know by its kind and tail, and a harness by the folder it runs in", () => {
  const words = placeWords("bogus", "01HXYZABCDEFGHJKMNPQRSTVWX", emptyPlaceIndex());
  assert.ok(words.includes("bogus"), words);
  const out = footerSessions([], [{ id: "s1", kind: "worker", state: { state: "running" }, harness: "claude-code", agent: "general-agent", workstream: null, cwd: "/Users/ada/Projects/shelf" }]);
  assert.equal(out.harnesses.length, 1);
  assert.ok(out.harnesses[0].place.includes("Projects/shelf"), out.harnesses[0].place);
});

test("a hosted state nobody knows has no words, and a host's channel is filled in to the screens' shape", () => {
  assert.equal(stateWords({ state: { state: "bogus" } }), null);
  const def = channelDefOf({ id: "design", name: "Design" });
  assert.deepEqual(def.audience, { audience: "workspace" });
  assert.equal(def.roster.policy, "listed");
});

test("a tab added to the leaf it is in only changes which is active; a leaf closed deep in the tree folds its split; a column split has a horizontal divider", () => {
  let t = singleLeaf("p1", ["t1", "t2"], "t2");
  const same = addTab(t, "p1", "t1", true);
  assert.equal(leaves(same)[0].active, "t1");
  assert.equal(addTab(t, "p1", "t1", false), t, "nothing to change");
  t = splitLeaf(t, "p1", "row", "s1", "p2");
  t = addTab(t, "p2", "t3");
  t = splitLeaf(t, "p2", "col", "s2", "p3");
  t = addTab(t, "p3", "t4");
  const closed = closeLeaf(t, "p3");
  assert.deepEqual(leaves(closed).map((l) => l.id), ["p1", "p2"]);
  assert.ok(leaves(closed)[1].tabs.includes("t4"), "its tab moved to its neighbour");
  const cols = dividers(t).filter((d) => d.dir === "col");
  assert.equal(cols.length, 1);
  assert.equal(cols[0].splitId, "s2");
});

test("a dimension nobody knows groups nothing, a metric nobody knows reads as a dash, and the disk is split by goal, project, harness and terminal", () => {
  const rows = dimensionRows("bogus", [{ kind: "session", harness: "h", cpu: 1, memory: 1 }], emptyPlaceIndex(), "cpu", 1);
  assert.deepEqual(rows, []);
  assert.deepEqual(metricWords("bogus", null, null, null, null), { value: "—", title: "" });
  const disk = { dirs: [
    { path: "goals/01HXYZABCDEFGHJKMNPQRSTVWX", bytes: 5 },
    { path: "projects/shelf", bytes: 7 },
    { path: "sessions/claude", bytes: 9 },
    { path: "run/terminals", bytes: 11 },
  ] };
  for (const [dimension, value] of [["goals", 5], ["projects", 7], ["harnesses", 9], ["terminals", 11]]) {
    const out = overlayRows("disk", dimension, { shares: [], disk });
    assert.equal(out.length, 1, dimension);
    assert.equal(out[0].value, value, dimension);
  }
});

test("an ask is titled by its purpose, a turn in a conversation nobody indexed is said as one, and an ask of another purpose is said plainly", () => {
  const decision = titleOf({ kind: "ask", origin: { origin: "ask", purpose: { kind: "decision" } } });
  const commit = titleOf({ kind: "ask", origin: { origin: "ask", purpose: { kind: "commit_message" } } });
  const other = titleOf({ kind: "ask", origin: { origin: "ask", purpose: { kind: "bogus" } } });
  assert.notEqual(decision, commit);
  assert.ok(other.length > 0);
  const turn = originWords({ kind: "conversation", conversation: "c-nobody-indexed" });
  assert.ok(turn.length > 0);
  const ask = originWords({ kind: "ask", origin: { origin: "ask", purpose: { kind: "bogus" } } });
  assert.ok(ask.length > 0);
});

test("a flight that fails is refused to a joiner that could still have left", async () => {
  const flights = createSingleFlight();
  const controller = new AbortController();
  await assert.rejects(flights.join("k", () => Promise.reject(new Error("no answer")), controller.signal), /no answer/);
  assert.equal(flights.inFlight("k"), false);
});

test("a value the store cannot write is kept without its text, and a storage that throws is none", () => {
  const kept = new KeptMemory({ key: "k", version: 1, caps: {}, hands: { storage: () => null, now: () => 1, later: () => 0, cancel: () => {} } });
  assert.equal(kept.put("place", "name", 1n), true);
  const before = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", { configurable: true, get() { throw new Error("denied"); } });
  try {
    assert.equal(webStorage(), null);
  } finally {
    if (before) Object.defineProperty(globalThis, "localStorage", before);
    else delete globalThis.localStorage;
  }
});

test("an update check in a state nobody knows, and a state nobody knows, read as unexpected failures", () => {
  assert.equal(updateState({ app: "1.0.0", check: { state: "bogus" }, loading: false, error: null }).reason, "unexpected");
  assert.ok(updateWords({ kind: "bogus" }).line.length > 0);
});

test("a folder toggles open and shut, an action nobody knows leaves the tree as it is, and a count of something else is its number", () => {
  const state = initialState([]);
  const opened = reduce(state, { type: "toggle", path: "a" });
  assert.equal(opened.open.a, true);
  const shut = reduce(opened, { type: "toggle", path: "a" });
  assert.equal(shut.open.a, undefined);
  assert.equal(reduce(state, { type: "bogus" }), state);
  assert.equal(countWords("bogus", 3), "3");
});

test("a copy's name past a thousand siblings is stamped with the time", () => {
  const siblings = new Set();
  let name = "";
  for (let i = 0; i < 1000; i += 1) {
    name = nextCopyName("plan.md", siblings);
    siblings.add(name);
  }
  assert.ok(/\d{10,}/.test(name), name);
});

test("a key nobody steps with keeps the slot, and the small words of the settings", () => {
  const rows = [{ id: "a", depth: 0, kind: "dir" }, { id: "b", depth: 0, kind: "file" }];
  const slots = dropSlots(rows, "b");
  assert.ok(slots.length > 0);
  assert.equal(stepSlot(rows, slots, slots[0], "Home"), slots[0]);
  assert.ok(policyWords("nobody").length > 0);
  assert.ok(policyWords("bogus").length > 0);
  const now = 10_000_000;
  assert.ok(checkedWords(now - 3 * 86400, now).includes("3"));
  assert.equal(roleChangeWords("admin", "owner"), null);
  assert.equal(matcherWords({ kind: "bogus" }), "");
});
