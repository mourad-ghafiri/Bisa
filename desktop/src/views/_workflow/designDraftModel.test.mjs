/**
 * A design in progress, as it is kept across a restart. Run with
 * `node --test desktop/src/views/_workflow/designDraftModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { readFileSync } from "node:fs";

import { Lru } from "../../shell/lru.mjs";
import { MAX_DRAFTS, draftBody } from "./designDraftModel.mjs";
import { canUndo, create, push } from "./history.mjs";
import { blankWorkflow } from "./stepKinds.mjs";

const drawing = () => ({
  name: "Ship it",
  description: "",
  inputs: [],
  steps: [
    { id: "start", name: "Start", kind: "start", next: "build" },
    { id: "build", name: "Build", kind: "agent", instructions: "build the thing" },
  ],
  tags: ["delivery"],
  decision_making: false,
});

test("a drawing reads back as it was kept", () => {
  const kept = JSON.parse(JSON.stringify(drawing()));
  assert.deepEqual(draftBody(kept), drawing());
  assert.equal(draftBody(kept), kept, "the body itself, not a copy with fields left out");
  assert.ok(draftBody(JSON.parse(JSON.stringify(blankWorkflow("Untitled")))), "a blank canvas is a drawing too");
  assert.ok(draftBody({ name: "", steps: [] }), "with nothing drawn yet");
});

test("what is kept is the drawing as it stands, and it comes back as a history of one entry", () => {
  let history = create(drawing());
  history = push(history, { ...drawing(), name: "Renamed" });
  assert.ok(canUndo(history));
  const kept = JSON.parse(JSON.stringify(history.present));
  const restored = create(draftBody(kept));
  assert.equal(restored.present.name, "Renamed");
  assert.equal(canUndo(restored), false, "the undo history is the window's, never the memory's");
  assert.deepEqual([restored.past, restored.future], [[], []]);
});

test("what is no drawing is nothing", () => {
  for (const bad of [
    null,
    undefined,
    "a workflow",
    7,
    [],
    [drawing()],
    {},
    { name: "No steps" },
    { steps: [] },
    { name: 7, steps: [] },
    { name: "Steps that are no list", steps: {} },
    { name: "A step that is no record", steps: ["build"] },
    { name: "A step with no id", steps: [{ kind: "agent" }] },
    { name: "A step with an empty id", steps: [{ id: "", kind: "agent" }] },
    { name: "A step with no kind", steps: [{ id: "build" }] },
    { ...drawing(), inputs: "none" },
    { ...drawing(), tags: [7] },
    // A history kept whole by another version is not a drawing.
    { past: [], present: drawing(), future: [], capacity: 100 },
  ]) {
    assert.equal(draftBody(bad), null, JSON.stringify(bad));
  }
});

test("the window holds the newest designs with their undo history; one past the cap has lost its history and nothing else", () => {
  assert.equal(MAX_DRAFTS, 16);
  // The store's own shape: a history a goal, the newest drawn kept — and the drawing as it stands in a memory beside it.
  const held = new Lru(MAX_DRAFTS);
  const memory = new Map();
  const draw = (goal, body) => {
    const was = held.get(goal);
    const next = was ? push(was, body) : push(create(drawing()), body);
    held.set(goal, next);
    memory.set(goal, JSON.parse(JSON.stringify(next.present)));
  };
  const draftOf = (goal) => {
    const found = held.get(goal);
    if (found !== undefined) return found;
    const body = draftBody(memory.get(goal));
    const restored = body ? create(body) : null;
    held.set(goal, restored);
    return restored;
  };
  for (let n = 0; n <= MAX_DRAFTS; n += 1) draw(`goal-${n}`, { ...drawing(), name: `Design ${n}` });
  assert.equal(held.size, MAX_DRAFTS, "a window that lives for weeks holds sixteen histories, never every one it drew");
  assert.equal(held.has("goal-0"), false, "the oldest drawn went first");
  assert.ok(canUndo(held.get(`goal-${MAX_DRAFTS}`)), "the newest keeps its undo");
  const back = draftOf("goal-0");
  assert.equal(back.present.name, "Design 0", "the drawing comes back as it stood");
  assert.equal(canUndo(back), false, "as after a restart: a history of one entry");
  assert.equal(draftOf("goal-0"), back, "and is the same history until it is drawn on");
  assert.equal(draftOf("never-drawn"), null);

  const store = readFileSync(new URL("./designDraftStore.ts", import.meta.url), "utf8");
  assert.ok(store.includes("const drafts = new Lru<Draft>(MAX_DRAFTS);"), "the store holds its histories in the bounded map");
  assert.ok(!/new Map<string, Draft>/.test(store), "and in no map that grows for the life of the window");
  assert.ok(store.includes("docViews.keepQuietly(placeOfGoal(goal), DRAWING, next ? next.present : null);"), "the drawing as it stands is in the memory from its first edit");
});
