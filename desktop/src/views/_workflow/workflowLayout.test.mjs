/**
 * The layout, pinned where it matters: a step's own position is kept; a step
 * with none is placed by the layered layout, the same way every time, below
 * what is placed; a drop onto a card lands beside it; *Tidy* redraws every
 * card and puts the list in reading order; the first edit writes the picture.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { BOUNDARY_ROW, GAP_Y, GRID, NODE_HEIGHT, NODE_WIDTH, estimateOf, layout, nudgeFree, overlaps, sideOnly, snapTo, tidy, withPositions } from "./workflowLayout.mjs";
import { toGraph } from "./workflowGraph.mjs";

const step = (id, then = [], extra = {}) => ({ id, name: id, kind: "agent", instructions: id, then, ...extra });
const wf = (...steps) => ({ name: "w", steps });
const row = NODE_HEIGHT + GAP_Y;
const box = (p) => ({ ...p, width: NODE_WIDTH, height: NODE_HEIGHT });

test("a chain with no positions is one column, one row per step, on the grid", () => {
  const { positions, size, loops } = layout(wf(step("a", [{ to: "b" }]), step("b", [{ to: "c" }]), step("c")));
  assert.deepEqual([...positions.keys()], ["a", "b", "c"]);
  assert.equal(positions.get("a").x, positions.get("b").x);
  assert.equal(positions.get("a").y, 0);
  assert.equal(positions.get("b").y, row);
  assert.equal(positions.get("c").y, 2 * row);
  for (const p of positions.values()) assert.ok(p.x % GRID === 0 && p.y % GRID === 0, `on the grid: ${JSON.stringify(p)}`);
  assert.equal(size.width, NODE_WIDTH);
  assert.equal(size.height, 2 * row + NODE_HEIGHT);
  assert.equal(loops.size, 0);
});

test("a diamond puts its arms side by side in steps order, above the join", () => {
  const { positions } = layout(wf(step("d", [{ to: "b" }, { to: "c" }]), step("b", [{ to: "s" }]), step("c", [{ to: "s" }]), step("s")));
  assert.equal(positions.get("b").y, positions.get("c").y);
  assert.ok(positions.get("b").x < positions.get("c").x, "arms in steps order");
  assert.ok(positions.get("c").x - positions.get("b").x >= NODE_WIDTH, "arms do not overlap");
  assert.equal(positions.get("s").y, 2 * row);
});

test("a loop is not ranked: its back edge is listed and the loop's target stays above", () => {
  const { positions, loops } = layout(wf(step("a", [{ to: "b" }]), step("b", [{ to: "a" }, { to: "c" }]), step("c")));
  assert.equal(positions.get("a").y, 0);
  assert.equal(positions.get("b").y, row);
  assert.deepEqual([...loops], ["b->a"]);
});

test("a step with a position keeps it, whatever its flows say", () => {
  const w = wf(step("a", [{ to: "b" }], { position: { x: 400, y: 800 } }), step("b", [], { position: { x: 40, y: 16 } }));
  const { positions } = layout(w);
  assert.deepEqual(positions.get("a"), { x: 400, y: 800 }, "the source stands below its target because a person put it there");
  assert.deepEqual(positions.get("b"), { x: 40, y: 16 });
  // A position off the grid — a template's, written by hand — is drawn as written.
  assert.deepEqual(layout(wf(step("a", [], { position: { x: 43, y: 21 } }))).positions.get("a"), { x: 43, y: 21 });
});

test("steps without a position are placed as a block below the placed ones, in their layered picture, free of overlap", () => {
  const w = wf(step("a", [{ to: "b" }], { position: { x: 100, y: 100 } }), step("b", [{ to: "c" }]), step("c"));
  const { positions } = layout(w);
  assert.deepEqual(positions.get("a"), { x: 100, y: 100 });
  assert.ok(positions.get("b").y >= 100 + NODE_HEIGHT + GAP_Y, "the block starts a gap below the placed card");
  assert.equal(positions.get("c").y - positions.get("b").y, row, "the block keeps its own rows");
  assert.equal(positions.get("b").x, 100, "the block lines up under the placed cards");
  for (const [i, p] of [...positions.entries()]) for (const [j, q] of [...positions.entries()]) if (i !== j) assert.ok(!overlaps(box(p), box(q)), `${i} covers ${j}`);
});

test("the derived picture is deterministic and ignores measured sizes", () => {
  const w = wf(step("d", [{ to: "b" }, { to: "c" }]), step("b", [{ to: "s" }]), step("c", [{ to: "s" }]), step("s"));
  const a = JSON.stringify([...layout(w).positions.entries()]);
  const b = JSON.stringify([...layout(w).positions.entries()]);
  assert.equal(a, b);
});

test("a flow to a missing step is ignored, not a phantom row; a step nothing reaches is still drawn", () => {
  const { positions } = layout(wf(step("a", [{ to: "gone" }]), step("x", [{ to: "y" }]), step("y", [{ to: "x" }])));
  assert.equal(positions.size, 3);
  assert.ok(!positions.has("gone"));
  assert.equal(positions.get("a").y, 0);
});

test("an empty workflow has no positions and no size", () => {
  const { positions, size, loops } = layout(wf());
  assert.equal(positions.size, 0);
  assert.deepEqual(size, { width: 0, height: 0 });
  assert.equal(loops.size, 0);
});

test("snapping rounds to the grid, in whole pixels, and a zero or missing grid is the canvas's own", () => {
  assert.deepEqual(snapTo({ x: 13, y: 12 }), { x: 16, y: 16 });
  assert.deepEqual(snapTo({ x: 13, y: 12 }, 20), { x: 20, y: 20 });
  assert.deepEqual(snapTo({ x: 13, y: 12 }, 0), { x: 16, y: 16 });
  assert.deepEqual(snapTo({ x: -3, y: 4.4 }), { x: 0, y: 8 });
});

test("a drop on a free spot stays; a drop onto a card lands beside it, then below when the row is full", () => {
  const positions = new Map([["a", { x: 0, y: 0 }], ["b", { x: NODE_WIDTH + 40, y: 0 }]]);
  assert.deepEqual(nudgeFree(positions, null, "n", { x: 0, y: 400 }), { x: 0, y: 400 }, "free: untouched");
  const beside = nudgeFree(positions, null, "n", { x: 8, y: 8 });
  assert.ok(!overlaps(box(beside), box(positions.get("a"))) && !overlaps(box(beside), box(positions.get("b"))), JSON.stringify(beside));
  assert.ok(beside.x > positions.get("b").x, "pushed right past both cards");
  assert.equal(beside.y, 8, "the row is kept");
  // A card moved onto its own place is not in its own way.
  assert.deepEqual(nudgeFree(positions, null, "a", { x: 0, y: 0 }), { x: 0, y: 0 });
  // Measured sizes are what a card covers.
  const sizes = new Map([["a", { width: 600, height: NODE_HEIGHT }]]);
  const wide = nudgeFree(positions, sizes, "n", { x: 300, y: 0 });
  assert.ok(wide.x >= 600, `past the wide card: ${JSON.stringify(wide)}`);
});

test("tidy lays every card out again at the measured sizes and puts the steps in reading order", () => {
  const w = wf(
    step("s", [], { position: { x: 0, y: 0 } }),
    step("c", [{ to: "s" }], { position: { x: 900, y: 900 } }),
    step("b", [{ to: "s" }]),
    step("d", [{ to: "b" }, { to: "c" }], { position: { x: 500, y: 500 } }),
  );
  const t = tidy(w, { sizes: new Map([["d", { width: NODE_WIDTH, height: 120 }]]) });
  assert.deepEqual(t.steps.map((s) => s.id), ["d", "c", "b", "s"], "top to bottom, then left to right — rank-mates in the steps order they had");
  assert.ok(t.steps.every((s) => s.position), "every step is placed");
  assert.equal(t.steps[0].position.y, 0);
  assert.equal(t.steps[1].position.y, 120 + GAP_Y, "the measured tall card pushes the next row");
  assert.equal(JSON.stringify(tidy(w).steps), JSON.stringify(tidy(w).steps), "deterministic");
  assert.equal(tidy(wf()).steps.length, 0);
});

test("withPositions fills only what is missing, and hands the same definition back when nothing is", () => {
  const w = wf(step("a", [{ to: "b" }], { position: { x: 5, y: 5 } }), step("b"));
  const { positions } = layout(w);
  const filled = withPositions(w, positions);
  assert.deepEqual(filled.steps[0].position, { x: 5, y: 5 }, "a placed step keeps its own place, unsnapped in the body");
  assert.deepEqual(filled.steps[1].position, positions.get("b"));
  assert.equal(withPositions(filled, positions), filled, "nothing missing: the same object");
  assert.equal(withPositions(w, new Map()), w, "nothing known: the same object");
});

test("a long workflow is laid out whole, quickly, with no two cards on one spot — a fan as well as a line", () => {
  const n = 500;
  const line = wf(...Array.from({ length: n }, (_, i) => step(`s${i}`, i + 1 < n ? [{ to: `s${i + 1}` }] : [])));
  const began = performance.now();
  const laid = layout(line);
  assert.equal(laid.positions.size, n);
  assert.equal(laid.positions.get(`s${n - 1}`).y, (n - 1) * row, "one row per step, to the end");
  assert.equal(laid.loops.size, 0);
  // One root fanning out to 200 leaves: a row of cards, none on another.
  const fan = wf(step("root", Array.from({ length: 200 }, (_, i) => ({ to: `leaf${i}` }))), ...Array.from({ length: 200 }, (_, i) => step(`leaf${i}`)));
  const spread = layout(fan);
  const spots = new Set([...spread.positions.values()].map((p) => `${p.x},${p.y}`));
  assert.equal(spots.size, 201, "every card has a spot of its own");
  const tidied = tidy(line);
  assert.equal(tidied.steps.length, n);
  assert.ok(tidied.steps.every((s) => s.position && Number.isFinite(s.position.x) && Number.isFinite(s.position.y)), "Tidy writes every card's place");
  const took = performance.now() - began;
  assert.ok(took < 3000, `a canvas this long still draws: ${Math.round(took)} ms`);
});

test("every kind is one card, the new start, emit and parallel included; a card with boundary chips is a row taller", () => {
  for (const kind of ["start", "emit", "parallel", "agent", "decide", "end"]) assert.deepEqual(estimateOf({ kind }), { width: NODE_WIDTH, height: NODE_HEIGHT }, kind);
  const chips = { kind: "approval", boundaries: [{ name: "late", on: { event: "after", secs: 60 }, act: "divert" }] };
  assert.deepEqual(estimateOf(chips), { width: NODE_WIDTH, height: NODE_HEIGHT + BOUNDARY_ROW });
  assert.deepEqual(estimateOf({ kind: "approval", boundaries: [] }), { width: NODE_WIDTH, height: NODE_HEIGHT });
  assert.deepEqual(estimateOf(null), { width: NODE_WIDTH, height: NODE_HEIGHT });
  // The rank below a card with chips starts under its chips.
  const { positions } = layout(wf(step("a", [{ to: "b" }], { kind: "approval", prompt: "?", boundaries: chips.boundaries }), step("b")));
  assert.equal(positions.get("b").y, NODE_HEIGHT + BOUNDARY_ROW + GAP_Y);
});

test("a divert's path leaves the card's lower right: its target stands right of the main path, whatever the steps' order", () => {
  const review = step("review", [{ to: "escalate", branch: "late" }, { to: "ship" }], {
    kind: "approval",
    prompt: "?",
    boundaries: [{ name: "late", on: { event: "after", secs: 60 }, act: "divert" }],
  });
  const w = wf({ id: "begin", name: "Begin", kind: "start", on: { event: "manual" }, then: [{ to: "review" }] }, review, step("escalate"), step("ship"));
  const { positions } = layout(w);
  assert.equal(positions.get("ship").y, positions.get("escalate").y, "rank-mates");
  assert.ok(positions.get("escalate").x > positions.get("ship").x, "the escalation stands to the right, though it comes first in the list");
  assert.deepEqual([...sideOnly(toGraph(w).edges)], ["escalate"]);
  // A step a main flow also reaches is no side path.
  const both = wf(review, step("escalate"), step("ship", [{ to: "escalate" }]));
  assert.equal(sideOnly(toGraph(both).edges).size, 0);
});

test("a ring, a flow to a step that is not there, and an empty workflow are drawn, never thrown on", () => {
  const ring = layout(wf(step("a", [{ to: "b" }]), step("b", [{ to: "c" }]), step("c", [{ to: "a" }])));
  assert.equal(ring.positions.size, 3);
  assert.ok(ring.loops.size >= 1, "the edge home is drawn as the loop it is");
  const dangling = layout(wf(step("a", [{ to: "ghost" }])));
  assert.deepEqual([...dangling.positions.keys()], ["a"], "a flow to nothing draws no card for nothing");
  const empty = layout(wf());
  assert.equal(empty.positions.size, 0);
  assert.deepEqual(empty.size, { width: 0, height: 0 });
});

