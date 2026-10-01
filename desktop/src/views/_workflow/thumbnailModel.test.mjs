/**
 * A workflow's thumbnail is the designer's picture, as shapes: the cards where
 * the layout puts them, a flow from a bottom handle to a top, a branch at its
 * own handle, an on-fail route from the left, a loop out and back on the
 * right, a padded viewBox — and nothing for an empty definition. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/thumbnailModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { LOOP_OUT, PAD, thumbnail } from "./thumbnailModel.mjs";
import { GAP_Y, NODE_HEIGHT, NODE_WIDTH, layout } from "./workflowLayout.mjs";

const step = (id, then = [], extra = {}) => ({ id, name: id, kind: "agent", instructions: id, then, ...extra });
const wf = (...steps) => ({ name: "w", steps });
const row = NODE_HEIGHT + GAP_Y;

test("a chain is one column of cards at the layout's places, each flow from a bottom centre to the next top centre", () => {
  const t = thumbnail(wf(step("a", [{ to: "b" }]), step("b", [{ to: "c" }]), step("c")));
  assert.deepEqual(
    t.nodes.map((n) => [n.id, n.kind, n.x, n.y, n.w, n.h]),
    [
      ["a", "agent", 0, 0, NODE_WIDTH, NODE_HEIGHT],
      ["b", "agent", 0, row, NODE_WIDTH, NODE_HEIGHT],
      ["c", "agent", 0, 2 * row, NODE_WIDTH, NODE_HEIGHT],
    ],
  );
  assert.equal(t.edges.length, 2);
  const ab = t.edges.find((e) => e.id === "a->b");
  assert.equal(ab.kind, "then");
  assert.deepEqual(ab.points, [
    { x: NODE_WIDTH / 2, y: NODE_HEIGHT },
    { x: NODE_WIDTH / 2, y: row },
  ]);
  assert.deepEqual(t.viewBox, { x: -PAD, y: -PAD, w: NODE_WIDTH + 2 * PAD, h: 2 * row + NODE_HEIGHT + 2 * PAD }, "the picture, padded");
});

test("the cards stand where the designer draws them: a step's own position is kept, the placeless are laid out the same way", () => {
  const placed = wf(step("a", [{ to: "b" }], { position: { x: 400, y: 80 } }), step("b"));
  const t = thumbnail(placed);
  const a = t.nodes.find((n) => n.id === "a");
  assert.deepEqual([a.x, a.y], [400, 80], "its own position");
  const b = t.nodes.find((n) => n.id === "b");
  assert.deepEqual([b.x, b.y], [layout(placed).positions.get("b").x, layout(placed).positions.get("b").y], "the layout's, as on the canvas");
});

test("a branching step's flows leave at the branch's own handle, spread across the bottom edge in the branches' order", () => {
  const decide = { id: "d", name: "Which?", kind: "decide", rules: [{ when: { condition: "answered", step: "x" }, branch: "yes" }], otherwise: "no", then: [{ to: "y", branch: "yes" }, { to: "n", branch: "no" }] };
  const t = thumbnail(wf(decide, step("y"), step("n")));
  const yes = t.edges.find((e) => e.id === "d->y#yes");
  const no = t.edges.find((e) => e.id === "d->n#no");
  const d = t.nodes.find((n) => n.id === "d");
  assert.equal(yes.points[0].x, d.x + NODE_WIDTH / 3, "the first of two branches at a third");
  assert.equal(no.points[0].x, d.x + (2 * NODE_WIDTH) / 3, "the second at two thirds");
  assert.equal(yes.points[0].y, d.y + NODE_HEIGHT, "both leave the bottom");
  assert.equal(d.name, "Which?", "a card wears the step's name");
});

test("an on-fail route leaves the source's left and arrives at the target's top, in its own kind", () => {
  const t = thumbnail(wf(step("a", [{ to: "b" }], { on_fail: { on_fail: "then", step: "fix" } }), step("b"), step("fix")));
  const fail = t.edges.find((e) => e.kind === "on_fail");
  assert.ok(fail, "the route is a line");
  const a = t.nodes.find((n) => n.id === "a");
  const fix = t.nodes.find((n) => n.id === "fix");
  assert.deepEqual(fail.points[0], { x: a.x, y: a.y + NODE_HEIGHT * 0.65 }, "from the left edge");
  assert.deepEqual(fail.points[1], { x: fix.x + NODE_WIDTH / 2, y: fix.y }, "to the top centre");
});

test("a loop leaves the source's right, goes out past every card it passes and comes back to the target's right — four points, its own kind, inside the viewBox", () => {
  const t = thumbnail(wf(step("a", [{ to: "b" }]), step("b", [{ to: "a" }])));
  const loop = t.edges.find((e) => e.kind === "loop");
  assert.ok(loop, "the flow back is a loop");
  assert.equal(loop.id, "b->a");
  assert.equal(loop.points.length, 4);
  const [leave, out1, out2, arrive] = loop.points;
  const a = t.nodes.find((n) => n.id === "a");
  const b = t.nodes.find((n) => n.id === "b");
  assert.deepEqual(leave, { x: b.x + NODE_WIDTH, y: b.y + NODE_HEIGHT * 0.65 });
  assert.equal(out1.x, Math.max(a.x, b.x) + NODE_WIDTH + LOOP_OUT);
  assert.equal(out1.x, out2.x, "straight out, straight back");
  assert.deepEqual(arrive, { x: a.x + NODE_WIDTH, y: a.y + NODE_HEIGHT * 0.35 });
  assert.ok(t.viewBox.x + t.viewBox.w >= out1.x + PAD, "the viewBox holds the loop's far edge");
  assert.equal(t.edges.filter((e) => e.kind === "then").length, 1, "the forward flow is still a flow");
});

test("a flow to a step that is not there draws no line; an empty definition draws nothing", () => {
  const t = thumbnail(wf(step("a", [{ to: "gone" }])));
  assert.equal(t.nodes.length, 1);
  assert.deepEqual(t.edges, [], "the validator's problem, not a line");
  assert.deepEqual(thumbnail({ name: "empty", steps: [] }), { viewBox: { x: 0, y: 0, w: 1, h: 1 }, nodes: [], edges: [] });
  assert.deepEqual(thumbnail({ name: "bare" }).nodes, []);
});

test("each card says its family, so an event is drawn as the canvas's pill and a gateway as its own", () => {
  const t = thumbnail(wf({ id: "begin", name: "Begin", kind: "start", on: { event: "manual" }, then: [{ to: "fan" }] }, { id: "fan", name: "Fan", kind: "parallel", then: [{ to: "a" }] }, step("a", [{ to: "stop" }]), { id: "stop", name: "Stop", kind: "end" }));
  assert.deepEqual(t.nodes.map((n) => [n.id, n.family]), [
    ["begin", "event"],
    ["fan", "gateway"],
    ["a", "task"],
    ["stop", "event"],
  ]);
});

test("a divert's path leaves the card's lower right, and the step's own flow moves left to share the edge", () => {
  const review = step("review", [{ to: "ship" }, { to: "escalate", branch: "late" }], { kind: "approval", prompt: "?", boundaries: [{ name: "late", on: { event: "after", secs: 60 }, act: "divert" }] });
  const t = thumbnail(wf(review, step("ship"), step("escalate")));
  const r = t.nodes.find((n) => n.id === "review");
  const late = t.edges.find((e) => e.id === "review->escalate#late");
  assert.equal(late.kind, "boundary");
  assert.deepEqual(late.points[0], { x: r.x + (NODE_WIDTH * 75) / 100, y: r.y + NODE_HEIGHT }, "at the divert's handle, three quarters along");
  const own = t.edges.find((e) => e.id === "review->ship");
  assert.equal(own.kind, "then");
  assert.equal(own.points[0].x, r.x + (NODE_WIDTH * 30) / 100, "the normal flow keeps the left");
});

test("the same definition draws the same picture every time", () => {
  const d = wf(step("a", [{ to: "b" }, { to: "c" }]), step("b", [{ to: "d" }]), step("c", [{ to: "d" }]), step("d"));
  assert.deepEqual(thumbnail(d), thumbnail(d));
});
