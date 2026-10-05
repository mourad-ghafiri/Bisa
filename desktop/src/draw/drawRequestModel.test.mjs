import { strict as assert } from "node:assert";
import { test } from "node:test";
import { DRAWING_GONE, MERMAID_REFUSED, NOT_A_SKELETON, UNKNOWN_ACTION, drawnResult, mergeElements, planDrawRequest, refusedResult, saveBase, snapshotName, snapshotResult } from "./drawRequestModel.mjs";

test("a bridge save states the open canvas's record when it drew there, else the hash it read", () => {
  assert.equal(saveBase(true, "record", "read"), "record", "two writers save through the open canvas");
  assert.equal(saveBase(true, null, "read"), "read", "an open canvas not loaded yet has no record");
  assert.equal(saveBase(false, "record", "read"), "read", "an offscreen canvas never borrows an open one's record");
});

test("a request is planned as a draw, a Mermaid or a snapshot, and refused in a sentence otherwise", () => {
  assert.deepEqual(planDrawRequest({ action: "draw", drawing: "01D", elements: [{ type: "rectangle", x: 0, y: 0 }], replace: true }), {
    kind: "draw",
    drawing: "01D",
    elements: [{ type: "rectangle", x: 0, y: 0 }],
    replace: true,
  });
  assert.deepEqual(planDrawRequest({ action: "mermaid", drawing: "01D", text: " flowchart LR\n a --> b " }), { kind: "mermaid", drawing: "01D", text: "flowchart LR\n a --> b", replace: false });
  assert.deepEqual(planDrawRequest({ action: "snapshot", drawing: "01D" }), { kind: "snapshot", drawing: "01D" });
  assert.deepEqual(planDrawRequest({ action: "draw", elements: [] }), { kind: "refuse", error: DRAWING_GONE });
  assert.deepEqual(planDrawRequest({ action: "draw", drawing: "01D", elements: "no" }), { kind: "refuse", error: NOT_A_SKELETON });
  assert.deepEqual(planDrawRequest({ action: "draw", drawing: "01D", elements: [null] }), { kind: "refuse", error: NOT_A_SKELETON });
  assert.deepEqual(planDrawRequest({ action: "mermaid", drawing: "01D", text: "  " }), { kind: "refuse", error: MERMAID_REFUSED });
  assert.deepEqual(planDrawRequest({ action: "read", drawing: "01D" }), { kind: "refuse", error: UNKNOWN_ACTION }, "the engine answers a reading itself");
});

test("incoming elements replace whole, or override by id and append, never dropping the person's", () => {
  const a = { id: "a", v: 1 };
  const b = { id: "b", v: 1 };
  const a2 = { id: "a", v: 2 };
  const c = { id: "c", v: 1 };
  assert.deepEqual(mergeElements([a, b], [a2, c], true), [a2, c]);
  assert.deepEqual(mergeElements([a, b], [a2, c], false), [a2, b, c]);
  assert.deepEqual(mergeElements([], [c], false), [c]);
  assert.deepEqual(mergeElements([a], [], false), [a]);
});

test("the answers carry what the agent reads: the hash and count, the snapshot by reference, a refusal's words", () => {
  assert.deepEqual(drawnResult("01D", "h1", 4), { ok: true, drawing: "01D", hash: "h1", element_count: 4 });
  const ref = { sha256: "ab".repeat(32), name: "drawing-01D-x.png", mime: "image/png", size: 10 };
  assert.deepEqual(snapshotResult("01D", ref, { width: 1280, height: 720 }), { ok: true, drawing: "01D", snapshot: ref, width: 1280, height: 720 });
  assert.deepEqual(refusedResult("no"), { ok: false, error: "no" });
  assert.deepEqual(refusedResult("no", "01D"), { ok: false, error: "no", drawing: "01D" });
  assert.match(snapshotName("01ABC", new Date("2026-09-24T10:11:12.345Z")), /^drawing-01ABC-20260924T101112Z\.png$/);
  assert.match(snapshotName("", new Date(0)), /^drawing-drawing-/);
});

test("the bridge performs one request at a time, remembers only the latest it took up, and keeps no scene for a drawing that is not open", async () => {
  const { readFileSync } = await import("node:fs");
  const { HANDLED_KEPT } = await import("./drawRequestModel.mjs");
  const { Lru } = await import("../shell/lru.mjs");
  const bridge = readFileSync(new URL("./drawBridge.ts", import.meta.url), "utf8");
  // One canvas offscreen: two requests performed together would save one drawing's shapes into the other.
  assert.ok(bridge.includes("const mine = turn.then(() => performOne(pending));"), "each request waits for the one before it");
  assert.ok(bridge.includes("turn = mine.catch("), "and one that throws ends its own turn, nobody else's");
  assert.ok(!/^export async function performDrawRequest/m.test(bridge), "no request starts performing at the call");
  // Taken up once, however it was heard — and the memory of that is bounded.
  assert.ok(bridge.includes("const handled = new Lru<true>(HANDLED_KEPT);") && !bridge.includes("new Set<string>()"), "never every request since the window opened");
  const kept = new Lru(HANDLED_KEPT);
  for (let i = 0; i < HANDLED_KEPT + 50; i++) kept.set(`r${i}`, true);
  assert.equal(kept.has("r0"), false, "the oldest go");
  assert.equal(kept.has(`r${HANDLED_KEPT + 49}`), true);
  assert.equal(kept.has("r50"), true, "the latest are all there: a request still on the parked list is never performed twice");
  // A save moves the record only of the canvas it went through: an offscreen
  // save leaves an open canvas's record alone, and a close forgets its own.
  const live = readFileSync(new URL("./liveScene.ts", import.meta.url), "utf8");
  assert.ok(live.includes("if (live && live.api === by) scenes.set(drawing, { api: live.api, saved });"), "only the canvas that saved moves its record");
  assert.ok(live.includes("if (scenes.get(drawing)?.api === api) scenes.delete(drawing);"), "a close never forgets a canvas that took the drawing meanwhile");
  assert.ok(bridge.includes("noteSaved(detail.id, { hash: drawing.hash, elements }, canvas.api);"), "the bridge names the canvas it saved through");
  assert.ok(bridge.includes("saveBase(canvas.live, lastSaved(detail.id, canvas.api)?.hash ?? null, read.hash)"), "and states that canvas's hash, else the one it read");
  // A 409 re-reads the scene, not the hash alone: the agent's shapes join what the store holds.
  const retry = bridge.slice(bridge.indexOf("e.status === 409 && attempt === 0"), bridge.indexOf("continue;"));
  assert.ok(retry.includes("read = (await api.drawing(detail.id)).drawing;"), "the store's scene is re-read");
  assert.ok(retry.includes("current = drawnElements(mod.restoreElements(read.scene.elements"), "and the incoming elements join that scene on the next pass");
  assert.ok(bridge.indexOf("const canvas = await canvasFor(detail);") > bridge.indexOf("regenerateIds: false"), "the canvas is chosen after the shapes are laid out, as late as it can be");
  assert.ok(bridge.includes('log.warn("draw", "an answer to a drawing request was not taken'), "a late answer is a warning: the engine announces its save");
});
