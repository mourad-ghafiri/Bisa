import { strict as assert } from "node:assert";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { DRAWING_GONE, MERMAID_REFUSED, NOT_A_SKELETON, UNKNOWN_ACTION, drawingWords, drawnResult, mergeElements, planDrawRequest, refusedResult, saveBase, snapshotName, snapshotResult } from "./drawRequestModel.mjs";

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
  const lost = bridge.indexOf('e.status === 409 && conflictStep(canvas.live, attempt) === "merge"');
  const retry = bridge.slice(lost, bridge.indexOf("continue;", lost));
  assert.ok(retry.includes("read = (await api.drawing(detail.id)).drawing;"), "the store's scene is re-read");
  assert.ok(retry.includes("current = drawnElements(mod.restoreElements(read.scene.elements"), "and the incoming elements join that scene on the next pass");
  assert.ok(bridge.indexOf("const canvas = await canvasFor(detail);") > bridge.indexOf("regenerateIds: false"), "the canvas is chosen after the shapes are laid out, as late as it can be");
  assert.ok(bridge.includes('log.warn("draw", "an answer to a drawing request was not taken'), "a late answer is a warning: the engine announces its save");
});

test("the working dot's words count the agents drawing", () => {
  const one = drawingWords(1);
  const several = drawingWords(3);
  assert.ok(one && several && one !== several, `${one} / ${several}`);
  assert.ok(several.includes("3"), several);
});

// --- the hardening pass ---

test("a request is answered within the engine's wait, taken up only while the engine still waits, and its answer retried when the node did not take it", async () => {
  const m = await import("./drawRequestModel.mjs");
  assert.ok(m.REQUEST_DEADLINE_MS < 60_000, "under the engine's sixty seconds, so the agent hears the desktop's word and not a silence");
  // A frame carries no time: it is now. A parked row is as old as it says.
  assert.equal(m.requestExpired(0, 1_000_000), false);
  assert.equal(m.requestExpired(1000, 1000 * 1000 + 30_000), false);
  assert.equal(m.requestExpired(1000, 1000 * 1000 + 61_000), true, "the engine gave up on it a moment ago");
  // The answer: sent again when the node did not answer or failed; never when it said nothing waits.
  assert.equal(m.answerRetries(0, 0), true);
  assert.equal(m.answerRetries(503, 1), true);
  assert.equal(m.answerRetries(404, 0), false, "nothing waits under that id: the engine has already answered the agent");
  assert.equal(m.answerRetries(400, 0), false);
  assert.equal(m.answerRetries(0, m.ANSWER_TRIES - 1), false, "a last try is the last");
});

test("a Mermaid diagram the canvas can only show as a picture is refused before any canvas is touched", async () => {
  const { mermaidDrawable } = await import("./drawRequestModel.mjs");
  assert.equal(mermaidDrawable([{ type: "rectangle" }, { type: "arrow" }, { type: "text" }]), true);
  assert.equal(mermaidDrawable([{ type: "image", fileId: "x" }]), false, "a pie, a gantt, a mindmap: the converter hands back one image, which the store refuses");
  assert.equal(mermaidDrawable([]), false, "nothing to draw is nothing drawn");
});

test("a save that lost on the open canvas is answered as moved; an offscreen one merges into what the store holds, once", async () => {
  const { conflictStep } = await import("./drawRequestModel.mjs");
  assert.equal(conflictStep(true, 0), "moved", "the person's canvas is never replaced under them by an agent's retry");
  assert.equal(conflictStep(false, 0), "merge");
  assert.equal(conflictStep(false, 1), "moved");
});

test("the wiring: every request is answered, the queue cannot hang on a hidden window, and a closed drawing keeps its ground", () => {
  const bridge = readFileSync(new URL("./drawBridge.ts", import.meta.url), "utf8");
  const one = bridge.slice(bridge.indexOf("async function performOne("), bridge.indexOf("async function working<T>("));
  assert.ok(one.includes("} catch (e) {") && one.includes("CANVAS_FAILED"), "a throw anywhere in the work is a refusal, answered");
  assert.ok(one.includes("REQUEST_DEADLINE_MS") && one.includes("CANVAS_SLOW"), "and so is work that outlasts the deadline");
  assert.ok(one.includes("requestExpired(") && one.includes("answerRetries("), "the model says what is too old to take up and which answer is sent again");
  const settle = bridge.slice(bridge.indexOf("async function settle("), bridge.indexOf("async function canvasFor("));
  assert.ok(settle.includes("setTimeout"), "a window that is hidden paints no frame: the wait ends on a clock too");
  assert.ok(bridge.includes("appState: canvasAppState(detail.scene.app_state)"), "the offscreen canvas is loaded with the drawing's own background and grid");
  assert.ok(bridge.includes("mermaidDrawable(") && bridge.indexOf("mermaidDrawable(") < bridge.indexOf("const canvas = await canvasFor(detail);"), "refused before a canvas is chosen");
  assert.ok(bridge.includes("conflictStep("), "the model decides what a lost save does");
  assert.ok(bridge.includes("prefersReducedMotion()"), "the scroll to an agent's shapes does not animate for someone who asked for less motion");
  const offscreen = readFileSync(new URL("./offscreen.ts", import.meta.url), "utf8");
  assert.ok(offscreen.includes("OFFSCREEN_MOUNT_MS") && offscreen.includes("reject("), "a canvas that never mounts fails the request, not the queue");
  assert.ok(offscreen.includes("OFFSCREEN_IDLE_MS"), "and is let go of once nobody has needed it for a while");
});
